//! Embedded Rhai scripting runtime for `.rhai` skills.
//!
//! Rhai scripts run in-process on a blocking thread (via `spawn_blocking`)
//! with a rich API surface: HTTP, file I/O, environment, Telegram, sleep
//! with cooperative cancellation, and logging to the skill's log file.

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rhai::{Dynamic, Engine, EvalAltResult, Map};

/// Shared context passed into every registered Rhai function.
#[derive(Clone)]
pub struct RhaiSkillCtx {
    /// Set to `true` when the skill should stop.
    pub cancel: Arc<AtomicBool>,
    /// Skill-level environment variables (manifest env + credentials + system).
    pub env_vars: HashMap<String, String>,
    /// Writable data directory for this skill.
    pub data_dir: PathBuf,
    /// Append-only log file handle.
    pub log_file: Arc<Mutex<std::fs::File>>,
    /// Telegram bot token (if configured).
    pub telegram_token: Option<String>,
    /// Telegram chat ID (if configured).
    pub telegram_chat_id: Option<String>,
}

/// Build a Rhai `Engine` with the skill API registered.
pub fn build_engine(ctx: Arc<RhaiSkillCtx>) -> Engine {
    let mut engine = Engine::new();

    // Redirect `print` and `debug` to the skill log file.
    {
        let log = ctx.log_file.clone();
        engine.on_print(move |s| {
            if let Ok(mut f) = log.lock() {
                let _ = writeln!(f, "{s}");
                let _ = f.flush();
            }
        });
    }
    {
        let log = ctx.log_file.clone();
        engine.on_debug(move |s, src, pos| {
            let loc = match src {
                Some(src) => format!("{src} @ {pos}"),
                None => format!("{pos}"),
            };
            if let Ok(mut f) = log.lock() {
                let _ = writeln!(f, "[DEBUG {loc}] {s}");
                let _ = f.flush();
            }
        });
    }

    // -- env(key) -> String ------------------------------------------------
    {
        let c = ctx.clone();
        engine.register_fn("env", move |key: &str| -> String {
            c.env_vars.get(key).cloned().unwrap_or_default()
        });
    }

    // -- timestamp() -> String (RFC 3339 UTC) ------------------------------
    engine.register_fn("timestamp", || -> String {
        chrono::Utc::now().to_rfc3339()
    });

    // -- sleep_ms(ms) — cooperative sleep that checks cancellation ---------
    {
        let c = ctx.clone();
        engine.register_fn("sleep_ms", move |ms: i64| -> Result<(), Box<EvalAltResult>> {
            check_cancel(&c)?;
            let total = Duration::from_millis(ms.max(0) as u64);
            let tick = Duration::from_millis(100);
            let start = Instant::now();
            while start.elapsed() < total {
                if c.cancel.load(Ordering::Relaxed) {
                    return Err("skill cancelled".into());
                }
                let remaining = total.saturating_sub(start.elapsed());
                std::thread::sleep(tick.min(remaining));
            }
            Ok(())
        });
    }

    // -- http_get(url) -> String -------------------------------------------
    {
        let c = ctx.clone();
        engine.register_fn("http_get", move |url: &str| -> Result<String, Box<EvalAltResult>> {
            check_cancel(&c)?;
            reqwest::blocking::get(url)
                .and_then(|r| r.text())
                .map_err(|e| format!("http_get failed: {e}").into())
        });
    }

    // -- http_post(url, body, content_type) -> String ----------------------
    {
        let c = ctx.clone();
        engine.register_fn(
            "http_post",
            move |url: &str, body: &str, content_type: &str| -> Result<String, Box<EvalAltResult>> {
                check_cancel(&c)?;
                let client = reqwest::blocking::Client::new();
                client
                    .post(url)
                    .header("Content-Type", content_type)
                    .body(body.to_string())
                    .send()
                    .and_then(|r| r.text())
                    .map_err(|e| format!("http_post failed: {e}").into())
            },
        );
    }

    // -- http_post_json(url, map) -> String --------------------------------
    {
        let c = ctx.clone();
        engine.register_fn(
            "http_post_json",
            move |url: &str, data: Map| -> Result<String, Box<EvalAltResult>> {
                check_cancel(&c)?;
                let json: serde_json::Value = rhai_map_to_json(&data);
                let client = reqwest::blocking::Client::new();
                client
                    .post(url)
                    .json(&json)
                    .send()
                    .and_then(|r| r.text())
                    .map_err(|e| format!("http_post_json failed: {e}").into())
            },
        );
    }

    // -- parse_json(text) -> Map -------------------------------------------
    engine.register_fn(
        "parse_json",
        |text: &str| -> Result<Dynamic, Box<EvalAltResult>> {
            let val: serde_json::Value = serde_json::from_str(text)
                .map_err(|e| format!("parse_json: {e}"))?;
            Ok(json_to_rhai(&val))
        },
    );

    // -- to_json(value) -> String ------------------------------------------
    engine.register_fn("to_json", |val: Dynamic| -> Result<String, Box<EvalAltResult>> {
        let json = rhai_dynamic_to_json(&val);
        serde_json::to_string(&json).map_err(|e| format!("to_json: {e}").into())
    });

    // -- read_file(relative_path) -> String --------------------------------
    {
        let c = ctx.clone();
        engine.register_fn("read_file", move |path: &str| -> Result<String, Box<EvalAltResult>> {
            let full = c.data_dir.join(path);
            if !full.starts_with(&c.data_dir) {
                return Err("path escapes data directory".into());
            }
            std::fs::read_to_string(&full)
                .map_err(|e| format!("read_file({path}): {e}").into())
        });
    }

    // -- write_file(relative_path, content) --------------------------------
    {
        let c = ctx.clone();
        engine.register_fn(
            "write_file",
            move |path: &str, content: &str| -> Result<(), Box<EvalAltResult>> {
                let full = c.data_dir.join(path);
                if !full.starts_with(&c.data_dir) {
                    return Err("path escapes data directory".into());
                }
                if let Some(parent) = full.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                std::fs::write(&full, content)
                    .map_err(|e| format!("write_file({path}): {e}").into())
            },
        );
    }

    // -- file_exists(relative_path) -> bool --------------------------------
    {
        let c = ctx.clone();
        engine.register_fn("file_exists", move |path: &str| -> bool {
            let full = c.data_dir.join(path);
            full.starts_with(&c.data_dir) && full.exists()
        });
    }

    // -- send_telegram(text) -> bool ---------------------------------------
    {
        let c = ctx.clone();
        engine.register_fn(
            "send_telegram",
            move |text: &str| -> Result<bool, Box<EvalAltResult>> {
                let token = match &c.telegram_token {
                    Some(t) => t,
                    None => return Err("TELEGRAM_BOT_TOKEN not configured".into()),
                };
                let chat_id = match &c.telegram_chat_id {
                    Some(id) => id,
                    None => return Err("TELEGRAM_CHAT_ID not configured".into()),
                };
                let url = format!("https://api.telegram.org/bot{token}/sendMessage");
                let client = reqwest::blocking::Client::new();
                let resp = client
                    .post(&url)
                    .json(&serde_json::json!({
                        "chat_id": chat_id,
                        "text": text,
                        "parse_mode": "HTML",
                    }))
                    .send()
                    .map_err(|e| format!("send_telegram: {e}"))?;
                Ok(resp.status().is_success())
            },
        );
    }

    // -- log(msg) — explicit log write (same as print but prefixed) --------
    {
        let log = ctx.log_file.clone();
        engine.register_fn("log", move |msg: &str| {
            if let Ok(mut f) = log.lock() {
                let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
                let _ = writeln!(f, "[{ts}] {msg}");
                let _ = f.flush();
            }
        });
    }

    // -- is_cancelled() -> bool --------------------------------------------
    {
        let c = ctx.clone();
        engine.register_fn("is_cancelled", move || -> bool {
            c.cancel.load(Ordering::Relaxed)
        });
    }

    engine
}

/// Compile and run a `.rhai` script file.  Returns when the script finishes
/// or when cancellation is detected inside a `sleep_ms` / `check_cancel`.
pub fn run_script(engine: &Engine, script_path: &std::path::Path) -> Result<(), String> {
    let ast = engine
        .compile_file(script_path.into())
        .map_err(|e| format!("compile error: {e}"))?;
    engine
        .run_ast(&ast)
        .map_err(|e| format!("runtime error: {e}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn check_cancel(ctx: &RhaiSkillCtx) -> Result<(), Box<EvalAltResult>> {
    if ctx.cancel.load(Ordering::Relaxed) {
        Err("skill cancelled".into())
    } else {
        Ok(())
    }
}

/// Convert a Rhai `Map` to a `serde_json::Value`.
fn rhai_map_to_json(map: &Map) -> serde_json::Value {
    let obj: serde_json::Map<String, serde_json::Value> = map
        .iter()
        .map(|(k, v)| (k.to_string(), rhai_dynamic_to_json(v)))
        .collect();
    serde_json::Value::Object(obj)
}

/// Convert an arbitrary Rhai `Dynamic` to `serde_json::Value`.
fn rhai_dynamic_to_json(val: &Dynamic) -> serde_json::Value {
    if val.is::<i64>() {
        serde_json::Value::Number(serde_json::Number::from(val.as_int().unwrap_or(0)))
    } else if val.is::<f64>() {
        serde_json::json!(val.as_float().unwrap_or(0.0))
    } else if val.is::<bool>() {
        serde_json::Value::Bool(val.as_bool().unwrap_or(false))
    } else if val.is::<rhai::ImmutableString>() {
        serde_json::Value::String(val.clone().into_string().unwrap_or_default())
    } else if val.is::<rhai::Array>() {
        let arr = val.clone().into_array().unwrap_or_default();
        serde_json::Value::Array(arr.iter().map(rhai_dynamic_to_json).collect())
    } else if val.is::<Map>() {
        let map = val.clone().cast::<Map>();
        rhai_map_to_json(&map)
    } else if val.is_unit() {
        serde_json::Value::Null
    } else {
        serde_json::Value::String(val.to_string())
    }
}

/// Convert a `serde_json::Value` to a Rhai `Dynamic`.
fn json_to_rhai(val: &serde_json::Value) -> Dynamic {
    match val {
        serde_json::Value::Null => Dynamic::UNIT,
        serde_json::Value::Bool(b) => Dynamic::from(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Dynamic::from(i)
            } else {
                Dynamic::from(n.as_f64().unwrap_or(0.0))
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
                map.insert(k.as_str().into(), json_to_rhai(v));
            }
            Dynamic::from(map)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_roundtrip() {
        let json = serde_json::json!({"name": "test", "count": 42, "tags": ["a", "b"]});
        let dyn_val = json_to_rhai(&json);
        let back = rhai_dynamic_to_json(&dyn_val);
        assert_eq!(json, back);
    }
}
