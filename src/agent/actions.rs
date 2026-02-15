use tracing::debug;

use crate::error::Result;
use crate::tools::{ToolCall, ToolOutput, ToolRegistry, ToolContext};

/// Parse a ToolCall from the approval queue's stored JSON.
pub fn parse_tool_call(value: &serde_json::Value) -> Result<ToolCall> {
    let tool = value
        .get("tool")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let params = value.get("params").cloned().unwrap_or_default();
    let reasoning = value
        .get("reasoning")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    Ok(ToolCall {
        tool,
        params,
        reasoning,
    })
}

/// Execute a tool call through the registry.
pub async fn execute_tool_call(
    registry: &ToolRegistry,
    ctx: &ToolContext,
    call: &ToolCall,
) -> Result<ToolOutput> {
    debug!(tool = %call.tool, "executing tool call");
    registry.execute(&call.tool, call.params.clone(), ctx).await
}
