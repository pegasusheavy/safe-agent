use std::net::{Ipv6Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use axum::Router;
use rustls_acme::caches::DirCache;
use rustls_acme::AcmeConfig;
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tracing::{error, info};

use crate::config::{Config, TlsConfig};
use crate::error::{Result, SafeAgentError};

/// Resolve the effective ACME configuration by merging config file values
/// with environment variable overrides.
pub fn resolve_tls_config(config: &Config) -> TlsConfig {
    let mut tls = config.tls.clone();

    if let Ok(v) = std::env::var("ACME_ENABLED") {
        tls.acme_enabled = v == "true" || v == "1";
    }

    if let Ok(domain) = std::env::var("ACME_DOMAIN") {
        tls.acme_domains = domain
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }

    if let Ok(email) = std::env::var("ACME_EMAIL") {
        tls.acme_email = email;
    }

    if let Ok(v) = std::env::var("ACME_PRODUCTION") {
        tls.acme_production = v == "true" || v == "1";
    }

    if let Ok(dir) = std::env::var("ACME_CACHE_DIR") {
        tls.acme_cache_dir = dir;
    }

    if let Ok(port) = std::env::var("ACME_PORT") {
        if let Ok(p) = port.parse::<u16>() {
            tls.acme_port = p;
        }
    }

    tls
}

/// Validate that the ACME configuration has all required fields.
/// Returns an error suitable for aborting startup.
pub fn validate_acme_config(tls: &TlsConfig) -> Result<()> {
    if tls.acme_domains.is_empty() {
        return Err(SafeAgentError::Config(
            "ACME is enabled but no domains are configured. \
             Set ACME_DOMAIN or tls.acme_domains in config.toml."
                .into(),
        ));
    }

    if tls.acme_email.is_empty() {
        return Err(SafeAgentError::Config(
            "ACME is enabled but no contact email is configured. \
             Set ACME_EMAIL or tls.acme_email in config.toml."
                .into(),
        ));
    }

    Ok(())
}

/// Start the HTTPS server with automatic Let's Encrypt certificate
/// provisioning via the TLS-ALPN-01 challenge.
///
/// Blocks until the first certificate is successfully obtained (or the
/// timeout expires).  If the certificate cannot be acquired, returns an
/// error so the caller can abort the process.
pub async fn serve_https(
    tls: &TlsConfig,
    app: Router,
    mut shutdown: broadcast::Receiver<()>,
) -> Result<()> {
    let domains: Vec<String> = tls.acme_domains.clone();
    let contacts: Vec<String> = if tls.acme_email.contains("mailto:") {
        vec![tls.acme_email.clone()]
    } else {
        vec![format!("mailto:{}", tls.acme_email)]
    };

    let cache_dir = if tls.acme_cache_dir.is_empty() {
        Config::data_dir().join("acme-cache")
    } else {
        PathBuf::from(&tls.acme_cache_dir)
    };

    // Ensure cache directory exists.
    std::fs::create_dir_all(&cache_dir).map_err(|e| {
        SafeAgentError::Config(format!(
            "failed to create ACME cache directory {}: {e}",
            cache_dir.display()
        ))
    })?;

    let env_label = if tls.acme_production {
        "production"
    } else {
        "staging"
    };

    info!(
        domains = ?domains,
        contacts = ?contacts,
        cache = %cache_dir.display(),
        environment = env_label,
        port = tls.acme_port,
        "starting ACME TLS server"
    );

    let mut state = AcmeConfig::new(domains.clone())
        .contact(contacts)
        .cache(DirCache::new(cache_dir))
        .directory_lets_encrypt(tls.acme_production)
        .state();

    let acceptor = state.axum_acceptor(state.default_rustls_config());

    // Spawn a background task that processes ACME events (certificate
    // renewals, errors, etc.).  We also use a oneshot channel to signal
    // when the first certificate is ready.
    let (cert_tx, cert_rx) = tokio::sync::oneshot::channel::<std::result::Result<(), String>>();
    let mut cert_tx = Some(cert_tx);

    tokio::spawn(async move {
        loop {
            match state.next().await {
                Some(Ok(ok)) => {
                    info!("ACME event: {:?}", ok);
                    // Signal success on first cert-related event.
                    if let Some(tx) = cert_tx.take() {
                        let _ = tx.send(Ok(()));
                    }
                }
                Some(Err(err)) => {
                    error!("ACME error: {:?}", err);
                    if let Some(tx) = cert_tx.take() {
                        let _ = tx.send(Err(format!("{err:?}")));
                    }
                }
                None => break,
            }
        }
    });

    // Wait for the first certificate or a timeout.  This is the gate
    // that makes the container fail if the CA cert cannot be obtained.
    let timeout = Duration::from_secs(120);
    info!(
        timeout_secs = timeout.as_secs(),
        "waiting for initial ACME certificate..."
    );

    match tokio::time::timeout(timeout, cert_rx).await {
        Ok(Ok(Ok(()))) => {
            info!("ACME certificate obtained — HTTPS is ready");
        }
        Ok(Ok(Err(acme_err))) => {
            error!(err = %acme_err, "ACME certificate acquisition failed");
            return Err(SafeAgentError::Config(format!(
                "Let's Encrypt certificate could not be obtained: {acme_err}"
            )));
        }
        Ok(Err(_recv_err)) => {
            error!("ACME event channel dropped before certificate was obtained");
            return Err(SafeAgentError::Config(
                "ACME internal error: event channel closed unexpectedly".into(),
            ));
        }
        Err(_) => {
            error!(
                timeout_secs = timeout.as_secs(),
                "timed out waiting for ACME certificate"
            );
            return Err(SafeAgentError::Config(format!(
                "Let's Encrypt certificate was not obtained within {}s. \
                 Ensure the domain resolves to this server and port {} is reachable.",
                timeout.as_secs(),
                tls.acme_port,
            )));
        }
    }

    let addr = SocketAddr::from((Ipv6Addr::UNSPECIFIED, tls.acme_port));
    info!(addr = %addr, "HTTPS server listening");

    // Run the HTTPS server with graceful shutdown.
    let server_future = axum_server::bind(addr)
        .acceptor(acceptor)
        .serve(app.into_make_service());

    tokio::select! {
        result = server_future => {
            if let Err(e) = result {
                return Err(SafeAgentError::Config(format!("HTTPS server error: {e}")));
            }
        }
        _ = shutdown.recv() => {
            info!("HTTPS server shutting down");
        }
    }

    Ok(())
}
