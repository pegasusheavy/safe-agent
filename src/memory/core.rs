use std::sync::Arc;

use rusqlite::Connection;
use tokio::sync::Mutex;

use crate::error::Result;

pub struct CoreMemory {
    db: Arc<Mutex<Connection>>,
}

impl CoreMemory {
    pub fn new(db: Arc<Mutex<Connection>>) -> Self {
        Self { db }
    }

    /// Initialize core memory with the personality from config (only if not already set).
    pub async fn init(&self, personality: &str) -> Result<()> {
        let db = self.db.lock().await;
        db.execute(
            "INSERT OR IGNORE INTO core_memory (id, personality) VALUES (1, ?1)",
            [personality],
        )?;
        Ok(())
    }

    /// Get the core personality string.
    pub async fn get(&self) -> Result<String> {
        let db = self.db.lock().await;
        let personality: String = db.query_row(
            "SELECT personality FROM core_memory WHERE id = 1",
            [],
            |row| row.get(0),
        )?;
        Ok(personality)
    }

    /// Update the core personality.
    pub async fn update(&self, personality: &str) -> Result<()> {
        let db = self.db.lock().await;
        db.execute(
            "UPDATE core_memory SET personality = ?1, updated_at = datetime('now') WHERE id = 1",
            [personality],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_db;

    #[tokio::test]
    async fn init_sets_personality() {
        let db = test_db();
        let core = CoreMemory::new(db);
        core.init("You are helpful").await.unwrap();
        let p = core.get().await.unwrap();
        assert_eq!(p, "You are helpful");
    }

    #[tokio::test]
    async fn init_is_idempotent() {
        let db = test_db();
        let core = CoreMemory::new(db);
        core.init("First").await.unwrap();
        core.init("Second").await.unwrap();
        let p = core.get().await.unwrap();
        assert_eq!(p, "First");
    }

    #[tokio::test]
    async fn update_changes_personality() {
        let db = test_db();
        let core = CoreMemory::new(db);
        core.init("Original").await.unwrap();
        core.update("Updated").await.unwrap();
        let p = core.get().await.unwrap();
        assert_eq!(p, "Updated");
    }

    #[tokio::test]
    async fn get_before_init_errors() {
        let db = test_db();
        let core = CoreMemory::new(db);
        assert!(core.get().await.is_err());
    }
}
