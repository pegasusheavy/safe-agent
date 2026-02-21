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
}
