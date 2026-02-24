use async_trait::async_trait;
use tracing::debug;

use super::{Tool, ToolContext, ToolOutput};
use crate::error::Result;

// -- WebSearch (DuckDuckGo) ----------------------------------------------

pub struct WebSearchTool {
    max_results: usize,
}

impl WebSearchTool {
    pub fn new(max_results: usize) -> Self {
        Self { max_results }
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web using DuckDuckGo. Returns a list of results with titles, URLs, and snippets."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results (default 10)"
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let query = params
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        if query.is_empty() {
            return Ok(ToolOutput::error("query is required"));
        }

        let limit = params
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(self.max_results as u64) as usize;

        debug!(query, limit, "searching DuckDuckGo");

        // Use DuckDuckGo HTML search (no API key needed)
        let url = format!(
            "https://html.duckduckgo.com/html/?q={}",
            urlencoding(query)
        );

        let resp = ctx
            .http_client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0 (compatible; SafeClaw/0.1)")
            .send()
            .await;

        match resp {
            Ok(r) => {
                let body = r.text().await.unwrap_or_default();
                let results = parse_ddg_html(&body, limit);
                if results.is_empty() {
                    Ok(ToolOutput::ok("No results found."))
                } else {
                    let mut out = String::new();
                    for (i, (title, url, snippet)) in results.iter().enumerate() {
                        out.push_str(&format!(
                            "{}. {}\n   {}\n   {}\n\n",
                            i + 1,
                            title,
                            url,
                            snippet,
                        ));
                    }
                    Ok(ToolOutput::ok(out))
                }
            }
            Err(e) => Ok(ToolOutput::error(format!("search failed: {e}"))),
        }
    }
}

/// Parse DuckDuckGo HTML search results page.
fn parse_ddg_html(html: &str, limit: usize) -> Vec<(String, String, String)> {
    let mut results = Vec::new();

    // Simple extraction of result blocks from DDG HTML
    for chunk in html.split("class=\"result__a\"").skip(1).take(limit) {
        let title = extract_between(chunk, ">", "</a>")
            .map(|s| strip_tags(&s))
            .unwrap_or_default();
        let url = extract_between(chunk, "href=\"", "\"").unwrap_or_default();
        let snippet = if let Some(s_start) = chunk.find("class=\"result__snippet\"") {
            let after = &chunk[s_start..];
            extract_between(after, ">", "</")
                .map(|s| strip_tags(&s))
                .unwrap_or_default()
        } else {
            String::new()
        };

        if !title.is_empty() {
            // DDG redirects through their URL; extract the actual URL
            let actual_url = if url.contains("uddg=") {
                url.split("uddg=")
                    .nth(1)
                    .and_then(|s| s.split('&').next())
                    .map(|s| urldecoding(s))
                    .unwrap_or(url)
            } else {
                url
            };
            results.push((title, actual_url, snippet));
        }
    }

    results
}

fn extract_between(text: &str, start: &str, end: &str) -> Option<String> {
    let s = text.find(start)?;
    let after = &text[s + start.len()..];
    let e = after.find(end)?;
    Some(after[..e].to_string())
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

pub fn urlencoding(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ' ' => "+".to_string(),
            _ => format!("%{:02X}", c as u32),
        })
        .collect()
}

fn urldecoding(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                out.push(byte as char);
            }
        } else if c == '+' {
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

// -- WebFetch ------------------------------------------------------------

pub struct WebFetchTool;

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetch a URL and return its content as readable text/markdown. Useful for reading web pages."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["url"],
            "properties": {
                "url": {
                    "type": "string",
                    "description": "URL to fetch"
                },
                "max_chars": {
                    "type": "integer",
                    "description": "Maximum characters to return (default 50000)"
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let url = params
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let max_chars = params
            .get("max_chars")
            .and_then(|v| v.as_u64())
            .unwrap_or(50_000) as usize;

        if url.is_empty() {
            return Ok(ToolOutput::error("url is required"));
        }

        debug!(url, max_chars, "fetching URL");

        let resp = ctx
            .http_client
            .get(url)
            .header("User-Agent", "Mozilla/5.0 (compatible; SafeClaw/0.1)")
            .send()
            .await;

        match resp {
            Ok(r) => {
                let content_type = r
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string();

                let body = r.text().await.unwrap_or_default();

                let text = if content_type.contains("text/html") {
                    htmd::convert(&body).unwrap_or(body)
                } else {
                    body
                };

                let truncated = if text.len() > max_chars {
                    format!("{}...\n[truncated at {} chars]", &text[..max_chars], max_chars)
                } else {
                    text
                };

                Ok(ToolOutput::ok(truncated))
            }
            Err(e) => Ok(ToolOutput::error(format!("fetch failed: {e}"))),
        }
    }
}
