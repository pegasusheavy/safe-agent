use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
use chromiumoxide::page::ScreenshotParams;
use futures::StreamExt;
use tokio::sync::OnceCell;
use tracing::{debug, warn};

use super::{Tool, ToolContext, ToolOutput};
use crate::error::Result;

/// Shared browser state: the `Browser` handle plus the background handler
/// task join-handle.  We keep both so the CDP event loop stays alive for the
/// lifetime of the tool.
struct BrowserState {
    browser: Browser,
    /// The handler task is detached — we hold the handle only so it is
    /// not accidentally dropped (which would kill the CDP connection).
    _handler: tokio::task::JoinHandle<()>,
}

/// Headless browser automation tool via Chrome DevTools Protocol.
///
/// Lazily launches a Chrome / Chromium instance on first use and keeps it
/// alive for the lifetime of the tool.  Each action (navigate, screenshot,
/// snapshot, evaluate) is executed on a single page that is reused across
/// calls — the LLM drives the browser step-by-step.
pub struct BrowserTool {
    headless: bool,
    data_dir: PathBuf,
    state: OnceCell<Arc<BrowserState>>,
}

impl BrowserTool {
    pub fn new(headless: bool, data_dir: PathBuf) -> Self {
        Self {
            headless,
            data_dir,
            state: OnceCell::new(),
        }
    }

    /// Launch Chrome and store the connection.  Returns the shared
    /// `BrowserState` or an error string suitable for `ToolOutput::error`.
    async fn get_or_init(&self) -> std::result::Result<Arc<BrowserState>, String> {
        self.state
            .get_or_try_init(|| async {
                let mut builder = BrowserConfig::builder()
                    .no_sandbox()
                    .arg("--disable-gpu")
                    .arg("--disable-dev-shm-usage");

                if !self.headless {
                    builder = builder.with_head();
                }

                let config = builder.build().map_err(|e| {
                    format!("Chrome/Chromium not found. Install via the binary installer or ensure google-chrome/chromium is on PATH. ({e})")
                })?;

                let (browser, mut handler) =
                    Browser::launch(config).await.map_err(|e| {
                        format!("Chrome/Chromium not found. Install via the binary installer or ensure google-chrome/chromium is on PATH. ({e})")
                    })?;

                // Spawn the CDP handler event-loop in the background.
                let handle = tokio::spawn(async move {
                    while let Some(event) = handler.next().await {
                        if event.is_err() {
                            warn!("browser handler error: {:?}", event.err());
                            break;
                        }
                    }
                });

                Ok(Arc::new(BrowserState {
                    browser,
                    _handler: handle,
                }))
            })
            .await
            .cloned()
    }
}

#[async_trait]
impl Tool for BrowserTool {
    fn name(&self) -> &str {
        "browser"
    }

    fn description(&self) -> &str {
        "Control a headless browser. Actions: navigate, screenshot, snapshot, evaluate. Requires Chrome/Chromium installed."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["navigate", "screenshot", "snapshot", "evaluate"],
                    "description": "Browser action to perform"
                },
                "url": {
                    "type": "string",
                    "description": "URL to navigate to (required for navigate)"
                },
                "script": {
                    "type": "string",
                    "description": "JavaScript to evaluate (required for evaluate)"
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        debug!(action, headless = self.headless, "browser action");

        // Lazily connect to Chrome.
        let state = match self.get_or_init().await {
            Ok(s) => s,
            Err(e) => return Ok(ToolOutput::error(e)),
        };

        match action {
            "navigate" => {
                let url = params
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                if url.is_empty() {
                    return Ok(ToolOutput::error("url is required for navigate"));
                }

                // Navigate with a 30-second timeout.
                let nav_result = tokio::time::timeout(
                    std::time::Duration::from_secs(30),
                    async {
                        let page = state.browser.new_page(url).await.map_err(|e| {
                            format!("Browser error: {e}")
                        })?;
                        let title = page.get_title().await
                            .map_err(|e| format!("Browser error: {e}"))?
                            .unwrap_or_default();
                        let current_url = page.url().await
                            .map_err(|e| format!("Browser error: {e}"))?
                            .unwrap_or_default();
                        Ok::<_, String>((title, current_url))
                    },
                )
                .await;

                match nav_result {
                    Ok(Ok((title, current_url))) => Ok(ToolOutput::ok(format!(
                        "Navigated to {current_url}\nTitle: {title}"
                    ))),
                    Ok(Err(e)) => Ok(ToolOutput::error(e)),
                    Err(_) => Ok(ToolOutput::error(
                        "Browser error: navigation timed out after 30 seconds",
                    )),
                }
            }

            "screenshot" => {
                let screenshot_dir = self.data_dir.join("screenshots");
                if let Err(e) = tokio::fs::create_dir_all(&screenshot_dir).await {
                    return Ok(ToolOutput::error(format!(
                        "Browser error: failed to create screenshots directory: {e}"
                    )));
                }

                let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S%.3f");
                let filename = format!("{timestamp}.png");
                let path = screenshot_dir.join(&filename);

                let result = tokio::time::timeout(
                    std::time::Duration::from_secs(30),
                    async {
                        // Get the list of pages; use the first one (most recently
                        // opened by navigate), or create a blank one.
                        let pages = state.browser.pages().await
                            .map_err(|e| format!("Browser error: {e}"))?;
                        let page = if let Some(p) = pages.into_iter().next() {
                            p
                        } else {
                            state.browser.new_page("about:blank").await
                                .map_err(|e| format!("Browser error: {e}"))?
                        };

                        let png_bytes = page
                            .screenshot(
                                ScreenshotParams::builder()
                                    .format(CaptureScreenshotFormat::Png)
                                    .full_page(true)
                                    .build(),
                            )
                            .await
                            .map_err(|e| format!("Browser error: {e}"))?;

                        tokio::fs::write(&path, &png_bytes).await.map_err(|e| {
                            format!("Browser error: failed to write screenshot: {e}")
                        })?;

                        Ok::<_, String>(path.clone())
                    },
                )
                .await;

                match result {
                    Ok(Ok(p)) => Ok(ToolOutput::ok(format!(
                        "Screenshot saved to {}",
                        p.display()
                    ))),
                    Ok(Err(e)) => Ok(ToolOutput::error(e)),
                    Err(_) => Ok(ToolOutput::error(
                        "Browser error: screenshot timed out after 30 seconds",
                    )),
                }
            }

            "snapshot" => {
                let result = tokio::time::timeout(
                    std::time::Duration::from_secs(30),
                    async {
                        let pages = state.browser.pages().await
                            .map_err(|e| format!("Browser error: {e}"))?;
                        let page = if let Some(p) = pages.into_iter().next() {
                            p
                        } else {
                            return Ok::<_, String>("No page open. Use navigate first.".to_string());
                        };

                        let text: String = page
                            .evaluate("document.body.innerText")
                            .await
                            .map_err(|e| format!("Browser error: {e}"))?
                            .into_value()
                            .map_err(|e| format!("Browser error: {e}"))?;

                        // Truncate to 8000 chars to keep context window manageable.
                        let truncated = if text.len() > 8000 {
                            format!("{}...\n[truncated at 8000 chars]", &text[..8000])
                        } else {
                            text
                        };

                        Ok(truncated)
                    },
                )
                .await;

                match result {
                    Ok(Ok(text)) => Ok(ToolOutput::ok(text)),
                    Ok(Err(e)) => Ok(ToolOutput::error(e)),
                    Err(_) => Ok(ToolOutput::error(
                        "Browser error: snapshot timed out after 30 seconds",
                    )),
                }
            }

            "evaluate" => {
                let script = params
                    .get("script")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                if script.is_empty() {
                    return Ok(ToolOutput::error("script is required for evaluate"));
                }

                let result = tokio::time::timeout(
                    std::time::Duration::from_secs(30),
                    async {
                        let pages = state.browser.pages().await
                            .map_err(|e| format!("Browser error: {e}"))?;
                        let page = if let Some(p) = pages.into_iter().next() {
                            p
                        } else {
                            return Err("No page open. Use navigate first.".to_string());
                        };

                        let eval_result = page
                            .evaluate(script)
                            .await
                            .map_err(|e| format!("Browser error: {e}"))?;

                        // Try to extract a JSON value; fall back to Debug repr.
                        let value: serde_json::Value = eval_result
                            .into_value()
                            .unwrap_or(serde_json::Value::Null);

                        Ok::<_, String>(serde_json::to_string_pretty(&value).unwrap_or_else(|_| {
                            value.to_string()
                        }))
                    },
                )
                .await;

                match result {
                    Ok(Ok(output)) => Ok(ToolOutput::ok(output)),
                    Ok(Err(e)) => Ok(ToolOutput::error(e)),
                    Err(_) => Ok(ToolOutput::error(
                        "Browser error: evaluation timed out after 30 seconds",
                    )),
                }
            }

            other => Ok(ToolOutput::error(format!("unknown browser action: {other}"))),
        }
    }
}
