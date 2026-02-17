use std::sync::Arc;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::error;

/// Structured audit log for every security-relevant event.
///
/// Events include tool executions, approval decisions, LLM calls,
/// rate-limit hits, PII detection, 2FA challenges, and permission denials.
pub struct AuditLogger {
    db: Arc<Mutex<Connection>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: i64,
    pub event_type: String,
    pub tool: Option<String>,
    pub action: Option<String>,
    pub user_context: Option<String>,
    pub reasoning: Option<String>,
    pub params_json: Option<String>,
    pub result: Option<String>,
    pub success: Option<bool>,
    pub source: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditSummary {
    pub total_events: u64,
    pub tool_calls: u64,
    pub approvals: u64,
    pub rejections: u64,
    pub rate_limits: u64,
    pub pii_detections: u64,
    pub twofa_challenges: u64,
    pub permission_denials: u64,
}

impl AuditLogger {
    pub fn new(db: Arc<Mutex<Connection>>) -> Self {
        Self { db }
    }

    /// Log a security-relevant event.
    pub async fn log(
        &self,
        event_type: &str,
        tool: Option<&str>,
        action: Option<&str>,
        user_context: Option<&str>,
        reasoning: Option<&str>,
        params_json: Option<&str>,
        result: Option<&str>,
        success: Option<bool>,
        source: &str,
    ) {
        let db = self.db.lock().await;
        if let Err(e) = db.execute(
            "INSERT INTO audit_log (event_type, tool, action, user_context, reasoning, params_json, result, success, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                event_type,
                tool,
                action,
                user_context,
                reasoning,
                params_json,
                result,
                success,
                source,
            ],
        ) {
            error!("failed to write audit log: {e}");
        }
    }

    /// Convenience: log a tool execution.
    pub async fn log_tool_call(
        &self,
        tool_name: &str,
        params: &serde_json::Value,
        result_preview: &str,
        success: bool,
        source: &str,
        reasoning: &str,
        user_context: &str,
    ) {
        let params_str = serde_json::to_string(params).unwrap_or_default();
        self.log(
            "tool_call",
            Some(tool_name),
            Some(if success { "execute" } else { "fail" }),
            Some(user_context),
            Some(reasoning),
            Some(&params_str),
            Some(result_preview),
            Some(success),
            source,
        )
        .await;
    }

    /// Convenience: log an approval decision.
    pub async fn log_approval(
        &self,
        tool_name: &str,
        action: &str,
        reasoning: &str,
        source: &str,
    ) {
        self.log(
            "approval",
            Some(tool_name),
            Some(action),
            None,
            Some(reasoning),
            None,
            None,
            None,
            source,
        )
        .await;
    }

    /// Convenience: log a rate-limit event.
    pub async fn log_rate_limit(&self, tool_name: &str, source: &str) {
        self.log(
            "rate_limit",
            Some(tool_name),
            Some("block"),
            None,
            None,
            None,
            Some("rate limit exceeded"),
            Some(false),
            source,
        )
        .await;
    }

    /// Convenience: log PII detection.
    pub async fn log_pii_detected(&self, description: &str, action: &str, source: &str) {
        self.log(
            "pii_detected",
            None,
            Some(action),
            None,
            None,
            None,
            Some(description),
            Some(false),
            source,
        )
        .await;
    }

    /// Convenience: log 2FA challenge.
    pub async fn log_2fa(&self, tool_name: &str, action: &str, source: &str) {
        self.log(
            "2fa",
            Some(tool_name),
            Some(action),
            None,
            None,
            None,
            None,
            None,
            source,
        )
        .await;
    }

    /// Convenience: log permission denied.
    pub async fn log_permission_denied(&self, tool_name: &str, reason: &str, source: &str) {
        self.log(
            "permission_denied",
            Some(tool_name),
            Some("block"),
            None,
            None,
            None,
            Some(reason),
            Some(false),
            source,
        )
        .await;
    }

    /// Query recent audit entries with optional filtering.
    pub async fn recent(
        &self,
        limit: usize,
        offset: usize,
        event_type: Option<&str>,
        tool: Option<&str>,
    ) -> Vec<AuditEntry> {
        let db = self.db.lock().await;

        let (sql, params_vec) = match (event_type, tool) {
            (Some(et), Some(t)) => (
                "SELECT id, event_type, tool, action, user_context, reasoning, params_json, result, success, source, created_at \
                 FROM audit_log WHERE event_type = ?1 AND tool = ?2 ORDER BY id DESC LIMIT ?3 OFFSET ?4",
                vec![
                    Box::new(et.to_string()) as Box<dyn rusqlite::types::ToSql>,
                    Box::new(t.to_string()),
                    Box::new(limit as i64),
                    Box::new(offset as i64),
                ],
            ),
            (Some(et), None) => (
                "SELECT id, event_type, tool, action, user_context, reasoning, params_json, result, success, source, created_at \
                 FROM audit_log WHERE event_type = ?1 ORDER BY id DESC LIMIT ?2 OFFSET ?3",
                vec![
                    Box::new(et.to_string()) as Box<dyn rusqlite::types::ToSql>,
                    Box::new(limit as i64),
                    Box::new(offset as i64),
                ],
            ),
            (None, Some(t)) => (
                "SELECT id, event_type, tool, action, user_context, reasoning, params_json, result, success, source, created_at \
                 FROM audit_log WHERE tool = ?1 ORDER BY id DESC LIMIT ?2 OFFSET ?3",
                vec![
                    Box::new(t.to_string()) as Box<dyn rusqlite::types::ToSql>,
                    Box::new(limit as i64),
                    Box::new(offset as i64),
                ],
            ),
            (None, None) => (
                "SELECT id, event_type, tool, action, user_context, reasoning, params_json, result, success, source, created_at \
                 FROM audit_log ORDER BY id DESC LIMIT ?1 OFFSET ?2",
                vec![
                    Box::new(limit as i64) as Box<dyn rusqlite::types::ToSql>,
                    Box::new(offset as i64),
                ],
            ),
        };

        let params_refs: Vec<&dyn rusqlite::types::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

        let mut stmt = match db.prepare(sql) {
            Ok(s) => s,
            Err(e) => {
                error!("audit query failed: {e}");
                return Vec::new();
            }
        };

        let rows = stmt
            .query_map(params_refs.as_slice(), |row| {
                Ok(AuditEntry {
                    id: row.get(0)?,
                    event_type: row.get(1)?,
                    tool: row.get(2)?,
                    action: row.get(3)?,
                    user_context: row.get(4)?,
                    reasoning: row.get(5)?,
                    params_json: row.get(6)?,
                    result: row.get(7)?,
                    success: row.get(8)?,
                    source: row.get(9)?,
                    created_at: row.get(10)?,
                })
            })
            .ok();

        match rows {
            Some(r) => r.filter_map(|r| r.ok()).collect(),
            None => Vec::new(),
        }
    }

    /// Get aggregate statistics from the audit log.
    pub async fn summary(&self) -> AuditSummary {
        let db = self.db.lock().await;
        let count = |event_type: &str| -> u64 {
            db.query_row(
                "SELECT COUNT(*) FROM audit_log WHERE event_type = ?1",
                [event_type],
                |row| row.get(0),
            )
            .unwrap_or(0)
        };

        let total: u64 = db
            .query_row("SELECT COUNT(*) FROM audit_log", [], |row| row.get(0))
            .unwrap_or(0);

        let approvals: u64 = db
            .query_row(
                "SELECT COUNT(*) FROM audit_log WHERE event_type = 'approval' AND action = 'approve'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let rejections: u64 = db
            .query_row(
                "SELECT COUNT(*) FROM audit_log WHERE event_type = 'approval' AND action = 'reject'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        AuditSummary {
            total_events: total,
            tool_calls: count("tool_call"),
            approvals,
            rejections,
            rate_limits: count("rate_limit"),
            pii_detections: count("pii_detected"),
            twofa_challenges: count("2fa"),
            permission_denials: count("permission_denied"),
        }
    }

    /// Get reasoning chain for a specific tool call (last N audit entries
    /// that led to a given result). This powers the "explain" feature.
    pub async fn explain_action(&self, audit_id: i64) -> Vec<AuditEntry> {
        let db = self.db.lock().await;

        // Get the target entry's timestamp and tool
        let target: Option<(String, Option<String>, String)> = db
            .query_row(
                "SELECT created_at, tool, source FROM audit_log WHERE id = ?1",
                [audit_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .ok();

        let (created_at, tool, _source) = match target {
            Some(t) => t,
            None => return Vec::new(),
        };

        fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<AuditEntry> {
            Ok(AuditEntry {
                id: row.get(0)?,
                event_type: row.get(1)?,
                tool: row.get(2)?,
                action: row.get(3)?,
                user_context: row.get(4)?,
                reasoning: row.get(5)?,
                params_json: row.get(6)?,
                result: row.get(7)?,
                success: row.get(8)?,
                source: row.get(9)?,
                created_at: row.get(10)?,
            })
        }

        let mut entries: Vec<AuditEntry> = Vec::new();

        if let Some(ref t) = tool {
            let sql = "SELECT id, event_type, tool, action, user_context, reasoning, params_json, result, success, source, created_at \
                 FROM audit_log \
                 WHERE id <= ?1 AND (tool = ?2 OR event_type IN ('approval', 'rate_limit', '2fa', 'pii_detected', 'permission_denied')) \
                 AND created_at >= datetime(?3, '-1 minute') \
                 ORDER BY id DESC LIMIT 10";
            if let Ok(mut stmt) = db.prepare(sql) {
                if let Ok(rows) = stmt.query_map(
                    rusqlite::params![audit_id, t, created_at],
                    row_to_entry,
                ) {
                    entries = rows.filter_map(|r| r.ok()).collect();
                }
            }
        } else {
            let sql = "SELECT id, event_type, tool, action, user_context, reasoning, params_json, result, success, source, created_at \
                 FROM audit_log WHERE id <= ?1 ORDER BY id DESC LIMIT 10";
            if let Ok(mut stmt) = db.prepare(sql) {
                if let Ok(rows) = stmt.query_map([audit_id], row_to_entry) {
                    entries = rows.filter_map(|r| r.ok()).collect();
                }
            }
        }

        entries.reverse(); // oldest first
        entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn make_logger() -> AuditLogger {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrate(&conn).unwrap();
        AuditLogger::new(Arc::new(Mutex::new(conn)))
    }

    #[tokio::test]
    async fn test_log_and_recent() {
        let logger = make_logger().await;
        logger
            .log_tool_call("exec", &serde_json::json!({"cmd": "ls"}), "file list", true, "agent", "list files", "user said ls")
            .await;
        logger.log_rate_limit("exec", "agent").await;
        logger.log_pii_detected("SSN found", "redact", "agent").await;

        let entries = logger.recent(10, 0, None, None).await;
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].event_type, "pii_detected");
        assert_eq!(entries[1].event_type, "rate_limit");
        assert_eq!(entries[2].event_type, "tool_call");
    }

    #[tokio::test]
    async fn test_filter_by_event_type() {
        let logger = make_logger().await;
        logger.log_tool_call("exec", &serde_json::json!({}), "ok", true, "agent", "", "").await;
        logger.log_rate_limit("exec", "agent").await;

        let entries = logger.recent(10, 0, Some("rate_limit"), None).await;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].event_type, "rate_limit");
    }

    #[tokio::test]
    async fn test_filter_by_tool() {
        let logger = make_logger().await;
        logger.log_tool_call("exec", &serde_json::json!({}), "ok", true, "agent", "", "").await;
        logger.log_tool_call("web_search", &serde_json::json!({}), "ok", true, "agent", "", "").await;

        let entries = logger.recent(10, 0, None, Some("web_search")).await;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].tool.as_deref(), Some("web_search"));
    }

    #[tokio::test]
    async fn test_summary() {
        let logger = make_logger().await;
        logger.log_tool_call("exec", &serde_json::json!({}), "ok", true, "agent", "", "").await;
        logger.log_tool_call("exec", &serde_json::json!({}), "fail", false, "agent", "", "").await;
        logger.log_rate_limit("exec", "agent").await;
        logger.log_pii_detected("SSN", "redact", "agent").await;
        logger.log_2fa("exec", "challenge", "agent").await;
        logger.log_permission_denied("exec", "blocked", "agent").await;
        logger.log_approval("exec", "approve", "ok", "dashboard").await;
        logger.log_approval("exec", "reject", "no", "dashboard").await;

        let summary = logger.summary().await;
        assert_eq!(summary.total_events, 8);
        assert_eq!(summary.tool_calls, 2);
        assert_eq!(summary.approvals, 1);
        assert_eq!(summary.rejections, 1);
        assert_eq!(summary.rate_limits, 1);
        assert_eq!(summary.pii_detections, 1);
        assert_eq!(summary.twofa_challenges, 1);
        assert_eq!(summary.permission_denials, 1);
    }

    #[tokio::test]
    async fn test_explain_action() {
        let logger = make_logger().await;
        logger.log_tool_call("exec", &serde_json::json!({"cmd": "rm -rf /"}), "done", true, "agent", "delete all", "user said delete").await;

        let entries = logger.recent(1, 0, None, None).await;
        let chain = logger.explain_action(entries[0].id).await;
        assert!(!chain.is_empty());
        assert_eq!(chain[0].reasoning.as_deref(), Some("delete all"));
    }
}
