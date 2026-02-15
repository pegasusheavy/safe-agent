use async_trait::async_trait;
use tracing::debug;

use super::{Tool, ToolContext, ToolOutput};
use crate::error::Result;

/// Messaging platform tool for sending messages across Discord, Telegram,
/// Slack, and other platforms.
///
/// Scaffold — individual platform adapters will be added as the system matures.
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
        "Send messages across messaging platforms (Discord, Telegram, Slack). Actions: send, search, react, reply."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["action", "platform"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["send", "search", "react", "reply"],
                    "description": "Messaging action to perform"
                },
                "platform": {
                    "type": "string",
                    "enum": ["telegram", "discord", "slack"],
                    "description": "Target messaging platform"
                },
                "channel": {
                    "type": "string",
                    "description": "Channel or chat ID"
                },
                "text": {
                    "type": "string",
                    "description": "Message text"
                },
                "reply_to": {
                    "type": "string",
                    "description": "Message ID to reply to"
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let action = params.get("action").and_then(|v| v.as_str()).unwrap_or_default();
        let platform = params.get("platform").and_then(|v| v.as_str()).unwrap_or_default();

        debug!(action, platform, "message tool");

        // TODO: Wire up platform adapters
        Ok(ToolOutput::ok(format!(
            "Message {action} on {platform} — platform adapter pending"
        )))
    }
}
