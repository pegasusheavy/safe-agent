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

    /// Append a message to conversation history (no user association).
    pub async fn append(&self, role: &str, content: &str) -> Result<()> {
        self.append_with_user(role, content, None).await
    }

    /// Append a message with an optional user_id for multi-user isolation.
    pub async fn append_with_user(&self, role: &str, content: &str, user_id: Option<&str>) -> Result<()> {
        let db = self.db.lock().await;
        db.execute(
            "INSERT INTO conversation_history (role, content, user_id) VALUES (?1, ?2, ?3)",
            rusqlite::params![role, content, user_id],
        )?;

        // Prune old messages beyond the window (per user if user_id is set)
        if let Some(uid) = user_id {
            db.execute(
                "DELETE FROM conversation_history WHERE user_id = ?2 AND id NOT IN (
                    SELECT id FROM conversation_history WHERE user_id = ?2 ORDER BY id DESC LIMIT ?1
                )",
                rusqlite::params![self.window_size as i64, uid],
            )?;
        } else {
            db.execute(
                "DELETE FROM conversation_history WHERE user_id IS NULL AND id NOT IN (
                    SELECT id FROM conversation_history WHERE user_id IS NULL ORDER BY id DESC LIMIT ?1
                )",
                [self.window_size as i64],
            )?;
        }

        Ok(())
    }

    /// Get the most recent conversation messages (within the window).
    pub async fn recent(&self) -> Result<Vec<ConversationMessage>> {
        self.recent_for_user(None).await
    }

    /// Get the most recent conversation messages for a specific user.
    /// If `user_id` is None, returns messages with no user association
    /// (backward-compatible single-user mode).
    pub async fn recent_for_user(&self, user_id: Option<&str>) -> Result<Vec<ConversationMessage>> {
        let db = self.db.lock().await;

        let (sql, messages) = if let Some(uid) = user_id {
            let mut stmt = db.prepare(
                "SELECT id, role, content, created_at FROM conversation_history
                 WHERE user_id = ?1
                 ORDER BY id DESC LIMIT ?2",
            )?;
            let msgs = stmt
                .query_map(rusqlite::params![uid, self.window_size as i64], |row| {
                    Ok(ConversationMessage {
                        id: row.get(0)?,
                        role: row.get(1)?,
                        content: row.get(2)?,
                        created_at: row.get(3)?,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            ("user-filtered", msgs)
        } else {
            let mut stmt = db.prepare(
                "SELECT id, role, content, created_at FROM conversation_history
                 ORDER BY id DESC LIMIT ?1",
            )?;
            let msgs = stmt
                .query_map([self.window_size as i64], |row| {
                    Ok(ConversationMessage {
                        id: row.get(0)?,
                        role: row.get(1)?,
                        content: row.get(2)?,
                        created_at: row.get(3)?,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            ("all", msgs)
        };
        let _ = sql; // suppress warning

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_db;

    #[tokio::test]
    async fn append_and_recent() {
        let db = test_db();
        let conv = ConversationMemory::new(db, 50);
        conv.append("user", "hello").await.unwrap();
        conv.append("assistant", "hi there").await.unwrap();
        let msgs = conv.recent().await.unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[0].content, "hello");
        assert_eq!(msgs[1].role, "assistant");
        assert_eq!(msgs[1].content, "hi there");
    }

    #[tokio::test]
    async fn window_prunes_old_messages() {
        let db = test_db();
        let conv = ConversationMemory::new(db, 3);
        for i in 0..5 {
            conv.append("user", &format!("msg {i}")).await.unwrap();
        }
        let msgs = conv.recent().await.unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].content, "msg 2");
        assert_eq!(msgs[2].content, "msg 4");
    }

    #[tokio::test]
    async fn recent_empty() {
        let db = test_db();
        let conv = ConversationMemory::new(db, 50);
        let msgs = conv.recent().await.unwrap();
        assert!(msgs.is_empty());
    }

    #[tokio::test]
    async fn recent_returns_oldest_first() {
        let db = test_db();
        let conv = ConversationMemory::new(db, 50);
        conv.append("user", "first").await.unwrap();
        conv.append("assistant", "second").await.unwrap();
        conv.append("user", "third").await.unwrap();
        let msgs = conv.recent().await.unwrap();
        assert_eq!(msgs[0].content, "first");
        assert_eq!(msgs[2].content, "third");
    }

    #[tokio::test]
    async fn format_for_prompt_no_messages() {
        let db = test_db();
        let conv = ConversationMemory::new(db, 50);
        let (text, pending) = conv.format_for_prompt().await.unwrap();
        assert!(text.is_empty());
        assert!(!pending);
    }

    #[tokio::test]
    async fn format_for_prompt_pending_user_message() {
        let db = test_db();
        let conv = ConversationMemory::new(db, 50);
        conv.append("user", "what time is it?").await.unwrap();
        let (text, pending) = conv.format_for_prompt().await.unwrap();
        assert!(pending);
        assert!(text.contains("what time is it?"));
    }

    #[tokio::test]
    async fn format_for_prompt_answered_user_message() {
        let db = test_db();
        let conv = ConversationMemory::new(db, 50);
        conv.append("user", "hello").await.unwrap();
        conv.append("assistant", "hi!").await.unwrap();
        let (_text, pending) = conv.format_for_prompt().await.unwrap();
        assert!(!pending);
    }

    #[tokio::test]
    async fn format_for_prompt_system_after_user_not_pending() {
        let db = test_db();
        let conv = ConversationMemory::new(db, 50);
        conv.append("user", "do something").await.unwrap();
        conv.append("system", "[tool result]").await.unwrap();
        let (_text, pending) = conv.format_for_prompt().await.unwrap();
        assert!(!pending);
    }
}
