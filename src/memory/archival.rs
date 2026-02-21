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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_db;

    #[tokio::test]
    async fn list_empty() {
        let db = test_db();
        let arch = ArchivalMemory::new(db);
        let entries = arch.list(0, 10).await.unwrap();
        assert!(entries.is_empty());
    }
}
