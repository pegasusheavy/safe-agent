//! Cron job runner — checks cron_jobs table each tick and executes due jobs.
//!
//! Uses the `cron` crate to parse cron expressions and determine if a job
//! is due based on its `last_run_at` vs. the current time.

use std::str::FromStr;

use chrono::{DateTime, Utc};
use tracing::{debug, error, info, warn};

use crate::error::Result;
use crate::tools::ToolCall;

use super::Agent;

/// A cron job loaded from the DB.
struct CronJob {
    id: String,
    name: String,
    schedule: String,
    tool_call_json: String,
    last_run_at: Option<String>,
}

impl Agent {
    /// Check all enabled cron jobs and execute those that are due.
    pub async fn run_due_cron_jobs(&self) -> Result<()> {
        let jobs = self.load_enabled_cron_jobs().await?;
        if jobs.is_empty() {
            return Ok(());
        }

        let now = Utc::now();

        for job in &jobs {
            if self.cron_is_due(job, now) {
                info!(job_id = %job.id, name = %job.name, "executing due cron job");

                let call: std::result::Result<serde_json::Value, _> =
                    serde_json::from_str(&job.tool_call_json);

                match call {
                    Ok(value) => {
                        let tool = value
                            .get("tool")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string();
                        let params = value
                            .get("params")
                            .cloned()
                            .unwrap_or(serde_json::Value::Object(Default::default()));
                        let reasoning = format!("Scheduled cron job: {}", job.name);

                        let tc = ToolCall {
                            tool: tool.clone(),
                            params,
                            reasoning,
                        };

                        let result =
                            super::actions::execute_tool_call(&self.tools, &self.ctx, &tc).await;

                        match result {
                            Ok(output) => {
                                let status = if output.success { "success" } else { "error" };
                                info!(
                                    job_id = %job.id,
                                    tool = %tool,
                                    status,
                                    "cron job executed"
                                );

                                self.emit_event(serde_json::json!({
                                    "type": "tool_result",
                                    "tool": tool,
                                    "success": output.success,
                                    "output_preview": super::truncate_preview(&output.output, 200),
                                    "context": "cron",
                                    "cron_job": job.name,
                                }));

                                // Send proactive notification for non-trivial results
                                if !output.output.is_empty() && output.output.len() > 5 {
                                    let msg = format!(
                                        "[Cron: {}] {}: {}",
                                        job.name,
                                        status,
                                        super::truncate_preview(&output.output, 500),
                                    );
                                    self.ctx.messaging.send_all(&msg).await;
                                }
                            }
                            Err(e) => {
                                error!(
                                    job_id = %job.id,
                                    tool = %tool,
                                    err = %e,
                                    "cron job failed"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            job_id = %job.id,
                            err = %e,
                            "invalid tool_call JSON in cron job"
                        );
                    }
                }

                self.update_cron_last_run(&job.id, now).await.ok();
            }
        }

        Ok(())
    }

    async fn load_enabled_cron_jobs(&self) -> Result<Vec<CronJob>> {
        let db = self.ctx.db.lock().await;
        let mut stmt = db.prepare(
            "SELECT id, name, schedule, tool_call, last_run_at
             FROM cron_jobs WHERE enabled = 1",
        )?;

        let jobs = stmt
            .query_map([], |row| {
                Ok(CronJob {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    schedule: row.get(2)?,
                    tool_call_json: row.get(3)?,
                    last_run_at: row.get(4)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(jobs)
    }

    /// Determine if a cron job is due based on its schedule expression.
    fn cron_is_due(&self, job: &CronJob, now: DateTime<Utc>) -> bool {
        let schedule = match cron::Schedule::from_str(&job.schedule) {
            Ok(s) => s,
            Err(e) => {
                debug!(
                    job_id = %job.id,
                    schedule = %job.schedule,
                    err = %e,
                    "invalid cron expression"
                );
                return false;
            }
        };

        match &job.last_run_at {
            None => true,
            Some(last_str) => {
                let last_run = match parse_datetime(last_str) {
                    Some(dt) => dt,
                    None => {
                        debug!(
                            job_id = %job.id,
                            last_run_at = %last_str,
                            "unparseable last_run_at — treating as due"
                        );
                        return true;
                    }
                };

                // Find the next occurrence after last_run_at
                schedule
                    .after(&last_run)
                    .next()
                    .map(|next| next <= now)
                    .unwrap_or(false)
            }
        }
    }

    async fn update_cron_last_run(&self, job_id: &str, at: DateTime<Utc>) -> Result<()> {
        let db = self.ctx.db.lock().await;
        db.execute(
            "UPDATE cron_jobs SET last_run_at = ?1 WHERE id = ?2",
            rusqlite::params![at.to_rfc3339(), job_id],
        )?;
        Ok(())
    }
}

/// Parse a datetime string in various common formats.
fn parse_datetime(s: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc));
    }
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc));
    }
    None
}
