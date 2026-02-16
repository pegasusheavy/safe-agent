pub mod auth;
pub mod handlers;
pub mod routes;
pub mod sse;

use std::sync::Arc;

use rusqlite::Connection;
use tokio::sync::{broadcast, Mutex};
use tracing::info;

use crate::agent::Agent;
use crate::config::Config;
use crate::error::{Result, SafeAgentError};

pub async fn serve(
    config: Config,
    agent: Arc<Agent>,
    db: Arc<Mutex<Connection>>,
    mut shutdown: broadcast::Receiver<()>,
) -> Result<()> {
    let app = routes::build(agent, config.clone(), db)?;

    let listener = tokio::net::TcpListener::bind(&config.dashboard_bind)
        .await
        .map_err(|e| SafeAgentError::Config(format!("failed to bind {}: {e}", config.dashboard_bind)))?;

    info!(bind = %config.dashboard_bind, "dashboard listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = shutdown.recv().await;
        })
        .await
        .map_err(|e| SafeAgentError::Config(format!("dashboard server error: {e}")))?;

    Ok(())
}
