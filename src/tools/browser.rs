use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
use chromiumoxide::page::ScreenshotParams;
use futures::StreamExt;
use tokio::sync::OnceCell;
use tracing::{debug, info, warn};

use super::{Tool, ToolContext, ToolOutput};
use crate::error::Result;

struct BrowserState {
    browser: Browser,
    _handler: tokio::task::JoinHandle<()>,
}

/// Headless browser automation tool via Chrome DevTools Protocol.
///
/// Actions:
/// - `navigate` — open a URL
/// - `auth_navigate` — navigate with OAuth token injection
/// - `screenshot` — full-page screenshot saved to disk
/// - `screenshot_describe` — screenshot + DOM element map for visual grounding
/// - `click_element` — click an element by CSS selector or description
/// - `snapshot` — extract page text content
/// - `evaluate` — run arbitrary JavaScript
/// - `scrape` — structured data extraction via CSS selectors
/// - `bookmark` — save current page to knowledge graph
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

    /// Get the first open page or create a blank one.
    async fn current_page(
        state: &BrowserState,
    ) -> std::result::Result<chromiumoxide::Page, String> {
        let pages = state
            .browser
            .pages()
            .await
            .map_err(|e| format!("Browser error: {e}"))?;
        if let Some(p) = pages.into_iter().next() {
            Ok(p)
        } else {
            state
                .browser
                .new_page("about:blank")
                .await
                .map_err(|e| format!("Browser error: {e}"))
        }
    }
}

#[async_trait]
impl Tool for BrowserTool {
    fn name(&self) -> &str {
        "browser"
    }

    fn description(&self) -> &str {
        "Control a headless browser. Actions: navigate, auth_navigate (with OAuth tokens), \
         screenshot, screenshot_describe (visual grounding with element map), \
         click_element (by CSS selector or index), snapshot (text extraction), \
         evaluate (JS), scrape (CSS selector extraction), bookmark (save page to knowledge graph)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": [
                        "navigate", "auth_navigate", "screenshot",
                        "screenshot_describe", "click_element",
                        "snapshot", "evaluate", "scrape", "bookmark"
                    ],
                    "description": "Browser action to perform"
                },
                "url": {
                    "type": "string",
                    "description": "URL to navigate to (for navigate/auth_navigate)"
                },
                "provider": {
                    "type": "string",
                    "description": "OAuth provider name for auth_navigate (e.g. 'google', 'github')"
                },
                "account": {
                    "type": "string",
                    "description": "OAuth account identifier for auth_navigate"
                },
                "script": {
                    "type": "string",
                    "description": "JavaScript to evaluate (for evaluate action)"
                },
                "selector": {
                    "type": "string",
                    "description": "CSS selector (for click_element, scrape)"
                },
                "element_index": {
                    "type": "integer",
                    "description": "Element index from screenshot_describe output (for click_element)"
                },
                "attributes": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "HTML attributes to extract per element (for scrape, e.g. ['href', 'src'])"
                },
                "title": {
                    "type": "string",
                    "description": "Custom title for bookmark (defaults to page title)"
                },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Tags for the bookmark"
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        debug!(action, headless = self.headless, "browser action");

        let state = match self.get_or_init().await {
            Ok(s) => s,
            Err(e) => return Ok(ToolOutput::error(e)),
        };

        match action {
            // ── navigate ────────────────────────────────────────────────
            "navigate" => {
                let url = params
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                if url.is_empty() {
                    return Ok(ToolOutput::error("url is required for navigate"));
                }

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

            // ── auth_navigate ───────────────────────────────────────────
            "auth_navigate" => {
                let url = params
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let provider = params
                    .get("provider")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let account = params
                    .get("account")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();

                if url.is_empty() {
                    return Ok(ToolOutput::error("url is required for auth_navigate"));
                }
                if provider.is_empty() {
                    return Ok(ToolOutput::error("provider is required for auth_navigate"));
                }

                // Load the OAuth token from the database
                let token = {
                    let db = ctx.db.lock().await;
                    let acct = if account.is_empty() { "default" } else { account };
                    db.query_row(
                        "SELECT access_token FROM oauth_tokens WHERE provider = ?1 AND account = ?2",
                        rusqlite::params![provider, acct],
                        |row| row.get::<_, String>(0),
                    )
                    .ok()
                };

                let access_token = match token {
                    Some(t) => t,
                    None => {
                        return Ok(ToolOutput::error(format!(
                            "No OAuth token found for provider '{provider}' account '{account}'. \
                             Connect the account in the dashboard Settings > OAuth tab."
                        )));
                    }
                };

                let nav_result = tokio::time::timeout(
                    std::time::Duration::from_secs(30),
                    async {
                        let page = state.browser.new_page(url).await.map_err(|e| {
                            format!("Browser error: {e}")
                        })?;

                        // Inject the OAuth token as an Authorization header via CDP
                        // fetch.enable + requestPaused interception.
                        // Simpler approach: set a cookie with the token and inject
                        // via JS for APIs that accept Bearer tokens.
                        let inject_script = format!(
                            r#"
                            (function() {{
                                const origFetch = window.fetch;
                                window.fetch = function(url, opts) {{
                                    opts = opts || {{}};
                                    opts.headers = opts.headers || {{}};
                                    if (opts.headers instanceof Headers) {{
                                        opts.headers.set('Authorization', 'Bearer {token}');
                                    }} else {{
                                        opts.headers['Authorization'] = 'Bearer {token}';
                                    }}
                                    return origFetch.call(this, url, opts);
                                }};

                                const origXHR = XMLHttpRequest.prototype.open;
                                XMLHttpRequest.prototype.open = function() {{
                                    this._authPatched = true;
                                    return origXHR.apply(this, arguments);
                                }};
                                const origSend = XMLHttpRequest.prototype.send;
                                XMLHttpRequest.prototype.send = function() {{
                                    if (this._authPatched) {{
                                        this.setRequestHeader('Authorization', 'Bearer {token}');
                                    }}
                                    return origSend.apply(this, arguments);
                                }};
                            }})();
                            "#,
                            token = access_token.replace('\\', "\\\\").replace('\'', "\\'"),
                        );

                        page.evaluate(inject_script)
                            .await
                            .map_err(|e| format!("Browser error injecting auth: {e}"))?;

                        let title = page
                            .get_title()
                            .await
                            .map_err(|e| format!("Browser error: {e}"))?
                            .unwrap_or_default();
                        let current_url = page
                            .url()
                            .await
                            .map_err(|e| format!("Browser error: {e}"))?
                            .unwrap_or_default();

                        Ok::<_, String>((title, current_url))
                    },
                )
                .await;

                match nav_result {
                    Ok(Ok((title, current_url))) => Ok(ToolOutput::ok(format!(
                        "Authenticated navigation to {current_url}\n\
                         Title: {title}\n\
                         Provider: {provider}\n\
                         OAuth token injected into fetch/XHR requests."
                    ))),
                    Ok(Err(e)) => Ok(ToolOutput::error(e)),
                    Err(_) => Ok(ToolOutput::error(
                        "Browser error: auth_navigate timed out after 30 seconds",
                    )),
                }
            }

            // ── screenshot ──────────────────────────────────────────────
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
                        let page = Self::current_page(&state).await?;
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

            // ── screenshot_describe (visual grounding) ──────────────────
            "screenshot_describe" => {
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
                        let page = Self::current_page(&state).await?;

                        // Take screenshot
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

                        // Extract interactive elements with bounding boxes
                        let element_map: String = page
                            .evaluate(ELEMENT_MAP_SCRIPT)
                            .await
                            .map_err(|e| format!("Browser error: {e}"))?
                            .into_value()
                            .map_err(|e| format!("Browser error: {e}"))?;

                        let page_title = page
                            .get_title()
                            .await
                            .map_err(|e| format!("Browser error: {e}"))?
                            .unwrap_or_default();
                        let page_url = page
                            .url()
                            .await
                            .map_err(|e| format!("Browser error: {e}"))?
                            .unwrap_or_default();

                        Ok::<_, String>((path.clone(), element_map, page_title, page_url))
                    },
                )
                .await;

                match result {
                    Ok(Ok((p, elements, title, url))) => {
                        let truncated = if elements.len() > 6000 {
                            format!("{}...\n[truncated]", &elements[..6000])
                        } else {
                            elements
                        };
                        Ok(ToolOutput::ok(format!(
                            "Screenshot saved to {}\n\
                             URL: {url}\n\
                             Title: {title}\n\n\
                             == INTERACTIVE ELEMENTS ==\n\
                             Use click_element with 'selector' or 'element_index' to click.\n\n\
                             {truncated}",
                            p.display()
                        )))
                    }
                    Ok(Err(e)) => Ok(ToolOutput::error(e)),
                    Err(_) => Ok(ToolOutput::error(
                        "Browser error: screenshot_describe timed out after 30 seconds",
                    )),
                }
            }

            // ── click_element ───────────────────────────────────────────
            "click_element" => {
                let selector = params
                    .get("selector")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let element_index = params
                    .get("element_index")
                    .and_then(|v| v.as_i64());

                if selector.is_empty() && element_index.is_none() {
                    return Ok(ToolOutput::error(
                        "Either 'selector' or 'element_index' is required for click_element",
                    ));
                }

                let result = tokio::time::timeout(
                    std::time::Duration::from_secs(30),
                    async {
                        let page = Self::current_page(&state).await?;

                        let click_script = if !selector.is_empty() {
                            format!(
                                r#"(function() {{
                                    const el = document.querySelector({sel});
                                    if (!el) return 'Element not found: {raw_sel}';
                                    el.scrollIntoView({{behavior: 'smooth', block: 'center'}});
                                    el.click();
                                    return 'Clicked: ' + (el.textContent || el.tagName).trim().substring(0, 100);
                                }})()"#,
                                sel = serde_json::to_string(selector).unwrap_or_default(),
                                raw_sel = selector.replace('\'', "\\'"),
                            )
                        } else {
                            let idx = element_index.unwrap();
                            format!(
                                r#"(function() {{
                                    const all = document.querySelectorAll(
                                        'a, button, input, select, textarea, [role="button"], [onclick], [tabindex]'
                                    );
                                    if ({idx} < 0 || {idx} >= all.length)
                                        return 'Element index {idx} out of range (0-' + (all.length-1) + ')';
                                    const el = all[{idx}];
                                    el.scrollIntoView({{behavior: 'smooth', block: 'center'}});
                                    el.click();
                                    return 'Clicked element [{idx}]: ' + (el.textContent || el.tagName).trim().substring(0, 100);
                                }})()"#,
                                idx = idx,
                            )
                        };

                        let result: String = page
                            .evaluate(click_script)
                            .await
                            .map_err(|e| format!("Browser error: {e}"))?
                            .into_value()
                            .map_err(|e| format!("Browser error: {e}"))?;

                        // Wait briefly for any navigation or DOM updates
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

                        let new_url = page
                            .url()
                            .await
                            .map_err(|e| format!("Browser error: {e}"))?
                            .unwrap_or_default();

                        Ok::<_, String>(format!("{result}\nCurrent URL: {new_url}"))
                    },
                )
                .await;

                match result {
                    Ok(Ok(output)) => Ok(ToolOutput::ok(output)),
                    Ok(Err(e)) => Ok(ToolOutput::error(e)),
                    Err(_) => Ok(ToolOutput::error(
                        "Browser error: click_element timed out after 30 seconds",
                    )),
                }
            }

            // ── snapshot ────────────────────────────────────────────────
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

            // ── evaluate ────────────────────────────────────────────────
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
                        let page = Self::current_page(&state).await?;

                        let eval_result = page
                            .evaluate(script)
                            .await
                            .map_err(|e| format!("Browser error: {e}"))?;

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

            // ── scrape ──────────────────────────────────────────────────
            "scrape" => {
                let selector = params
                    .get("selector")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                if selector.is_empty() {
                    return Ok(ToolOutput::error("selector is required for scrape"));
                }
                let attributes: Vec<String> = params
                    .get("attributes")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();

                let result = tokio::time::timeout(
                    std::time::Duration::from_secs(30),
                    async {
                        let page = Self::current_page(&state).await?;

                        let attrs_json =
                            serde_json::to_string(&attributes).unwrap_or_else(|_| "[]".into());

                        let scrape_script = format!(
                            r#"(function() {{
                                const sel = {sel};
                                const attrs = {attrs};
                                const els = document.querySelectorAll(sel);
                                if (els.length === 0) return JSON.stringify({{
                                    count: 0, elements: [], error: 'No elements matched selector: ' + sel
                                }});
                                const results = [];
                                els.forEach(function(el, i) {{
                                    const item = {{
                                        index: i,
                                        tag: el.tagName.toLowerCase(),
                                        text: (el.textContent || '').trim().substring(0, 500)
                                    }};
                                    if (el.tagName === 'A') item.href = el.href;
                                    if (el.tagName === 'IMG') {{
                                        item.src = el.src;
                                        item.alt = el.alt;
                                    }}
                                    attrs.forEach(function(attr) {{
                                        const val = el.getAttribute(attr);
                                        if (val !== null) item[attr] = val;
                                    }});
                                    results.push(item);
                                }});
                                return JSON.stringify({{count: results.length, elements: results}});
                            }})()"#,
                            sel = serde_json::to_string(selector).unwrap_or_default(),
                            attrs = attrs_json,
                        );

                        let raw: String = page
                            .evaluate(scrape_script)
                            .await
                            .map_err(|e| format!("Browser error: {e}"))?
                            .into_value()
                            .map_err(|e| format!("Browser error: {e}"))?;

                        // Pretty-print the JSON result
                        let parsed: serde_json::Value =
                            serde_json::from_str(&raw).unwrap_or(serde_json::Value::String(raw));
                        let pretty =
                            serde_json::to_string_pretty(&parsed).unwrap_or_else(|_| parsed.to_string());

                        let truncated = if pretty.len() > 10000 {
                            format!("{}...\n[truncated at 10000 chars]", &pretty[..10000])
                        } else {
                            pretty
                        };

                        Ok::<_, String>(truncated)
                    },
                )
                .await;

                match result {
                    Ok(Ok(output)) => Ok(ToolOutput::ok(output)),
                    Ok(Err(e)) => Ok(ToolOutput::error(e)),
                    Err(_) => Ok(ToolOutput::error(
                        "Browser error: scrape timed out after 30 seconds",
                    )),
                }
            }

            // ── bookmark ────────────────────────────────────────────────
            "bookmark" => {
                let custom_title = params
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let tags: Vec<String> = params
                    .get("tags")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();

                let result = tokio::time::timeout(
                    std::time::Duration::from_secs(30),
                    async {
                        let page = Self::current_page(&state).await?;

                        let page_url = page
                            .url()
                            .await
                            .map_err(|e| format!("Browser error: {e}"))?
                            .unwrap_or_default();

                        if page_url.is_empty() || page_url == "about:blank" {
                            return Err("No page open. Use navigate first.".to_string());
                        }

                        let page_title = page
                            .get_title()
                            .await
                            .map_err(|e| format!("Browser error: {e}"))?
                            .unwrap_or_default();

                        // Extract a summary of the page content
                        let page_text: String = page
                            .evaluate(
                                "(document.querySelector('meta[name=\"description\"]')?.content || \
                                 document.body.innerText.substring(0, 1000)).trim()",
                            )
                            .await
                            .map_err(|e| format!("Browser error: {e}"))?
                            .into_value()
                            .map_err(|e| format!("Browser error: {e}"))?;

                        let title = if custom_title.is_empty() {
                            &page_title
                        } else {
                            custom_title
                        };

                        Ok::<_, String>((
                            page_url,
                            title.to_string(),
                            page_text,
                            tags.clone(),
                        ))
                    },
                )
                .await;

                let (url, title, description, tag_list) = match result {
                    Ok(Ok(data)) => data,
                    Ok(Err(e)) => return Ok(ToolOutput::error(e)),
                    Err(_) => {
                        return Ok(ToolOutput::error(
                            "Browser error: bookmark timed out after 30 seconds",
                        ))
                    }
                };

                // Store in knowledge graph
                let db = ctx.db.lock().await;

                // Create the bookmark node
                let truncated_desc = if description.len() > 1000 {
                    format!("{}...", &description[..1000])
                } else {
                    description
                };

                let node_content = format!("URL: {url}\n\n{truncated_desc}");
                db.execute(
                    "INSERT INTO knowledge_nodes (label, node_type, content, confidence) VALUES (?1, 'bookmark', ?2, 1.0)",
                    rusqlite::params![title, node_content],
                )?;
                let node_id = db.last_insert_rowid();

                // Also store in archival memory for searchability
                let archival_content = format!("Bookmarked: {title}\nURL: {url}\n{truncated_desc}");
                db.execute(
                    "INSERT INTO archival_memory (content, category) VALUES (?1, 'bookmark')",
                    [&archival_content],
                )?;

                // Create tag nodes and edges
                for tag in &tag_list {
                    // Find or create the tag node
                    let tag_node_id: i64 = match db.query_row(
                        "SELECT id FROM knowledge_nodes WHERE label = ?1 AND node_type = 'tag'",
                        [tag],
                        |row| row.get(0),
                    ) {
                        Ok(id) => id,
                        Err(_) => {
                            db.execute(
                                "INSERT INTO knowledge_nodes (label, node_type, content, confidence) VALUES (?1, 'tag', '', 1.0)",
                                [tag],
                            )?;
                            db.last_insert_rowid()
                        }
                    };

                    db.execute(
                        "INSERT OR IGNORE INTO knowledge_edges (source_id, target_id, relation, weight) VALUES (?1, ?2, 'tagged', 1.0)",
                        rusqlite::params![node_id, tag_node_id],
                    )?;
                }

                drop(db);

                let tag_info = if tag_list.is_empty() {
                    String::new()
                } else {
                    format!("\nTags: {}", tag_list.join(", "))
                };

                info!(
                    url = %url,
                    title = %title,
                    node_id,
                    tags = ?tag_list,
                    "page bookmarked to knowledge graph"
                );

                Ok(ToolOutput::ok(format!(
                    "Bookmarked: {title}\nURL: {url}\nKnowledge graph node ID: {node_id}{tag_info}\n\
                     Saved to both knowledge graph and archival memory for future retrieval."
                )))
            }

            other => Ok(ToolOutput::error(format!("unknown browser action: {other}"))),
        }
    }
}

/// JavaScript that builds a numbered map of interactive elements with their
/// bounding boxes, text, and attributes. Used by `screenshot_describe` for
/// visual grounding — the LLM can reference elements by index.
const ELEMENT_MAP_SCRIPT: &str = r#"
(function() {
    const interactive = document.querySelectorAll(
        'a, button, input, select, textarea, [role="button"], [onclick], [tabindex], label, details, summary'
    );
    const lines = [];
    interactive.forEach(function(el, i) {
        const rect = el.getBoundingClientRect();
        if (rect.width === 0 && rect.height === 0) return;
        const tag = el.tagName.toLowerCase();
        let desc = '';
        if (tag === 'input') {
            desc = el.type + (el.placeholder ? ' placeholder="' + el.placeholder + '"' : '') + (el.value ? ' value="' + el.value.substring(0, 50) + '"' : '');
        } else if (tag === 'select') {
            const opt = el.options[el.selectedIndex];
            desc = opt ? opt.textContent.trim().substring(0, 50) : '';
        } else if (tag === 'a') {
            desc = (el.textContent || '').trim().substring(0, 80) + ' → ' + (el.href || '').substring(0, 100);
        } else {
            desc = (el.textContent || '').trim().substring(0, 80);
        }
        const x = Math.round(rect.x);
        const y = Math.round(rect.y);
        const w = Math.round(rect.width);
        const h = Math.round(rect.height);
        lines.push('[' + i + '] <' + tag + '> ' + desc + '  @(' + x + ',' + y + ' ' + w + 'x' + h + ')');
    });
    return lines.join('\n');
})()
"#;
