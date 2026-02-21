use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;

use super::{Tool, ToolContext, ToolOutput};
use crate::error::Result;
use crate::vector::chunker;
use crate::vector::VectorStore;

// ---------------------------------------------------------------------------
// VectorSearchTool
// ---------------------------------------------------------------------------

/// Semantic search across the vector store (memories and/or documents).
pub struct VectorSearchTool {
    pub store: Arc<VectorStore>,
}

#[async_trait]
impl Tool for VectorSearchTool {
    fn name(&self) -> &str {
        "vector_search"
    }

    fn description(&self) -> &str {
        "Search the vector store for content semantically similar to the query. \
         Searches memories, documents, or both."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Natural-language search query"
                },
                "table": {
                    "type": "string",
                    "enum": ["memories", "documents", "all"],
                    "description": "Which table to search (default: all)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Max results to return (default: 5)"
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let query = params
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        if query.is_empty() {
            return Ok(ToolOutput::error("query is required"));
        }

        let table = params
            .get("table")
            .and_then(|v| v.as_str())
            .unwrap_or("all");

        let limit = params
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as usize;

        let results = self.store.search(query, table, limit).await?;

        if results.is_empty() {
            return Ok(ToolOutput::ok("No results found."));
        }

        let formatted: Vec<String> = results
            .iter()
            .enumerate()
            .map(|(i, r)| {
                format!(
                    "{}. [{}] (score: {:.3}) {}",
                    i + 1,
                    r.table,
                    r.score,
                    r.content
                )
            })
            .collect();

        Ok(ToolOutput::ok_with_meta(
            formatted.join("\n\n"),
            serde_json::json!({ "count": results.len() }),
        ))
    }
}

// ---------------------------------------------------------------------------
// VectorIngestTool
// ---------------------------------------------------------------------------

/// Ingest a file into the vector store's documents table.
pub struct VectorIngestTool {
    pub store: Arc<VectorStore>,
}

#[async_trait]
impl Tool for VectorIngestTool {
    fn name(&self) -> &str {
        "vector_ingest"
    }

    fn description(&self) -> &str {
        "Ingest a file into the vector store. Supports text, markdown, code, \
         and PDF files. The file is chunked and each chunk is embedded."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to ingest"
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let path_str = params
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        if path_str.is_empty() {
            return Ok(ToolOutput::error("path is required"));
        }

        let resolved = ctx.sandbox.resolve(Path::new(path_str))?;

        let (chunks, file_type) = chunker::chunk_file(&resolved)?;

        if chunks.is_empty() {
            return Ok(ToolOutput::ok(format!(
                "File is empty or produced no chunks: {path_str}"
            )));
        }

        let count = self
            .store
            .ingest_chunks(&chunks, path_str, file_type)
            .await?;

        Ok(ToolOutput::ok_with_meta(
            format!("Ingested {count} chunks from {path_str} (type: {file_type})"),
            serde_json::json!({
                "chunks": count,
                "file_type": file_type,
                "path": path_str,
            }),
        ))
    }
}

// ---------------------------------------------------------------------------
// VectorRememberTool
// ---------------------------------------------------------------------------

/// Store text as a semantic memory in the vector store.
pub struct VectorRememberTool {
    pub store: Arc<VectorStore>,
}

#[async_trait]
impl Tool for VectorRememberTool {
    fn name(&self) -> &str {
        "vector_remember"
    }

    fn description(&self) -> &str {
        "Store text as a semantic memory in the vector store. Use this for \
         facts, instructions, or context that should be retrievable by \
         meaning later."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["content"],
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The text to remember"
                },
                "category": {
                    "type": "string",
                    "description": "Category label (default: general)"
                },
                "source": {
                    "type": "string",
                    "description": "Source identifier (default: agent)"
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let content = params
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        if content.is_empty() {
            return Ok(ToolOutput::error("content is required"));
        }

        let category = params
            .get("category")
            .and_then(|v| v.as_str())
            .unwrap_or("general");

        let source = params
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("agent");

        let id = self.store.remember(content, category, source).await?;

        Ok(ToolOutput::ok_with_meta(
            format!("Stored memory {id} in category '{category}'"),
            serde_json::json!({
                "id": id,
                "category": category,
                "source": source,
            }),
        ))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::embed::{ApiEmbedder, Embedder};

    async fn dummy_store() -> Arc<VectorStore> {
        let dir = tempfile::tempdir().expect("create temp dir");
        let embedder = Embedder::Api(
            ApiEmbedder::new(
                "test-key".to_string(),
                None,
                Some("http://127.0.0.1:1".to_string()),
                None,
            )
            .expect("create embedder"),
        );
        let store = VectorStore::new(dir.path(), embedder)
            .await
            .expect("create store");
        // Leak the tempdir so it stays alive for the duration of the test.
        std::mem::forget(dir);
        Arc::new(store)
    }

    // -- Tool metadata --------------------------------------------------------

    #[tokio::test]
    async fn search_tool_metadata() {
        let store = dummy_store().await;
        let tool = VectorSearchTool { store };
        assert_eq!(tool.name(), "vector_search");
        assert!(!tool.description().is_empty());

        let schema = tool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("query")));
    }

    #[tokio::test]
    async fn ingest_tool_metadata() {
        let store = dummy_store().await;
        let tool = VectorIngestTool { store };
        assert_eq!(tool.name(), "vector_ingest");
        assert!(!tool.description().is_empty());

        let schema = tool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("path")));
    }

    #[tokio::test]
    async fn remember_tool_metadata() {
        let store = dummy_store().await;
        let tool = VectorRememberTool { store };
        assert_eq!(tool.name(), "vector_remember");
        assert!(!tool.description().is_empty());

        let schema = tool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("content")));
    }

    // -- Parameter validation -------------------------------------------------

    #[tokio::test]
    async fn search_empty_query_returns_error() {
        let store = dummy_store().await;
        let tool = VectorSearchTool { store };
        let ctx = test_ctx();
        let result = tool
            .execute(serde_json::json!({"query": ""}), &ctx)
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.output.contains("query is required"));
    }

    #[tokio::test]
    async fn ingest_empty_path_returns_error() {
        let store = dummy_store().await;
        let tool = VectorIngestTool { store };
        let ctx = test_ctx();
        let result = tool
            .execute(serde_json::json!({"path": ""}), &ctx)
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.output.contains("path is required"));
    }

    #[tokio::test]
    async fn remember_empty_content_returns_error() {
        let store = dummy_store().await;
        let tool = VectorRememberTool { store };
        let ctx = test_ctx();
        let result = tool
            .execute(serde_json::json!({"content": ""}), &ctx)
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.output.contains("content is required"));
    }

    // -- Helpers --------------------------------------------------------------

    fn test_ctx() -> ToolContext {
        use crate::messaging::MessagingManager;
        use crate::security::SandboxedFs;
        use crate::trash::TrashManager;

        let base = std::env::temp_dir().join(format!("sa-vectest-{}", std::process::id()));
        let sandbox_dir = base.join("sandbox");
        let trash_dir = base.join("trash");
        std::fs::create_dir_all(&sandbox_dir).unwrap();
        std::fs::create_dir_all(&trash_dir).unwrap();

        ToolContext {
            sandbox: SandboxedFs::new(sandbox_dir).unwrap(),
            db: Arc::new(tokio::sync::Mutex::new(
                rusqlite::Connection::open_in_memory().unwrap(),
            )),
            http_client: reqwest::Client::new(),
            messaging: Arc::new(MessagingManager::new()),
            trash: Arc::new(TrashManager::new(&trash_dir).unwrap()),
            vector_store: None,
        }
    }
}
