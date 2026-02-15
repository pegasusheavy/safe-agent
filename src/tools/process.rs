use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;
use tracing::debug;

use super::{Tool, ToolContext, ToolOutput};
use crate::error::Result;

/// Tracks background processes spawned by the exec tool.
#[derive(Debug)]
struct ProcessEntry {
    pid: u32,
    command: String,
    started_at: chrono::DateTime<chrono::Utc>,
}

pub struct ProcessTool {
    processes: Arc<Mutex<HashMap<String, ProcessEntry>>>,
}

impl ProcessTool {
    pub fn new() -> Self {
        Self {
            processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl Tool for ProcessTool {
    fn name(&self) -> &str {
        "process"
    }

    fn description(&self) -> &str {
        "Manage background processes. Actions: list, kill."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "kill"],
                    "description": "Action to perform"
                },
                "pid": {
                    "type": "integer",
                    "description": "Process ID (required for kill)"
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        match action {
            "list" => {
                let procs = self.processes.lock().await;
                if procs.is_empty() {
                    return Ok(ToolOutput::ok("No background processes running."));
                }
                let mut out = String::from("Background processes:\n");
                for (id, entry) in procs.iter() {
                    out.push_str(&format!(
                        "  [{}] PID {} — {} (started {})\n",
                        id, entry.pid, entry.command, entry.started_at
                    ));
                }
                Ok(ToolOutput::ok(out))
            }
            "kill" => {
                let pid = params
                    .get("pid")
                    .and_then(|v| v.as_u64())
                    .map(|p| p as u32);

                match pid {
                    Some(pid) => {
                        debug!(pid, "killing process");
                        // Send SIGTERM via kill command
                        let _ = tokio::process::Command::new("kill")
                            .arg("-TERM")
                            .arg(pid.to_string())
                            .output()
                            .await;
                        let mut procs = self.processes.lock().await;
                        procs.retain(|_, e| e.pid != pid);
                        Ok(ToolOutput::ok(format!("Sent SIGTERM to PID {pid}")))
                    }
                    None => Ok(ToolOutput::error("pid is required for kill action")),
                }
            }
            other => Ok(ToolOutput::error(format!("unknown action: {other}"))),
        }
    }
}
