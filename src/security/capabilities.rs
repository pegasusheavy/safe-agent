use std::collections::{HashMap, HashSet};

use tracing::warn;

use crate::config::SecurityConfig;
use crate::error::{Result, SafeAgentError};

/// Capability-based permission checker for tool execution.
///
/// Instead of blanket tool approval, this checks fine-grained capabilities
/// like "can read calendar but not write" or "can search web but not exec shell."
pub struct CapabilityChecker {
    /// Tools that are completely blocked.
    blocked_tools: HashSet<String>,
    /// Per-tool capability restrictions. If a tool is listed here, only the
    /// specified operations are allowed.
    tool_capabilities: HashMap<String, HashSet<String>>,
}

/// Result of a capability check.
#[derive(Debug, Clone, PartialEq)]
pub enum CapabilityVerdict {
    /// The tool call is allowed.
    Allowed,
    /// The tool is completely blocked.
    Blocked(String),
    /// The specific operation/capability is not permitted for this tool.
    CapabilityDenied {
        tool: String,
        operation: String,
        allowed: Vec<String>,
    },
}

impl CapabilityChecker {
    pub fn new(config: &SecurityConfig) -> Self {
        let blocked_tools: HashSet<String> = config.blocked_tools.iter().cloned().collect();
        let tool_capabilities: HashMap<String, HashSet<String>> = config
            .tool_capabilities
            .iter()
            .map(|(k, v)| (k.clone(), v.iter().cloned().collect()))
            .collect();

        Self {
            blocked_tools,
            tool_capabilities,
        }
    }

    /// Check whether a tool call is permitted.
    ///
    /// `tool_name` is the tool being invoked.
    /// `params` is the full parameter JSON — used to infer the operation
    /// for tools that have capability restrictions.
    pub fn check(&self, tool_name: &str, params: &serde_json::Value) -> CapabilityVerdict {
        // Check if tool is entirely blocked
        if self.blocked_tools.contains(tool_name) {
            warn!(tool = %tool_name, "blocked tool invocation");
            return CapabilityVerdict::Blocked(format!("tool '{tool_name}' is blocked by security policy"));
        }

        // Check fine-grained capabilities
        if let Some(allowed_caps) = self.tool_capabilities.get(tool_name) {
            let operation = infer_operation(tool_name, params);
            if !operation.is_empty() && !allowed_caps.contains(&operation) {
                warn!(
                    tool = %tool_name,
                    operation = %operation,
                    allowed = ?allowed_caps,
                    "capability denied"
                );
                return CapabilityVerdict::CapabilityDenied {
                    tool: tool_name.to_string(),
                    operation,
                    allowed: allowed_caps.iter().cloned().collect(),
                };
            }
        }

        CapabilityVerdict::Allowed
    }

    /// Convert a negative verdict to an error result.
    pub fn check_or_error(&self, tool_name: &str, params: &serde_json::Value) -> Result<()> {
        match self.check(tool_name, params) {
            CapabilityVerdict::Allowed => Ok(()),
            CapabilityVerdict::Blocked(msg) => {
                Err(SafeAgentError::PermissionDenied(msg))
            }
            CapabilityVerdict::CapabilityDenied {
                tool,
                operation,
                allowed,
            } => Err(SafeAgentError::PermissionDenied(format!(
                "tool '{tool}' operation '{operation}' not allowed (permitted: {})",
                allowed.join(", ")
            ))),
        }
    }

    /// Check if a tool is blocked entirely.
    pub fn is_blocked(&self, tool_name: &str) -> bool {
        self.blocked_tools.contains(tool_name)
    }
}

/// Infer the operation/capability from tool parameters.
///
/// This maps common tool parameter patterns to capability names:
/// - exec tool: the command name (first word of `command` param)
/// - file tools: "read" or "write" based on tool name
/// - web tools: "search" or "fetch"
fn infer_operation(tool_name: &str, params: &serde_json::Value) -> String {
    match tool_name {
        "exec" => {
            // Extract the command name from the `command` parameter
            let cmd = params
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            cmd.split_whitespace()
                .next()
                .unwrap_or("")
                .to_string()
        }
        "read_file" => "read".to_string(),
        "write_file" | "edit_file" | "apply_patch" => "write".to_string(),
        "delete_file" => "delete".to_string(),
        "web_search" => "search".to_string(),
        "web_fetch" => "fetch".to_string(),
        "cron" => {
            params
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        }
        "goal" => {
            params
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        }
        "message" => {
            params
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("send")
                .to_string()
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(
        blocked: Vec<&str>,
        caps: Vec<(&str, Vec<&str>)>,
    ) -> SecurityConfig {
        SecurityConfig {
            blocked_tools: blocked.into_iter().map(|s| s.to_string()).collect(),
            tool_capabilities: caps
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.into_iter().map(|s| s.to_string()).collect()))
                .collect(),
            ..SecurityConfig::default()
        }
    }

    #[test]
    fn test_allowed_when_no_restrictions() {
        let config = make_config(vec![], vec![]);
        let checker = CapabilityChecker::new(&config);
        assert_eq!(
            checker.check("exec", &serde_json::json!({"command": "ls"})),
            CapabilityVerdict::Allowed
        );
    }

    #[test]
    fn test_blocked_tool() {
        let config = make_config(vec!["exec"], vec![]);
        let checker = CapabilityChecker::new(&config);
        match checker.check("exec", &serde_json::json!({})) {
            CapabilityVerdict::Blocked(msg) => assert!(msg.contains("exec")),
            other => panic!("expected Blocked, got {:?}", other),
        }
    }

    #[test]
    fn test_capability_allowed() {
        let config = make_config(vec![], vec![("exec", vec!["ls", "cat", "echo"])]);
        let checker = CapabilityChecker::new(&config);
        assert_eq!(
            checker.check("exec", &serde_json::json!({"command": "ls -la"})),
            CapabilityVerdict::Allowed
        );
    }

    #[test]
    fn test_capability_denied() {
        let config = make_config(vec![], vec![("exec", vec!["ls", "cat"])]);
        let checker = CapabilityChecker::new(&config);
        match checker.check("exec", &serde_json::json!({"command": "rm -rf /"})) {
            CapabilityVerdict::CapabilityDenied { tool, operation, .. } => {
                assert_eq!(tool, "exec");
                assert_eq!(operation, "rm");
            }
            other => panic!("expected CapabilityDenied, got {:?}", other),
        }
    }

    #[test]
    fn test_file_tool_capabilities() {
        let config = make_config(vec![], vec![("read_file", vec!["read"]), ("write_file", vec![])]);
        let checker = CapabilityChecker::new(&config);
        assert_eq!(
            checker.check("read_file", &serde_json::json!({"path": "test.txt"})),
            CapabilityVerdict::Allowed
        );
        // write_file has empty capabilities -> "write" not in set
        match checker.check("write_file", &serde_json::json!({"path": "test.txt"})) {
            CapabilityVerdict::CapabilityDenied { operation, .. } => {
                assert_eq!(operation, "write");
            }
            other => panic!("expected CapabilityDenied, got {:?}", other),
        }
    }

    #[test]
    fn test_is_blocked() {
        let config = make_config(vec!["dangerous_tool"], vec![]);
        let checker = CapabilityChecker::new(&config);
        assert!(checker.is_blocked("dangerous_tool"));
        assert!(!checker.is_blocked("safe_tool"));
    }

    #[test]
    fn test_check_or_error() {
        let config = make_config(vec!["blocked"], vec![]);
        let checker = CapabilityChecker::new(&config);
        assert!(checker.check_or_error("allowed", &serde_json::json!({})).is_ok());
        assert!(checker.check_or_error("blocked", &serde_json::json!({})).is_err());
    }
}
