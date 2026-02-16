pub mod commands;
pub mod notifications;

use std::sync::Arc;

use rusqlite::Connection;
use teloxide::prelude::*;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::agent::Agent;
use crate::config::TelegramConfig;

/// Shared state accessible by Telegram handlers.
#[derive(Clone)]
pub struct TelegramState {
    pub db: Arc<Mutex<Connection>>,
    pub config: TelegramConfig,
    pub agent: Arc<Agent>,
}

/// Start the Telegram bot in the background. Returns the Bot handle and a shutdown sender.
pub async fn start(
    db: Arc<Mutex<Connection>>,
    config: TelegramConfig,
    agent: Arc<Agent>,
) -> crate::error::Result<(Bot, tokio::sync::oneshot::Sender<()>)> {
    let token = crate::config::Config::telegram_bot_token()?;
    let bot = Bot::new(token);
    let bot_clone = bot.clone();

    let state = TelegramState {
        db,
        config: config.clone(),
        agent,
    };

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    tokio::spawn(async move {
        info!("telegram bot starting");

        // Wrap the dispatcher in a restart loop so that if the long-polling
        // connection drops (which can happen silently after hours of uptime),
        // we automatically reconnect instead of going silent forever.
        let mut shutdown_rx = shutdown_rx;
        loop {
            let state_clone = state.clone();
            let bot_inner = bot_clone.clone();

            let handler = dptree::entry()
                .branch(
                    Update::filter_message()
                        .endpoint(commands::handle_message),
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

            // Brief delay before reconnecting to avoid tight loops on persistent errors
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            info!("restarting telegram dispatcher");
        }
    });

    Ok((bot, shutdown_tx))
}
