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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tool_call_full() {
        let json = serde_json::json!({
            "tool": "exec",
            "params": {"command": "ls -la"},
            "reasoning": "list files"
        });
        let call = parse_tool_call(&json).unwrap();
        assert_eq!(call.tool, "exec");
        assert_eq!(call.params["command"], "ls -la");
        assert_eq!(call.reasoning, "list files");
    }

    #[test]
    fn parse_tool_call_missing_fields() {
        let json = serde_json::json!({});
        let call = parse_tool_call(&json).unwrap();
        assert_eq!(call.tool, "");
        assert_eq!(call.params, serde_json::Value::default());
        assert_eq!(call.reasoning, "");
    }

    #[test]
    fn parse_tool_call_partial_fields() {
        let json = serde_json::json!({"tool": "read_file"});
        let call = parse_tool_call(&json).unwrap();
        assert_eq!(call.tool, "read_file");
        assert_eq!(call.reasoning, "");
    }

    #[test]
    fn parse_tool_call_non_string_tool() {
        let json = serde_json::json!({"tool": 42});
        let call = parse_tool_call(&json).unwrap();
        assert_eq!(call.tool, "");
    }

    #[test]
    fn parse_tool_call_params_preserved() {
        let json = serde_json::json!({
            "tool": "web_search",
            "params": {"query": "rust testing", "limit": 10},
            "reasoning": "search"
        });
        let call = parse_tool_call(&json).unwrap();
        assert_eq!(call.params["query"], "rust testing");
        assert_eq!(call.params["limit"], 10);
    }
}
