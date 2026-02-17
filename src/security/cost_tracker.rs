use std::sync::Arc;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

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

/// Estimate cost based on model/backend pricing.
/// Returns cost in USD.
pub fn estimate_cost(backend: &str, model: &str, prompt_tokens: u32, completion_tokens: u32) -> f64 {
    let (prompt_rate, completion_rate) = model_pricing(backend, model);
    let prompt_cost = (prompt_tokens as f64 / 1_000_000.0) * prompt_rate;
    let completion_cost = (completion_tokens as f64 / 1_000_000.0) * completion_rate;
    prompt_cost + completion_cost
}

/// Returns (prompt_price_per_million, completion_price_per_million) in USD.
/// Uses approximate pricing; actual costs may vary.
fn model_pricing(backend: &str, model: &str) -> (f64, f64) {
    let model_lower = model.to_lowercase();

    match backend {
        "openrouter" | "claude" => {
            if model_lower.contains("opus") {
                (15.0, 75.0)
            } else if model_lower.contains("sonnet") {
                (3.0, 15.0)
            } else if model_lower.contains("haiku") {
                (0.25, 1.25)
            } else if model_lower.contains("gpt-4o") {
                (2.5, 10.0)
            } else if model_lower.contains("gpt-4") {
                (10.0, 30.0)
            } else if model_lower.contains("o3") {
                (10.0, 40.0)
            } else if model_lower.contains("gemini") && model_lower.contains("pro") {
                (1.25, 5.0)
            } else if model_lower.contains("gemini") && model_lower.contains("flash") {
                (0.075, 0.30)
            } else if model_lower.contains("llama") || model_lower.contains("mistral") {
                (0.20, 0.60)
            } else if model_lower.contains("deepseek") {
                (0.14, 0.28)
            } else {
                // Default: moderate pricing
                (3.0, 15.0)
            }
        }
        "local" => (0.0, 0.0),
        _ => (3.0, 15.0),
    }
}

impl CostTracker {
    pub fn new(db: Arc<Mutex<Connection>>, daily_limit: f64) -> Self {
        Self { db, daily_limit }
    }

    /// Record a usage event and return whether the daily limit is exceeded.
    pub async fn record(
        &self,
        backend: &str,
        model: &str,
        prompt_tokens: u32,
        completion_tokens: u32,
        context: &str,
    ) -> bool {
        let total_tokens = prompt_tokens + completion_tokens;
        let cost = estimate_cost(backend, model, prompt_tokens, completion_tokens);

        let db = self.db.lock().await;
        if let Err(e) = db.execute(
            "INSERT INTO llm_usage (backend, model, prompt_tokens, completion_tokens, total_tokens, estimated_cost, context)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                backend,
                model,
                prompt_tokens,
                completion_tokens,
                total_tokens,
                cost,
                context,
            ],
        ) {
            error!("failed to record LLM usage: {e}");
        }

        if cost > 0.0 {
            info!(
                backend,
                model,
                prompt_tokens,
                completion_tokens,
                cost_usd = format!("{cost:.6}"),
                "LLM usage recorded"
            );
        }

        // Check daily limit
        if self.daily_limit > 0.0 {
            let today_cost: f64 = db
                .query_row(
                    "SELECT COALESCE(SUM(estimated_cost), 0) FROM llm_usage WHERE date(created_at) = date('now')",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(0.0);

            if today_cost > self.daily_limit {
                warn!(
                    today_cost = format!("{today_cost:.4}"),
                    limit = format!("{:.4}", self.daily_limit),
                    "daily LLM cost limit exceeded"
                );
                return true;
            }
        }

        false
    }

    /// Check if the daily cost limit has been exceeded (without recording).
    pub async fn is_limit_exceeded(&self) -> bool {
        if self.daily_limit <= 0.0 {
            return false;
        }

        let db = self.db.lock().await;
        let today_cost: f64 = db
            .query_row(
                "SELECT COALESCE(SUM(estimated_cost), 0) FROM llm_usage WHERE date(created_at) = date('now')",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0.0);

        today_cost > self.daily_limit
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

#[cfg(test)]
mod tests {
    use super::*;

    async fn make_tracker(daily_limit: f64) -> CostTracker {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrate(&conn).unwrap();
        CostTracker::new(Arc::new(Mutex::new(conn)), daily_limit)
    }

    #[test]
    fn test_estimate_cost() {
        let cost = estimate_cost("openrouter", "anthropic/claude-sonnet-4", 1000, 500);
        assert!(cost > 0.0);

        let local = estimate_cost("local", "llama3", 1000, 500);
        assert_eq!(local, 0.0);
    }

    #[test]
    fn test_model_pricing() {
        let (p, c) = model_pricing("openrouter", "anthropic/claude-3.5-sonnet");
        assert_eq!(p, 3.0);
        assert_eq!(c, 15.0);

        let (p, c) = model_pricing("openrouter", "anthropic/claude-3-opus");
        assert_eq!(p, 15.0);
        assert_eq!(c, 75.0);

        let (p, c) = model_pricing("local", "anything");
        assert_eq!(p, 0.0);
        assert_eq!(c, 0.0);
    }

    #[tokio::test]
    async fn test_record_and_summary() {
        let tracker = make_tracker(0.0).await;
        tracker.record("openrouter", "sonnet", 1000, 500, "message").await;
        tracker.record("openrouter", "sonnet", 2000, 1000, "goal_task").await;

        let summary = tracker.summary().await;
        assert_eq!(summary.today_requests, 2);
        assert!(summary.today_usd > 0.0);
        assert!(summary.total_tokens > 0);
        assert!(!summary.limit_exceeded);
    }

    #[tokio::test]
    async fn test_daily_limit_exceeded() {
        let tracker = make_tracker(0.0001).await;
        let exceeded = tracker.record("openrouter", "opus", 100000, 50000, "message").await;
        assert!(exceeded);
        assert!(tracker.is_limit_exceeded().await);
    }

    #[tokio::test]
    async fn test_recent() {
        let tracker = make_tracker(0.0).await;
        tracker.record("openrouter", "sonnet", 100, 50, "message").await;
        tracker.record("openrouter", "haiku", 200, 100, "goal").await;

        let records = tracker.recent(10).await;
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].model, "haiku"); // newest first
    }
}
