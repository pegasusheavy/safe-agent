use tokio::process::Child;
use tokio::sync::watch;

use crate::config::CloudflareConfig;

pub async fn start(
    _config: &CloudflareConfig,
    _local_port: u16,
    _url_tx: watch::Sender<Option<String>>,
) -> Option<Child> {
    todo!("cloudflare provider not yet implemented")
}
