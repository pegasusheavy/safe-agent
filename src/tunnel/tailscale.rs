use tokio::process::Child;
use tokio::sync::watch;

use crate::config::TailscaleConfig;

pub async fn start(
    _config: &TailscaleConfig,
    _local_port: u16,
    _url_tx: watch::Sender<Option<String>>,
) -> Option<Child> {
    todo!("tailscale provider not yet implemented")
}
