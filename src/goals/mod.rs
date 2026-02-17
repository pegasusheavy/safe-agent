use std::sync::Arc;

use rusqlite::Connection;
use tokio::sync::Mutex;
use tracing::{debug, info};
use uuid::Uuid;

use crate::error::{Result, SafeAgentError};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Goal {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: GoalStatus,
    pub priority: i32,
    pub parent_goal_id: Option<String>,
    pub reflection: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    Active,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl GoalStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "active" => Self::Active,
            "paused" => Self::Paused,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            _ => Self::Active,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GoalTask {
    pub id: String,
    pub goal_id: String,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub tool_call: Option<serde_json::Value>,
    pub depends_on: Vec<String>,
    pub result: Option<String>,
    pub sort_order: i32,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Skipped,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "pending" => Self::Pending,
            "in_progress" => Self::InProgress,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "skipped" => Self::Skipped,
            _ => Self::Pending,
        }
    }
}

/// Summary of a goal with task progress.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GoalSummary {
    #[serde(flatten)]
    pub goal: Goal,
    pub total_tasks: usize,
    pub completed_tasks: usize,
    pub failed_tasks: usize,
}

// ---------------------------------------------------------------------------
// GoalManager
// ---------------------------------------------------------------------------

pub struct GoalManager {
    db: Arc<Mutex<Connection>>,
}

impl GoalManager {
    pub fn new(db: Arc<Mutex<Connection>>) -> Self {
        Self { db }
    }

    // -- Goal CRUD ----------------------------------------------------------

    /// Create a new goal. Returns the goal ID.
    pub async fn create_goal(
        &self,
        title: &str,
        description: &str,
        priority: i32,
        parent_goal_id: Option<&str>,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let db = self.db.lock().await;
        db.execute(
            "INSERT INTO goals (id, title, description, priority, parent_goal_id)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, title, description, priority, parent_goal_id],
        )?;
        info!(goal_id = %id, title, "goal created");
        Ok(id)
    }

    /// Get a goal by ID.
    pub async fn get_goal(&self, id: &str) -> Result<Goal> {
        let db = self.db.lock().await;
        db.query_row(
            "SELECT id, title, description, status, priority, parent_goal_id,
                    reflection, created_at, updated_at, completed_at
             FROM goals WHERE id = ?1",
            [id],
            |row| Ok(Self::row_to_goal(row)),
        )
        .map_err(|_| SafeAgentError::Config(format!("goal '{id}' not found")))
    }

    /// List goals with optional status filter.
    pub async fn list_goals(
        &self,
        status: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<GoalSummary>> {
        let db = self.db.lock().await;
        let mut goals = Vec::new();

        let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = if let Some(s) = status
        {
            (
                "SELECT id, title, description, status, priority, parent_goal_id,
                        reflection, created_at, updated_at, completed_at
                 FROM goals WHERE status = ?1
                 ORDER BY priority DESC, created_at DESC
                 LIMIT ?2 OFFSET ?3"
                    .to_string(),
                vec![
                    Box::new(s.to_string()),
                    Box::new(limit as i64),
                    Box::new(offset as i64),
                ],
            )
        } else {
            (
                "SELECT id, title, description, status, priority, parent_goal_id,
                        reflection, created_at, updated_at, completed_at
                 FROM goals
                 ORDER BY priority DESC, created_at DESC
                 LIMIT ?1 OFFSET ?2"
                    .to_string(),
                vec![Box::new(limit as i64), Box::new(offset as i64)],
            )
        };

        let mut stmt = db.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(params_refs.as_slice(), |row| Ok(Self::row_to_goal(row)))?;

        for row in rows {
            let goal = row?;
            let (total, completed, failed) = Self::task_counts_for(&db, &goal.id)?;
            goals.push(GoalSummary {
                goal,
                total_tasks: total,
                completed_tasks: completed,
                failed_tasks: failed,
            });
        }

        Ok(goals)
    }

    /// Update goal status.
    pub async fn update_goal_status(&self, id: &str, status: GoalStatus) -> Result<()> {
        let db = self.db.lock().await;
        let completed_at = if status == GoalStatus::Completed || status == GoalStatus::Failed {
            Some(chrono::Utc::now().to_rfc3339())
        } else {
            None
        };

        db.execute(
            "UPDATE goals SET status = ?1, updated_at = datetime('now'), completed_at = ?2
             WHERE id = ?3",
            rusqlite::params![status.as_str(), completed_at, id],
        )?;

        info!(goal_id = %id, status = status.as_str(), "goal status updated");
        Ok(())
    }

    /// Set the self-reflection text on a goal (called after completion).
    pub async fn set_reflection(&self, id: &str, reflection: &str) -> Result<()> {
        let db = self.db.lock().await;
        db.execute(
            "UPDATE goals SET reflection = ?1, updated_at = datetime('now') WHERE id = ?2",
            rusqlite::params![reflection, id],
        )?;
        Ok(())
    }

    // -- Task CRUD ----------------------------------------------------------

    /// Add a task to a goal. Returns the task ID.
    pub async fn add_task(
        &self,
        goal_id: &str,
        title: &str,
        description: &str,
        tool_call: Option<serde_json::Value>,
        depends_on: &[String],
        sort_order: i32,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let tool_call_str = tool_call.map(|v| serde_json::to_string(&v).unwrap_or_default());
        let depends_str = if depends_on.is_empty() {
            None
        } else {
            Some(depends_on.join(","))
        };

        let db = self.db.lock().await;
        db.execute(
            "INSERT INTO goal_tasks (id, goal_id, title, description, tool_call, depends_on, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![id, goal_id, title, description, tool_call_str, depends_str, sort_order],
        )?;

        debug!(task_id = %id, goal_id, title, "task added to goal");
        Ok(id)
    }

    /// Get all tasks for a goal.
    pub async fn get_tasks(&self, goal_id: &str) -> Result<Vec<GoalTask>> {
        let db = self.db.lock().await;
        let mut stmt = db.prepare(
            "SELECT id, goal_id, title, description, status, tool_call, depends_on,
                    result, sort_order, created_at, completed_at
             FROM goal_tasks WHERE goal_id = ?1
             ORDER BY sort_order ASC, created_at ASC",
        )?;

        let rows = stmt.query_map([goal_id], |row| Ok(Self::row_to_task(row)))?;
        let mut tasks = Vec::new();
        for row in rows {
            tasks.push(row?);
        }
        Ok(tasks)
    }

    /// Update task status and optionally set result text.
    pub async fn update_task_status(
        &self,
        task_id: &str,
        status: TaskStatus,
        result: Option<&str>,
    ) -> Result<()> {
        let completed_at = if status == TaskStatus::Completed || status == TaskStatus::Failed {
            Some(chrono::Utc::now().to_rfc3339())
        } else {
            None
        };

        let db = self.db.lock().await;
        db.execute(
            "UPDATE goal_tasks SET status = ?1, result = ?2, completed_at = ?3
             WHERE id = ?4",
            rusqlite::params![status.as_str(), result, completed_at, task_id],
        )?;

        // Also update the parent goal's updated_at
        db.execute(
            "UPDATE goals SET updated_at = datetime('now')
             WHERE id = (SELECT goal_id FROM goal_tasks WHERE id = ?1)",
            [task_id],
        )?;

        Ok(())
    }

    /// Find the next actionable task across all active goals.
    ///
    /// A task is actionable when:
    /// 1. Its goal is active
    /// 2. Its status is pending
    /// 3. All its dependencies are completed
    ///
    /// Returns the highest-priority goal's earliest actionable task.
    pub async fn next_actionable_task(&self) -> Result<Option<(Goal, GoalTask)>> {
        let db = self.db.lock().await;

        // Get active goals ordered by priority
        let mut goal_stmt = db.prepare(
            "SELECT id, title, description, status, priority, parent_goal_id,
                    reflection, created_at, updated_at, completed_at
             FROM goals WHERE status = 'active'
             ORDER BY priority DESC, created_at ASC",
        )?;

        let goals: Vec<Goal> = goal_stmt
            .query_map([], |row| Ok(Self::row_to_goal(row)))?
            .filter_map(|r| r.ok())
            .collect();

        for goal in goals {
            // Get pending tasks for this goal
            let mut task_stmt = db.prepare(
                "SELECT id, goal_id, title, description, status, tool_call, depends_on,
                        result, sort_order, created_at, completed_at
                 FROM goal_tasks WHERE goal_id = ?1 AND status = 'pending'
                 ORDER BY sort_order ASC, created_at ASC",
            )?;

            let tasks: Vec<GoalTask> = task_stmt
                .query_map([&goal.id], |row| Ok(Self::row_to_task(row)))?
                .filter_map(|r| r.ok())
                .collect();

            for task in tasks {
                // Check if all dependencies are satisfied
                if task.depends_on.is_empty() {
                    return Ok(Some((goal, task)));
                }

                let all_deps_done = task.depends_on.iter().all(|dep_id| {
                    let status: std::result::Result<String, _> = db.query_row(
                        "SELECT status FROM goal_tasks WHERE id = ?1",
                        [dep_id],
                        |row| row.get(0),
                    );
                    matches!(status.as_deref(), Ok("completed"))
                });

                if all_deps_done {
                    return Ok(Some((goal, task)));
                }
            }

            // If a goal has no pending tasks left, check if it should be marked complete
            let pending_count: i64 = db.query_row(
                "SELECT COUNT(*) FROM goal_tasks
                 WHERE goal_id = ?1 AND status IN ('pending', 'in_progress')",
                [&goal.id],
                |row| row.get(0),
            )?;

            if pending_count == 0 {
                // All tasks are done (completed/failed/skipped) — mark goal
                let failed_count: i64 = db.query_row(
                    "SELECT COUNT(*) FROM goal_tasks
                     WHERE goal_id = ?1 AND status = 'failed'",
                    [&goal.id],
                    |row| row.get(0),
                )?;

                let total_count: i64 = db.query_row(
                    "SELECT COUNT(*) FROM goal_tasks WHERE goal_id = ?1",
                    [&goal.id],
                    |row| row.get(0),
                )?;

                if total_count > 0 {
                    // Has tasks but all are done — auto-complete
                    let new_status = if failed_count > 0 {
                        "failed"
                    } else {
                        "completed"
                    };
                    db.execute(
                        "UPDATE goals SET status = ?1, updated_at = datetime('now'),
                         completed_at = datetime('now') WHERE id = ?2",
                        rusqlite::params![new_status, goal.id],
                    )?;
                    info!(
                        goal_id = %goal.id,
                        status = new_status,
                        "goal auto-completed (all tasks done)"
                    );
                }
            }
        }

        Ok(None)
    }

    /// Count active goals.
    pub async fn active_goal_count(&self) -> Result<i64> {
        let db = self.db.lock().await;
        let count: i64 = db.query_row(
            "SELECT COUNT(*) FROM goals WHERE status = 'active'",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    // -- Helpers ------------------------------------------------------------

    fn task_counts_for(
        db: &Connection,
        goal_id: &str,
    ) -> std::result::Result<(usize, usize, usize), rusqlite::Error> {
        let total: i64 =
            db.query_row("SELECT COUNT(*) FROM goal_tasks WHERE goal_id = ?1", [goal_id], |r| {
                r.get(0)
            })?;
        let completed: i64 = db.query_row(
            "SELECT COUNT(*) FROM goal_tasks WHERE goal_id = ?1 AND status = 'completed'",
            [goal_id],
            |r| r.get(0),
        )?;
        let failed: i64 = db.query_row(
            "SELECT COUNT(*) FROM goal_tasks WHERE goal_id = ?1 AND status = 'failed'",
            [goal_id],
            |r| r.get(0),
        )?;
        Ok((total as usize, completed as usize, failed as usize))
    }

    fn row_to_goal(row: &rusqlite::Row) -> Goal {
        Goal {
            id: row.get(0).unwrap_or_default(),
            title: row.get(1).unwrap_or_default(),
            description: row.get(2).unwrap_or_default(),
            status: GoalStatus::from_str(&row.get::<_, String>(3).unwrap_or_default()),
            priority: row.get(4).unwrap_or(0),
            parent_goal_id: row.get(5).unwrap_or(None),
            reflection: row.get(6).unwrap_or(None),
            created_at: row.get(7).unwrap_or_default(),
            updated_at: row.get(8).unwrap_or_default(),
            completed_at: row.get(9).unwrap_or(None),
        }
    }

    fn row_to_task(row: &rusqlite::Row) -> GoalTask {
        let tool_call_str: Option<String> = row.get(5).unwrap_or(None);
        let depends_str: Option<String> = row.get(6).unwrap_or(None);

        GoalTask {
            id: row.get(0).unwrap_or_default(),
            goal_id: row.get(1).unwrap_or_default(),
            title: row.get(2).unwrap_or_default(),
            description: row.get(3).unwrap_or_default(),
            status: TaskStatus::from_str(&row.get::<_, String>(4).unwrap_or_default()),
            tool_call: tool_call_str.and_then(|s| serde_json::from_str(&s).ok()),
            depends_on: depends_str
                .map(|s| s.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect())
                .unwrap_or_default(),
            result: row.get(7).unwrap_or(None),
            sort_order: row.get(8).unwrap_or(0),
            created_at: row.get(9).unwrap_or_default(),
            completed_at: row.get(10).unwrap_or(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[tokio::test]
    async fn create_and_get_goal() {
        let db = db::test_db();
        let mgr = GoalManager::new(db);

        let id = mgr.create_goal("Test goal", "A description", 5, None).await.unwrap();
        let goal = mgr.get_goal(&id).await.unwrap();
        assert_eq!(goal.title, "Test goal");
        assert_eq!(goal.description, "A description");
        assert_eq!(goal.priority, 5);
        assert_eq!(goal.status, GoalStatus::Active);
        assert!(goal.parent_goal_id.is_none());
    }

    #[tokio::test]
    async fn list_goals_with_filter() {
        let db = db::test_db();
        let mgr = GoalManager::new(db);

        mgr.create_goal("Active goal", "", 1, None).await.unwrap();
        let id2 = mgr.create_goal("Paused goal", "", 2, None).await.unwrap();
        mgr.update_goal_status(&id2, GoalStatus::Paused).await.unwrap();

        let active = mgr.list_goals(Some("active"), 100, 0).await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].goal.title, "Active goal");

        let all = mgr.list_goals(None, 100, 0).await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn add_tasks_and_get() {
        let db = db::test_db();
        let mgr = GoalManager::new(db);

        let goal_id = mgr.create_goal("Task goal", "", 0, None).await.unwrap();
        let t1 = mgr.add_task(&goal_id, "Step 1", "First step", None, &[], 0).await.unwrap();
        let _t2 = mgr
            .add_task(&goal_id, "Step 2", "Depends on step 1", None, &[t1.clone()], 1)
            .await
            .unwrap();

        let tasks = mgr.get_tasks(&goal_id).await.unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].title, "Step 1");
        assert!(tasks[0].depends_on.is_empty());
        assert_eq!(tasks[1].title, "Step 2");
        assert_eq!(tasks[1].depends_on, vec![t1]);
    }

    #[tokio::test]
    async fn next_actionable_task_respects_deps() {
        let db = db::test_db();
        let mgr = GoalManager::new(db);

        let goal_id = mgr.create_goal("Dep goal", "", 10, None).await.unwrap();
        let t1 = mgr.add_task(&goal_id, "First", "", None, &[], 0).await.unwrap();
        let _t2 = mgr.add_task(&goal_id, "Second", "", None, &[t1.clone()], 1).await.unwrap();

        // First actionable should be t1 (no deps)
        let (_, task) = mgr.next_actionable_task().await.unwrap().unwrap();
        assert_eq!(task.title, "First");

        // Complete t1
        mgr.update_task_status(&t1, TaskStatus::Completed, Some("done")).await.unwrap();

        // Now t2 should be actionable
        let (_, task) = mgr.next_actionable_task().await.unwrap().unwrap();
        assert_eq!(task.title, "Second");
    }

    #[tokio::test]
    async fn goal_auto_completes_when_all_tasks_done() {
        let db = db::test_db();
        let mgr = GoalManager::new(db);

        let goal_id = mgr.create_goal("Auto-complete", "", 0, None).await.unwrap();
        let t1 = mgr.add_task(&goal_id, "Only task", "", None, &[], 0).await.unwrap();

        mgr.update_task_status(&t1, TaskStatus::Completed, None).await.unwrap();

        // next_actionable_task triggers auto-completion check
        let result = mgr.next_actionable_task().await.unwrap();
        assert!(result.is_none());

        let goal = mgr.get_goal(&goal_id).await.unwrap();
        assert_eq!(goal.status, GoalStatus::Completed);
    }

    #[tokio::test]
    async fn reflection() {
        let db = db::test_db();
        let mgr = GoalManager::new(db);

        let id = mgr.create_goal("Reflect me", "", 0, None).await.unwrap();
        mgr.set_reflection(&id, "The result was good.").await.unwrap();

        let goal = mgr.get_goal(&id).await.unwrap();
        assert_eq!(goal.reflection.as_deref(), Some("The result was good."));
    }

    #[tokio::test]
    async fn active_goal_count() {
        let db = db::test_db();
        let mgr = GoalManager::new(db);

        assert_eq!(mgr.active_goal_count().await.unwrap(), 0);
        mgr.create_goal("One", "", 0, None).await.unwrap();
        mgr.create_goal("Two", "", 0, None).await.unwrap();
        assert_eq!(mgr.active_goal_count().await.unwrap(), 2);
    }
}
