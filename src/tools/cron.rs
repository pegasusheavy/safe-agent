use async_trait::async_trait;
use tracing::debug;
use uuid::Uuid;

use super::{Tool, ToolContext, ToolOutput};
use crate::error::Result;

/// Cron scheduling tool — manages scheduled tasks stored in SQLite.
pub struct CronTool;

impl CronTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for CronTool {
    fn name(&self) -> &str {
        "cron"
    }

    fn description(&self) -> &str {
        "Manage scheduled tasks. Actions: list, add, remove, enable, disable."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "add", "remove", "enable", "disable"],
                    "description": "Cron action to perform"
                },
                "job_id": {
                    "type": "string",
                    "description": "Job ID (for remove/enable/disable)"
                },
                "name": {
                    "type": "string",
                    "description": "Job name (for add)"
                },
                "schedule": {
                    "type": "string",
                    "description": "Cron expression (for add), e.g. '0 */5 * * * *'"
                },
                "tool": {
                    "type": "string",
                    "description": "Tool to invoke on schedule (for add)"
                },
                "tool_params": {
                    "type": "object",
                    "description": "Parameters for the scheduled tool call (for add)"
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let action = params.get("action").and_then(|v| v.as_str()).unwrap_or_default();

        match action {
            "list" => {
                let db = ctx.db.lock().await;
                let mut stmt = db.prepare(
                    "SELECT id, name, schedule, tool_call, enabled, last_run_at, created_at
                     FROM cron_jobs ORDER BY created_at DESC",
                )?;
                let jobs: Vec<String> = stmt
                    .query_map([], |row| {
                        let enabled: bool = row.get::<_, i32>(4)? != 0;
                        Ok(format!(
                            "[{}] {} — schedule={} enabled={} last_run={}",
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            enabled,
                            row.get::<_, Option<String>>(5)?.unwrap_or_else(|| "never".into()),
                        ))
                    })?
                    .collect::<std::result::Result<Vec<_>, _>>()?;

                if jobs.is_empty() {
                    Ok(ToolOutput::ok("No cron jobs configured."))
                } else {
                    Ok(ToolOutput::ok(jobs.join("\n")))
                }
            }
            "add" => {
                let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("unnamed");
                let schedule = params.get("schedule").and_then(|v| v.as_str()).unwrap_or_default();
                let tool = params.get("tool").and_then(|v| v.as_str()).unwrap_or_default();
                let tool_params = params.get("tool_params").cloned().unwrap_or(serde_json::Value::Object(Default::default()));

                if schedule.is_empty() || tool.is_empty() {
                    return Ok(ToolOutput::error("schedule and tool are required for add"));
                }

                let id = Uuid::new_v4().to_string();
                let tool_call = serde_json::json!({ "tool": tool, "params": tool_params });
                let tool_call_str = serde_json::to_string(&tool_call)?;

                debug!(id, name, schedule, tool, "adding cron job");

                let db = ctx.db.lock().await;
                db.execute(
                    "INSERT INTO cron_jobs (id, name, schedule, tool_call) VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![id, name, schedule, tool_call_str],
                )?;

                Ok(ToolOutput::ok_with_meta(
                    format!("Added cron job '{name}' ({schedule})"),
                    serde_json::json!({ "job_id": id }),
                ))
            }
            "remove" => {
                let job_id = params.get("job_id").and_then(|v| v.as_str()).unwrap_or_default();
                if job_id.is_empty() {
                    return Ok(ToolOutput::error("job_id is required for remove"));
                }
                let db = ctx.db.lock().await;
                let rows = db.execute("DELETE FROM cron_jobs WHERE id = ?1", [job_id])?;
                if rows > 0 {
                    Ok(ToolOutput::ok(format!("Removed cron job {job_id}")))
                } else {
                    Ok(ToolOutput::error(format!("Job {job_id} not found")))
                }
            }
            "enable" | "disable" => {
                let job_id = params.get("job_id").and_then(|v| v.as_str()).unwrap_or_default();
                if job_id.is_empty() {
                    return Ok(ToolOutput::error("job_id is required"));
                }
                let enabled = if action == "enable" { 1 } else { 0 };
                let db = ctx.db.lock().await;
                let rows = db.execute(
                    "UPDATE cron_jobs SET enabled = ?1 WHERE id = ?2",
                    rusqlite::params![enabled, job_id],
                )?;
                if rows > 0 {
                    Ok(ToolOutput::ok(format!("Job {job_id} {action}d")))
                } else {
                    Ok(ToolOutput::error(format!("Job {job_id} not found")))
                }
            }
            other => Ok(ToolOutput::error(format!("unknown action: {other}"))),
        }
    }
}
