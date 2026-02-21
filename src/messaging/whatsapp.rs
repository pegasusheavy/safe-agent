use std::path::PathBuf;

use async_trait::async_trait;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::config::WhatsAppConfig;
use crate::error::{Result, SafeAgentError};

use super::MessagingBackend;

// ---------------------------------------------------------------------------
// WhatsApp backend
// ---------------------------------------------------------------------------

pub struct WhatsAppBackend {
    config: WhatsAppConfig,
    http: reqwest::Client,
    bridge_url: String,
    bridge_process: Mutex<Option<Child>>,
}

impl WhatsAppBackend {
    /// Create a new WhatsApp backend. Call `start_bridge()` after construction
    /// to spawn the Baileys Node.js bridge subprocess.
    pub fn new(config: WhatsAppConfig) -> Self {
        let bridge_url = format!("http://127.0.0.1:{}", config.bridge_port);
        Self {
            config,
            http: reqwest::Client::new(),
            bridge_url,
            bridge_process: Mutex::new(None),
        }
    }

    /// Spawn the Baileys bridge Node.js process.
    pub async fn start_bridge(&self, data_dir: PathBuf) -> Result<()> {
        let bridge_dir = self.find_bridge_dir()?;
        let auth_dir = data_dir.join("whatsapp").join("auth");
        std::fs::create_dir_all(&auth_dir).map_err(|e| {
            SafeAgentError::Messaging(format!("failed to create whatsapp auth dir: {e}"))
        })?;

        info!(
            bridge_dir = %bridge_dir.display(),
            port = self.config.bridge_port,
            "starting whatsapp bridge"
        );

        let child = Command::new("node")
            .arg("index.js")
            .current_dir(&bridge_dir)
            .env("PORT", self.config.bridge_port.to_string())
            .env("AUTH_DIR", auth_dir.to_string_lossy().to_string())
            .env(
                "WEBHOOK_URL",
                format!(
                    "http://127.0.0.1:{}/api/messaging/incoming",
                    self.config.webhook_port
                ),
            )
            .env(
                "ALLOWED_NUMBERS",
                self.config.allowed_numbers.join(","),
            )
            .kill_on_drop(true)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| SafeAgentError::Messaging(format!("failed to spawn bridge: {e}")))?;

        *self.bridge_process.lock().await = Some(child);

        // Wait briefly for the bridge to initialize
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        info!("whatsapp bridge started");

        Ok(())
    }

    /// Locate the bridge directory. Checks common locations.
    fn find_bridge_dir(&self) -> Result<PathBuf> {
        let candidates = [
            PathBuf::from("src/messaging/whatsapp-bridge"),
            PathBuf::from("/app/whatsapp-bridge"),
        ];
        for p in &candidates {
            if p.join("index.js").exists() {
                return Ok(p.clone());
            }
        }
        Err(SafeAgentError::Messaging(
            "whatsapp-bridge/index.js not found".to_string(),
        ))
    }

}

#[async_trait]
impl MessagingBackend for WhatsAppBackend {
    fn platform_name(&self) -> &str {
        "whatsapp"
    }

    fn max_message_length(&self) -> usize {
        // WhatsApp messages can be up to ~65536 bytes, but practically
        // keep them shorter for readability.
        4096
    }

    async fn send_message(&self, channel: &str, text: &str) -> Result<()> {
        debug!(channel, "sending whatsapp message via bridge");

        let resp = self
            .http
            .post(format!("{}/send", self.bridge_url))
            .json(&serde_json::json!({
                "to": channel,
                "text": text,
            }))
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
            .map_err(|e| SafeAgentError::Messaging(format!("whatsapp send failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(SafeAgentError::Messaging(format!(
                "whatsapp bridge returned {status}: {body}"
            )));
        }

        Ok(())
    }

    async fn send_typing(&self, channel: &str) -> Result<()> {
        // The bridge doesn't support typing indicators yet — this is a no-op.
        debug!(channel, "whatsapp typing indicator (no-op)");
        Ok(())
    }
}

impl Drop for WhatsAppBackend {
    fn drop(&mut self) {
        // best-effort kill on drop (kill_on_drop handles this, but be explicit)
        warn!("whatsapp backend dropped");
    }
}
