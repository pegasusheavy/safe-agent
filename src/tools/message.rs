use async_trait::async_trait;
use teloxide::prelude::*;
use tracing::{debug, info};

use super::{Tool, ToolContext, ToolOutput};
use crate::error::Result;

/// Messaging tool — currently supports Telegram.
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
        "Send a message via Telegram. Params: {\"text\": \"your message\"}"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["text"],
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Message text to send"
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

        let bot = match &ctx.telegram_bot {
            Some(b) => b,
            None => return Ok(ToolOutput::error("Telegram bot not available")),
        };

        let chat_id = match ctx.telegram_chat_id {
            Some(id) => ChatId(id),
            None => return Ok(ToolOutput::error("No Telegram chat ID configured")),
        };

        debug!(chat_id = %chat_id, "sending telegram message");

        match bot.send_message(chat_id, text).await {
            Ok(_) => {
                info!("telegram message sent successfully");
                Ok(ToolOutput::ok("Message sent"))
            }
            Err(e) => Ok(ToolOutput::error(format!("Failed to send: {e}"))),
        }
    }
}
