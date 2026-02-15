use std::sync::Arc;

use rusqlite::Connection;
use tokio::sync::Mutex;
use tracing::{debug, warn};

use crate::error::{Result, SafeAgentError};

const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

/// Retrieve the Google access token, refreshing if expired.
pub async fn get_access_token(
    db: &Arc<Mutex<Connection>>,
    http: &reqwest::Client,
) -> Result<String> {
    let conn = db.lock().await;
    let row = conn.query_row(
        "SELECT access_token, refresh_token, expires_at FROM oauth_tokens WHERE provider = 'google'",
        [],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        },
    );
    drop(conn);

    let (access_token, refresh_token, expires_at) = row
        .map_err(|_| SafeAgentError::OAuth("No Google tokens stored".into()))?;

    // Check if token is expired
    let is_expired = if let Some(ref exp) = expires_at {
        if let Ok(exp_dt) = chrono::DateTime::parse_from_rfc3339(exp) {
            chrono::Utc::now() > exp_dt - chrono::Duration::minutes(5)
        } else {
            true
        }
    } else {
        true
    };

    if !is_expired {
        return Ok(access_token);
    }

    // Token is expired — try to refresh
    let refresh = refresh_token
        .ok_or_else(|| SafeAgentError::OAuth("No refresh token available".into()))?;

    debug!("refreshing Google access token");

    let client_id = std::env::var("GOOGLE_CLIENT_ID")
        .map_err(|_| SafeAgentError::OAuth("GOOGLE_CLIENT_ID not set".into()))?;
    let client_secret = std::env::var("GOOGLE_CLIENT_SECRET")
        .map_err(|_| SafeAgentError::OAuth("GOOGLE_CLIENT_SECRET not set".into()))?;

    let resp = http
        .post(GOOGLE_TOKEN_URL)
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("refresh_token", refresh.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .map_err(|e| SafeAgentError::OAuth(format!("refresh request failed: {e}")))?;

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| SafeAgentError::OAuth(format!("parse refresh response: {e}")))?;

    let new_access = body
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| SafeAgentError::OAuth("no access_token in refresh response".into()))?;

    let expires_in = body.get("expires_in").and_then(|v| v.as_i64()).unwrap_or(3600);
    let new_expires = chrono::Utc::now() + chrono::Duration::seconds(expires_in);

    // Update in DB
    let conn = db.lock().await;
    conn.execute(
        "UPDATE oauth_tokens SET access_token = ?1, expires_at = ?2, updated_at = datetime('now')
         WHERE provider = 'google'",
        rusqlite::params![new_access, new_expires.to_rfc3339()],
    )?;

    debug!("Google token refreshed, expires at {}", new_expires);
    Ok(new_access.to_string())
}

/// Delete stored tokens.
pub async fn clear_tokens(db: &Arc<Mutex<Connection>>) -> Result<()> {
    let conn = db.lock().await;
    conn.execute("DELETE FROM oauth_tokens WHERE provider = 'google'", [])?;
    warn!("Google tokens cleared");
    Ok(())
}
