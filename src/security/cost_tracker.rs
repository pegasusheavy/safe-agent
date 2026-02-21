use std::sync::Arc;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::error;

/// Tracks LLM token usage and estimated costs per request.
pub struct CostTracker {
    db: Arc<Mutex<Connection>>,
    /// Maximum daily spend in USD (0.0 = unlimited).
    daily_limit: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub id: i64,
    pub backend: String,
    pub model: String,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    pub estimated_cost: f64,
    pub context: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostSummary {
    /// Total cost today in USD.
    pub today_usd: f64,
    /// Total tokens today.
    pub today_tokens: u64,
    /// Total cost this month.
    pub month_usd: f64,
    /// Total cost all-time.
    pub total_usd: f64,
    /// Total tokens all-time.
    pub total_tokens: u64,
    /// Requests today.
    pub today_requests: u64,
    /// Daily limit in USD (0 = unlimited).
    pub daily_limit_usd: f64,
    /// Whether the daily limit has been exceeded.
    pub limit_exceeded: bool,
}

impl CostTracker {
    pub fn new(db: Arc<Mutex<Connection>>, daily_limit: f64) -> Self {
        Self { db, daily_limit }
    }

    /// Get a cost summary for the dashboard.
    pub async fn summary(&self) -> CostSummary {
        let db = self.db.lock().await;

        let today_usd: f64 = db
            .query_row(
                "SELECT COALESCE(SUM(estimated_cost), 0) FROM llm_usage WHERE date(created_at) = date('now')",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0.0);

        let today_tokens: u64 = db
            .query_row(
                "SELECT COALESCE(SUM(total_tokens), 0) FROM llm_usage WHERE date(created_at) = date('now')",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let today_requests: u64 = db
            .query_row(
                "SELECT COUNT(*) FROM llm_usage WHERE date(created_at) = date('now')",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let month_usd: f64 = db
            .query_row(
                "SELECT COALESCE(SUM(estimated_cost), 0) FROM llm_usage WHERE strftime('%Y-%m', created_at) = strftime('%Y-%m', 'now')",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0.0);

        let total_usd: f64 = db
            .query_row(
                "SELECT COALESCE(SUM(estimated_cost), 0) FROM llm_usage",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0.0);

        let total_tokens: u64 = db
            .query_row(
                "SELECT COALESCE(SUM(total_tokens), 0) FROM llm_usage",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        CostSummary {
            today_usd,
            today_tokens,
            month_usd,
            total_usd,
            total_tokens,
            today_requests,
            daily_limit_usd: self.daily_limit,
            limit_exceeded: self.daily_limit > 0.0 && today_usd > self.daily_limit,
        }
    }

    /// Get recent usage records.
    pub async fn recent(&self, limit: usize) -> Vec<UsageRecord> {
        let db = self.db.lock().await;
        let mut stmt = match db.prepare(
            "SELECT id, backend, model, prompt_tokens, completion_tokens, total_tokens, estimated_cost, context, created_at \
             FROM llm_usage ORDER BY id DESC LIMIT ?1",
        ) {
            Ok(s) => s,
            Err(e) => {
                error!("cost tracker query failed: {e}");
                return Vec::new();
            }
        };

        let rows = stmt
            .query_map([limit as i64], |row| {
                Ok(UsageRecord {
                    id: row.get(0)?,
                    backend: row.get(1)?,
                    model: row.get(2)?,
                    prompt_tokens: row.get(3)?,
                    completion_tokens: row.get(4)?,
                    total_tokens: row.get(5)?,
                    estimated_cost: row.get(6)?,
                    context: row.get(7)?,
                    created_at: row.get(8)?,
                })
            })
            .ok();

        match rows {
            Some(r) => r.filter_map(|r| r.ok()).collect(),
            None => Vec::new(),
        }
    }
}

