pub mod types;

use std::sync::Arc;

use rusqlite::Connection;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::error::{Result, SafeAgentError};
use types::{ApprovalStatus, PendingAction};

pub struct ApprovalQueue {
    db: Arc<Mutex<Connection>>,
    expiry_secs: u64,
}

impl ApprovalQueue {
    pub fn new(db: Arc<Mutex<Connection>>, expiry_secs: u64) -> Self {
        Self { db, expiry_secs }
    }

    /// Propose a new action for approval.
    pub async fn propose(
        &self,
        action: serde_json::Value,
        reasoning: &str,
        context: &str,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let action_json = serde_json::to_string(&action)?;
        let db = self.db.lock().await;
        db.execute(
            "INSERT INTO pending_actions (id, action_json, reasoning, context) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, action_json, reasoning, context],
        )?;
        Ok(id)
    }

    /// Approve a single action.
    pub async fn approve(&self, id: &str) -> Result<()> {
        let db = self.db.lock().await;
        let rows = db.execute(
            "UPDATE pending_actions SET status = 'approved', resolved_at = datetime('now')
             WHERE id = ?1 AND status = 'pending'",
            [id],
        )?;
        if rows == 0 {
            return Err(SafeAgentError::Approval(format!(
                "action {id} not found or not pending"
            )));
        }
        // Update stats
        db.execute(
            "UPDATE agent_stats SET total_approved = total_approved + 1 WHERE id = 1",
            [],
        )?;
        Ok(())
    }

    /// Reject a single action.
    pub async fn reject(&self, id: &str) -> Result<()> {
        let db = self.db.lock().await;
        let rows = db.execute(
            "UPDATE pending_actions SET status = 'rejected', resolved_at = datetime('now')
             WHERE id = ?1 AND status = 'pending'",
            [id],
        )?;
        if rows == 0 {
            return Err(SafeAgentError::Approval(format!(
                "action {id} not found or not pending"
            )));
        }
        db.execute(
            "UPDATE agent_stats SET total_rejected = total_rejected + 1 WHERE id = 1",
            [],
        )?;
        Ok(())
    }

    /// Approve all pending actions.
    pub async fn approve_all(&self) -> Result<u64> {
        let db = self.db.lock().await;
        let count = db.execute(
            "UPDATE pending_actions SET status = 'approved', resolved_at = datetime('now')
             WHERE status = 'pending'",
            [],
        )?;
        if count > 0 {
            db.execute(
                &format!("UPDATE agent_stats SET total_approved = total_approved + {count} WHERE id = 1"),
                [],
            )?;
        }
        Ok(count as u64)
    }

    /// Reject all pending actions.
    pub async fn reject_all(&self) -> Result<u64> {
        let db = self.db.lock().await;
        let count = db.execute(
            "UPDATE pending_actions SET status = 'rejected', resolved_at = datetime('now')
             WHERE status = 'pending'",
            [],
        )?;
        if count > 0 {
            db.execute(
                &format!("UPDATE agent_stats SET total_rejected = total_rejected + {count} WHERE id = 1"),
                [],
            )?;
        }
        Ok(count as u64)
    }

    /// Get the next approved action (FIFO).
    pub async fn next_approved(&self) -> Result<Option<PendingAction>> {
        let db = self.db.lock().await;
        let mut stmt = db.prepare(
            "SELECT id, action_json, reasoning, context, status, proposed_at, resolved_at
             FROM pending_actions
             WHERE status = 'approved'
             ORDER BY proposed_at ASC
             LIMIT 1",
        )?;
        let mut rows = stmt.query_map([], |row| {
            let status_str: String = row.get(4)?;
            Ok(PendingAction {
                id: row.get(0)?,
                action: serde_json::from_str(&row.get::<_, String>(1)?).unwrap_or_default(),
                reasoning: row.get(2)?,
                context: row.get(3)?,
                status: parse_status(&status_str),
                proposed_at: row.get(5)?,
                resolved_at: row.get(6)?,
            })
        })?;
        match rows.next() {
            Some(Ok(action)) => Ok(Some(action)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }

    /// Mark an action as executed or failed.
    pub async fn mark_executed(&self, id: &str, success: bool) -> Result<()> {
        let status = if success { "executed" } else { "failed" };
        let db = self.db.lock().await;
        db.execute(
            "UPDATE pending_actions SET status = ?1, resolved_at = datetime('now') WHERE id = ?2",
            rusqlite::params![status, id],
        )?;
        Ok(())
    }

    /// List all pending actions.
    pub async fn list_pending(&self) -> Result<Vec<PendingAction>> {
        let db = self.db.lock().await;
        let mut stmt = db.prepare(
            "SELECT id, action_json, reasoning, context, status, proposed_at, resolved_at
             FROM pending_actions
             WHERE status = 'pending'
             ORDER BY proposed_at ASC",
        )?;
        let actions = stmt
            .query_map([], |row| {
                let status_str: String = row.get(4)?;
                Ok(PendingAction {
                    id: row.get(0)?,
                    action: serde_json::from_str(&row.get::<_, String>(1)?).unwrap_or_default(),
                    reasoning: row.get(2)?,
                    context: row.get(3)?,
                    status: parse_status(&status_str),
                    proposed_at: row.get(5)?,
                    resolved_at: row.get(6)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(actions)
    }

    /// Expire stale pending actions older than the configured expiry.
    pub async fn expire_stale(&self) -> Result<u64> {
        let db = self.db.lock().await;
        let count = db.execute(
            &format!(
                "UPDATE pending_actions SET status = 'expired', resolved_at = datetime('now')
                 WHERE status = 'pending'
                 AND proposed_at < datetime('now', '-{} seconds')",
                self.expiry_secs
            ),
            [],
        )?;
        Ok(count as u64)
    }
}

fn parse_status(s: &str) -> ApprovalStatus {
    match s {
        "pending" => ApprovalStatus::Pending,
        "approved" => ApprovalStatus::Approved,
        "rejected" => ApprovalStatus::Rejected,
        "expired" => ApprovalStatus::Expired,
        "executed" => ApprovalStatus::Executed,
        "failed" => ApprovalStatus::Failed,
        _ => ApprovalStatus::Pending,
    }
}
