use async_trait::async_trait;
use tracing::debug;
use uuid::Uuid;

use super::{Tool, ToolContext, ToolOutput};
use crate::error::Result;

/// Multi-agent session coordination tool.
pub struct SessionsListTool;

#[async_trait]
impl Tool for SessionsListTool {
    fn name(&self) -> &str {
        "sessions_list"
    }

    fn description(&self) -> &str {
        "List all active agent sessions with their status, labels, and recent messages."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of sessions to return (default 20)"
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let limit = params.get("limit").and_then(|v| v.as_i64()).unwrap_or(20);
        let db = ctx.db.lock().await;

        let mut stmt = db.prepare(
            "SELECT id, label, agent_id, status, created_at, updated_at
             FROM sessions WHERE status = 'active'
             ORDER BY updated_at DESC LIMIT ?1",
        )?;

        let sessions: Vec<String> = stmt
            .query_map([limit], |row| {
                Ok(format!(
                    "[{}] label={} agent={} status={} created={}",
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        if sessions.is_empty() {
            Ok(ToolOutput::ok("No active sessions."))
        } else {
            Ok(ToolOutput::ok(sessions.join("\n")))
        }
    }
}

/// View another session's message history.
pub struct SessionsHistoryTool;

#[async_trait]
impl Tool for SessionsHistoryTool {
    fn name(&self) -> &str {
        "sessions_history"
    }

    fn description(&self) -> &str {
        "Get the message history of a specific session."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["session_id"],
            "properties": {
                "session_id": { "type": "string" },
                "limit": { "type": "integer", "description": "Max messages (default 20)" }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let session_id = params.get("session_id").and_then(|v| v.as_str()).unwrap_or_default();
        let limit = params.get("limit").and_then(|v| v.as_i64()).unwrap_or(20);

        if session_id.is_empty() {
            return Ok(ToolOutput::error("session_id is required"));
        }

        let db = ctx.db.lock().await;
        let mut stmt = db.prepare(
            "SELECT role, content, created_at FROM session_messages
             WHERE session_id = ?1 ORDER BY id DESC LIMIT ?2",
        )?;

        let messages: Vec<String> = stmt
            .query_map(rusqlite::params![session_id, limit], |row| {
                Ok(format!(
                    "[{}] {}: {}",
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        if messages.is_empty() {
            Ok(ToolOutput::ok("No messages in this session."))
        } else {
            let mut msgs = messages;
            msgs.reverse();
            Ok(ToolOutput::ok(msgs.join("\n")))
        }
    }
}

/// Send a message to another session.
pub struct SessionsSendTool;

#[async_trait]
impl Tool for SessionsSendTool {
    fn name(&self) -> &str {
        "sessions_send"
    }

    fn description(&self) -> &str {
        "Send a message to another agent session, triggering that agent to process it."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["session_id", "message"],
            "properties": {
                "session_id": { "type": "string" },
                "message": { "type": "string" }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let session_id = params.get("session_id").and_then(|v| v.as_str()).unwrap_or_default();
        let message = params.get("message").and_then(|v| v.as_str()).unwrap_or_default();

        if session_id.is_empty() || message.is_empty() {
            return Ok(ToolOutput::error("session_id and message are required"));
        }

        debug!(session_id, "sending message to session");

        let db = ctx.db.lock().await;
        db.execute(
            "INSERT INTO session_messages (session_id, role, content) VALUES (?1, 'user', ?2)",
            rusqlite::params![session_id, message],
        )?;
        db.execute(
            "UPDATE sessions SET updated_at = datetime('now') WHERE id = ?1",
            [session_id],
        )?;

        Ok(ToolOutput::ok(format!("Message sent to session {session_id}")))
    }
}

/// Spawn a new agent session for a sub-task.
pub struct SessionsSpawnTool;

#[async_trait]
impl Tool for SessionsSpawnTool {
    fn name(&self) -> &str {
        "sessions_spawn"
    }

    fn description(&self) -> &str {
        "Spawn a new agent session for a sub-task. Returns the new session ID."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["task"],
            "properties": {
                "task": { "type": "string", "description": "Task description for the new session" },
                "label": { "type": "string", "description": "Human-readable label for the session" },
                "agent_id": { "type": "string", "description": "Agent ID to use (default: 'default')" }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let task = params.get("task").and_then(|v| v.as_str()).unwrap_or_default();
        let label = params.get("label").and_then(|v| v.as_str()).unwrap_or("sub-task");
        let agent_id = params.get("agent_id").and_then(|v| v.as_str()).unwrap_or("default");

        if task.is_empty() {
            return Ok(ToolOutput::error("task is required"));
        }

        let session_id = Uuid::new_v4().to_string();
        debug!(session_id, label, agent_id, "spawning session");

        let db = ctx.db.lock().await;
        db.execute(
            "INSERT INTO sessions (id, label, agent_id) VALUES (?1, ?2, ?3)",
            rusqlite::params![session_id, label, agent_id],
        )?;
        db.execute(
            "INSERT INTO session_messages (session_id, role, content) VALUES (?1, 'system', ?2)",
            rusqlite::params![session_id, task],
        )?;

        Ok(ToolOutput::ok_with_meta(
            format!("Spawned session {session_id} ({label})"),
            serde_json::json!({ "session_id": session_id }),
        ))
    }
}
