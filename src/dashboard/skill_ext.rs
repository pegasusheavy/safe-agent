use std::collections::HashMap;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{Json, Response};

use super::routes::DashState;
use crate::skills::extensions::SkillExtensionInfo;

// ---------------------------------------------------------------------------
// GET /api/skills/extensions — list all skill extensions
// ---------------------------------------------------------------------------

pub async fn list_extensions(
    State(state): State<DashState>,
) -> Json<Vec<SkillExtensionInfo>> {
    let ext_mgr = state.extension_manager.lock().await;
    Json(ext_mgr.list_extensions())
}

// ---------------------------------------------------------------------------
// GET/POST /api/skills/{name}/ext/{*path} — dynamic Rhai route dispatch
// ---------------------------------------------------------------------------

pub async fn skill_ext_handler(
    State(state): State<DashState>,
    Path((skill_name, ext_path)): Path<(String, String)>,
    Query(query_params): Query<HashMap<String, String>>,
    headers: HeaderMap,
    method: axum::http::Method,
    body: String,
) -> Response<Body> {
    let ext_mgr = state.extension_manager.lock().await;

    let path = format!("/{}", ext_path.trim_start_matches('/'));

    let mut header_map = HashMap::new();
    for (key, value) in headers.iter() {
        if let Ok(v) = value.to_str() {
            header_map.insert(key.to_string(), v.to_string());
        }
    }

    match ext_mgr
        .handle_request(
            &skill_name,
            method.as_str(),
            &path,
            &query_params,
            &body,
            &header_map,
        )
        .await
    {
        Ok(rhai_resp) => {
            let status = StatusCode::from_u16(rhai_resp.status).unwrap_or(StatusCode::OK);
            let mut builder = Response::builder().status(status);

            builder = builder.header(header::CONTENT_TYPE, &rhai_resp.content_type);
            for (k, v) in &rhai_resp.headers {
                builder = builder.header(k.as_str(), v.as_str());
            }

            builder
                .body(Body::from(rhai_resp.body))
                .unwrap_or_else(|_| {
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::from("response build error"))
                        .unwrap()
                })
        }
        Err(e) => {
            let body = serde_json::json!({"error": e}).to_string();
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .unwrap()
        }
    }
}

// ---------------------------------------------------------------------------
// GET /skills/{name}/ui/{*path} — serve static files from skill directories
// ---------------------------------------------------------------------------

pub async fn skill_static_file(
    State(state): State<DashState>,
    Path((skill_name, file_path)): Path<(String, String)>,
) -> Response<Body> {
    let ext_mgr = state.extension_manager.lock().await;

    let ui_path = format!("ui/{}", file_path.trim_start_matches('/'));

    match ext_mgr.read_static_file(&skill_name, &ui_path) {
        Some((content, content_type)) => {
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, content_type)
                .header(header::CACHE_CONTROL, "public, max-age=60")
                .body(Body::from(content))
                .unwrap()
        }
        None => {
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .header(header::CONTENT_TYPE, "text/plain")
                .body(Body::from("not found"))
                .unwrap()
        }
    }
}

// ---------------------------------------------------------------------------
// GET /skills/{name}/page — serve the skill's full-page UI
// ---------------------------------------------------------------------------

pub async fn skill_page(
    State(state): State<DashState>,
    Path(skill_name): Path<String>,
) -> Response<Body> {
    let ext_mgr = state.extension_manager.lock().await;

    let ext = match ext_mgr.get_extension(&skill_name) {
        Some(e) => e,
        None => {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("skill not found"))
                .unwrap();
        }
    };

    let page_path = match &ext.ui.page {
        Some(p) => p.clone(),
        None => {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("skill has no page"))
                .unwrap();
        }
    };

    match ext_mgr.read_static_file(&skill_name, &page_path) {
        Some((content, content_type)) => {
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(content))
                .unwrap()
        }
        None => {
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("page file not found"))
                .unwrap()
        }
    }
}
