use async_trait::async_trait;

use super::{Tool, ToolContext, ToolOutput};
use crate::error::Result;

/// Search archival memory via full-text search.
pub struct MemorySearchTool;

#[async_trait]
impl Tool for MemorySearchTool {
    fn name(&self) -> &str {
        "memory_search"
    }

    fn description(&self) -> &str {
        "Search the agent's archival memory using full-text search. Returns matching entries with category and timestamp."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query"
                },
                "limit": {
                    "type": "integer",
                    "description": "Max results (default 10)"
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let query = params.get("query").and_then(|v| v.as_str()).unwrap_or_default();
        let limit = params.get("limit").and_then(|v| v.as_i64()).unwrap_or(10);

        if query.is_empty() {
            return Ok(ToolOutput::error("query is required"));
        }

        let db = ctx.db.lock().await;
        let mut stmt = db.prepare(
            "SELECT am.id, am.content, am.category, am.created_at
             FROM archival_memory_fts fts
             JOIN archival_memory am ON am.id = fts.rowid
             WHERE archival_memory_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )?;

        let entries: Vec<String> = stmt
            .query_map(rusqlite::params![query, limit], |row| {
                Ok(format!(
                    "[{}] [{}] {}",
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(1)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        if entries.is_empty() {
            Ok(ToolOutput::ok("No matching memories found."))
        } else {
            Ok(ToolOutput::ok(entries.join("\n")))
        }
    }
}

/// Get a specific archival memory entry by ID.
pub struct MemoryGetTool;

#[async_trait]
impl Tool for MemoryGetTool {
    fn name(&self) -> &str {
        "memory_get"
    }

    fn description(&self) -> &str {
        "Retrieve a specific archival memory entry by ID."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["id"],
            "properties": {
                "id": {
                    "type": "integer",
                    "description": "Memory entry ID"
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let id = params.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
        if id == 0 {
            return Ok(ToolOutput::error("id is required"));
        }

        let db = ctx.db.lock().await;
        let result = db.query_row(
            "SELECT id, content, category, created_at FROM archival_memory WHERE id = ?1",
            [id],
            |row| {
                Ok(format!(
                    "[{}] [{}] {}",
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(1)?,
                ))
            },
        );

        match result {
            Ok(entry) => Ok(ToolOutput::ok(entry)),
            Err(_) => Ok(ToolOutput::error(format!("Memory entry {id} not found"))),
        }
    }
}
