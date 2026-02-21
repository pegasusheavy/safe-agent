use async_trait::async_trait;
use tracing::debug;

use super::{Tool, ToolContext, ToolOutput};
use crate::error::Result;

/// Headless browser automation tool via Chrome DevTools Protocol.
///
/// Currently a scaffold — full CDP integration (chromiumoxide) will be
/// wired up once the core agent loop is proven out. For now it supports
/// basic navigation and screenshot via the `exec` fallback to a headless
/// Chrome process.
pub struct BrowserTool {
    headless: bool,
}

impl BrowserTool {
    pub fn new(headless: bool) -> Self {
        Self { headless }
    }
}

#[async_trait]
impl Tool for BrowserTool {
    fn name(&self) -> &str {
        "browser"
    }

    fn description(&self) -> &str {
        "Control a headless browser. Actions: navigate, screenshot, snapshot, evaluate. Requires Chrome/Chromium installed."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["navigate", "screenshot", "snapshot", "evaluate"],
                    "description": "Browser action to perform"
                },
                "url": {
                    "type": "string",
                    "description": "URL to navigate to (required for navigate)"
                },
                "script": {
                    "type": "string",
                    "description": "JavaScript to evaluate (required for evaluate)"
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        debug!(action, headless = self.headless, "browser action");

        match action {
            "navigate" => {
                let url = params
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                if url.is_empty() {
                    return Ok(ToolOutput::error("url is required for navigate"));
                }
                // TODO: Full CDP integration
                Ok(ToolOutput::ok(format!(
                    "Browser navigation to {url} — CDP integration pending"
                )))
            }
            "screenshot" => {
                Ok(ToolOutput::ok(
                    "Screenshot capture — CDP integration pending".to_string(),
                ))
            }
            "snapshot" => {
                Ok(ToolOutput::ok(
                    "Accessibility snapshot — CDP integration pending".to_string(),
                ))
            }
            "evaluate" => {
                let script = params
                    .get("script")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                if script.is_empty() {
                    return Ok(ToolOutput::error("script is required for evaluate"));
                }
                Ok(ToolOutput::ok(
                    "JavaScript evaluation — CDP integration pending".to_string(),
                ))
            }
            other => Ok(ToolOutput::error(format!("unknown browser action: {other}"))),
        }
    }
}
