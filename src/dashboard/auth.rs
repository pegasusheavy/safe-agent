use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Json, Response};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use super::routes::DashState;

const COOKIE_NAME: &str = "sa_token";

/// JWT expiry: 7 days (in seconds).
const TOKEN_EXPIRY_SECS: u64 = 7 * 24 * 60 * 60;

/// JWT claims embedded in the token.
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    /// Subject — always "dashboard".
    sub: String,
    /// Issued-at (unix timestamp).
    iat: u64,
    /// Expiry (unix timestamp).
    exp: u64,
}

/// Extract and validate the JWT from the request's cookie header.
fn validate_token(req: &Request<Body>, secret: &[u8]) -> bool {
    let Some(cookie_header) = req.headers().get(axum::http::header::COOKIE) else {
        return false;
    };
    let Ok(cookies) = cookie_header.to_str() else {
        return false;
    };

    for pair in cookies.split(';') {
        let pair = pair.trim();
        if let Some(token) = pair.strip_prefix(&format!("{COOKIE_NAME}=")) {
            let key = DecodingKey::from_secret(secret);
            let mut validation = Validation::new(jsonwebtoken::Algorithm::HS256);
            validation.set_required_spec_claims(&["sub", "exp", "iat"]);
            validation.validate_exp = true;

            if decode::<Claims>(token, &key, &validation).is_ok() {
                return true;
            }
        }
    }

    false
}

/// Mint a new JWT signed with the server's secret.
fn mint_token(secret: &[u8]) -> Result<String, jsonwebtoken::errors::Error> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let claims = Claims {
        sub: "dashboard".to_string(),
        iat: now,
        exp: now + TOKEN_EXPIRY_SECS,
    };

    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret))
}

// ---------------------------------------------------------------------------
// Middleware
// ---------------------------------------------------------------------------

/// Middleware that enforces JWT authentication on all API routes.
///
/// Always passes through: static assets (`/`, `/style.css`, `/app.js`)
/// and auth endpoints (`/api/auth/*`).
pub async fn require_auth(
    State(state): State<DashState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let path = req.uri().path();

    if path == "/"
        || path == "/style.css"
        || path == "/app.js"
        || path.starts_with("/api/auth/")
    {
        return next.run(req).await;
    }

    if validate_token(&req, &state.jwt_secret) {
        return next.run(req).await;
    }

    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "error": "unauthorized" })),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// Endpoints
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct LoginBody {
    pub password: String,
}

/// POST /api/auth/login — validate password, return JWT in an HttpOnly cookie.
pub async fn login(
    State(state): State<DashState>,
    Json(body): Json<LoginBody>,
) -> Response {
    if body.password != state.dashboard_password {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "ok": false, "error": "invalid password" })),
        )
            .into_response();
    }

    let token = match mint_token(&state.jwt_secret) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("failed to mint JWT: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "ok": false, "error": "internal error" })),
            )
                .into_response();
        }
    };

    let cookie = format!(
        "{COOKIE_NAME}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={TOKEN_EXPIRY_SECS}"
    );

    let mut headers = axum::http::HeaderMap::new();
    headers.insert(axum::http::header::SET_COOKIE, cookie.parse().unwrap());

    (headers, Json(serde_json::json!({ "ok": true }))).into_response()
}

/// POST /api/auth/logout — clear the JWT cookie.
pub async fn logout() -> Response {
    let cookie = format!("{COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0");
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(axum::http::header::SET_COOKIE, cookie.parse().unwrap());
    (headers, Json(serde_json::json!({ "ok": true }))).into_response()
}

/// GET /api/auth/check — report whether the current request carries a valid JWT.
pub async fn check(
    State(state): State<DashState>,
    req: Request<Body>,
) -> Json<serde_json::Value> {
    let authenticated = validate_token(&req, &state.jwt_secret);
    Json(serde_json::json!({ "required": true, "authenticated": authenticated }))
}
