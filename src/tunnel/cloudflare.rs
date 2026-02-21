use std::process::Stdio;

use tokio::io::AsyncBufReadExt;
use tokio::process::{Child, Command};
use tokio::sync::watch;
use tracing::{error, info, warn};

use crate::config::CloudflareConfig;

/// Start a Cloudflare Tunnel and discover the public URL.
///
/// Supports three modes:
/// - **Static URL**: `config.url` or `CLOUDFLARE_TUNNEL_URL` is set — no
///   subprocess, just publish the URL directly.
/// - **Quick tunnel** (no `tunnel_id`): `cloudflared tunnel --url http://localhost:<port>`.
///   cloudflared prints the trycloudflare.com URL to stderr.
/// - **Named tunnel** (`tunnel_id` + `credentials_file`): `cloudflared tunnel run`.
///   The hostname is taken from config and used directly once cloudflared
///   reports a connection.
pub async fn start(
    config: &CloudflareConfig,
    local_port: u16,
    url_tx: watch::Sender<Option<String>>,
) -> Option<Child> {
    // Static URL override — no subprocess needed.
    let static_url = std::env::var("CLOUDFLARE_TUNNEL_URL")
        .ok()
        .filter(|u| !u.is_empty())
        .or_else(|| {
            if config.url.is_empty() { None } else { Some(config.url.clone()) }
        });

    if let Some(url) = static_url {
        info!(url = %url, "using static Cloudflare tunnel URL");
        let _ = url_tx.send(Some(url));
        return None;
    }

    let bin = std::env::var("CLOUDFLARED_BIN")
        .unwrap_or_else(|_| config.bin.clone());

    let mut cmd = Command::new(&bin);

    if !config.tunnel_id.is_empty() {
        // Named tunnel mode
        cmd.arg("tunnel");
        if !config.credentials_file.is_empty() {
            cmd.arg("--credentials-file").arg(&config.credentials_file);
        }
        cmd.arg("run").arg(&config.tunnel_id);
    } else {
        // Quick tunnel mode
        cmd.arg("tunnel")
            .arg("--url")
            .arg(format!("http://localhost:{local_port}"));
    }

    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    info!(bin = %bin, tunnel_id = %config.tunnel_id, "starting cloudflared");

    let mut child = match cmd.spawn() {
        Ok(c) => {
            info!(pid = ?c.id(), "cloudflared process spawned");
            c
        }
        Err(e) => {
            error!(err = %e, bin = %bin, "failed to spawn cloudflared — tunnel disabled");
            return None;
        }
    };

    // Parse stderr to discover the URL.
    let stderr = child.stderr.take().expect("stderr was piped");
    let hostname = config.hostname.clone();
    let has_tunnel_id = !config.tunnel_id.is_empty();

    tokio::spawn(async move {
        let reader = tokio::io::BufReader::new(stderr);
        let mut lines = reader.lines();

        while let Ok(Some(line)) = lines.next_line().await {
            if !has_tunnel_id {
                // Quick tunnel: look for the trycloudflare.com URL
                if let Some(url) = extract_quick_tunnel_url(&line) {
                    info!(public_url = %url, "cloudflare quick tunnel ready");
                    let _ = url_tx.send(Some(url));
                }
            } else if !hostname.is_empty() {
                // Named tunnel: once we see a connection registered, publish the hostname.
                if line.contains("Registered tunnel connection")
                    || line.contains("Connection registered")
                    || line.contains("connIndex=")
                {
                    let url = format!("https://{hostname}");
                    info!(public_url = %url, "cloudflare named tunnel ready");
                    let _ = url_tx.send(Some(url));
                }
            }
        }

        warn!("cloudflared stderr stream ended");
    });

    Some(child)
}

/// Extract the trycloudflare.com URL from a cloudflared stderr line.
fn extract_quick_tunnel_url(line: &str) -> Option<String> {
    if let Some(start) = line.find("https://") {
        let rest = &line[start..];
        let end = rest.find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
            .unwrap_or(rest.len());
        let url = &rest[..end];
        if url.contains("trycloudflare.com") {
            return Some(url.to_string());
        }
    }
    None
}
