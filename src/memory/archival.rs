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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_db;

    #[tokio::test]
    async fn archive_and_list() {
        let db = test_db();
        let arch = ArchivalMemory::new(db);
        let id = arch.archive("Test content", "notes").await.unwrap();
        assert!(id > 0);
        let entries = arch.list(0, 10).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].content, "Test content");
        assert_eq!(entries[0].category, "notes");
    }

    #[tokio::test]
    async fn archive_multiple_and_list_paged() {
        let db = test_db();
        let arch = ArchivalMemory::new(db);
        for i in 0..5 {
            arch.archive(&format!("entry {i}"), "cat").await.unwrap();
        }
        let page1 = arch.list(0, 3).await.unwrap();
        assert_eq!(page1.len(), 3);
        let page2 = arch.list(3, 3).await.unwrap();
        assert_eq!(page2.len(), 2);
    }

    #[tokio::test]
    async fn search_finds_matching() {
        let db = test_db();
        let arch = ArchivalMemory::new(db);
        arch.archive("The quick brown fox", "animals").await.unwrap();
        arch.archive("Rust is a systems language", "programming").await.unwrap();
        let results = arch.search("fox", 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("fox"));
    }

    #[tokio::test]
    async fn search_no_results() {
        let db = test_db();
        let arch = ArchivalMemory::new(db);
        arch.archive("hello world", "test").await.unwrap();
        let results = arch.search("nonexistent", 10).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn list_empty() {
        let db = test_db();
        let arch = ArchivalMemory::new(db);
        let entries = arch.list(0, 10).await.unwrap();
        assert!(entries.is_empty());
    }
}
