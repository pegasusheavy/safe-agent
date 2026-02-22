use async_trait::async_trait;
use tracing::debug;

use crate::error::{Result, SafeAgentError};

use super::{split_message, MessagingBackend};

/// Signal messaging backend using the bridge pattern.
///
/// Delegates to an external signal-cli-rest-api (or compatible) HTTP bridge
/// running as a separate process. The bridge handles Signal protocol
/// registration, key management, and message delivery; this struct only
/// handles outgoing messages via the bridge's REST API.
///
/// Incoming messages are handled by the bridge POSTing to safe-agent's
/// `/api/messaging/incoming` webhook with `platform: "signal"`.
pub struct SignalBackend {
    bridge_url: String,
    http: reqwest::Client,
}

impl SignalBackend {
    pub fn new(bridge_url: String) -> Self {
        Self {
            bridge_url,
            http: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl MessagingBackend for SignalBackend {
    fn platform_name(&self) -> &str {
        "signal"
    }

    fn max_message_length(&self) -> usize {
        2000
    }

    async fn send_message(&self, channel: &str, text: &str) -> Result<()> {
        debug!(channel, "sending signal message via bridge");

        for chunk in split_message(text, self.max_message_length()) {
            let resp = self
                .http
                .post(format!("{}/send", self.bridge_url))
                .json(&serde_json::json!({
                    "to": channel,
                    "text": chunk,
                }))
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await
                .map_err(|e| {
                    SafeAgentError::Messaging(format!("signal bridge send failed: {e}"))
                })?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(SafeAgentError::Messaging(format!(
                    "signal bridge returned {status}: {body}"
                )));
            }
        }
        Ok(())
    }

    async fn send_typing(&self, _channel: &str) -> Result<()> {
        // Signal bridges generally don't support typing indicators
        Ok(())
    }
}
