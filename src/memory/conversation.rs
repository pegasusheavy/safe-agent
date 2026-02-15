use std::sync::Arc;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub id: i64,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

pub struct ConversationMemory {
    db: Arc<Mutex<Connection>>,
    window_size: usize,
}

impl ConversationMemory {
    pub fn new(db: Arc<Mutex<Connection>>, window_size: usize) -> Self {
        Self { db, window_size }
    }

    /// Append a message to conversation history.
    pub async fn append(&self, role: &str, content: &str) -> Result<()> {
        let db = self.db.lock().await;
        db.execute(
            "INSERT INTO conversation_history (role, content) VALUES (?1, ?2)",
            [role, content],
        )?;

        // Prune old messages beyond the window
        db.execute(
            "DELETE FROM conversation_history WHERE id NOT IN (
                SELECT id FROM conversation_history ORDER BY id DESC LIMIT ?1
            )",
            [self.window_size as i64],
        )?;

        Ok(())
    }

    /// Get the most recent conversation messages (within the window).
    pub async fn recent(&self) -> Result<Vec<ConversationMessage>> {
        let db = self.db.lock().await;
        let mut stmt = db.prepare(
            "SELECT id, role, content, created_at FROM conversation_history
             ORDER BY id DESC LIMIT ?1",
        )?;
        let messages = stmt
            .query_map([self.window_size as i64], |row| {
                Ok(ConversationMessage {
                    id: row.get(0)?,
                    role: row.get(1)?,
                    content: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        // Reverse so oldest is first
        let mut messages = messages;
        messages.reverse();
        Ok(messages)
    }

    /// Format recent conversation for inclusion in a prompt.
    pub async fn format_for_prompt(&self) -> Result<String> {
        let messages = self.recent().await?;
        let mut out = String::new();
        for msg in &messages {
            out.push_str(&format!("[{}] {}: {}\n", msg.created_at, msg.role, msg.content));
        }
        Ok(out)
    }
}
