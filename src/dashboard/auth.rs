use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Json, Redirect, Response};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

use super::authn;
use super::oauth;
use super::routes::DashState;

// ---------------------------------------------------------------------------
// Login rate limiter
// ---------------------------------------------------------------------------

/// Per-IP login attempt tracker.
struct AttemptTracker {
    count: u32,
    window_start: std::time::Instant,
    locked_until: Option<std::time::Instant>,
}

/// In-memory rate limiter for login attempts.
///
/// Thresholds (within a 15-minute window):
/// *  5 failures → 30 s lockout
/// * 10 failures → 5 min lockout
/// * 20 failures → 1 hr lockout
///
/// Successful login resets the counter for that IP.
pub struct LoginRateLimiter {
    attempts: tokio::sync::Mutex<std::collections::HashMap<std::net::IpAddr, AttemptTracker>>,
}

const RATE_WINDOW: std::time::Duration = std::time::Duration::from_secs(15 * 60);

impl LoginRateLimiter {
    pub fn new() -> Self {
        Self {
            attempts: tokio::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Record a failed login attempt.  Returns the lockout duration (if any).
    pub async fn record_failure(&self, ip: std::net::IpAddr) -> Option<std::time::Duration> {
        let mut map = self.attempts.lock().await;
        let now = std::time::Instant::now();

        let entry = map.entry(ip).or_insert_with(|| AttemptTracker {
            count: 0,
            window_start: now,
            locked_until: None,
        });

        // Reset if window has expired
        if now.duration_since(entry.window_start) > RATE_WINDOW {
            entry.count = 0;
            entry.window_start = now;
            entry.locked_until = None;
        }

        entry.count += 1;

        let lockout = if entry.count >= 20 {
            Some(std::time::Duration::from_secs(3600))
        } else if entry.count >= 10 {
            Some(std::time::Duration::from_secs(300))
        } else if entry.count >= 5 {
            Some(std::time::Duration::from_secs(30))
        } else {
            None
        };

        if let Some(dur) = lockout {
            entry.locked_until = Some(now + dur);
        }

        lockout
    }

    /// Check if an IP is currently locked out.  Returns remaining lockout
    /// duration, or `None` if the IP is allowed to attempt login.
    pub async fn check(&self, ip: std::net::IpAddr) -> Option<std::time::Duration> {
        let map = self.attempts.lock().await;
        let now = std::time::Instant::now();

        if let Some(entry) = map.get(&ip) {
            if let Some(until) = entry.locked_until {
                if now < until {
                    return Some(until - now);
                }
            }
        }
        None
    }

    /// Reset the counter after a successful login.
    pub async fn reset(&self, ip: std::net::IpAddr) {
        self.attempts.lock().await.remove(&ip);
    }
}

const COOKIE_NAME: &str = "sa_token";

/// JWT expiry: 7 days (in seconds).
const TOKEN_EXPIRY_SECS: u64 = 7 * 24 * 60 * 60;

/// JWT claims embedded in the token.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Claims {
    /// Subject — "dashboard" for password, the email for SSO, or a username.
    sub: String,
    /// Issued-at (unix timestamp).
    iat: u64,
    /// Expiry (unix timestamp).
    exp: u64,
    /// Login method: "password" or "sso:{provider}".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    method: Option<String>,
    /// User ID (multi-user mode). None for legacy single-user sessions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    user_id: Option<String>,
    /// User role. None for legacy sessions (treated as admin).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    /// CSRF token — must be echoed back via `X-CSRF-Token` header on
    /// state-mutating requests (POST/PUT/DELETE).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    csrf: Option<String>,
}

/// Returns `true` when the dashboard is reachable over HTTPS.
///
/// Checks the `TUNNEL_URL` env var for an `https://` prefix; if not set
/// (or not HTTPS) returns `false`.  Used to decide whether cookies need
/// the `Secure` flag and `SameSite=Strict`.
fn is_https() -> bool {
    std::env::var("TUNNEL_URL")
        .map(|url| url.starts_with("https://"))
        .unwrap_or(false)
}

/// Build a `Set-Cookie` string with correct security flags.
///
/// * Always: `HttpOnly`, `Path=/`
/// * HTTPS detected: `Secure; SameSite=Strict`
/// * HTTP only:      `SameSite=Lax`
fn make_session_cookie(token: &str) -> String {
    let secure = is_https();
    let same_site = if secure { "Strict" } else { "Lax" };
    let mut cookie = format!(
        "{COOKIE_NAME}={token}; Path=/; HttpOnly; SameSite={same_site}; Max-Age={TOKEN_EXPIRY_SECS}"
    );
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

/// Extract and validate the JWT from the request's cookie header.
/// Returns the decoded claims if valid, or None.
fn extract_claims(req: &Request<Body>, secret: &[u8]) -> Option<Claims> {
    let cookie_header = req.headers().get(axum::http::header::COOKIE)?;
    let cookies = cookie_header.to_str().ok()?;

    for pair in cookies.split(';') {
        let pair = pair.trim();
        if let Some(token) = pair.strip_prefix(&format!("{COOKIE_NAME}=")) {
            let key = DecodingKey::from_secret(secret);
            let mut validation = Validation::new(jsonwebtoken::Algorithm::HS256);
            validation.set_required_spec_claims(&["sub", "exp", "iat"]);
            validation.validate_exp = true;

            if let Ok(data) = decode::<Claims>(token, &key, &validation) {
                return Some(data.claims);
            }
        }
    }

    None
}

/// Backward-compatible: returns true if there's any valid JWT.
fn validate_token(req: &Request<Body>, secret: &[u8]) -> bool {
    extract_claims(req, secret).is_some()
}

/// Mint a new JWT signed with the server's secret.
fn mint_token(secret: &[u8], subject: &str, method: &str) -> Result<String, jsonwebtoken::errors::Error> {
    mint_token_with_user(secret, subject, method, None, None)
}

/// Mint a JWT with user identity and CSRF token embedded.
fn mint_token_with_user(
    secret: &[u8],
    subject: &str,
    method: &str,
    user_id: Option<&str>,
    role: Option<&str>,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let csrf_bytes: [u8; 32] = rand::random();
    let csrf_token: String = csrf_bytes.iter().map(|b| format!("{b:02x}")).collect();

    let claims = Claims {
        sub: subject.to_string(),
        iat: now,
        exp: now + TOKEN_EXPIRY_SECS,
        method: Some(method.to_string()),
        user_id: user_id.map(|s| s.to_string()),
        role: role.map(|s| s.to_string()),
        csrf: Some(csrf_token),
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
        || path.starts_with("/oauth/")
        || path.starts_with("/skills/")
        || path == "/api/messaging/incoming"
    {
        return next.run(req).await;
    }

    let Some(claims) = extract_claims(&req, &state.jwt_secret) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "unauthorized" })),
        )
            .into_response();
    };

    // CSRF check: POST/PUT/DELETE must carry X-CSRF-Token matching the JWT claim
    let method = req.method().clone();
    if method == axum::http::Method::POST
        || method == axum::http::Method::PUT
        || method == axum::http::Method::DELETE
    {
        if let Some(ref expected_csrf) = claims.csrf {
            let header_csrf = req
                .headers()
                .get("X-CSRF-Token")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if header_csrf != expected_csrf {
                return (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({ "error": "CSRF token mismatch" })),
                )
                    .into_response();
            }
        }
    }

    next.run(req).await
}

// ---------------------------------------------------------------------------
// Endpoints
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Security headers middleware
// ---------------------------------------------------------------------------

/// Middleware that appends defensive HTTP headers to every response.
pub async fn security_headers(req: Request<Body>, next: Next) -> Response {
    let mut resp = next.run(req).await;
    let headers = resp.headers_mut();

    headers.insert(
        axum::http::header::X_FRAME_OPTIONS,
        "DENY".parse().unwrap(),
    );
    headers.insert(
        axum::http::header::X_CONTENT_TYPE_OPTIONS,
        "nosniff".parse().unwrap(),
    );
    headers.insert(
        axum::http::header::REFERRER_POLICY,
        "strict-origin-when-cross-origin".parse().unwrap(),
    );
    // Disable browser XSS auditor (modern best practice)
    headers.insert(
        axum::http::header::HeaderName::from_static("x-xss-protection"),
        "0".parse().unwrap(),
    );
    headers.insert(
        axum::http::header::CONTENT_SECURITY_POLICY,
        "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'"
            .parse()
            .unwrap(),
    );
    if is_https() {
        headers.insert(
            axum::http::header::STRICT_TRANSPORT_SECURITY,
            "max-age=31536000; includeSubDomains".parse().unwrap(),
        );
    }
    resp
}

/// Extract the client IP from proxy headers, falling back to loopback.
fn client_ip(headers: &axum::http::HeaderMap) -> std::net::IpAddr {
    if let Some(xff) = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
    {
        if let Some(ip) = xff.split(',').next().and_then(|s| s.trim().parse().ok()) {
            return ip;
        }
    }
    if let Some(real_ip) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
        if let Ok(ip) = real_ip.trim().parse() {
            return ip;
        }
    }
    std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
}

#[derive(Deserialize)]
pub struct LoginBody {
    pub password: String,
    /// Optional username for multi-user login.
    #[serde(default)]
    pub username: Option<String>,
}

/// POST /api/auth/login — validate password, return JWT in an HttpOnly cookie.
///
/// Supports two modes:
/// 1. **Legacy**: just `password` → checks against dashboard_password config.
/// 2. **Multi-user**: `username` + `password` → authenticates against the users table.
///
/// Rate-limited: 5 failures → 30 s lockout, 10 → 5 min, 20 → 1 hr.
pub async fn login(
    State(state): State<DashState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<LoginBody>,
) -> Response {
    // Rate-limit check
    let ip = client_ip(&headers);
    if let Some(remaining) = state.login_limiter.check(ip).await {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "ok": false,
                "error": "too many login attempts",
                "retry_after_secs": remaining.as_secs(),
            })),
        )
            .into_response();
    }

    if !state.config.dashboard.password_enabled {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "ok": false, "error": "password login is disabled" })),
        )
            .into_response();
    }

    // Multi-user login: username + password
    if let Some(ref username) = body.username {
        let user = state.agent.user_manager.authenticate(username, &body.password).await;
        match user {
            Some(u) => {
                // Check if 2FA is required before issuing a full session JWT.
                let totp_enabled = state.agent.user_manager.is_totp_enabled(&u.id).await;
                let has_passkeys = if let Some(ref pm) = state.passkey_manager {
                    pm.has_passkeys(&u.id).await
                } else {
                    false
                };

                if totp_enabled || has_passkeys {
                    // Mint a short-lived challenge token (not a session)
                    let challenge = match authn::mint_challenge_token(&state.jwt_secret, &u.id) {
                        Ok(t) => t,
                        Err(e) => {
                            error!("failed to mint 2FA challenge token: {e}");
                            return (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(serde_json::json!({ "ok": false, "error": "internal error" })),
                            ).into_response();
                        }
                    };

                    let mut methods = Vec::new();
                    if totp_enabled { methods.push("totp"); }
                    if has_passkeys { methods.push("passkey"); }

                    info!(username = %u.username, "2FA challenge issued");
                    return Json(serde_json::json!({
                        "ok": false,
                        "requires_2fa": true,
                        "challenge_token": challenge,
                        "methods": methods,
                        "user_id": u.id,
                    })).into_response();
                }

                // No 2FA — issue full session JWT directly
                let token = match mint_token_with_user(
                    &state.jwt_secret,
                    &u.username,
                    "password",
                    Some(&u.id),
                    Some(u.role.as_str()),
                ) {
                    Ok(t) => t,
                    Err(e) => {
                        error!("failed to mint JWT: {e}");
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(serde_json::json!({ "ok": false, "error": "internal error" })),
                        ).into_response();
                    }
                };

                let cookie = make_session_cookie(&token);
                let mut headers = axum::http::HeaderMap::new();
                headers.insert(axum::http::header::SET_COOKIE, cookie.parse().unwrap());

                state.login_limiter.reset(ip).await;
                info!(username = %u.username, role = %u.role, "user login successful");
                return (headers, Json(serde_json::json!({
                    "ok": true,
                    "user": {
                        "id": u.id,
                        "username": u.username,
                        "display_name": u.display_name,
                        "role": u.role,
                    }
                }))).into_response();
            }
            None => {
                warn!(username, "multi-user login failed");
                state.login_limiter.record_failure(ip).await;
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({ "ok": false, "error": "invalid username or password" })),
                ).into_response();
            }
        }
    }

    // Legacy single-password login
    if state.dashboard_password.is_empty() || body.password != state.dashboard_password {
        state.login_limiter.record_failure(ip).await;
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "ok": false, "error": "invalid password" })),
        )
            .into_response();
    }

    let token = match mint_token(&state.jwt_secret, "dashboard", "password") {
        Ok(t) => t,
        Err(e) => {
            error!("failed to mint JWT: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "ok": false, "error": "internal error" })),
            )
                .into_response();
        }
    };

    state.login_limiter.reset(ip).await;

    let cookie = make_session_cookie(&token);

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
/// Also returns user identity if available (multi-user mode).
pub async fn check(
    State(state): State<DashState>,
    req: Request<Body>,
) -> Json<serde_json::Value> {
    match extract_claims(&req, &state.jwt_secret) {
        Some(claims) => {
            let mut resp = serde_json::json!({
                "required": true,
                "authenticated": true,
                "subject": claims.sub,
                "method": claims.method,
            });
            if let Some(ref uid) = claims.user_id {
                resp["user_id"] = serde_json::json!(uid);
            }
            if let Some(ref role) = claims.role {
                resp["role"] = serde_json::json!(role);
            }
            if let Some(ref csrf) = claims.csrf {
                resp["csrf_token"] = serde_json::json!(csrf);
            }
            Json(resp)
        }
        None => {
            Json(serde_json::json!({ "required": true, "authenticated": false }))
        }
    }
}

// ---------------------------------------------------------------------------
// GET /api/auth/info — describes available login methods for the frontend
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct SsoProviderInfo {
    id: String,
    name: String,
    icon: String,
    login_url: String,
}

pub async fn login_info(
    State(state): State<DashState>,
) -> Json<serde_json::Value> {
    let password_enabled = state.config.dashboard.password_enabled
        && !state.dashboard_password.is_empty();

    let sso_providers: Vec<SsoProviderInfo> = state
        .config
        .dashboard
        .sso_providers
        .iter()
        .filter_map(|id| {
            let provider = oauth::find_provider(id)?;
            // Only show providers that have client credentials configured
            let _ = sso_client_credentials(&state, provider)?;
            Some(SsoProviderInfo {
                id: provider.id.to_string(),
                name: provider.name.to_string(),
                icon: provider.icon.to_string(),
                login_url: format!("/api/auth/sso/{}/start", provider.id),
            })
        })
        .collect();

    // Check if there are any registered users (multi-user mode)
    let user_count = state.agent.user_manager.count().await;

    Json(serde_json::json!({
        "password_enabled": password_enabled,
        "sso_providers": sso_providers,
        "multi_user": user_count > 0,
        "passkeys_available": state.passkey_manager.is_some(),
    }))
}

// ---------------------------------------------------------------------------
// SSO OAuth flow for dashboard login
// ---------------------------------------------------------------------------

/// Resolve client credentials for an SSO provider. Checks env vars and
/// the skill credential store (same as the main OAuth module).
fn sso_client_credentials(
    state: &DashState,
    provider: &oauth::OAuthProvider,
) -> Option<(String, String)> {
    // Try env vars first
    if let (Ok(id), Ok(secret)) = (
        std::env::var(provider.client_id_env),
        std::env::var(provider.client_secret_env),
    ) {
        if !id.is_empty() && !secret.is_empty() {
            return Some((id, secret));
        }
    }

    // Fall back to skill credential store
    let skill_name = format!("{}-oauth", provider.id);
    let sm = state.agent.skill_manager.try_lock().ok()?;
    let creds = sm.get_credentials(&skill_name, None);
    let id = creds.get(provider.client_id_key)?.clone();
    let secret = creds.get(provider.client_secret_key)?.clone();
    if id.is_empty() || secret.is_empty() {
        return None;
    }
    Some((id, secret))
}

fn sso_callback_url(provider_id: &str) -> String {
    if let Ok(tunnel) = std::env::var("TUNNEL_URL") {
        if !tunnel.is_empty() {
            return format!("{tunnel}/api/auth/sso/{provider_id}/callback");
        }
    }
    let bind = std::env::var("DASHBOARD_BIND")
        .unwrap_or_else(|_| "http://localhost:3031".into());
    format!("{bind}/api/auth/sso/{provider_id}/callback")
}

/// GET /api/auth/sso/{provider}/start — redirect to the OAuth provider
/// for dashboard authentication (minimal scopes: just email).
pub async fn sso_start(
    State(state): State<DashState>,
    Path(provider_id): Path<String>,
) -> Result<Redirect, (StatusCode, Json<serde_json::Value>)> {
    // Check that this provider is allowed for SSO
    if !state.config.dashboard.sso_providers.iter().any(|p| p == &provider_id) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": format!("SSO via {provider_id} not enabled") })),
        ));
    }

    let provider = oauth::find_provider(&provider_id).ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "unknown provider" })))
    })?;

    let (client_id, _) = sso_client_credentials(&state, provider).ok_or_else(|| {
        (StatusCode::PRECONDITION_FAILED, Json(serde_json::json!({
            "error": format!("{} not configured — set {} and {}", provider.name, provider.client_id_env, provider.client_secret_env)
        })))
    })?;

    let redirect_uri = sso_callback_url(&provider_id);

    // Use minimal scopes for SSO — just enough to get the email
    let sso_scopes = match provider.id {
        "google" => "openid email profile",
        "microsoft" => "openid email profile User.Read",
        "github" => "user:email",
        "discord" => "identify email",
        "linkedin" => "openid profile email",
        _ => "openid email profile",
    };

    let mut url = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&state=sso",
        provider.auth_url,
        oauth_urlencoding(&client_id),
        oauth_urlencoding(&redirect_uri),
        oauth_urlencoding(sso_scopes),
    );

    // Add prompt=consent for Google to always show account picker
    if provider.id == "google" {
        url.push_str("&prompt=select_account");
    }

    // Twitter PKCE
    if provider.id == "twitter" {
        url.push_str("&code_challenge_method=plain&code_challenge=challenge");
    }

    info!(provider = provider.id, "starting SSO login flow");
    Ok(Redirect::temporary(&url))
}

#[derive(Deserialize)]
pub struct SsoCallbackParams {
    code: Option<String>,
    error: Option<String>,
    #[allow(dead_code)]
    state: Option<String>,
}

/// GET /api/auth/sso/{provider}/callback — exchange code, verify email, issue JWT.
pub async fn sso_callback(
    State(state): State<DashState>,
    Path(provider_id): Path<String>,
    Query(params): Query<SsoCallbackParams>,
) -> Response {
    let error_page = |msg: &str| -> Response {
        axum::response::Html(format!(
            r#"<!DOCTYPE html><html><head><title>SSO Error</title>
            <style>body{{font-family:system-ui;background:#1a1a1a;color:#e0e0e0;display:flex;justify-content:center;align-items:center;height:100vh;margin:0}}
            .card{{background:#2a2a2a;border-radius:12px;padding:2rem 3rem;text-align:center;box-shadow:0 4px 20px rgba(0,0,0,.5);max-width:400px}}
            h2{{color:#ef4444}}a{{color:#ff9800;text-decoration:none}}</style></head>
            <body><div class="card"><h2>SSO Login Failed</h2><p>{msg}</p><p><a href="/">Back to Dashboard</a></p></div></body></html>"#
        )).into_response()
    };

    // Check provider is allowed
    if !state.config.dashboard.sso_providers.iter().any(|p| p == &provider_id) {
        return error_page("This SSO provider is not enabled.");
    }

    let provider = match oauth::find_provider(&provider_id) {
        Some(p) => p,
        None => return error_page("Unknown provider."),
    };

    if let Some(err) = params.error {
        warn!(provider = provider.id, error = %err, "SSO OAuth error");
        return error_page(&format!("OAuth error: {err}"));
    }

    let code = match params.code {
        Some(c) => c,
        None => return error_page("No authorization code received."),
    };

    let (client_id, client_secret) = match sso_client_credentials(&state, provider) {
        Some(c) => c,
        None => return error_page("OAuth provider not configured."),
    };

    let redirect_uri = sso_callback_url(&provider_id);

    // Exchange the code for tokens
    let client = reqwest::Client::new();
    let mut form = vec![
        ("code", code.as_str()),
        ("redirect_uri", redirect_uri.as_str()),
        ("grant_type", "authorization_code"),
        ("client_id", client_id.as_str()),
        ("client_secret", client_secret.as_str()),
    ];

    let req = match provider.token_exchange {
        oauth::TokenExchangeStyle::Standard => {
            client.post(provider.token_url).form(&form)
        }
        oauth::TokenExchangeStyle::GitHubStyle => {
            client.post(provider.token_url).form(&form).header("Accept", "application/json")
        }
        oauth::TokenExchangeStyle::BasicAuth => {
            // Remove client_id/secret from form, use basic auth instead
            form.retain(|(k, _)| *k != "client_id" && *k != "client_secret");
            client.post(provider.token_url).form(&form).basic_auth(&client_id, Some(&client_secret))
        }
    };

    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => return error_page(&format!("Token exchange failed: {e}")),
    };

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        error!(provider = provider.id, body = %body, "SSO token exchange failed");
        return error_page("Token exchange failed.");
    }

    let token_json: serde_json::Value = match resp.json().await {
        Ok(j) => j,
        Err(e) => return error_page(&format!("Failed to parse token response: {e}")),
    };

    let access_token = match token_json.get("access_token").and_then(|v| v.as_str()) {
        Some(t) => t.to_string(),
        None => return error_page("No access token in response."),
    };

    // Fetch user identity (email)
    let email = fetch_sso_email(provider, &access_token).await;
    let email = match email {
        Some(e) => e,
        None => return error_page("Could not determine your email address from the provider."),
    };

    info!(provider = provider.id, email = %email, "SSO login attempt");

    // Check if this email is allowed
    if !state.config.dashboard.sso_allowed_emails.is_empty()
        && !state.config.dashboard.sso_allowed_emails.iter().any(|e| {
            e.eq_ignore_ascii_case(&email)
        })
    {
        warn!(email = %email, "SSO login denied: email not in allowed list");
        return error_page(&format!("Your email ({email}) is not authorized to access this dashboard."));
    }

    // Try to find a matching user by email for multi-user mode
    let method = format!("sso:{}", provider.id);
    let (user_id, role) = if let Some(user) = state.agent.user_manager.get_by_email(&email).await {
        if !user.enabled {
            return error_page("Your account is disabled. Contact an administrator.");
        }
        state.agent.user_manager.touch(&user.id).await;
        info!(provider = provider.id, email = %email, user = %user.username, "SSO login matched user");
        (Some(user.id), Some(user.role.as_str().to_string()))
    } else {
        (None, None)
    };

    let token = match mint_token_with_user(
        &state.jwt_secret,
        &email,
        &method,
        user_id.as_deref(),
        role.as_deref(),
    ) {
        Ok(t) => t,
        Err(e) => {
            error!("failed to mint JWT for SSO: {e}");
            return error_page("Internal error generating session.");
        }
    };

    info!(provider = provider.id, email = %email, "SSO login successful");

    // Set cookie and redirect to dashboard
    let cookie = make_session_cookie(&token);

    let mut headers = axum::http::HeaderMap::new();
    headers.insert(axum::http::header::SET_COOKIE, cookie.parse().unwrap());
    headers.insert(
        axum::http::header::LOCATION,
        "/".parse().unwrap(),
    );

    (StatusCode::FOUND, headers).into_response()
}

/// Fetch the user's email from the SSO provider's userinfo endpoint.
async fn fetch_sso_email(provider: &oauth::OAuthProvider, access_token: &str) -> Option<String> {
    if provider.userinfo_url.is_empty() {
        return None;
    }

    let client = reqwest::Client::new();
    let resp = match provider.userinfo_method {
        oauth::UserInfoMethod::BearerGet | oauth::UserInfoMethod::SlackStyle => {
            client.get(provider.userinfo_url)
                .bearer_auth(access_token)
                .header("User-Agent", "safe-agent/1.0")
                .send().await.ok()?
        }
        oauth::UserInfoMethod::BearerPost => {
            client.post(provider.userinfo_url)
                .bearer_auth(access_token)
                .header("User-Agent", "safe-agent/1.0")
                .send().await.ok()?
        }
    };

    if !resp.status().is_success() {
        return None;
    }

    let json: serde_json::Value = resp.json().await.ok()?;

    // Handle provider-specific nesting
    let root = if provider.id == "slack" {
        json.get("user").unwrap_or(&json)
    } else if provider.id == "twitter" {
        json.get("data").unwrap_or(&json)
    } else {
        &json
    };

    // GitHub special: /user endpoint may not have email, use /user/emails
    if provider.id == "github" {
        if let Some(email) = root.get("email").and_then(|v| v.as_str()) {
            if !email.is_empty() {
                return Some(email.to_string());
            }
        }
        // Fallback: fetch from /user/emails
        let emails_resp = client.get("https://api.github.com/user/emails")
            .bearer_auth(access_token)
            .header("User-Agent", "safe-agent/1.0")
            .send().await.ok()?;
        if emails_resp.status().is_success() {
            let emails: Vec<serde_json::Value> = emails_resp.json().await.ok()?;
            for e in &emails {
                if e.get("primary").and_then(|v| v.as_bool()).unwrap_or(false) {
                    if let Some(addr) = e.get("email").and_then(|v| v.as_str()) {
                        return Some(addr.to_string());
                    }
                }
            }
            // Just use first email
            if let Some(first) = emails.first() {
                return first.get("email").and_then(|v| v.as_str()).map(|s| s.to_string());
            }
        }
        return root.get("login").and_then(|v| v.as_str()).map(|s| s.to_string());
    }

    match provider.email_json_path {
        oauth::EmailPath::Field(key) => root.get(key).and_then(|v| v.as_str()).map(|s| s.to_string()),
        oauth::EmailPath::FieldOrFallback(a, b) => {
            root.get(a).and_then(|v| v.as_str())
                .or_else(|| root.get(b).and_then(|v| v.as_str()))
                .map(|s| s.to_string())
        }
    }
}

fn oauth_urlencoding(s: &str) -> String {
    s.replace('%', "%25")
        .replace(' ', "%20")
        .replace('&', "%26")
        .replace('=', "%3D")
        .replace('+', "%2B")
        .replace('/', "%2F")
        .replace(':', "%3A")
        .replace('?', "%3F")
        .replace('#', "%23")
        .replace('@', "%40")
}

// ---------------------------------------------------------------------------
// 2FA verification (step 2 of login: challenge_token + TOTP code or recovery)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct Verify2faBody {
    pub challenge_token: String,
    #[serde(default)]
    pub totp_code: Option<String>,
    #[serde(default)]
    pub recovery_code: Option<String>,
}

/// POST /api/auth/2fa/verify — exchange a challenge token + TOTP/recovery code
/// for a full session JWT.
pub async fn verify_2fa(
    State(state): State<DashState>,
    Json(body): Json<Verify2faBody>,
) -> Response {
    let Some(user_id) = authn::verify_challenge_token(&state.jwt_secret, &body.challenge_token) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "ok": false, "error": "invalid or expired challenge token" })),
        ).into_response();
    };

    let user = match state.agent.user_manager.get_by_id(&user_id).await {
        Ok(u) => u,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "ok": false, "error": "user not found" })),
            ).into_response();
        }
    };

    // Verify TOTP code
    if let Some(ref code) = body.totp_code {
        if !state.agent.user_manager.verify_totp(&user.id, code).await {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "ok": false, "error": "invalid TOTP code" })),
            ).into_response();
        }
    } else if let Some(ref code) = body.recovery_code {
        if !state.agent.user_manager.verify_recovery_code(&user.id, code).await {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "ok": false, "error": "invalid recovery code" })),
            ).into_response();
        }
    } else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "ok": false, "error": "totp_code or recovery_code required" })),
        ).into_response();
    }

    // Issue full session JWT
    let token = match mint_token_with_user(
        &state.jwt_secret,
        &user.username,
        "password+2fa",
        Some(&user.id),
        Some(user.role.as_str()),
    ) {
        Ok(t) => t,
        Err(e) => {
            error!("failed to mint JWT after 2FA: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "ok": false, "error": "internal error" })),
            ).into_response();
        }
    };

    let cookie = make_session_cookie(&token);
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(axum::http::header::SET_COOKIE, cookie.parse().unwrap());

    info!(username = %user.username, "2FA verification successful");
    (headers, Json(serde_json::json!({
        "ok": true,
        "user": {
            "id": user.id,
            "username": user.username,
            "display_name": user.display_name,
            "role": user.role,
        }
    }))).into_response()
}

// ---------------------------------------------------------------------------
// TOTP management endpoints (require active session)
// ---------------------------------------------------------------------------

/// Helper: extract user_id from the JWT session cookie.
fn session_user_id(req: &Request<Body>, secret: &[u8]) -> Option<String> {
    extract_claims(req, secret).and_then(|c| c.user_id)
}

/// GET /api/auth/2fa/status — report TOTP status and passkey count.
pub async fn totp_status(
    State(state): State<DashState>,
    req: Request<Body>,
) -> Response {
    let Some(user_id) = session_user_id(&req, &state.jwt_secret) else {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "not authenticated" }))).into_response();
    };

    let totp_enabled = state.agent.user_manager.is_totp_enabled(&user_id).await;
    let passkey_count = if let Some(ref pm) = state.passkey_manager {
        pm.list_passkeys(&user_id).await.len()
    } else {
        0
    };
    let passkeys_available = state.passkey_manager.is_some();

    Json(serde_json::json!({
        "totp_enabled": totp_enabled,
        "passkey_count": passkey_count,
        "passkeys_available": passkeys_available,
    })).into_response()
}

/// POST /api/auth/2fa/setup — generate a new TOTP secret + recovery codes.
pub async fn setup_totp(
    State(state): State<DashState>,
    req: Request<Body>,
) -> Response {
    let Some(user_id) = session_user_id(&req, &state.jwt_secret) else {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "not authenticated" }))).into_response();
    };

    let user = match state.agent.user_manager.get_by_id(&user_id).await {
        Ok(u) => u,
        Err(_) => return (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "user not found" }))).into_response(),
    };

    match state.agent.user_manager.setup_totp(&user_id).await {
        Ok((secret, recovery_codes)) => {
            let uri = authn::totp_uri(&secret, &user.username, "safe-agent");
            Json(serde_json::json!({
                "ok": true,
                "secret": secret,
                "otpauth_uri": uri,
                "recovery_codes": recovery_codes,
            })).into_response()
        }
        Err(e) => {
            error!(user_id, err = %e, "failed to setup TOTP");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "ok": false, "error": e.to_string() }))).into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct EnableTotpBody {
    pub code: String,
}

/// POST /api/auth/2fa/enable — verify a TOTP code and activate 2FA.
pub async fn enable_totp(
    State(state): State<DashState>,
    req: Request<Body>,
) -> Response {
    let Some(user_id) = session_user_id(&req, &state.jwt_secret) else {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "not authenticated" }))).into_response();
    };

    // Parse body manually since we already consumed the request
    let body_bytes = match axum::body::to_bytes(req.into_body(), 4096).await {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "invalid body" }))).into_response(),
    };
    let body: EnableTotpBody = match serde_json::from_slice(&body_bytes) {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "invalid JSON" }))).into_response(),
    };

    match state.agent.user_manager.enable_totp(&user_id, &body.code).await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "ok": false, "error": e.to_string() }))).into_response(),
    }
}

#[derive(Deserialize)]
pub struct DisableTotpBody {
    pub code: String,
}

/// POST /api/auth/2fa/disable — disable TOTP 2FA (requires valid TOTP code).
pub async fn disable_totp(
    State(state): State<DashState>,
    req: Request<Body>,
) -> Response {
    let Some(user_id) = session_user_id(&req, &state.jwt_secret) else {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "not authenticated" }))).into_response();
    };

    let body_bytes = match axum::body::to_bytes(req.into_body(), 4096).await {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "invalid body" }))).into_response(),
    };
    let body: DisableTotpBody = match serde_json::from_slice(&body_bytes) {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "invalid JSON" }))).into_response(),
    };

    // Verify code first
    if !state.agent.user_manager.verify_totp(&user_id, &body.code).await {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "ok": false, "error": "invalid TOTP code" }))).into_response();
    }

    match state.agent.user_manager.disable_totp(&user_id).await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "ok": false, "error": e.to_string() }))).into_response(),
    }
}

// ---------------------------------------------------------------------------
// Passkey (WebAuthn) management endpoints
// ---------------------------------------------------------------------------

/// POST /api/auth/passkey/register/start — begin passkey registration.
pub async fn passkey_register_start(
    State(state): State<DashState>,
    req: Request<Body>,
) -> Response {
    let Some(user_id) = session_user_id(&req, &state.jwt_secret) else {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "not authenticated" }))).into_response();
    };

    let Some(ref pm) = state.passkey_manager else {
        return (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({ "error": "passkeys not configured" }))).into_response();
    };

    let user = match state.agent.user_manager.get_by_id(&user_id).await {
        Ok(u) => u,
        Err(_) => return (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "user not found" }))).into_response(),
    };

    match pm.start_registration(&user.id, &user.username, &user.display_name).await {
        Ok(ccr) => Json(serde_json::json!({
            "ok": true,
            "options": ccr,
        })).into_response(),
        Err(e) => {
            error!(user_id, err = %e, "passkey registration start failed");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "ok": false, "error": e.to_string() }))).into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct PasskeyRegisterFinishBody {
    pub credential: webauthn_rs::prelude::RegisterPublicKeyCredential,
    #[serde(default)]
    pub name: Option<String>,
}

/// POST /api/auth/passkey/register/finish — complete passkey registration.
pub async fn passkey_register_finish(
    State(state): State<DashState>,
    req: Request<Body>,
) -> Response {
    let Some(user_id) = session_user_id(&req, &state.jwt_secret) else {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "not authenticated" }))).into_response();
    };

    let Some(ref pm) = state.passkey_manager else {
        return (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({ "error": "passkeys not configured" }))).into_response();
    };

    let body_bytes = match axum::body::to_bytes(req.into_body(), 65536).await {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "invalid body" }))).into_response(),
    };
    let body: PasskeyRegisterFinishBody = match serde_json::from_slice(&body_bytes) {
        Ok(b) => b,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": format!("invalid JSON: {e}") }))).into_response(),
    };

    let name = body.name.as_deref().unwrap_or("Passkey");
    match pm.finish_registration(&user_id, &body.credential, name).await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "ok": false, "error": e.to_string() }))).into_response(),
    }
}

#[derive(Deserialize)]
pub struct PasskeyAuthStartBody {
    pub challenge_token: String,
}

/// POST /api/auth/passkey/authenticate/start — begin passkey authentication
/// during the 2FA flow (requires a valid challenge token).
pub async fn passkey_auth_start(
    State(state): State<DashState>,
    Json(body): Json<PasskeyAuthStartBody>,
) -> Response {
    let Some(user_id) = authn::verify_challenge_token(&state.jwt_secret, &body.challenge_token) else {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "invalid or expired challenge token" }))).into_response();
    };

    let Some(ref pm) = state.passkey_manager else {
        return (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({ "error": "passkeys not configured" }))).into_response();
    };

    match pm.start_authentication(&user_id).await {
        Ok(rcr) => Json(serde_json::json!({
            "ok": true,
            "options": rcr,
        })).into_response(),
        Err(e) => {
            error!(user_id = %user_id, err = %e, "passkey auth start failed");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "ok": false, "error": e.to_string() }))).into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct PasskeyAuthFinishBody {
    pub challenge_token: String,
    pub credential: webauthn_rs::prelude::PublicKeyCredential,
}

/// POST /api/auth/passkey/authenticate/finish — complete passkey authentication,
/// issue full session JWT.
pub async fn passkey_auth_finish(
    State(state): State<DashState>,
    Json(body): Json<PasskeyAuthFinishBody>,
) -> Response {
    let Some(user_id) = authn::verify_challenge_token(&state.jwt_secret, &body.challenge_token) else {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "invalid or expired challenge token" }))).into_response();
    };

    let Some(ref pm) = state.passkey_manager else {
        return (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({ "error": "passkeys not configured" }))).into_response();
    };

    if let Err(e) = pm.finish_authentication(&user_id, &body.credential).await {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "ok": false, "error": e.to_string() })),
        ).into_response();
    }

    // Issue full session JWT
    let user = match state.agent.user_manager.get_by_id(&user_id).await {
        Ok(u) => u,
        Err(_) => return (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "user not found" }))).into_response(),
    };

    let token = match mint_token_with_user(
        &state.jwt_secret,
        &user.username,
        "password+passkey",
        Some(&user.id),
        Some(user.role.as_str()),
    ) {
        Ok(t) => t,
        Err(e) => {
            error!("failed to mint JWT after passkey auth: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "ok": false, "error": "internal error" }))).into_response();
        }
    };

    let cookie = make_session_cookie(&token);
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(axum::http::header::SET_COOKIE, cookie.parse().unwrap());

    info!(username = %user.username, "passkey authentication successful");
    (headers, Json(serde_json::json!({
        "ok": true,
        "user": {
            "id": user.id,
            "username": user.username,
            "display_name": user.display_name,
            "role": user.role,
        }
    }))).into_response()
}

/// GET /api/auth/passkeys — list the current user's registered passkeys.
pub async fn list_passkeys(
    State(state): State<DashState>,
    req: Request<Body>,
) -> Response {
    let Some(user_id) = session_user_id(&req, &state.jwt_secret) else {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "not authenticated" }))).into_response();
    };

    let Some(ref pm) = state.passkey_manager else {
        let empty: Vec<()> = vec![];
        return Json(serde_json::json!({ "passkeys": empty, "available": false })).into_response();
    };

    let passkeys = pm.list_passkeys(&user_id).await;
    Json(serde_json::json!({ "passkeys": passkeys, "available": true })).into_response()
}

/// DELETE /api/auth/passkeys/{id} — delete a passkey.
pub async fn delete_passkey(
    State(state): State<DashState>,
    Path(passkey_id): Path<String>,
    req: Request<Body>,
) -> Response {
    let Some(user_id) = session_user_id(&req, &state.jwt_secret) else {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "not authenticated" }))).into_response();
    };

    let Some(ref pm) = state.passkey_manager else {
        return (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({ "error": "passkeys not configured" }))).into_response();
    };

    match pm.delete_passkey(&user_id, &passkey_id).await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "ok": false, "error": e.to_string() }))).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    #[test]
    fn mint_and_validate_token() {
        let secret = b"test-jwt-secret-12345";
        let token = mint_token(secret, "dashboard", "password").unwrap();
        assert!(!token.is_empty());

        let req = Request::builder()
            .header(axum::http::header::COOKIE, format!("{COOKIE_NAME}={token}"))
            .body(Body::empty())
            .unwrap();
        assert!(validate_token(&req, secret));
    }

    #[test]
    fn validate_token_no_cookie() {
        let secret = b"secret";
        let req = Request::builder().body(Body::empty()).unwrap();
        assert!(!validate_token(&req, secret));
    }

    #[test]
    fn validate_token_wrong_secret() {
        let secret = b"correct-secret";
        let token = mint_token(secret, "sub", "method").unwrap();
        let req = Request::builder()
            .header(axum::http::header::COOKIE, format!("{COOKIE_NAME}={token}"))
            .body(Body::empty())
            .unwrap();
        assert!(!validate_token(&req, b"wrong-secret"));
    }

    #[test]
    fn validate_token_malformed() {
        let req = Request::builder()
            .header(axum::http::header::COOKIE, format!("{COOKIE_NAME}=not-a-jwt"))
            .body(Body::empty())
            .unwrap();
        assert!(!validate_token(&req, b"secret"));
    }

    #[test]
    fn validate_token_among_other_cookies() {
        let secret = b"sec";
        let token = mint_token(secret, "test", "pw").unwrap();
        let cookie = format!("other=value; {COOKIE_NAME}={token}; foo=bar");
        let req = Request::builder()
            .header(axum::http::header::COOKIE, cookie)
            .body(Body::empty())
            .unwrap();
        assert!(validate_token(&req, secret));
    }

    #[test]
    fn claims_serde_roundtrip() {
        let claims = Claims {
            sub: "user@example.com".to_string(),
            iat: 1000,
            exp: 2000,
            method: Some("sso:google".to_string()),
            user_id: Some("u-123".to_string()),
            role: Some("admin".to_string()),
            csrf: Some("abc123".to_string()),
        };
        let json = serde_json::to_string(&claims).unwrap();
        let decoded: Claims = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.sub, "user@example.com");
        assert_eq!(decoded.method.as_deref(), Some("sso:google"));
        assert_eq!(decoded.user_id.as_deref(), Some("u-123"));
        assert_eq!(decoded.role.as_deref(), Some("admin"));
    }

    #[test]
    fn claims_method_none_skips_serialization() {
        let claims = Claims {
            sub: "dashboard".to_string(),
            iat: 1000,
            exp: 2000,
            method: None,
            user_id: None,
            role: None,
            csrf: None,
        };
        let json = serde_json::to_string(&claims).unwrap();
        assert!(!json.contains("method"));
        assert!(!json.contains("user_id"));
        assert!(!json.contains("role"));
        assert!(!json.contains("csrf"));
    }

    #[test]
    fn oauth_urlencoding_special_chars() {
        assert_eq!(oauth_urlencoding("hello world"), "hello%20world");
        assert_eq!(oauth_urlencoding("a&b=c"), "a%26b%3Dc");
        assert_eq!(oauth_urlencoding("https://example.com"), "https%3A%2F%2Fexample.com");
        assert_eq!(oauth_urlencoding("a+b"), "a%2Bb");
        assert_eq!(oauth_urlencoding("x@y"), "x%40y");
        assert_eq!(oauth_urlencoding("q?k=v#frag"), "q%3Fk%3Dv%23frag");
    }

    #[test]
    fn oauth_urlencoding_percent() {
        assert_eq!(oauth_urlencoding("50%"), "50%25");
    }

    #[test]
    fn oauth_urlencoding_empty() {
        assert_eq!(oauth_urlencoding(""), "");
    }

    #[test]
    fn oauth_urlencoding_no_special() {
        assert_eq!(oauth_urlencoding("hello"), "hello");
    }

    #[test]
    fn sso_callback_url_uses_tunnel() {
        unsafe { std::env::set_var("TUNNEL_URL", "https://myapp.ngrok.io"); }
        let url = sso_callback_url("google");
        assert_eq!(url, "https://myapp.ngrok.io/api/auth/sso/google/callback");
        unsafe { std::env::remove_var("TUNNEL_URL"); }
    }

    #[test]
    fn sso_callback_url_falls_back_to_dashboard_bind() {
        unsafe { std::env::remove_var("TUNNEL_URL"); }
        unsafe { std::env::set_var("DASHBOARD_BIND", "http://localhost:9999"); }
        let url = sso_callback_url("github");
        assert_eq!(url, "http://localhost:9999/api/auth/sso/github/callback");
        unsafe { std::env::remove_var("DASHBOARD_BIND"); }
    }
}
