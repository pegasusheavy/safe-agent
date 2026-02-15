use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use rusqlite::Connection;
use tokio::sync::Mutex;

use crate::agent::Agent;
use crate::config::Config;

use super::handlers;
use super::sse;

/// State shared across all routes.
#[derive(Clone)]
pub struct DashState {
    pub agent: Arc<Agent>,
    pub config: Config,
    pub db: Arc<Mutex<Connection>>,
}

pub fn build(agent: Arc<Agent>, config: Config, db: Arc<Mutex<Connection>>) -> Router {
    let state = DashState {
        agent,
        config,
        db,
    };

    Router::new()
        // Dashboard UI
        .route("/", get(serve_index))
        .route("/style.css", get(serve_css))
        .route("/app.js", get(serve_js))
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
        // API — Google OAuth
        .route("/auth/google", get(handlers::google_auth_redirect))
        .route("/auth/google/callback", get(handlers::google_auth_callback))
        .route("/api/google/status", get(handlers::google_status))
        .route("/auth/google/disconnect", post(handlers::google_disconnect))
        // API — Tools
        .route("/api/tools", get(handlers::list_tools))
        // SSE
        .route("/api/events", get(sse::events))
        .with_state(state)
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
