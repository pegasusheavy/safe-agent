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
    ///
    /// Returns `(text, has_pending_user_message)` where `has_pending_user_message`
    /// is true only if the most recent message is from the user and hasn't been
    /// responded to yet (no tool execution or assistant reply after it).
    ///
    /// Only includes user messages that are still pending (unanswered).
    /// Already-handled user messages are omitted to prevent the model from
    /// re-responding to old messages.
    pub async fn format_for_prompt(&self) -> Result<(String, bool)> {
        let messages = self.recent().await?;

        // First pass: find whether the last message is an unanswered user message.
        // Walk backwards from the most recent message:
        // - If we hit a user message first → it's pending
        // - If we hit assistant/system first → all user messages are handled
        let mut has_pending = false;
        let mut pending_user_content: Option<String> = None;
        for msg in messages.iter().rev() {
            if msg.role == "user" {
                has_pending = true;
                pending_user_content = Some(msg.content.chars().take(200).collect());
                break;
            } else if msg.role == "assistant" || msg.role == "system" {
                // Agent already acted after the last user message
                break;
            }
        }

        // Build prompt text: only include the pending user message
        // and a few recent system messages for context
        let mut out = String::new();
        let max_total = 500;

        // Include recent system messages (tool results) for context
        for msg in &messages {
            if msg.role == "system" {
                let content: String = msg.content.chars().take(200).collect();
                let line = format!("[{}] {}\n", msg.role, content);
                if out.len() + line.len() > max_total {
                    break;
                }
                out.push_str(&line);
            }
            // Skip assistant and user messages — we only add the pending user message below
        }

        // Only add the user message if it's pending
        if let Some(content) = &pending_user_content {
            let line = format!("USER MESSAGE: {}\n", content);
            if out.len() + line.len() <= max_total {
                out.push_str(&line);
            }
        }

        Ok((out, has_pending))
    }
}
