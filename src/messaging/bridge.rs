use async_trait::async_trait;
use tracing::debug;

use crate::error::{Result, SafeAgentError};

use super::MessagingBackend;

/// Generic HTTP bridge backend for messaging platforms that use an external
/// bridge process (e.g. iMessage AppleScript bridge, Android Termux bridge).
///
/// The bridge must implement:
/// - `POST /send` with JSON `{"to": "...", "text": "..."}` → `{"ok": true}`
/// - `GET /status` → `{"state": "connected"|"disconnected", ...}`
///
/// Incoming messages are handled by the bridge POSTing to safeclaw's
/// `/api/messaging/incoming` webhook — this struct only handles outgoing.
pub struct BridgeBackend {
    platform: String,
    bridge_url: String,
    max_length: usize,
    http: reqwest::Client,
}

impl BridgeBackend {
    pub fn new(platform: String, bridge_url: String, max_length: usize) -> Self {
        Self {
            platform,
            bridge_url,
            max_length,
            http: reqwest::Client::new(),
        }
    }

    /// Check bridge health.
    #[allow(dead_code)]
    pub async fn status(&self) -> Result<BridgeStatus> {
        let resp = self
            .http
            .get(format!("{}/status", self.bridge_url))
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| {
                SafeAgentError::Messaging(format!("{} bridge unreachable: {e}", self.platform))
            })?;

        let body: serde_json::Value = resp.json().await.map_err(|e| {
            SafeAgentError::Messaging(format!("{} bridge bad response: {e}", self.platform))
        })?;

        Ok(BridgeStatus {
            state: body
                .get("state")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            info: body
                .get("info")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        })
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct BridgeStatus {
    pub state: String,
    pub info: String,
}

#[async_trait]
impl MessagingBackend for BridgeBackend {
    fn platform_name(&self) -> &str {
        &self.platform
    }

    fn max_message_length(&self) -> usize {
        self.max_length
    }

    async fn send_message(&self, channel: &str, text: &str) -> Result<()> {
        debug!(platform = %self.platform, channel, "sending message via bridge");

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
            .map_err(|e| {
                SafeAgentError::Messaging(format!("{} bridge send failed: {e}", self.platform))
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(SafeAgentError::Messaging(format!(
                "{} bridge returned {status}: {body}",
                self.platform
            )));
        }

        Ok(())
    }

    async fn send_typing(&self, channel: &str) -> Result<()> {
        debug!(
            platform = %self.platform,
            channel,
            "typing indicator (no-op for bridge)"
        );
        Ok(())
    }
}
