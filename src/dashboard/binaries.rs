use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

use super::routes::DashState;

/// GET /api/binaries — list all known binaries with install status.
pub async fn list_binaries(State(state): State<DashState>) -> impl IntoResponse {
    let binaries = state.installer.list();
    Json(binaries)
}

/// GET /api/binaries/{name} — get status of a specific binary.
pub async fn get_binary(
    State(state): State<DashState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match state.installer.get(&name) {
        Some(info) => Ok(Json(info)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

/// POST /api/binaries/{name} — install a binary (runs in background).
pub async fn install_binary(
    State(state): State<DashState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    // Validate the name exists in registry
    if state.installer.get(&name).is_none() {
        return (StatusCode::NOT_FOUND, "unknown binary".to_string());
    }

    let installer = state.installer.clone();
    let binary_name = name.clone();

    // Spawn background task
    tokio::spawn(async move {
        if let Err(e) = installer.install(&binary_name).await {
            tracing::error!(name = %binary_name, err = %e, "background install failed");
        }
    });

    (StatusCode::ACCEPTED, format!("installing {name}"))
}

/// DELETE /api/binaries/{name} — uninstall a binary.
pub async fn uninstall_binary(
    State(state): State<DashState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match state.installer.uninstall(&name).await {
        Ok(()) => Ok((StatusCode::OK, format!("{name} uninstalled"))),
        Err(e) => Err((StatusCode::BAD_REQUEST, e.to_string())),
    }
}
