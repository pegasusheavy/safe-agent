use std::sync::Arc;

use async_trait::async_trait;
use rusqlite::Connection;
use teloxide::prelude::*;
use teloxide::types::ChatAction;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use crate::agent::Agent;
use crate::config::TelegramConfig;
use crate::error::Result;

use super::commands::{handle_bot_command, CommandPrefix, CommandResult};
use super::{split_message, MessagingBackend};

// ---------------------------------------------------------------------------
// MessagingBackend implementation
// ---------------------------------------------------------------------------

pub struct TelegramBackend {
    bot: Bot,
}

impl TelegramBackend {
    pub fn new(bot: Bot) -> Self {
        Self { bot }
    }

    pub fn bot(&self) -> &Bot {
        &self.bot
    }
}

#[async_trait]
impl MessagingBackend for TelegramBackend {
    fn platform_name(&self) -> &str {
        "telegram"
    }

    fn max_message_length(&self) -> usize {
        4096
    }

    async fn send_message(&self, channel: &str, text: &str) -> Result<()> {
        let chat_id: i64 = channel
            .parse()
            .map_err(|_| crate::error::SafeAgentError::Messaging(
                format!("invalid telegram chat id: {channel}"),
            ))?;
        let cid = ChatId(chat_id);

        for chunk in split_message(text, self.max_message_length()) {
            if let Err(e) = self.bot.send_message(cid, chunk).await {
                error!(chat_id, err = %e, "failed to send telegram message");
                return Err(crate::error::SafeAgentError::Messaging(format!(
                    "telegram send failed: {e}"
                )));
            }
        }
        Ok(())
    }

    async fn send_typing(&self, channel: &str) -> Result<()> {
        let chat_id: i64 = channel
            .parse()
            .map_err(|_| crate::error::SafeAgentError::Messaging(
                format!("invalid telegram chat id: {channel}"),
            ))?;
        let _ = self.bot.send_chat_action(ChatId(chat_id), ChatAction::Typing).await;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Dispatcher (long-polling loop)
// ---------------------------------------------------------------------------

/// Shared state accessible by Telegram handlers.
#[derive(Clone)]
struct TelegramState {
    db: Arc<Mutex<Connection>>,
    config: TelegramConfig,
    agent: Arc<Agent>,
    backend: Arc<TelegramBackend>,
}

/// Start the Telegram long-polling dispatcher. Returns the bot handle and a
/// shutdown oneshot.
pub async fn start(
    db: Arc<Mutex<Connection>>,
    config: TelegramConfig,
    agent: Arc<Agent>,
    backend: Arc<TelegramBackend>,
) -> Result<tokio::sync::oneshot::Sender<()>> {
    let bot = backend.bot().clone();

    let state = TelegramState {
        db,
        config: config.clone(),
        agent,
        backend,
    };

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    tokio::spawn(async move {
        info!("telegram bot starting");

        let mut shutdown_rx = shutdown_rx;
        loop {
            let state_clone = state.clone();
            let bot_inner = bot.clone();

            let handler = dptree::entry().branch(
                Update::filter_message().endpoint(handle_message),
            );

            let mut dispatcher = Dispatcher::builder(bot_inner, handler)
                .dependencies(dptree::deps![state_clone])
                .default_handler(|upd| async move {
                    warn!("unhandled telegram update: {:?}", upd.kind);
                })
                .error_handler(LoggingErrorHandler::with_custom_text(
                    "telegram handler error",
                ))
                .build();

            tokio::select! {
                _ = dispatcher.dispatch() => {
                    error!("telegram dispatcher exited, restarting in 5 seconds...");
                },
                _ = &mut shutdown_rx => {
                    info!("telegram bot shutting down");
                    return;
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            info!("restarting telegram dispatcher");
        }
    });

    Ok(shutdown_tx)
}

// ---------------------------------------------------------------------------
// Message handler
// ---------------------------------------------------------------------------

async fn handle_message(
    bot: Bot,
    msg: Message,
    state: TelegramState,
) -> ResponseResult<()> {
    let chat_id = msg.chat.id.0;
    info!(chat_id, "telegram message received");

    // Authorization check
    if !state.config.allowed_chat_ids.is_empty()
        && !state.config.allowed_chat_ids.contains(&chat_id)
    {
        bot.send_message(
            msg.chat.id,
            "⛔ Unauthorized. Your chat ID is not in the allowed list.",
        )
        .await?;
        return Ok(());
    }

    let text = msg.text().unwrap_or("");
    info!(chat_id, text, "telegram message authorized");

    match handle_bot_command(text, CommandPrefix::Slash, &state.db, &state.agent).await {
        CommandResult::Reply(reply) => {
            for chunk in split_message(&reply, 4096) {
                bot.send_message(msg.chat.id, chunk).await?;
            }
        }
        CommandResult::NotACommand => {
            // Free-text message → send to agent
            let _ = bot
                .send_chat_action(msg.chat.id, ChatAction::Typing)
                .await;

            let agent = state.agent.clone();
            let chat = msg.chat.id;
            let user_text = text.to_string();

            // Look up user by Telegram user ID for multi-user routing
            let telegram_user_id = msg.from.as_ref().map(|u| u.id.0 as i64);
            let user_ctx = if let Some(tg_uid) = telegram_user_id {
                agent.user_manager.get_by_telegram_id(tg_uid).await
                    .map(|u| crate::users::UserContext::from_user(&u, "telegram"))
            } else {
                None
            };

            tokio::spawn(async move {
                let typing_bot = bot.clone();
                let typing_handle = tokio::spawn(async move {
                    loop {
                        tokio::time::sleep(tokio::time::Duration::from_secs(4)).await;
                        if typing_bot
                            .send_chat_action(chat, ChatAction::Typing)
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                });

                let result = agent.handle_message_as(&user_text, user_ctx.as_ref()).await;
                typing_handle.abort();

                match result {
                    Ok(reply) => {
                        for chunk in split_message(&reply, 4096) {
                            if let Err(e) = bot.send_message(chat, chunk).await {
                                error!("failed to send telegram reply: {e}");
                            }
                        }
                    }
                    Err(e) => {
                        error!("agent generation failed: {e}");
                        let _ = bot
                            .send_message(chat, format!("⚠️ Error: {e}"))
                            .await;
                    }
                }
            });
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Notification helpers (backward-compatible with old telegram::notifications)
// ---------------------------------------------------------------------------

/// Send a notification to the operator's Telegram chat.
pub async fn notify(bot: &Bot, chat_id: ChatId, message: &str) {
    debug!(chat_id = %chat_id, "sending telegram notification");
    if let Err(e) = bot.send_message(chat_id, message).await {
        error!("failed to send telegram notification: {e}");
    }
}

/// Notify about a new pending action.
pub async fn notify_pending_action(
    bot: &Bot,
    chat_id: ChatId,
    action_id: &str,
    tool_name: &str,
    reasoning: &str,
) {
    let msg = format!(
        "🔔 *New pending action*\nID: `{action_id}`\nTool: `{tool_name}`\n\
         Reason: _{reasoning}_\n\nUse /approve {action_id} or /reject {action_id}",
    );
    notify(bot, chat_id, &msg).await;
}

/// Notify about a tool execution result.
pub async fn notify_result(
    bot: &Bot,
    chat_id: ChatId,
    tool_name: &str,
    success: bool,
    output: &str,
) {
    let icon = if success { "✅" } else { "❌" };
    let truncated = if output.len() > 500 {
        format!("{}...", &output[..500])
    } else {
        output.to_string()
    };
    let msg = format!("{icon} *{tool_name}*\n```\n{truncated}\n```");
    notify(bot, chat_id, &msg).await;
}

/// Notify about an agent error.
pub async fn notify_error(bot: &Bot, chat_id: ChatId, error: &str) {
    let msg = format!("🚨 *Agent error*\n{error}");
    notify(bot, chat_id, &msg).await;
}
