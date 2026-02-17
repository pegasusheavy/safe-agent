use tracing::warn;

use crate::tools::ToolCall;

/// The result of parsing an LLM response that may contain tool_call blocks.
#[derive(Debug, Clone)]
pub struct ParsedResponse {
    /// Natural-language text fragments (reasoning, commentary) with tool calls
    /// stripped out.
    pub text: String,
    /// Tool calls found in the response, in order of appearance.
    pub tool_calls: Vec<ToolCall>,
}

/// Parse `tool_call` fenced blocks from LLM output.
///
/// The LLM is instructed to wrap tool calls like this:
///
/// ```text
/// Some reasoning text...
///
/// ```tool_call
/// {"tool": "exec", "params": {"command": "ls"}, "reasoning": "list files"}
/// ```
///
/// More text...
/// ```
///
/// This function extracts every such block, parses the JSON into `ToolCall`
/// structs, and collects all remaining text into `ParsedResponse::text`.
pub fn parse_llm_response(response: &str) -> ParsedResponse {
    let mut text_parts: Vec<&str> = Vec::new();
    let mut tool_calls: Vec<ToolCall> = Vec::new();

    let mut remaining = response;

    loop {
        // Find the next ```tool_call block
        let Some(start_marker_pos) = find_tool_call_start(remaining) else {
            // No more tool_call blocks — the rest is plain text
            text_parts.push(remaining);
            break;
        };

        // Everything before the marker is plain text
        let before = &remaining[..start_marker_pos];
        if !before.trim().is_empty() {
            text_parts.push(before);
        }

        // Skip past the opening marker line
        let after_marker = &remaining[start_marker_pos..];
        let newline_pos = after_marker.find('\n').unwrap_or(after_marker.len());
        let body_start = start_marker_pos + newline_pos + 1;

        if body_start >= remaining.len() {
            // Malformed: opening marker at end of string with no body
            text_parts.push(remaining);
            break;
        }

        let body_region = &remaining[body_start..];

        // Find the closing ```
        let Some(close_pos) = find_closing_fence(body_region) else {
            // No closing fence — treat the rest as text
            warn!("tool_call block missing closing fence");
            text_parts.push(remaining);
            break;
        };

        let json_body = &body_region[..close_pos].trim();

        // Parse the JSON into a ToolCall
        match parse_tool_call_json(json_body) {
            Some(call) => tool_calls.push(call),
            None => {
                warn!(json = %json_body, "failed to parse tool_call JSON");
            }
        }

        // Advance past the closing ``` line
        let after_close = &body_region[close_pos..];
        let line_end = after_close.find('\n').map(|p| p + 1).unwrap_or(after_close.len());
        remaining = &body_region[close_pos + line_end..];
    }

    let text = text_parts
        .join("\n")
        .lines()
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();

    ParsedResponse { text, tool_calls }
}

/// Find the byte offset of the start of a ```tool_call line.
/// Matches lines that start with ``` followed by "tool_call" (with optional whitespace).
fn find_tool_call_start(s: &str) -> Option<usize> {
    let mut search_from = 0;
    while search_from < s.len() {
        let haystack = &s[search_from..];
        let Some(backtick_pos) = haystack.find("```") else {
            return None;
        };
        let abs_pos = search_from + backtick_pos;

        // Make sure backticks are at start of line (or start of string)
        let at_line_start = abs_pos == 0 || s.as_bytes()[abs_pos - 1] == b'\n';
        if !at_line_start {
            search_from = abs_pos + 3;
            continue;
        }

        // Check that after ``` we have "tool_call" (with optional whitespace)
        let after_backticks = &s[abs_pos + 3..];
        let trimmed = after_backticks.split('\n').next().unwrap_or("").trim();
        if trimmed == "tool_call" {
            return Some(abs_pos);
        }

        search_from = abs_pos + 3;
    }
    None
}

/// Find the closing ``` fence (at start of line) in a body region.
fn find_closing_fence(body: &str) -> Option<usize> {
    let mut search_from = 0;
    while search_from < body.len() {
        let haystack = &body[search_from..];
        let Some(pos) = haystack.find("```") else {
            return None;
        };
        let abs_pos = search_from + pos;

        // Check it's at start of line
        let at_line_start = abs_pos == 0 || body.as_bytes()[abs_pos - 1] == b'\n';
        if at_line_start {
            return Some(abs_pos);
        }

        search_from = abs_pos + 3;
    }
    None
}

/// Parse a JSON string into a ToolCall.
fn parse_tool_call_json(json: &str) -> Option<ToolCall> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let obj = value.as_object()?;

    let tool = obj.get("tool")?.as_str()?.to_string();
    if tool.is_empty() {
        return None;
    }

    let params = obj.get("params").cloned().unwrap_or(serde_json::Value::Object(Default::default()));
    let reasoning = obj
        .get("reasoning")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Some(ToolCall {
        tool,
        params,
        reasoning,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_tool_calls() {
        let response = "Just a normal text response with no tool calls.";
        let parsed = parse_llm_response(response);
        assert!(parsed.tool_calls.is_empty());
        assert_eq!(parsed.text, response);
    }

    #[test]
    fn test_single_tool_call() {
        let response = r#"Let me check that for you.

```tool_call
{"tool": "exec", "params": {"command": "ls -la"}, "reasoning": "list files"}
```

I'll have the results shortly."#;

        let parsed = parse_llm_response(response);
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].tool, "exec");
        assert!(parsed.text.contains("Let me check that for you."));
        assert!(parsed.text.contains("I'll have the results shortly."));
        assert!(!parsed.text.contains("tool_call"));
    }

    #[test]
    fn test_multiple_tool_calls() {
        let response = r#"I need to do two things.

```tool_call
{"tool": "read_file", "params": {"path": "config.toml"}, "reasoning": "read config"}
```

And also:

```tool_call
{"tool": "exec", "params": {"command": "date"}, "reasoning": "check time"}
```

Done."#;

        let parsed = parse_llm_response(response);
        assert_eq!(parsed.tool_calls.len(), 2);
        assert_eq!(parsed.tool_calls[0].tool, "read_file");
        assert_eq!(parsed.tool_calls[1].tool, "exec");
    }

    #[test]
    fn test_tool_call_only() {
        let response = r#"```tool_call
{"tool": "exec", "params": {"command": "whoami"}, "reasoning": "check user"}
```"#;

        let parsed = parse_llm_response(response);
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].tool, "exec");
    }

    #[test]
    fn test_malformed_json_skipped() {
        let response = r#"```tool_call
{not valid json}
```

Some text after."#;

        let parsed = parse_llm_response(response);
        assert!(parsed.tool_calls.is_empty());
        assert!(parsed.text.contains("Some text after."));
    }

    #[test]
    fn test_missing_tool_field_skipped() {
        let response = r#"```tool_call
{"params": {"command": "ls"}, "reasoning": "no tool field"}
```"#;

        let parsed = parse_llm_response(response);
        assert!(parsed.tool_calls.is_empty());
    }
}
