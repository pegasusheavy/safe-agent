use async_trait::async_trait;
use tracing::{debug, info};

use super::{Tool, ToolContext, ToolOutput};
use crate::error::Result;

/// Messaging tool — sends messages via the primary messaging backend
/// (Telegram, WhatsApp, or whatever is configured first).
pub struct MessageTool;

impl MessageTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for MessageTool {
    fn name(&self) -> &str {
        "message"
    }

    fn description(&self) -> &str {
        "Send a message to the operator via the primary messaging platform. Params: {\"text\": \"your message\", \"platform\": \"telegram|whatsapp (optional)\"}"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["text"],
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Message text to send"
                },
                "platform": {
                    "type": "string",
                    "description": "Target platform (optional, defaults to primary)"
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let text = params
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        if text.is_empty() {
            return Ok(ToolOutput::error("Missing 'text' parameter"));
        }

        let platform = params.get("platform").and_then(|v| v.as_str());

        if ctx.messaging.is_empty() {
            return Ok(ToolOutput::error("No messaging backends configured"));
        }

        match platform {
            Some(p) => {
                // Send to a specific platform
                let backend = match ctx.messaging.get(p) {
                    Some(b) => b,
                    None => {
                        return Ok(ToolOutput::error(format!(
                            "Unknown platform '{p}'. Available: {}",
                            ctx.messaging.platforms().join(", ")
                        )));
                    }
                };
                let channel = match ctx.messaging.primary_channel(p) {
                    Some(c) => c.to_string(),
                    None => {
                        return Ok(ToolOutput::error(format!(
                            "No primary channel for platform '{p}'"
                        )));
                    }
                };
                debug!(platform = p, channel = %channel, "sending message");
                backend.send_message(&channel, text).await?;
                info!(platform = p, "message sent successfully");
                Ok(ToolOutput::ok(format!("Message sent via {p}")))
            }
            None => {
                // Send via the primary (first) backend
                let (backend, channel) = match ctx.messaging.default_channel() {
                    Some(pair) => pair,
                    None => return Ok(ToolOutput::error("No messaging backends configured")),
                };
                let platform_name = backend.platform_name().to_string();
                let channel_str = channel.to_string();
                debug!(platform = %platform_name, channel = %channel_str, "sending message");
                backend.send_message(&channel_str, text).await?;
                info!(platform = %platform_name, "message sent successfully");
                Ok(ToolOutput::ok(format!("Message sent via {platform_name}")))
            }
        }
    }
}
