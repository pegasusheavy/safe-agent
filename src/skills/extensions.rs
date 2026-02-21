use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rhai::{Dynamic, Engine, Map, Scope, AST};
use rusqlite::Connection;
use tracing::{error, info};

// ---------------------------------------------------------------------------
// Skill route definition (collected when evaluating routes.rhai)
// ---------------------------------------------------------------------------

/// A single route registered by a skill's Rhai script.
#[derive(Clone)]
pub struct SkillRoute {
    pub method: String,
    pub path: String,
    pub handler_name: String,
}

/// All extension data loaded from a skill directory.
#[derive(Clone)]
pub struct SkillExtension {
    pub skill_name: String,
    pub skill_dir: PathBuf,
    pub routes: Vec<SkillRoute>,
    pub ast: Option<Arc<AST>>,
    pub ui: SkillUiConfig,
}

/// UI extension configuration from skill.toml [ui] section.
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub struct SkillUiConfig {
    /// Path to an HTML panel shown in the skill's expanded card.
    #[serde(default)]
    pub panel: Option<String>,
    /// Path to a full-page HTML file served at /skills/{name}/page.
    #[serde(default)]
    pub page: Option<String>,
    /// Path to custom CSS injected into the dashboard.
    #[serde(default)]
    pub style: Option<String>,
    /// Path to custom JS injected into the dashboard.
    #[serde(default)]
    pub script: Option<String>,
    /// Widget type for inline display in the skill card header.
    #[serde(default)]
    pub widget: Option<String>,
}

// ---------------------------------------------------------------------------
// Extension manager
// ---------------------------------------------------------------------------

/// Manages Rhai-based skill extensions (routes + UI).
pub struct ExtensionManager {
    engine: Engine,
    extensions: HashMap<String, SkillExtension>,
    db_path: PathBuf,
    skills_dir: PathBuf,
}

impl ExtensionManager {
    pub fn new(skills_dir: PathBuf, db_path: PathBuf) -> Self {
        let engine = create_engine(db_path.clone(), skills_dir.clone());
        Self {
            engine,
            extensions: HashMap::new(),
            db_path,
            skills_dir,
        }
    }

    /// Load extensions from all skill directories that have a routes.rhai file.
    pub fn discover(&mut self) {
        let Ok(entries) = std::fs::read_dir(&self.skills_dir) else {
            return;
        };

        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }

            let skill_name = dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            if skill_name.is_empty() {
                continue;
            }

            // Load UI config from skill.toml
            let ui = load_ui_config(&dir);

            // Load Rhai routes
            let routes_file = dir.join("routes.rhai");
            let (routes, ast) = if routes_file.exists() {
                match self.load_routes(&routes_file, &skill_name) {
                    Ok((r, a)) => (r, Some(Arc::new(a))),
                    Err(e) => {
                        error!(skill = %skill_name, err = %e, "failed to load routes.rhai");
                        (vec![], None)
                    }
                }
            } else {
                (vec![], None)
            };

            let has_ext = !routes.is_empty() || ui.panel.is_some() || ui.page.is_some();

            if has_ext {
                info!(
                    skill = %skill_name,
                    routes = routes.len(),
                    has_panel = ui.panel.is_some(),
                    has_page = ui.page.is_some(),
                    "loaded skill extension"
                );
            }

            self.extensions.insert(
                skill_name.clone(),
                SkillExtension {
                    skill_name,
                    skill_dir: dir,
                    routes,
                    ast,
                    ui,
                },
            );
        }
    }

    /// Parse a routes.rhai file and collect route registrations.
    fn load_routes(
        &self,
        path: &Path,
        skill_name: &str,
    ) -> Result<(Vec<SkillRoute>, AST), String> {
        let source = std::fs::read_to_string(path)
            .map_err(|e| format!("read error: {e}"))?;

        let ast = self.engine.compile(&source)
            .map_err(|e| format!("compile error: {e}"))?;

        // Evaluate the script to collect route registrations.
        // The script calls `register_route(method, path, handler_fn_name)`.
        let mut scope = Scope::new();
        scope.push("__routes", rhai::Array::new());
        scope.push("__skill_name", skill_name.to_string());

        let _ = self.engine
            .eval_ast_with_scope::<Dynamic>(&mut scope, &ast)
            .map_err(|e| format!("eval error: {e}"))?;

        // Read the collected routes from the scope
        let collected: rhai::Array = scope
            .get_value("__routes")
            .unwrap_or_default();

        let mut skill_routes = Vec::new();
        for item in collected {
            if let Some(map) = item.try_cast::<Map>() {
                let method = map.get("method")
                    .and_then(|v| v.clone().into_string().ok())
                    .unwrap_or_default()
                    .to_uppercase();
                let path = map.get("path")
                    .and_then(|v| v.clone().into_string().ok())
                    .unwrap_or_default();
                let handler = map.get("handler")
                    .and_then(|v| v.clone().into_string().ok())
                    .unwrap_or_default();

                if !method.is_empty() && !path.is_empty() && !handler.is_empty() {
                    skill_routes.push(SkillRoute {
                        method,
                        path,
                        handler_name: handler,
                    });
                }
            }
        }

        Ok((skill_routes, ast))
    }

    /// Execute a Rhai route handler and return the response.
    pub async fn handle_request(
        &self,
        skill_name: &str,
        method: &str,
        path: &str,
        query_params: &HashMap<String, String>,
        body: &str,
        headers: &HashMap<String, String>,
    ) -> Result<RhaiResponse, String> {
        let ext = self.extensions.get(skill_name)
            .ok_or_else(|| format!("skill '{}' has no extensions", skill_name))?;

        let ast = ext.ast.as_ref()
            .ok_or_else(|| format!("skill '{}' has no routes.rhai", skill_name))?;

        // Find matching route
        let route = ext.routes.iter()
            .find(|r| r.method == method.to_uppercase() && r.path == path)
            .ok_or_else(|| format!("no route {} {} in skill '{}'", method, path, skill_name))?;

        // Build request object for Rhai
        let mut req_map = Map::new();
        req_map.insert("method".into(), Dynamic::from(method.to_string()));
        req_map.insert("path".into(), Dynamic::from(path.to_string()));
        req_map.insert("body".into(), Dynamic::from(body.to_string()));

        let mut q_map = Map::new();
        for (k, v) in query_params {
            q_map.insert(k.clone().into(), Dynamic::from(v.clone()));
        }
        req_map.insert("query".into(), Dynamic::from(q_map));

        let mut h_map = Map::new();
        for (k, v) in headers {
            h_map.insert(k.clone().into(), Dynamic::from(v.clone()));
        }
        req_map.insert("headers".into(), Dynamic::from(h_map));

        // Prepare scope with skill context
        let mut scope = Scope::new();
        scope.push("__skill_name", skill_name.to_string());
        scope.push("__skill_dir", ext.skill_dir.to_string_lossy().to_string());
        scope.push("__data_dir", ext.skill_dir.join("data").to_string_lossy().to_string());
        scope.push("__routes", rhai::Array::new());

        // Execute the handler function
        let handler_name = route.handler_name.clone();
        let result = self.engine
            .call_fn::<Dynamic>(&mut scope, ast, &handler_name, (Dynamic::from(req_map),))
            .map_err(|e| format!("handler error: {e}"))?;

        // Parse the response
        parse_rhai_response(result)
    }

    /// Get extension info for a skill.
    pub fn get_extension(&self, skill_name: &str) -> Option<&SkillExtension> {
        self.extensions.get(skill_name)
    }

    /// List all skills that have extensions.
    pub fn list_extensions(&self) -> Vec<SkillExtensionInfo> {
        self.extensions
            .values()
            .filter(|ext| !ext.routes.is_empty() || ext.ui.panel.is_some() || ext.ui.page.is_some())
            .map(|ext| SkillExtensionInfo {
                skill_name: ext.skill_name.clone(),
                route_count: ext.routes.len(),
                routes: ext.routes.iter().map(|r| format!("{} {}", r.method, r.path)).collect(),
                ui: ext.ui.clone(),
            })
            .collect()
    }

    /// Read a static file from a skill's directory.
    pub fn read_static_file(&self, skill_name: &str, file_path: &str) -> Option<(Vec<u8>, String)> {
        let ext = self.extensions.get(skill_name)?;

        // Use PathJail for proper canonicalization-based traversal prevention
        let jail = crate::security::PathJail::new(ext.skill_dir.clone())?;
        let safe_path = jail.validate(file_path)?;

        let content = std::fs::read(&safe_path).ok()?;
        let content_type = match safe_path.extension().and_then(|e| e.to_str()) {
            Some("html") => "text/html",
            Some("css") => "text/css",
            Some("js") => "application/javascript",
            Some("json") => "application/json",
            Some("png") => "image/png",
            Some("svg") => "image/svg+xml",
            Some("woff2") => "font/woff2",
            _ => "application/octet-stream",
        }
        .to_string();

        Some((content, content_type))
    }
}

// ---------------------------------------------------------------------------
// Rhai response parsing
// ---------------------------------------------------------------------------

/// Response from a Rhai handler.
pub struct RhaiResponse {
    pub status: u16,
    pub content_type: String,
    pub body: String,
    pub headers: HashMap<String, String>,
}

fn parse_rhai_response(result: Dynamic) -> Result<RhaiResponse, String> {
    // If the result is a Map, treat it as a structured response
    if let Some(map) = result.clone().try_cast::<Map>() {
        let status = map.get("status")
            .and_then(|v| v.as_int().ok())
            .unwrap_or(200) as u16;
        let content_type = map.get("content_type")
            .and_then(|v| v.clone().into_string().ok())
            .unwrap_or_else(|| "application/json".to_string());
        let body = map.get("body")
            .map(|v| {
                if v.is_map() || v.is_array() {
                    serde_json::to_string(&rhai_to_json(v.clone())).unwrap_or_default()
                } else {
                    v.to_string()
                }
            })
            .unwrap_or_default();

        let mut headers = HashMap::new();
        if let Some(h) = map.get("headers").and_then(|v| v.clone().try_cast::<Map>()) {
            for (k, v) in h {
                headers.insert(k.to_string(), v.to_string());
            }
        }

        return Ok(RhaiResponse { status, content_type, body, headers });
    }

    // If it's a string, return it as plain text
    if let Ok(s) = result.clone().into_string() {
        return Ok(RhaiResponse {
            status: 200,
            content_type: "text/plain".into(),
            body: s,
            headers: HashMap::new(),
        });
    }

    // Otherwise try to JSON-serialize it
    let json = rhai_to_json(result);
    Ok(RhaiResponse {
        status: 200,
        content_type: "application/json".into(),
        body: serde_json::to_string(&json).unwrap_or("null".into()),
        headers: HashMap::new(),
    })
}

/// Convert a Rhai Dynamic value to serde_json::Value.
fn rhai_to_json(val: Dynamic) -> serde_json::Value {
    if val.is_unit() {
        serde_json::Value::Null
    } else if let Ok(b) = val.as_bool() {
        serde_json::Value::Bool(b)
    } else if let Ok(i) = val.as_int() {
        serde_json::json!(i)
    } else if let Ok(f) = val.as_float() {
        serde_json::json!(f)
    } else if let Ok(s) = val.clone().into_string() {
        serde_json::Value::String(s)
    } else if let Some(arr) = val.clone().try_cast::<rhai::Array>() {
        serde_json::Value::Array(arr.into_iter().map(rhai_to_json).collect())
    } else if let Some(map) = val.clone().try_cast::<Map>() {
        let obj: serde_json::Map<String, serde_json::Value> = map
            .into_iter()
            .map(|(k, v)| (k.to_string(), rhai_to_json(v)))
            .collect();
        serde_json::Value::Object(obj)
    } else {
        serde_json::Value::String(val.to_string())
    }
}

/// Convert serde_json::Value to a Rhai Dynamic.
pub fn json_to_rhai(val: &serde_json::Value) -> Dynamic {
    match val {
        serde_json::Value::Null => Dynamic::UNIT,
        serde_json::Value::Bool(b) => Dynamic::from(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Dynamic::from(i)
            } else if let Some(f) = n.as_f64() {
                Dynamic::from(f)
            } else {
                Dynamic::from(n.to_string())
            }
        }
        serde_json::Value::String(s) => Dynamic::from(s.clone()),
        serde_json::Value::Array(arr) => {
            let rhai_arr: rhai::Array = arr.iter().map(json_to_rhai).collect();
            Dynamic::from(rhai_arr)
        }
        serde_json::Value::Object(obj) => {
            let mut map = Map::new();
            for (k, v) in obj {
                map.insert(k.clone().into(), json_to_rhai(v));
            }
            Dynamic::from(map)
        }
    }
}

// ---------------------------------------------------------------------------
// Rhai engine setup with API functions
// ---------------------------------------------------------------------------

fn create_engine(db_path: PathBuf, skills_dir: PathBuf) -> Engine {
    let mut engine = Engine::new();

    // --- Route registration helper ---
    // Skills call: register_route("GET", "/path", "handler_fn_name")
    engine.register_fn("register_route", |method: String, path: String, handler: String| -> Map {
        let mut route = Map::new();
        route.insert("method".into(), Dynamic::from(method));
        route.insert("path".into(), Dynamic::from(path));
        route.insert("handler".into(), Dynamic::from(handler));
        route
    });

    // --- Response helpers ---
    engine.register_fn("json_response", |data: Dynamic| -> Map {
        let mut resp = Map::new();
        resp.insert("status".into(), Dynamic::from(200_i64));
        resp.insert("content_type".into(), Dynamic::from("application/json".to_string()));
        resp.insert("body".into(), data);
        resp
    });

    engine.register_fn("json_response", |status: i64, data: Dynamic| -> Map {
        let mut resp = Map::new();
        resp.insert("status".into(), Dynamic::from(status));
        resp.insert("content_type".into(), Dynamic::from("application/json".to_string()));
        resp.insert("body".into(), data);
        resp
    });

    engine.register_fn("html_response", |html: String| -> Map {
        let mut resp = Map::new();
        resp.insert("status".into(), Dynamic::from(200_i64));
        resp.insert("content_type".into(), Dynamic::from("text/html".to_string()));
        resp.insert("body".into(), Dynamic::from(html));
        resp
    });

    engine.register_fn("text_response", |text: String| -> Map {
        let mut resp = Map::new();
        resp.insert("status".into(), Dynamic::from(200_i64));
        resp.insert("content_type".into(), Dynamic::from("text/plain".to_string()));
        resp.insert("body".into(), Dynamic::from(text));
        resp
    });

    engine.register_fn("error_response", |status: i64, message: String| -> Map {
        let mut resp = Map::new();
        resp.insert("status".into(), Dynamic::from(status));
        resp.insert("content_type".into(), Dynamic::from("application/json".to_string()));
        let mut body = Map::new();
        body.insert("error".into(), Dynamic::from(message));
        resp.insert("body".into(), Dynamic::from(body));
        resp
    });

    // --- HTTP client functions (with URL validation) ---
    // Blocks file://, private networks, and localhost to prevent SSRF.
    engine.register_fn("http_get", |url: String| -> Dynamic {
        if let Err(e) = crate::security::validate_url(&url) {
            tracing::warn!(url = %url, err = %e, "http_get: URL blocked");
            return Dynamic::from(format!("error: {e}"));
        }
        match reqwest::blocking::get(&url) {
            Ok(resp) => {
                if let Ok(text) = resp.text() {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                        json_to_rhai(&json)
                    } else {
                        Dynamic::from(text)
                    }
                } else {
                    Dynamic::UNIT
                }
            }
            Err(e) => Dynamic::from(format!("error: {e}")),
        }
    });

    engine.register_fn("http_post", |url: String, body: String| -> Dynamic {
        if let Err(e) = crate::security::validate_url(&url) {
            tracing::warn!(url = %url, err = %e, "http_post: URL blocked");
            return Dynamic::from(format!("error: {e}"));
        }
        let client = reqwest::blocking::Client::new();
        match client.post(&url)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
        {
            Ok(resp) => {
                if let Ok(text) = resp.text() {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                        json_to_rhai(&json)
                    } else {
                        Dynamic::from(text)
                    }
                } else {
                    Dynamic::UNIT
                }
            }
            Err(e) => Dynamic::from(format!("error: {e}")),
        }
    });

    engine.register_fn("http_post", |url: String, body: Map| -> Dynamic {
        if let Err(e) = crate::security::validate_url(&url) {
            tracing::warn!(url = %url, err = %e, "http_post: URL blocked");
            return Dynamic::from(format!("error: {e}"));
        }
        let json_body = rhai_to_json(Dynamic::from(body));
        let client = reqwest::blocking::Client::new();
        match client.post(&url)
            .json(&json_body)
            .send()
        {
            Ok(resp) => {
                if let Ok(text) = resp.text() {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                        json_to_rhai(&json)
                    } else {
                        Dynamic::from(text)
                    }
                } else {
                    Dynamic::UNIT
                }
            }
            Err(e) => Dynamic::from(format!("error: {e}")),
        }
    });

    // --- File I/O (jailed to skills directory) ---
    // All data_* functions validate paths are inside the skills directory tree.
    // This prevents path traversal attacks from Rhai extensions.
    let jail_root = skills_dir.clone();
    let jail_r = jail_root.clone();
    engine.register_fn("data_read", move |path: String| -> Dynamic {
        let jail = match crate::security::PathJail::new(jail_r.clone()) {
            Some(j) => j,
            None => return Dynamic::from("error: cannot initialize path jail"),
        };
        match jail.validate(&path) {
            Some(safe_path) => match std::fs::read_to_string(&safe_path) {
                Ok(content) => Dynamic::from(content),
                Err(_) => Dynamic::UNIT,
            },
            None => {
                tracing::warn!(path = %path, "data_read: path rejected by jail");
                Dynamic::UNIT
            }
        }
    });

    let jail_w = jail_root.clone();
    engine.register_fn("data_write", move |path: String, content: String| -> bool {
        let jail = match crate::security::PathJail::new(jail_w.clone()) {
            Some(j) => j,
            None => return false,
        };
        match jail.validate(&path) {
            Some(safe_path) => {
                if let Some(parent) = safe_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                std::fs::write(&safe_path, content).is_ok()
            }
            None => {
                tracing::warn!(path = %path, "data_write: path rejected by jail");
                false
            }
        }
    });

    let jail_l = jail_root.clone();
    engine.register_fn("data_list", move |dir: String| -> rhai::Array {
        let jail = match crate::security::PathJail::new(jail_l.clone()) {
            Some(j) => j,
            None => return rhai::Array::new(),
        };
        let mut result = rhai::Array::new();
        match jail.validate(&dir) {
            Some(safe_dir) => {
                if let Ok(entries) = std::fs::read_dir(&safe_dir) {
                    for entry in entries.flatten() {
                        if let Some(name) = entry.file_name().to_str() {
                            result.push(Dynamic::from(name.to_string()));
                        }
                    }
                }
            }
            None => {
                tracing::warn!(dir = %dir, "data_list: path rejected by jail");
            }
        }
        result
    });

    let jail_e = jail_root.clone();
    engine.register_fn("data_exists", move |path: String| -> bool {
        let jail = match crate::security::PathJail::new(jail_e.clone()) {
            Some(j) => j,
            None => return false,
        };
        match jail.validate(&path) {
            Some(safe_path) => safe_path.exists(),
            None => {
                tracing::warn!(path = %path, "data_exists: path rejected by jail");
                false
            }
        }
    });

    // --- data_delete (moves to trash) ---
    let jail_d = jail_root.clone();
    let trash_data_dir = skills_dir
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .to_path_buf();
    engine.register_fn("data_delete", move |path: String| -> bool {
        let jail = match crate::security::PathJail::new(jail_d.clone()) {
            Some(j) => j,
            None => return false,
        };
        match jail.validate(&path) {
            Some(safe_path) => {
                if !safe_path.exists() {
                    tracing::warn!(path = %path, "data_delete: file not found");
                    return false;
                }
                match crate::trash::TrashManager::new(&trash_data_dir) {
                    Ok(trash) => match trash.trash(&safe_path, "rhai:data_delete") {
                        Ok(_) => true,
                        Err(e) => {
                            tracing::error!(path = %path, err = %e, "data_delete: trash failed");
                            false
                        }
                    },
                    Err(e) => {
                        tracing::error!(err = %e, "data_delete: cannot init trash");
                        false
                    }
                }
            }
            None => {
                tracing::warn!(path = %path, "data_delete: path rejected by jail");
                false
            }
        }
    });

    // --- JSON helpers ---
    engine.register_fn("json_parse", |text: String| -> Dynamic {
        match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(val) => json_to_rhai(&val),
            Err(_) => Dynamic::UNIT,
        }
    });

    engine.register_fn("json_stringify", |val: Dynamic| -> String {
        serde_json::to_string(&rhai_to_json(val)).unwrap_or_else(|_| "null".into())
    });

    engine.register_fn("json_stringify_pretty", |val: Dynamic| -> String {
        serde_json::to_string_pretty(&rhai_to_json(val)).unwrap_or_else(|_| "null".into())
    });

    // --- Environment (restricted to safe variables) ---
    engine.register_fn("env_get", |key: String| -> Dynamic {
        if !crate::security::is_safe_env_var(&key) {
            tracing::warn!(key = %key, "env_get: blocked access to sensitive variable");
            return Dynamic::UNIT;
        }
        match std::env::var(&key) {
            Ok(val) => Dynamic::from(val),
            Err(_) => Dynamic::UNIT,
        }
    });

    // --- Timestamp ---
    engine.register_fn("now_utc", || -> String {
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
    });

    engine.register_fn("now_epoch", || -> i64 {
        chrono::Utc::now().timestamp()
    });

    // --- Database access (with SQL validation) ---
    // db_query: read-only; only SELECT/WITH/EXPLAIN allowed.
    // db_execute: write; blocks DROP, ALTER, ATTACH, PRAGMA writes, etc.
    let db_path_query = db_path.clone();
    engine.register_fn("db_query", move |sql: String| -> Dynamic {
        if let Err(e) = crate::security::validate_sql_readonly(&sql) {
            tracing::warn!(sql = %sql, err = %e, "db_query: SQL blocked");
            return Dynamic::from(format!("error: {e}"));
        }
        db_execute_query(&db_path_query, &sql, &[])
    });

    let db_path_query2 = db_path.clone();
    engine.register_fn("db_query", move |sql: String, params: rhai::Array| -> Dynamic {
        if let Err(e) = crate::security::validate_sql_readonly(&sql) {
            tracing::warn!(sql = %sql, err = %e, "db_query: SQL blocked");
            return Dynamic::from(format!("error: {e}"));
        }
        let str_params: Vec<String> = params
            .iter()
            .map(|p| p.to_string())
            .collect();
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = str_params
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();
        db_execute_query(&db_path_query2, &sql, &param_refs)
    });

    let db_path_exec = db_path.clone();
    engine.register_fn("db_execute", move |sql: String| -> Dynamic {
        if let Err(e) = crate::security::validate_sql(&sql) {
            tracing::warn!(sql = %sql, err = %e, "db_execute: SQL blocked");
            return Dynamic::from(format!("error: {e}"));
        }
        db_execute_stmt(&db_path_exec, &sql, &[])
    });

    let db_path_exec2 = db_path;
    engine.register_fn("db_execute", move |sql: String, params: rhai::Array| -> Dynamic {
        if let Err(e) = crate::security::validate_sql(&sql) {
            tracing::warn!(sql = %sql, err = %e, "db_execute: SQL blocked");
            return Dynamic::from(format!("error: {e}"));
        }
        let str_params: Vec<String> = params
            .iter()
            .map(|p| p.to_string())
            .collect();
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = str_params
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();
        db_execute_stmt(&db_path_exec2, &sql, &param_refs)
    });

    engine
}

/// Execute a SQL query and return results as an array of maps.
fn db_execute_query(db_path: &Path, sql: &str, params: &[&dyn rusqlite::types::ToSql]) -> Dynamic {
    let conn = match Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(e) => return Dynamic::from(format!("db open error: {e}")),
    };

    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(e) => return Dynamic::from(format!("sql error: {e}")),
    };

    let col_names: Vec<String> = stmt
        .column_names()
        .iter()
        .map(|n| n.to_string())
        .collect();

    let rows: Vec<Dynamic> = match stmt.query_map(params, |row| {
        let mut map = Map::new();
        for (i, name) in col_names.iter().enumerate() {
            let val: rusqlite::Result<String> = row.get(i);
            match val {
                Ok(s) => { map.insert(name.clone().into(), Dynamic::from(s)); }
                Err(_) => {
                    // Try as integer
                    let ival: rusqlite::Result<i64> = row.get(i);
                    match ival {
                        Ok(n) => { map.insert(name.clone().into(), Dynamic::from(n)); }
                        Err(_) => { map.insert(name.clone().into(), Dynamic::UNIT); }
                    }
                }
            }
        }
        Ok(Dynamic::from(map))
    }) {
        Ok(mapped) => mapped.filter_map(|r| r.ok()).collect(),
        Err(e) => return Dynamic::from(format!("query error: {e}")),
    };

    Dynamic::from(rows)
}

/// Execute a SQL statement (INSERT, UPDATE, DELETE) and return rows affected.
fn db_execute_stmt(db_path: &Path, sql: &str, params: &[&dyn rusqlite::types::ToSql]) -> Dynamic {
    let conn = match Connection::open(db_path) {
        Ok(c) => c,
        Err(e) => return Dynamic::from(format!("db open error: {e}")),
    };

    match conn.execute(sql, params) {
        Ok(changed) => Dynamic::from(changed as i64),
        Err(e) => Dynamic::from(format!("execute error: {e}")),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn load_ui_config(skill_dir: &Path) -> SkillUiConfig {
    let manifest_path = skill_dir.join("skill.toml");
    if !manifest_path.exists() {
        return SkillUiConfig::default();
    }

    let content = match std::fs::read_to_string(&manifest_path) {
        Ok(c) => c,
        Err(_) => return SkillUiConfig::default(),
    };

    #[derive(serde::Deserialize)]
    struct Wrapper {
        #[serde(default)]
        ui: SkillUiConfig,
    }

    toml::from_str::<Wrapper>(&content)
        .map(|w| w.ui)
        .unwrap_or_default()
}

/// Info about a skill's extensions (for API responses).
#[derive(serde::Serialize)]
pub struct SkillExtensionInfo {
    pub skill_name: String,
    pub route_count: usize,
    pub routes: Vec<String>,
    pub ui: SkillUiConfig,
}
