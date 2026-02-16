use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Json, Redirect};
use serde::{Deserialize, Serialize};
use tracing::error;

use super::routes::DashState;
use crate::memory::knowledge::KnowledgeGraph;

#[derive(Serialize)]
pub struct StatusResponse {
    pub running: bool,
    pub paused: bool,
    pub agent_name: String,
    pub dashboard_bind: String,
    pub tick_interval_secs: u64,
    pub tools_count: usize,
}

#[derive(Serialize)]
pub struct ActionResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
}

#[derive(Deserialize)]
pub struct PaginationQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
}

#[derive(Deserialize)]
pub struct GoogleCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

// -- Status & Control ----------------------------------------------------

pub async fn get_status(State(state): State<DashState>) -> Json<StatusResponse> {
    Json(StatusResponse {
        running: true,
        paused: state.agent.is_paused(),
        agent_name: state.agent.config.agent_name.clone(),
        dashboard_bind: state.agent.config.dashboard_bind.clone(),
        tick_interval_secs: state.agent.config.tick_interval_secs,
        tools_count: state.agent.tools.len(),
    })
}

pub async fn get_stats(
    State(state): State<DashState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    state
        .agent
        .memory
        .get_stats()
        .await
        .map(|stats| Json(serde_json::to_value(stats).unwrap()))
        .map_err(|e| {
            error!("stats: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn pause_agent(State(state): State<DashState>) -> Json<ActionResponse> {
    state.agent.pause();
    state.agent.notify_update();
    Json(ActionResponse {
        ok: true,
        message: Some("agent paused".into()),
        count: None,
    })
}

pub async fn resume_agent(State(state): State<DashState>) -> Json<ActionResponse> {
    state.agent.resume();
    state.agent.notify_update();
    Json(ActionResponse {
        ok: true,
        message: Some("agent resumed".into()),
        count: None,
    })
}

pub async fn force_tick(
    State(state): State<DashState>,
) -> Result<Json<ActionResponse>, StatusCode> {
    state
        .agent
        .force_tick()
        .await
        .map(|_| {
            state.agent.notify_update();
            Json(ActionResponse {
                ok: true,
                message: Some("tick completed".into()),
                count: None,
            })
        })
        .map_err(|e| {
            error!("force tick: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

// -- Approval Queue ------------------------------------------------------

pub async fn get_pending(
    State(state): State<DashState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    state
        .agent
        .approval_queue
        .list_pending()
        .await
        .map(|actions| Json(serde_json::to_value(actions).unwrap()))
        .map_err(|e| {
            error!("list pending: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn approve_action(
    State(state): State<DashState>,
    Path(id): Path<String>,
) -> Result<Json<ActionResponse>, StatusCode> {
    state
        .agent
        .approval_queue
        .approve(&id)
        .await
        .map(|_| {
            state.agent.notify_update();
            Json(ActionResponse {
                ok: true,
                message: Some(format!("approved {id}")),
                count: None,
            })
        })
        .map_err(|e| {
            error!("approve: {e}");
            StatusCode::BAD_REQUEST
        })
}

pub async fn reject_action(
    State(state): State<DashState>,
    Path(id): Path<String>,
) -> Result<Json<ActionResponse>, StatusCode> {
    state
        .agent
        .approval_queue
        .reject(&id)
        .await
        .map(|_| {
            state.agent.notify_update();
            Json(ActionResponse {
                ok: true,
                message: Some(format!("rejected {id}")),
                count: None,
            })
        })
        .map_err(|e| {
            error!("reject: {e}");
            StatusCode::BAD_REQUEST
        })
}

pub async fn approve_all(
    State(state): State<DashState>,
) -> Result<Json<ActionResponse>, StatusCode> {
    state
        .agent
        .approval_queue
        .approve_all()
        .await
        .map(|count| {
            state.agent.notify_update();
            Json(ActionResponse {
                ok: true,
                message: None,
                count: Some(count),
            })
        })
        .map_err(|e| {
            error!("approve_all: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn reject_all(
    State(state): State<DashState>,
) -> Result<Json<ActionResponse>, StatusCode> {
    state
        .agent
        .approval_queue
        .reject_all()
        .await
        .map(|count| {
            state.agent.notify_update();
            Json(ActionResponse {
                ok: true,
                message: None,
                count: Some(count),
            })
        })
        .map_err(|e| {
            error!("reject_all: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

// -- Activity ------------------------------------------------------------

pub async fn get_activity(
    State(state): State<DashState>,
    Query(params): Query<PaginationQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);
    state
        .agent
        .memory
        .recent_activity(limit, offset)
        .await
        .map(|entries| Json(serde_json::to_value(entries).unwrap()))
        .map_err(|e| {
            error!("activity: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

// -- Memory --------------------------------------------------------------

pub async fn get_core_memory(
    State(state): State<DashState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    state
        .agent
        .memory
        .core
        .get()
        .await
        .map(|personality| Json(serde_json::json!({ "personality": personality })))
        .map_err(|e| {
            error!("core memory: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn get_conversation_memory(
    State(state): State<DashState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    state
        .agent
        .memory
        .conversation
        .recent()
        .await
        .map(|messages| Json(serde_json::to_value(messages).unwrap()))
        .map_err(|e| {
            error!("conversation memory: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn search_archival_memory(
    State(state): State<DashState>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let query = params.q.unwrap_or_default();
    if query.is_empty() {
        return state
            .agent
            .memory
            .archival
            .list(0, 50)
            .await
            .map(|entries| Json(serde_json::to_value(entries).unwrap()))
            .map_err(|e| {
                error!("archival list: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            });
    }
    state
        .agent
        .memory
        .archival
        .search(&query, 50)
        .await
        .map(|entries| Json(serde_json::to_value(entries).unwrap()))
        .map_err(|e| {
            error!("archival search: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

// -- Knowledge Graph -----------------------------------------------------

pub async fn get_knowledge_nodes(
    State(state): State<DashState>,
    Query(params): Query<PaginationQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let limit = params.limit.unwrap_or(50) as i64;
    let offset = params.offset.unwrap_or(0) as i64;
    let db = state.db.lock().await;
    let mut stmt = db
        .prepare(
            "SELECT id, label, node_type, content, confidence, created_at, updated_at
             FROM knowledge_nodes ORDER BY updated_at DESC LIMIT ?1 OFFSET ?2",
        )
        .map_err(|e| {
            error!("knowledge nodes: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let nodes: Vec<serde_json::Value> = stmt
        .query_map(rusqlite::params![limit, offset], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, i64>(0)?,
                "label": row.get::<_, String>(1)?,
                "node_type": row.get::<_, String>(2)?,
                "content": row.get::<_, String>(3)?,
                "confidence": row.get::<_, f64>(4)?,
                "created_at": row.get::<_, String>(5)?,
                "updated_at": row.get::<_, String>(6)?,
            }))
        })
        .map_err(|e| {
            error!("knowledge nodes: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(Json(serde_json::to_value(nodes).unwrap()))
}

pub async fn get_knowledge_node(
    State(state): State<DashState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let kg = KnowledgeGraph::new(state.db.clone());
    let node = kg.get_node(id).await.map_err(|e| {
        error!("knowledge node {id}: {e}");
        StatusCode::NOT_FOUND
    })?;
    Ok(Json(serde_json::to_value(node).unwrap()))
}

pub async fn get_knowledge_neighbors(
    State(state): State<DashState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let kg = KnowledgeGraph::new(state.db.clone());
    let neighbors = kg.neighbors(id, None).await.map_err(|e| {
        error!("knowledge neighbors: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let result: Vec<serde_json::Value> = neighbors
        .iter()
        .map(|(edge, node)| {
            serde_json::json!({
                "edge": {
                    "id": edge.id,
                    "relation": edge.relation,
                    "weight": edge.weight,
                    "source_id": edge.source_id,
                    "target_id": edge.target_id,
                },
                "node": {
                    "id": node.id,
                    "label": node.label,
                    "node_type": node.node_type,
                    "confidence": node.confidence,
                }
            })
        })
        .collect();
    Ok(Json(serde_json::to_value(result).unwrap()))
}

pub async fn search_knowledge(
    State(state): State<DashState>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let query = params.q.unwrap_or_default();
    if query.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let kg = KnowledgeGraph::new(state.db.clone());
    let nodes = kg.search(&query, 50).await.map_err(|e| {
        error!("knowledge search: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(serde_json::to_value(nodes).unwrap()))
}

pub async fn get_knowledge_stats(
    State(state): State<DashState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let kg = KnowledgeGraph::new(state.db.clone());
    let (nodes, edges) = kg.stats().await.map_err(|e| {
        error!("knowledge stats: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(serde_json::json!({ "nodes": nodes, "edges": edges })))
}

// -- Google OAuth --------------------------------------------------------

pub async fn google_auth_redirect(
    State(state): State<DashState>,
) -> Result<Redirect, StatusCode> {
    if !state.config.google.enabled {
        return Err(StatusCode::NOT_FOUND);
    }
    let (url, _state_param) = crate::google::oauth::authorization_url(&state.config).map_err(|e| {
        error!("google auth url: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Redirect::temporary(&url))
}

pub async fn google_auth_callback(
    State(state): State<DashState>,
    Query(params): Query<GoogleCallbackQuery>,
) -> Result<axum::response::Html<String>, StatusCode> {
    if let Some(err) = params.error {
        return Ok(axum::response::Html(format!(
            "<h1>Google OAuth Error</h1><p>{err}</p><a href=\"/\">Back to dashboard</a>"
        )));
    }

    let code = params.code.ok_or(StatusCode::BAD_REQUEST)?;
    let http = reqwest::Client::new();

    crate::google::oauth::exchange_code(&state.config, &code, &state.db, &http)
        .await
        .map_err(|e| {
            error!("google token exchange: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(axum::response::Html(
        "<h1>Google Connected!</h1><p>You can close this window.</p><script>window.close()</script>".to_string(),
    ))
}

pub async fn google_status(
    State(state): State<DashState>,
) -> Json<serde_json::Value> {
    let connected = crate::google::oauth::is_connected(&state.db).await;
    Json(serde_json::json!({
        "enabled": state.config.google.enabled,
        "connected": connected,
    }))
}

pub async fn google_disconnect(
    State(state): State<DashState>,
) -> Result<Json<ActionResponse>, StatusCode> {
    let http = reqwest::Client::new();
    crate::google::oauth::disconnect(&state.db, &http)
        .await
        .map_err(|e| {
            error!("google disconnect: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(ActionResponse {
        ok: true,
        message: Some("Google disconnected".into()),
        count: None,
    }))
}

// -- Tools ---------------------------------------------------------------

pub async fn list_tools(
    State(state): State<DashState>,
) -> Json<serde_json::Value> {
    let tools: Vec<serde_json::Value> = state
        .agent
        .tools
        .list()
        .iter()
        .map(|(name, desc)| {
            serde_json::json!({
                "name": name,
                "description": desc,
            })
        })
        .collect();
    Json(serde_json::to_value(tools).unwrap())
}

// -- Skills & Credentials ------------------------------------------------

pub async fn list_skills(
    State(state): State<DashState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let sm = state.agent.skill_manager.lock().await;
    let skills = sm.list();
    Ok(Json(serde_json::to_value(skills).unwrap()))
}

#[derive(Deserialize)]
pub struct SetCredentialBody {
    pub key: String,
    pub value: String,
}

pub async fn get_skill_credentials(
    State(state): State<DashState>,
    Path(skill_name): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let sm = state.agent.skill_manager.lock().await;
    let creds = sm.get_credentials(&skill_name);
    // Return keys + whether they have values, but never expose raw secret values
    let masked: Vec<serde_json::Value> = creds
        .keys()
        .map(|k| {
            serde_json::json!({
                "key": k,
                "has_value": true,
            })
        })
        .collect();
    Ok(Json(serde_json::to_value(masked).unwrap()))
}

pub async fn set_skill_credential(
    State(state): State<DashState>,
    Path(skill_name): Path<String>,
    Json(body): Json<SetCredentialBody>,
) -> Result<Json<ActionResponse>, StatusCode> {
    let mut sm = state.agent.skill_manager.lock().await;
    sm.set_credential(&skill_name, &body.key, &body.value)
        .map_err(|e| {
            error!("set credential: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(ActionResponse {
        ok: true,
        message: Some(format!("credential '{}' set for '{}'", body.key, skill_name)),
        count: None,
    }))
}

pub async fn delete_skill_credential(
    State(state): State<DashState>,
    Path((skill_name, key)): Path<(String, String)>,
) -> Result<Json<ActionResponse>, StatusCode> {
    let mut sm = state.agent.skill_manager.lock().await;
    sm.delete_credential(&skill_name, &key)
        .map_err(|e| {
            error!("delete credential: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(ActionResponse {
        ok: true,
        message: Some(format!("credential '{}' removed from '{}'", key, skill_name)),
        count: None,
    }))
}

pub async fn restart_skill(
    State(state): State<DashState>,
    Path(skill_name): Path<String>,
) -> Result<Json<ActionResponse>, StatusCode> {
    let mut sm = state.agent.skill_manager.lock().await;
    sm.stop_skill(&skill_name).await;
    drop(sm);
    // Short delay so process cleanup completes
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let mut sm = state.agent.skill_manager.lock().await;
    let _ = sm.reconcile().await;
    Ok(Json(ActionResponse {
        ok: true,
        message: Some(format!("skill '{}' restarted", skill_name)),
        count: None,
    }))
}

// -- Chat ----------------------------------------------------------------

#[derive(Deserialize)]
pub struct ChatMessageBody {
    pub message: String,
}

#[derive(Serialize)]
pub struct ChatResponse {
    pub reply: String,
    pub timestamp: String,
}

pub async fn send_chat_message(
    State(state): State<DashState>,
    Json(body): Json<ChatMessageBody>,
) -> Result<Json<ChatResponse>, StatusCode> {
    let message = body.message.trim().to_string();
    if message.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let reply = state
        .agent
        .handle_message(&message)
        .await
        .map_err(|e| {
            error!("chat: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let timestamp = chrono::Utc::now().to_rfc3339();

    Ok(Json(ChatResponse { reply, timestamp }))
}
