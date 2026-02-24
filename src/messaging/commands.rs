use std::sync::Arc;

use rusqlite::Connection;
use tokio::sync::Mutex;

use crate::agent::Agent;

/// A command prefix style. Telegram uses `/`, WhatsApp and others use `!`.
#[derive(Debug, Clone, Copy)]
pub enum CommandPrefix {
    Slash,
}

impl CommandPrefix {
    pub fn as_str(&self) -> &str {
        match self {
            CommandPrefix::Slash => "/",
        }
    }
}

/// Result of handling a bot command.
pub enum CommandResult {
    /// Send the contained text as a reply.
    Reply(String),
    /// The message wasn't a command — it's freeform text for the agent.
    NotACommand,
}

/// Attempt to handle a bot command. Returns `CommandResult::NotACommand` if
/// the message doesn't start with the expected command prefix.
///
/// `raw_text`: the full incoming message text  
/// `prefix`: the prefix style for this platform  
/// `db`: shared database connection  
/// `agent`: the agent instance  
pub async fn handle_bot_command(
    raw_text: &str,
    prefix: CommandPrefix,
    db: &Arc<Mutex<Connection>>,
    agent: &Arc<Agent>,
) -> CommandResult {
    let pfx = prefix.as_str();

    if !raw_text.starts_with(pfx) {
        return CommandResult::NotACommand;
    }

    let parts: Vec<&str> = raw_text.splitn(3, ' ').collect();
    let cmd = parts[0]
        .strip_prefix(pfx)
        .unwrap_or(parts[0])
        .split('@')
        .next()
        .unwrap_or("");

    match cmd {
        "start" | "help" => {
            let p = pfx;
            CommandResult::Reply(format!(
                "🤖 safeclaw Control\n\n\
                 {p}status - Agent status & stats\n\
                 {p}pending - List pending actions\n\
                 {p}approve <id|all> - Approve action(s)\n\
                 {p}reject <id|all> - Reject action(s)\n\
                 {p}pause - Pause agent loop\n\
                 {p}resume - Resume agent loop\n\
                 {p}tick - Force immediate tick\n\
                 {p}memory <query> - Search archival memory\n\
                 {p}help - This message\n\n\
                 Or just type a message and the agent will respond."
            ))
        }

        "status" => {
            let conn = db.lock().await;
            let stats = conn.query_row(
                "SELECT total_ticks, total_actions, total_approved, total_rejected, last_tick_at \
                 FROM agent_stats WHERE id = 1",
                [],
                |row| {
                    Ok(format!(
                        "📊 Ticks: {}\n⚡ Actions: {}\n✅ Approved: {}\n❌ Rejected: {}\n⏰ Last tick: {}",
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, Option<String>>(4)?.unwrap_or_else(|| "never".into()),
                    ))
                },
            );
            drop(conn);
            CommandResult::Reply(stats.unwrap_or_else(|_| "Could not fetch stats.".to_string()))
        }

        "pending" => {
            let actions = {
                let conn = db.lock().await;
                let mut stmt = conn
                    .prepare(
                        "SELECT id, action_json, reasoning FROM pending_actions \
                         WHERE status = 'pending' ORDER BY proposed_at DESC LIMIT 10",
                    )
                    .unwrap();
                stmt.query_map([], |row| {
                    Ok(format!(
                        "🔔 *{}*\n{}\n_Reason: {}_",
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .unwrap()
                .filter_map(|r| r.ok())
                .collect::<Vec<String>>()
            };

            if actions.is_empty() {
                CommandResult::Reply("No pending actions.".to_string())
            } else {
                CommandResult::Reply(actions.join("\n\n"))
            }
        }

        "approve" => {
            let target = parts.get(1).unwrap_or(&"");
            let conn = db.lock().await;
            if *target == "all" {
                let n = conn
                    .execute(
                        "UPDATE pending_actions SET status = 'approved', resolved_at = datetime('now') \
                         WHERE status = 'pending'",
                        [],
                    )
                    .unwrap_or(0);
                drop(conn);
                CommandResult::Reply(format!("✅ Approved {n} action(s)."))
            } else if !target.is_empty() {
                let n = conn
                    .execute(
                        "UPDATE pending_actions SET status = 'approved', resolved_at = datetime('now') \
                         WHERE id = ?1 AND status = 'pending'",
                        [target],
                    )
                    .unwrap_or(0);
                drop(conn);
                if n > 0 {
                    CommandResult::Reply(format!("✅ Approved {target}"))
                } else {
                    CommandResult::Reply(format!(
                        "Action {target} not found or already resolved."
                    ))
                }
            } else {
                CommandResult::Reply(format!("Usage: {pfx}approve <id|all>"))
            }
        }

        "reject" => {
            let target = parts.get(1).unwrap_or(&"");
            let conn = db.lock().await;
            if *target == "all" {
                let n = conn
                    .execute(
                        "UPDATE pending_actions SET status = 'rejected', resolved_at = datetime('now') \
                         WHERE status = 'pending'",
                        [],
                    )
                    .unwrap_or(0);
                drop(conn);
                CommandResult::Reply(format!("❌ Rejected {n} action(s)."))
            } else if !target.is_empty() {
                let n = conn
                    .execute(
                        "UPDATE pending_actions SET status = 'rejected', resolved_at = datetime('now') \
                         WHERE id = ?1 AND status = 'pending'",
                        [target],
                    )
                    .unwrap_or(0);
                drop(conn);
                if n > 0 {
                    CommandResult::Reply(format!("❌ Rejected {target}"))
                } else {
                    CommandResult::Reply(format!(
                        "Action {target} not found or already resolved."
                    ))
                }
            } else {
                CommandResult::Reply(format!("Usage: {pfx}reject <id|all>"))
            }
        }

        "tick" => {
            let agent_clone = agent.clone();
            tokio::spawn(async move {
                if let Err(e) = agent_clone.force_tick().await {
                    tracing::error!("forced tick failed: {e}");
                }
            });
            CommandResult::Reply("⏩ Forcing immediate tick...".to_string())
        }

        "pause" => {
            agent.pause();
            CommandResult::Reply("⏸ Agent paused.".to_string())
        }

        "resume" => {
            agent.resume();
            CommandResult::Reply("▶️ Agent resumed.".to_string())
        }

        "memory" => {
            let query = parts.get(1).unwrap_or(&"");
            if query.is_empty() {
                CommandResult::Reply(format!("Usage: {pfx}memory <search query>"))
            } else {
                let results = {
                    let conn = db.lock().await;
                    let mut stmt = conn
                        .prepare(
                            "SELECT am.content, am.category, am.created_at
                             FROM archival_memory_fts fts
                             JOIN archival_memory am ON am.id = fts.rowid
                             WHERE archival_memory_fts MATCH ?1
                             ORDER BY rank LIMIT 5",
                        )
                        .unwrap();
                    stmt.query_map([query], |row| {
                        Ok(format!(
                            "📌 [{}] {}: {}",
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(0)?,
                        ))
                    })
                    .unwrap()
                    .filter_map(|r| r.ok())
                    .collect::<Vec<String>>()
                };

                if results.is_empty() {
                    CommandResult::Reply("No matching memories.".to_string())
                } else {
                    CommandResult::Reply(results.join("\n\n"))
                }
            }
        }

        _ => CommandResult::Reply("Unknown command. Use help for available commands.".to_string()),
    }
}
