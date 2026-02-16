use std::sync::Arc;

use axum::middleware;
use axum::routing::{delete, get, post, put};
use axum::Router;
use rusqlite::Connection;
use tokio::sync::Mutex;

use crate::agent::Agent;
use crate::config::Config;
use crate::error::{Result, SafeAgentError};

use super::auth;
use super::handlers;
use super::sse;

/// State shared across all routes.
#[derive(Clone)]
pub struct DashState {
    pub agent: Arc<Agent>,
    pub config: Config,
    pub db: Arc<Mutex<Connection>>,
    /// The password users must provide to access the dashboard.
    pub dashboard_password: String,
    /// Secret bytes used to sign/verify HS256 JWT cookies.
    pub jwt_secret: Vec<u8>,
}

pub fn build(agent: Arc<Agent>, config: Config, db: Arc<Mutex<Connection>>) -> Result<Router> {
    let dashboard_password = std::env::var("DASHBOARD_PASSWORD")
        .ok()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            SafeAgentError::Config(
                "DASHBOARD_PASSWORD environment variable is required but not set".to_string(),
            )
        })?;

    let jwt_secret_str = std::env::var("JWT_SECRET")
        .ok()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            SafeAgentError::Config(
                "JWT_SECRET environment variable is required but not set".to_string(),
            )
        })?;

    let jwt_secret = jwt_secret_str.into_bytes();

    tracing::info!("dashboard password protection enabled (JWT auth)");

    let state = DashState {
        agent,
        config,
        db,
        dashboard_password,
        jwt_secret,
    };

    Ok(Router::new()
        // Dashboard UI
        .route("/", get(serve_index))
        .route("/style.css", get(serve_css))
        .route("/app.js", get(serve_js))
        // Auth
        .route("/api/auth/check", get(auth::check))
        .route("/api/auth/login", post(auth::login))
        .route("/api/auth/logout", post(auth::logout))
        // API — Status & Control
        .route("/api/status", get(handlers::get_status))
        .route("/api/stats", get(handlers::get_stats))
        .route("/api/agent/pause", post(handlers::pause_agent))
        .route("/api/agent/resume", post(handlers::resume_agent))
        .route("/api/agent/tick", post(handlers::force_tick))
        // API — Approval Queue
        .route("/api/pending", get(handlers::get_pending))
        .route("/api/pending/{id}/approve", post(handlers::approve_action))
        .route("/api/pending/{id}/reject", post(handlers::reject_action))
        .route("/api/pending/approve-all", post(handlers::approve_all))
        .route("/api/pending/reject-all", post(handlers::reject_all))
        // API — Activity
        .route("/api/activity", get(handlers::get_activity))
        // API — Memory
        .route("/api/memory/core", get(handlers::get_core_memory))
        .route("/api/memory/conversation", get(handlers::get_conversation_memory))
        .route("/api/memory/archival", get(handlers::search_archival_memory))
        // API — Knowledge Graph
        .route("/api/knowledge/nodes", get(handlers::get_knowledge_nodes))
        .route("/api/knowledge/nodes/{id}", get(handlers::get_knowledge_node))
        .route("/api/knowledge/nodes/{id}/neighbors", get(handlers::get_knowledge_neighbors))
        .route("/api/knowledge/search", get(handlers::search_knowledge))
        .route("/api/knowledge/stats", get(handlers::get_knowledge_stats))
        // API — Tools
        .route("/api/tools", get(handlers::list_tools))
        // API — Chat
        .route("/api/chat", post(handlers::send_chat_message))
        // API — Skills & Credentials
        .route("/api/skills", get(handlers::list_skills))
        .route("/api/skills/{name}/credentials", get(handlers::get_skill_credentials))
        .route("/api/skills/{name}/credentials", put(handlers::set_skill_credential))
        .route("/api/skills/{name}/credentials/{key}", delete(handlers::delete_skill_credential))
        .route("/api/skills/{name}/restart", post(handlers::restart_skill))
        // API — Tunnel
        .route("/api/tunnel/status", get(handlers::tunnel_status))
        // SSE
        .route("/api/events", get(sse::events))
        // Auth middleware — applied to all routes above
        .layer(middleware::from_fn_with_state(state.clone(), auth::require_auth))
        .with_state(state))
}

async fn serve_index() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("ui/index.html"))
}

async fn serve_css() -> (axum::http::HeaderMap, &'static str) {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        "text/css".parse().unwrap(),
    );
    (headers, include_str!("ui/style.css"))
}

async fn serve_js() -> (axum::http::HeaderMap, &'static str) {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        "application/javascript".parse().unwrap(),
    );
    (headers, include_str!("ui/app.js"))
}
