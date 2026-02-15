pub mod commands;
pub mod notifications;

use std::sync::Arc;

use rusqlite::Connection;
use teloxide::prelude::*;
use tokio::sync::Mutex;
use tracing::info;

use crate::config::TelegramConfig;

/// Shared state accessible by Telegram handlers.
#[derive(Clone)]
pub struct TelegramState {
    pub db: Arc<Mutex<Connection>>,
    pub config: TelegramConfig,
}

/// Start the Telegram bot in the background. Returns a shutdown sender.
pub async fn start(
    db: Arc<Mutex<Connection>>,
    config: TelegramConfig,
) -> crate::error::Result<tokio::sync::oneshot::Sender<()>> {
    let token = crate::config::Config::telegram_bot_token()?;
    let bot = Bot::new(token);

    let state = TelegramState {
        db,
        config: config.clone(),
    };

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    tokio::spawn(async move {
        info!("telegram bot starting");

        let handler = dptree::entry()
            .branch(
                Update::filter_message()
                    .endpoint(commands::handle_message),
            );

        let mut dispatcher = Dispatcher::builder(bot, handler)
            .dependencies(dptree::deps![state])
            .enable_ctrlc_handler()
            .build();

        tokio::select! {
            _ = dispatcher.dispatch() => {},
            _ = shutdown_rx => {
                info!("telegram bot shutting down");
            }
        }
    });

    Ok(shutdown_tx)
}
