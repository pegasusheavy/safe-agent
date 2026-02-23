use std::sync::Arc;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    pub id: i64,
    pub trigger: String,
    pub summary: String,
    pub actions: Vec<EpisodeAction>,
    pub outcome: String,
    pub user_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeAction {
    pub tool: String,
    pub params_summary: String,
    pub result_summary: String,
    pub success: bool,
}

pub struct EpisodicMemory {
    db: Arc<Mutex<Connection>>,
}

impl EpisodicMemory {
    pub fn new(db: Arc<Mutex<Connection>>) -> Self {
        Self { db }
    }

    /// Record a new episode (a completed interaction with tool calls and outcome).
    pub async fn record(
        &self,
        trigger: &str,
        summary: &str,
        actions: &[EpisodeAction],
        outcome: &str,
        user_id: Option<&str>,
    ) -> Result<i64> {
        let actions_json = serde_json::to_string(actions).unwrap_or_else(|_| "[]".to_string());
        let db = self.db.lock().await;
        db.execute(
            "INSERT INTO episodes (trigger, summary, actions, outcome, user_id) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![trigger, summary, actions_json, outcome, user_id],
        )?;
        Ok(db.last_insert_rowid())
    }

    /// Retrieve recent episodes (newest first).
    pub async fn recent(&self, limit: usize, user_id: Option<&str>) -> Result<Vec<Episode>> {
        let db = self.db.lock().await;

        let (sql, episodes) = if let Some(uid) = user_id {
            let mut stmt = db.prepare(
                "SELECT id, trigger, summary, actions, outcome, user_id, created_at
                 FROM episodes WHERE user_id = ?1 ORDER BY id DESC LIMIT ?2",
            )?;
            let eps = stmt
                .query_map(rusqlite::params![uid, limit as i64], map_episode)?
                .filter_map(|r| r.ok())
                .collect::<Vec<_>>();
            ("user", eps)
        } else {
            let mut stmt = db.prepare(
                "SELECT id, trigger, summary, actions, outcome, user_id, created_at
                 FROM episodes ORDER BY id DESC LIMIT ?1",
            )?;
            let eps = stmt
                .query_map([limit as i64], map_episode)?
                .filter_map(|r| r.ok())
                .collect::<Vec<_>>();
            ("all", eps)
        };
        let _ = sql;

        Ok(episodes)
    }

    /// Search episodes by keyword in summary or outcome.
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<Episode>> {
        let db = self.db.lock().await;
        let pattern = format!("%{query}%");
        let mut stmt = db.prepare(
            "SELECT id, trigger, summary, actions, outcome, user_id, created_at
             FROM episodes WHERE summary LIKE ?1 OR outcome LIKE ?1
             ORDER BY id DESC LIMIT ?2",
        )?;
        let episodes = stmt
            .query_map(rusqlite::params![pattern, limit as i64], map_episode)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(episodes)
    }

    /// Count total episodes.
    pub async fn count(&self) -> Result<i64> {
        let db = self.db.lock().await;
        let count: i64 = db.query_row("SELECT COUNT(*) FROM episodes", [], |r| r.get(0))?;
        Ok(count)
    }
}

fn map_episode(row: &rusqlite::Row) -> rusqlite::Result<Episode> {
    let actions_str: String = row.get(3)?;
    let actions: Vec<EpisodeAction> =
        serde_json::from_str(&actions_str).unwrap_or_default();
    Ok(Episode {
        id: row.get(0)?,
        trigger: row.get(1)?,
        summary: row.get(2)?,
        actions,
        outcome: row.get(4)?,
        user_id: row.get(5)?,
        created_at: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_db;

    #[tokio::test]
    async fn record_and_retrieve() {
        let db = test_db();
        let em = EpisodicMemory::new(db);

        let actions = vec![EpisodeAction {
            tool: "exec".to_string(),
            params_summary: "ls -la".to_string(),
            result_summary: "listed files".to_string(),
            success: true,
        }];

        let id = em
            .record("user_message", "user asked to list files", &actions, "success", None)
            .await
            .unwrap();
        assert!(id > 0);

        let episodes = em.recent(10, None).await.unwrap();
        assert_eq!(episodes.len(), 1);
        assert_eq!(episodes[0].trigger, "user_message");
        assert_eq!(episodes[0].actions.len(), 1);
        assert_eq!(episodes[0].actions[0].tool, "exec");
    }

    #[tokio::test]
    async fn search_episodes() {
        let db = test_db();
        let em = EpisodicMemory::new(db);

        em.record("user_message", "deployed to production", &[], "success", None)
            .await
            .unwrap();
        em.record("cron_job", "ran backup", &[], "completed", None)
            .await
            .unwrap();

        let results = em.search("production", 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].summary.contains("production"));
    }

    #[tokio::test]
    async fn count_episodes() {
        let db = test_db();
        let em = EpisodicMemory::new(db);

        assert_eq!(em.count().await.unwrap(), 0);
        em.record("test", "ep1", &[], "", None).await.unwrap();
        em.record("test", "ep2", &[], "", None).await.unwrap();
        assert_eq!(em.count().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn user_scoped_episodes() {
        let db = test_db();
        let em = EpisodicMemory::new(db);

        em.record("msg", "user1 action", &[], "ok", Some("u1"))
            .await
            .unwrap();
        em.record("msg", "user2 action", &[], "ok", Some("u2"))
            .await
            .unwrap();

        let u1_eps = em.recent(10, Some("u1")).await.unwrap();
        assert_eq!(u1_eps.len(), 1);
        assert!(u1_eps[0].summary.contains("user1"));
    }
}
