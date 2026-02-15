use async_trait::async_trait;

use super::{Tool, ToolContext, ToolOutput};
use crate::error::Result;

/// Image analysis tool — uses the LLM engine to describe/analyze images.
///
/// Scaffold — requires a vision-capable model or a separate image model.
pub struct ImageTool;

impl ImageTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for ImageTool {
    fn name(&self) -> &str {
        "image"
    }

    fn description(&self) -> &str {
        "Analyze an image and return a description. Provide either a file path (relative to sandbox) or a URL."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["image"],
            "properties": {
                "image": {
                    "type": "string",
                    "description": "Path to image file (sandbox-relative) or URL"
                },
                "prompt": {
                    "type": "string",
                    "description": "What to analyze (default: 'Describe the image.')"
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let image = params.get("image").and_then(|v| v.as_str()).unwrap_or_default();
        let _prompt = params
            .get("prompt")
            .and_then(|v| v.as_str())
            .unwrap_or("Describe the image.");

        if image.is_empty() {
            return Ok(ToolOutput::error("image path or URL is required"));
        }

        // TODO: Integrate with a vision-capable model
        Ok(ToolOutput::ok(
            "Image analysis requires a vision-capable model — integration pending".to_string(),
        ))
    }
}
