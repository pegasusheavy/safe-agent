use async_trait::async_trait;
use tracing::debug;

use super::{Tool, ToolContext, ToolOutput};
use crate::error::Result;

// -- Google Calendar Tool ------------------------------------------------

pub struct GoogleCalendarTool;

impl GoogleCalendarTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GoogleCalendarTool {
    fn name(&self) -> &str {
        "google_calendar"
    }

    fn description(&self) -> &str {
        "Manage Google Calendar events. Actions: list_events, create_event, update_event, delete_event. Requires Google SSO connection."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list_events", "create_event", "update_event", "delete_event"]
                },
                "calendar_id": { "type": "string", "description": "Calendar ID (default: 'primary')" },
                "event_id": { "type": "string", "description": "Event ID (for update/delete)" },
                "summary": { "type": "string", "description": "Event title (for create/update)" },
                "description": { "type": "string", "description": "Event description" },
                "start": { "type": "string", "description": "Start time (ISO 8601)" },
                "end": { "type": "string", "description": "End time (ISO 8601)" },
                "time_min": { "type": "string", "description": "Filter: earliest time (for list)" },
                "time_max": { "type": "string", "description": "Filter: latest time (for list)" },
                "max_results": { "type": "integer", "description": "Max events (for list, default 10)" }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let action = params.get("action").and_then(|v| v.as_str()).unwrap_or_default();

        // Check for OAuth token
        let db = ctx.db.lock().await;
        let token: std::result::Result<String, _> = db.query_row(
            "SELECT access_token FROM oauth_tokens WHERE provider = 'google'",
            [],
            |row| row.get(0),
        );
        drop(db);

        let access_token = match token {
            Ok(t) => t,
            Err(_) => return Ok(ToolOutput::error("Google not connected. Please authenticate via the dashboard.")),
        };

        debug!(action, "google calendar");

        match action {
            "list_events" => {
                let max_results = params.get("max_results").and_then(|v| v.as_u64()).unwrap_or(10);
                let mut url = format!(
                    "https://www.googleapis.com/calendar/v3/calendars/primary/events?maxResults={max_results}&singleEvents=true&orderBy=startTime"
                );
                if let Some(time_min) = params.get("time_min").and_then(|v| v.as_str()) {
                    url.push_str(&format!("&timeMin={time_min}"));
                }
                if let Some(time_max) = params.get("time_max").and_then(|v| v.as_str()) {
                    url.push_str(&format!("&timeMax={time_max}"));
                }

                let resp = ctx.http_client
                    .get(&url)
                    .bearer_auth(&access_token)
                    .send()
                    .await;

                match resp {
                    Ok(r) => {
                        let body = r.text().await.unwrap_or_default();
                        Ok(ToolOutput::ok(body))
                    }
                    Err(e) => Ok(ToolOutput::error(format!("API request failed: {e}"))),
                }
            }
            "create_event" => {
                let event = serde_json::json!({
                    "summary": params.get("summary").and_then(|v| v.as_str()).unwrap_or(""),
                    "description": params.get("description").and_then(|v| v.as_str()).unwrap_or(""),
                    "start": { "dateTime": params.get("start").and_then(|v| v.as_str()).unwrap_or("") },
                    "end": { "dateTime": params.get("end").and_then(|v| v.as_str()).unwrap_or("") },
                });

                let resp = ctx.http_client
                    .post("https://www.googleapis.com/calendar/v3/calendars/primary/events")
                    .bearer_auth(&access_token)
                    .json(&event)
                    .send()
                    .await;

                match resp {
                    Ok(r) => {
                        let body = r.text().await.unwrap_or_default();
                        Ok(ToolOutput::ok(format!("Event created: {body}")))
                    }
                    Err(e) => Ok(ToolOutput::error(format!("API request failed: {e}"))),
                }
            }
            "delete_event" => {
                let event_id = params.get("event_id").and_then(|v| v.as_str()).unwrap_or_default();
                if event_id.is_empty() {
                    return Ok(ToolOutput::error("event_id is required"));
                }
                let url = format!(
                    "https://www.googleapis.com/calendar/v3/calendars/primary/events/{event_id}"
                );
                let resp = ctx.http_client.delete(&url).bearer_auth(&access_token).send().await;
                match resp {
                    Ok(r) if r.status().is_success() => Ok(ToolOutput::ok(format!("Deleted event {event_id}"))),
                    Ok(r) => Ok(ToolOutput::error(format!("Delete failed: {}", r.status()))),
                    Err(e) => Ok(ToolOutput::error(format!("API request failed: {e}"))),
                }
            }
            other => Ok(ToolOutput::error(format!("unknown action: {other}"))),
        }
    }
}

// -- Google Drive Tool ---------------------------------------------------

pub struct GoogleDriveTool;

impl GoogleDriveTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GoogleDriveTool {
    fn name(&self) -> &str {
        "google_drive"
    }

    fn description(&self) -> &str {
        "Manage Google Drive files. Actions: list_files, upload_file, download_file, create_folder, delete_file. Requires Google SSO."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list_files", "upload_file", "download_file", "create_folder", "delete_file"]
                },
                "file_id": { "type": "string" },
                "name": { "type": "string", "description": "File or folder name" },
                "content": { "type": "string", "description": "File content (for upload)" },
                "path": { "type": "string", "description": "Sandbox path (for download destination)" },
                "query": { "type": "string", "description": "Search query (for list_files)" },
                "max_results": { "type": "integer" }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let action = params.get("action").and_then(|v| v.as_str()).unwrap_or_default();

        let db = ctx.db.lock().await;
        let token: std::result::Result<String, _> = db.query_row(
            "SELECT access_token FROM oauth_tokens WHERE provider = 'google'",
            [],
            |row| row.get(0),
        );
        drop(db);

        let access_token = match token {
            Ok(t) => t,
            Err(_) => return Ok(ToolOutput::error("Google not connected.")),
        };

        debug!(action, "google drive");

        match action {
            "list_files" => {
                let max_results = params.get("max_results").and_then(|v| v.as_u64()).unwrap_or(20);
                let mut url = format!(
                    "https://www.googleapis.com/drive/v3/files?pageSize={max_results}&fields=files(id,name,mimeType,modifiedTime)"
                );
                if let Some(q) = params.get("query").and_then(|v| v.as_str()) {
                    url.push_str(&format!("&q={}", super::web::urlencoding(q)));
                }
                let resp = ctx.http_client.get(&url).bearer_auth(&access_token).send().await;
                match resp {
                    Ok(r) => Ok(ToolOutput::ok(r.text().await.unwrap_or_default())),
                    Err(e) => Ok(ToolOutput::error(format!("API failed: {e}"))),
                }
            }
            "create_folder" => {
                let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("New Folder");
                let body = serde_json::json!({
                    "name": name,
                    "mimeType": "application/vnd.google-apps.folder"
                });
                let resp = ctx.http_client
                    .post("https://www.googleapis.com/drive/v3/files")
                    .bearer_auth(&access_token)
                    .json(&body)
                    .send()
                    .await;
                match resp {
                    Ok(r) => Ok(ToolOutput::ok(r.text().await.unwrap_or_default())),
                    Err(e) => Ok(ToolOutput::error(format!("API failed: {e}"))),
                }
            }
            "delete_file" => {
                let file_id = params.get("file_id").and_then(|v| v.as_str()).unwrap_or_default();
                if file_id.is_empty() {
                    return Ok(ToolOutput::error("file_id is required"));
                }
                let url = format!("https://www.googleapis.com/drive/v3/files/{file_id}");
                let resp = ctx.http_client.delete(&url).bearer_auth(&access_token).send().await;
                match resp {
                    Ok(r) if r.status().is_success() => Ok(ToolOutput::ok(format!("Deleted {file_id}"))),
                    Ok(r) => Ok(ToolOutput::error(format!("Delete failed: {}", r.status()))),
                    Err(e) => Ok(ToolOutput::error(format!("API failed: {e}"))),
                }
            }
            other => Ok(ToolOutput::ok(format!(
                "Google Drive {other} — full implementation pending"
            ))),
        }
    }
}

// -- Google Docs Tool ----------------------------------------------------

pub struct GoogleDocsTool;

impl GoogleDocsTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GoogleDocsTool {
    fn name(&self) -> &str {
        "google_docs"
    }

    fn description(&self) -> &str {
        "Create and read Google Docs. Actions: create_document, read_document. Requires Google SSO."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["create_document", "read_document"]
                },
                "document_id": { "type": "string", "description": "Document ID (for read)" },
                "title": { "type": "string", "description": "Document title (for create)" },
                "content": { "type": "string", "description": "Initial content (for create)" }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let action = params.get("action").and_then(|v| v.as_str()).unwrap_or_default();

        let db = ctx.db.lock().await;
        let token: std::result::Result<String, _> = db.query_row(
            "SELECT access_token FROM oauth_tokens WHERE provider = 'google'",
            [],
            |row| row.get(0),
        );
        drop(db);

        let access_token = match token {
            Ok(t) => t,
            Err(_) => return Ok(ToolOutput::error("Google not connected.")),
        };

        debug!(action, "google docs");

        match action {
            "create_document" => {
                let title = params.get("title").and_then(|v| v.as_str()).unwrap_or("Untitled");
                let body = serde_json::json!({ "title": title });
                let resp = ctx.http_client
                    .post("https://docs.googleapis.com/v1/documents")
                    .bearer_auth(&access_token)
                    .json(&body)
                    .send()
                    .await;
                match resp {
                    Ok(r) => Ok(ToolOutput::ok(r.text().await.unwrap_or_default())),
                    Err(e) => Ok(ToolOutput::error(format!("API failed: {e}"))),
                }
            }
            "read_document" => {
                let doc_id = params.get("document_id").and_then(|v| v.as_str()).unwrap_or_default();
                if doc_id.is_empty() {
                    return Ok(ToolOutput::error("document_id is required"));
                }
                let url = format!("https://docs.googleapis.com/v1/documents/{doc_id}");
                let resp = ctx.http_client.get(&url).bearer_auth(&access_token).send().await;
                match resp {
                    Ok(r) => Ok(ToolOutput::ok(r.text().await.unwrap_or_default())),
                    Err(e) => Ok(ToolOutput::error(format!("API failed: {e}"))),
                }
            }
            other => Ok(ToolOutput::error(format!("unknown action: {other}"))),
        }
    }
}
