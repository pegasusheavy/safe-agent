use std::sync::Arc;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchivalEntry {
    pub id: i64,
    pub content: String,
    pub category: String,
    pub created_at: String,
}

pub struct ArchivalMemory {
    db: Arc<Mutex<Connection>>,
}

impl ArchivalMemory {
    pub fn new(db: Arc<Mutex<Connection>>) -> Self {
        Self { db }
    }

    /// Store a new memory entry.
    pub async fn archive(&self, content: &str, category: &str) -> Result<i64> {
        let db = self.db.lock().await;
        db.execute(
            "INSERT INTO archival_memory (content, category) VALUES (?1, ?2)",
            [content, category],
        )?;
        Ok(db.last_insert_rowid())
    }

    /// Full-text search over archival memory.
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<ArchivalEntry>> {
        let db = self.db.lock().await;
        let mut stmt = db.prepare(
            "SELECT am.id, am.content, am.category, am.created_at
             FROM archival_memory_fts fts
             JOIN archival_memory am ON am.id = fts.rowid
             WHERE archival_memory_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )?;
        let entries = stmt
            .query_map(rusqlite::params![query, limit as i64], |row| {
                Ok(ArchivalEntry {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    category: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(entries)
    }

    /// Get all archival entries (paginated).
    pub async fn list(&self, offset: usize, limit: usize) -> Result<Vec<ArchivalEntry>> {
        let db = self.db.lock().await;
        let mut stmt = db.prepare(
            "SELECT id, content, category, created_at FROM archival_memory
             ORDER BY id DESC LIMIT ?1 OFFSET ?2",
        )?;
        let entries = stmt
            .query_map(rusqlite::params![limit as i64, offset as i64], |row| {
                Ok(ArchivalEntry {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    category: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(entries)
    }
}
