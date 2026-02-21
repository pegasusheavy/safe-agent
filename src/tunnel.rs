use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use tokio::process::{Child, Command};
use tokio::sync::watch;
use tracing::{error, info, warn};

use crate::config::TunnelConfig;

/// Response shape from ngrok's local agent API (`/api/tunnels`).
#[derive(serde::Deserialize)]
struct NgrokTunnelsResponse {
    tunnels: Vec<NgrokTunnel>,
}

#[derive(serde::Deserialize)]
struct NgrokTunnel {
    public_url: String,
}

/// Manages an ngrok tunnel subprocess and exposes the public URL.
///
/// On start, spawns `ngrok http <port>` and polls the local agent API
/// at `http://127.0.0.1:<inspect_port>/api/tunnels` until the public
/// URL is available.  The URL is published via a `watch` channel so
/// any number of consumers (skill manager, dashboard, etc.) can read
/// the current value without coordination.
pub struct TunnelManager {
    child: Option<Child>,
    url_tx: watch::Sender<Option<String>>,
    url_rx: watch::Receiver<Option<String>>,
}

impl TunnelManager {
    /// Start the ngrok tunnel and return a manager handle.
    ///
    /// The public URL won't be available immediately — call
    /// `public_url()` or subscribe via `url_receiver()` to get it
    /// once ngrok reports it.
    pub async fn start(config: &TunnelConfig, local_port: u16) -> Self {
        let (url_tx, url_rx) = watch::channel(None);

        let ngrok_bin = std::env::var("NGROK_BIN")
            .unwrap_or_else(|_| config.ngrok_bin.clone());

        let authtoken = std::env::var("NGROK_AUTHTOKEN").ok().or_else(|| {
            if config.authtoken.is_empty() {
                None
            } else {
                Some(config.authtoken.clone())
            }
        });

        let port = std::env::var("NGROK_PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(local_port);

        let inspect_addr = format!("127.0.0.1:{}", config.inspect_port);

        let mut cmd = Command::new(&ngrok_bin);
        cmd.arg("http")
            .arg(port.to_string())
            .arg("--log").arg("stderr")
            .arg("--log-format").arg("json");

        if let Some(ref token) = authtoken {
            cmd.arg("--authtoken").arg(token);
        }

        let domain = std::env::var("NGROK_DOMAIN")
            .ok()
            .filter(|d| !d.is_empty())
            .unwrap_or_else(|| config.domain.clone());

        if !domain.is_empty() {
            cmd.arg("--domain").arg(&domain);
        }

        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit());

        info!(
            ngrok_bin = %ngrok_bin,
            port,
            inspect_port = config.inspect_port,
            domain = %domain,
            "starting ngrok tunnel"
        );

        let child = match cmd.spawn() {
            Ok(c) => {
                info!(pid = ?c.id(), "ngrok process spawned");
                Some(c)
            }
            Err(e) => {
                error!(err = %e, bin = %ngrok_bin, "failed to spawn ngrok — tunnel disabled");
                return Self {
                    child: None,
                    url_tx,
                    url_rx,
                };
            }
        };

        // Spawn a task that polls the ngrok local API until the tunnel URL
        // appears, then keeps monitoring in case ngrok restarts the tunnel.
        let tx = url_tx.clone();
        let api_url = format!("http://{inspect_addr}/api/tunnels");
        let poll_interval = Duration::from_secs(config.poll_interval_secs);

        tokio::spawn(async move {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap_or_default();

            // Give ngrok a moment to start its local API server.
            tokio::time::sleep(Duration::from_secs(2)).await;

            let mut attempts = 0u32;
            loop {
                match client.get(&api_url).send().await {
                    Ok(resp) => {
                        if let Ok(body) = resp.json::<NgrokTunnelsResponse>().await {
                            if let Some(t) = body.tunnels.into_iter().find(|t| t.public_url.starts_with("https://")) {
                                let url = t.public_url;
                                if tx.send(Some(url.clone())).is_ok() {
                                    info!(public_url = %url, "ngrok tunnel ready");
                                }
                            }
                        }
                        attempts = 0;
                    }
                    Err(e) => {
                        attempts += 1;
                        if attempts <= 5 {
                            warn!(
                                attempt = attempts,
                                err = %e,
                                "ngrok API not ready yet, retrying..."
                            );
                        } else if attempts == 6 {
                            error!(
                                err = %e,
                                "ngrok API unreachable after multiple attempts — \
                                 check that ngrok is installed and NGROK_AUTHTOKEN is set"
                            );
                        }
                    }
                }

                tokio::time::sleep(poll_interval).await;
            }
        });

        Self {
            child,
            url_tx,
            url_rx,
        }
    }

    /// Get a `watch::Receiver` that updates whenever the tunnel URL changes.
    pub fn url_receiver(&self) -> watch::Receiver<Option<String>> {
        self.url_rx.clone()
    }

}

impl Drop for TunnelManager {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.child {
            // Best-effort kill on drop; can't await here.
            let _ = child.start_kill();
        }
    }
}

/// Shared handle to the tunnel manager's public URL.
///
/// Passed to the skill manager and dashboard so they can read the
/// current tunnel URL without owning the manager itself.
pub type TunnelUrl = Arc<watch::Receiver<Option<String>>>;

/// Create a `TunnelUrl` from a `TunnelManager`.
pub fn shared_url(mgr: &TunnelManager) -> TunnelUrl {
    Arc::new(mgr.url_receiver())
}
