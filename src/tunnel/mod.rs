use std::sync::Arc;

use tokio::process::Child;
use tokio::sync::watch;
use tracing::{error, info};

use crate::config::TunnelConfig;

mod cloudflare;
mod ngrok;
mod tailscale;

pub struct TunnelManager {
    child: Option<Child>,
    _url_tx: watch::Sender<Option<String>>,
    url_rx: watch::Receiver<Option<String>>,
}

impl TunnelManager {
    pub async fn start(config: &TunnelConfig, local_port: u16) -> Self {
        let (url_tx, url_rx) = watch::channel(None);

        let provider = config.provider.as_str();
        info!(provider, "starting tunnel");

        let child = match provider {
            "ngrok" => ngrok::start(&config.ngrok, local_port, url_tx.clone()).await,
            "cloudflare" => cloudflare::start(&config.cloudflare, local_port, url_tx.clone()).await,
            "tailscale" => tailscale::start(&config.tailscale, local_port, url_tx.clone()).await,
            other => {
                error!(provider = other, "unknown tunnel provider — tunnel disabled");
                None
            }
        };

        Self {
            child,
            _url_tx: url_tx,
            url_rx,
        }
    }

    pub fn url_receiver(&self) -> watch::Receiver<Option<String>> {
        self.url_rx.clone()
    }
}

impl Drop for TunnelManager {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.child {
            let _ = child.start_kill();
        }
    }
}

pub type TunnelUrl = Arc<watch::Receiver<Option<String>>>;

pub fn shared_url(mgr: &TunnelManager) -> TunnelUrl {
    Arc::new(mgr.url_receiver())
}
