use std::process::Stdio;

use tokio::process::{Child, Command};
use tokio::sync::watch;
use tracing::{error, info};

use crate::config::TailscaleConfig;

/// JSON shape from `tailscale status --json`.
#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct TailscaleStatus {
    #[serde(rename = "Self")]
    self_node: TailscaleNode,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct TailscaleNode {
    #[serde(rename = "DNSName")]
    dns_name: String,
}

/// Start Tailscale Funnel or Serve and discover the public URL.
///
/// Supports three modes:
/// - **Static URL**: `config.url` or `TAILSCALE_TUNNEL_URL` is set — no
///   subprocess, just publish the URL directly.
/// - **Funnel** (`mode = "funnel"`): public HTTPS via Tailscale Funnel.
///   Runs `tailscale funnel <port>`.
/// - **Serve** (`mode = "serve"`): tailnet-only HTTPS.
///   Runs `tailscale serve <port>`.
///
/// The public hostname is either taken from `config.hostname` or
/// discovered via `tailscale status --json` → `Self.DNSName`.
pub async fn start(
    config: &TailscaleConfig,
    local_port: u16,
    url_tx: watch::Sender<Option<String>>,
) -> Option<Child> {
    // Static URL override — no subprocess needed.
    let static_url = std::env::var("TAILSCALE_TUNNEL_URL")
        .ok()
        .filter(|u| !u.is_empty())
        .or_else(|| {
            if config.url.is_empty() { None } else { Some(config.url.clone()) }
        });

    if let Some(url) = static_url {
        info!(url = %url, "using static Tailscale tunnel URL");
        let _ = url_tx.send(Some(url));
        return None;
    }

    let bin = std::env::var("TAILSCALE_BIN")
        .unwrap_or_else(|_| config.bin.clone());

    // Discover hostname: config override or `tailscale status --json`
    let hostname = if !config.hostname.is_empty() {
        config.hostname.clone()
    } else {
        match discover_hostname(&bin).await {
            Some(h) => h,
            None => {
                error!("could not determine Tailscale hostname — tunnel disabled");
                return None;
            }
        }
    };

    // Determine subcommand based on mode
    let subcommand = match config.mode.as_str() {
        "serve" => "serve",
        _ => "funnel",
    };

    let mut cmd = Command::new(&bin);
    cmd.arg(subcommand)
        .arg(local_port.to_string());

    cmd.stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    info!(
        bin = %bin,
        mode = subcommand,
        hostname = %hostname,
        port = local_port,
        "starting tailscale tunnel"
    );

    let child = match cmd.spawn() {
        Ok(c) => {
            info!(pid = ?c.id(), "tailscale {subcommand} process spawned");
            c
        }
        Err(e) => {
            error!(err = %e, bin = %bin, "failed to spawn tailscale — tunnel disabled");
            return None;
        }
    };

    // Tailscale funnel/serve is ready quickly — the hostname is known.
    let hostname = hostname.trim_end_matches('.');
    let url = format!("https://{hostname}");
    info!(public_url = %url, "tailscale {subcommand} ready");
    let _ = url_tx.send(Some(url));

    Some(child)
}

/// Query `tailscale status --json` to discover this node's MagicDNS name.
async fn discover_hostname(bin: &str) -> Option<String> {
    let output = Command::new(bin)
        .arg("status")
        .arg("--json")
        .output()
        .await
        .map_err(|e| {
            error!(err = %e, bin, "failed to run 'tailscale status --json'");
            e
        })
        .ok()?;

    if !output.status.success() {
        error!(
            status = %output.status,
            "tailscale status exited with error"
        );
        return None;
    }

    let status: TailscaleStatus = serde_json::from_slice(&output.stdout)
        .map_err(|e| {
            error!(err = %e, "failed to parse tailscale status JSON");
            e
        })
        .ok()?;

    let dns_name = status.self_node.dns_name;
    if dns_name.is_empty() {
        error!("tailscale status returned empty DNSName");
        return None;
    }

    info!(dns_name = %dns_name, "discovered tailscale hostname");
    Some(dns_name)
}
