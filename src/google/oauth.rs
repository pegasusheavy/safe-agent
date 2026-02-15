use std::sync::Arc;

use rusqlite::Connection;
use tokio::sync::Mutex;
use tracing::info;

use crate::config::Config;
use crate::error::{Result, SafeAgentError};

const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_REVOKE_URL: &str = "https://oauth2.googleapis.com/revoke";

/// Scopes we request from Google.
const SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/calendar",
    "https://www.googleapis.com/auth/drive",
    "https://www.googleapis.com/auth/documents",
];

/// Build the Google OAuth2 authorization URL for the consent screen redirect.
pub fn authorization_url(config: &Config) -> Result<(String, String)> {
    let client_id = Config::google_client_id()?;
    let redirect_uri = &config.google.redirect_uri;
    let state = uuid::Uuid::new_v4().to_string();
    let scope = SCOPES.join(" ");

    let url = format!(
        "{GOOGLE_AUTH_URL}?client_id={client_id}&redirect_uri={redirect}&response_type=code&scope={scope}&state={state}&access_type=offline&prompt=consent",
        redirect = urlencoding(redirect_uri),
        scope = urlencoding(&scope),
    );

    Ok((url, state))
}

/// Exchange an authorization code for access + refresh tokens, and store them.
pub async fn exchange_code(
    config: &Config,
    code: &str,
    db: &Arc<Mutex<Connection>>,
    http: &reqwest::Client,
) -> Result<()> {
    let client_id = Config::google_client_id()?;
    let client_secret = Config::google_client_secret()?;
    let redirect_uri = &config.google.redirect_uri;

    let resp = http
        .post(GOOGLE_TOKEN_URL)
        .form(&[
            ("code", code),
            ("client_id", &client_id),
            ("client_secret", &client_secret),
            ("redirect_uri", redirect_uri),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .map_err(|e| SafeAgentError::OAuth(format!("token exchange failed: {e}")))?;

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| SafeAgentError::OAuth(format!("parse token response: {e}")))?;

    let access_token = body
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| SafeAgentError::OAuth("no access_token in response".into()))?;
    let refresh_token = body.get("refresh_token").and_then(|v| v.as_str());
    let expires_in = body.get("expires_in").and_then(|v| v.as_i64()).unwrap_or(3600);
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(expires_in);
    let scopes = SCOPES.join(" ");

    info!("Google OAuth tokens received, storing");

    let conn = db.lock().await;
    conn.execute(
        "INSERT OR REPLACE INTO oauth_tokens (provider, access_token, refresh_token, expires_at, scopes, updated_at)
         VALUES ('google', ?1, ?2, ?3, ?4, datetime('now'))",
        rusqlite::params![
            access_token,
            refresh_token,
            expires_at.to_rfc3339(),
            scopes,
        ],
    )?;

    Ok(())
}

/// Revoke Google tokens and remove from DB.
pub async fn disconnect(db: &Arc<Mutex<Connection>>, http: &reqwest::Client) -> Result<()> {
    let conn = db.lock().await;
    let token: std::result::Result<String, _> = conn.query_row(
        "SELECT access_token FROM oauth_tokens WHERE provider = 'google'",
        [],
        |row| row.get(0),
    );
    drop(conn);

    if let Ok(token) = token {
        let _ = http
            .post(GOOGLE_REVOKE_URL)
            .form(&[("token", &token)])
            .send()
            .await;
    }

    let conn = db.lock().await;
    conn.execute("DELETE FROM oauth_tokens WHERE provider = 'google'", [])?;
    info!("Google disconnected");
    Ok(())
}

/// Check if we have valid Google tokens.
pub async fn is_connected(db: &Arc<Mutex<Connection>>) -> bool {
    let conn = db.lock().await;
    conn.query_row(
        "SELECT COUNT(*) FROM oauth_tokens WHERE provider = 'google'",
        [],
        |row| row.get::<_, i64>(0),
    )
    .map(|c| c > 0)
    .unwrap_or(false)
}

fn urlencoding(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            _ => format!("%{:02X}", c as u32),
        })
        .collect()
}
