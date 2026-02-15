use async_trait::async_trait;
use tokio::process::Command;
use tracing::debug;

use super::{Tool, ToolContext, ToolOutput};
use crate::error::Result;

pub struct ExecTool {
    timeout_secs: u64,
}

impl ExecTool {
    pub fn new(timeout_secs: u64) -> Self {
        Self { timeout_secs }
    }
}

#[async_trait]
impl Tool for ExecTool {
    fn name(&self) -> &str {
        "exec"
    }

    fn description(&self) -> &str {
        "Execute a shell command and return its output. Commands run in a sandboxed environment and require operator approval."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                },
                "cwd": {
                    "type": "string",
                    "description": "Working directory (relative to sandbox root)"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Override timeout in seconds"
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let command = params
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        if command.is_empty() {
            return Ok(ToolOutput::error("command is required"));
        }

        let timeout = params
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(self.timeout_secs);

        let cwd = params
            .get("cwd")
            .and_then(|v| v.as_str())
            .map(std::path::Path::new);

        let work_dir = if let Some(rel) = cwd {
            ctx.sandbox.resolve(rel)?
        } else {
            ctx.sandbox.root().to_path_buf()
        };

        debug!(command, ?work_dir, timeout, "executing command");

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(timeout),
            Command::new("sh")
                .arg("-c")
                .arg(command)
                .current_dir(&work_dir)
                .output(),
        )
        .await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let code = output.status.code().unwrap_or(-1);

                let mut text = String::new();
                if !stdout.is_empty() {
                    text.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str("[stderr] ");
                    text.push_str(&stderr);
                }

                let meta = serde_json::json!({ "exit_code": code });

                if output.status.success() {
                    Ok(ToolOutput::ok_with_meta(text, meta))
                } else {
                    Ok(ToolOutput {
                        success: false,
                        output: format!("exit code {code}\n{text}"),
                        metadata: Some(meta),
                    })
                }
            }
            Ok(Err(e)) => Ok(ToolOutput::error(format!("failed to execute: {e}"))),
            Err(_) => Ok(ToolOutput::error(format!(
                "command timed out after {timeout}s"
            ))),
        }
    }
}
