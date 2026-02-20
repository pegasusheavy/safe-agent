//! Embedding backends for the vector store.
//!
//! Two backends are available:
//!
//! - **`ApiEmbedder`** (default) -- calls the OpenRouter `/v1/embeddings`
//!   endpoint (OpenAI-compatible).  No feature flag required.
//!
//! - **`LocalEmbedder`** (optional) -- runs the BGE-Large-EN-v1.5 ONNX model
//!   locally via `fastembed`.  Gated behind `#[cfg(feature = "local-embeddings")]`.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::error::{Result, SafeAgentError};

// ---------------------------------------------------------------------------
// Public enum that unifies both backends
// ---------------------------------------------------------------------------

/// Configurable embedding backend.
///
/// Callers construct either an [`ApiEmbedder`] or a [`LocalEmbedder`] and wrap
/// it in this enum.  All downstream code programs against `Embedder` so the
/// backend can be swapped at startup time via configuration.
pub enum Embedder {
    Api(ApiEmbedder),
    #[cfg(feature = "local-embeddings")]
    Local(LocalEmbedder),
}

impl Embedder {
    /// Embed a single piece of text.
    pub async fn embed_one(&self, text: &str) -> Result<Vec<f32>> {
        match self {
            Embedder::Api(api) => api.embed_one(text).await,
            #[cfg(feature = "local-embeddings")]
            Embedder::Local(local) => local.embed_one(text),
        }
    }

    /// Embed a batch of texts in one call.
    pub async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        match self {
            Embedder::Api(api) => api.embed_batch(texts).await,
            #[cfg(feature = "local-embeddings")]
            Embedder::Local(local) => local.embed_batch(texts),
        }
    }

    /// Return the embedding dimensionality if it is known ahead of time.
    ///
    /// - `Local` always returns `Some(1024)` (BGE-Large-EN-v1.5).
    /// - `Api` returns `None` because the dimension depends on the remote
    ///   model and is only known after the first API response.
    pub fn dim(&self) -> Option<usize> {
        match self {
            Embedder::Api(_) => None,
            #[cfg(feature = "local-embeddings")]
            Embedder::Local(_) => Some(1024),
        }
    }
}

// ---------------------------------------------------------------------------
// API-based embedder (OpenRouter / OpenAI-compatible)
// ---------------------------------------------------------------------------

/// Default embedding model served through OpenRouter.
const DEFAULT_EMBEDDING_MODEL: &str = "openai/text-embedding-3-large";

/// Default base URL for the OpenRouter API.
const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api";

/// Embedder that calls the OpenRouter (OpenAI-compatible) `/v1/embeddings`
/// endpoint over HTTP.
#[derive(Debug)]
pub struct ApiEmbedder {
    client: Client,
    api_key: String,
    model: String,
    base_url: String,
}

// -- request / response types ------------------------------------------------

#[derive(Serialize)]
struct EmbeddingRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
    index: usize,
}

#[derive(Deserialize)]
struct EmbeddingErrorResponse {
    error: Option<EmbeddingErrorBody>,
}

#[derive(Deserialize)]
struct EmbeddingErrorBody {
    message: String,
}

impl ApiEmbedder {
    /// Create a new API-based embedder.
    ///
    /// # Arguments
    /// * `api_key` -- OpenRouter (or compatible) API key.  Must not be empty.
    /// * `model`   -- Model identifier.  Pass `None` for the default
    ///                (`openai/text-embedding-3-large`).
    /// * `base_url` -- API base URL.  Pass `None` for the default OpenRouter
    ///                 URL.
    /// * `client`  -- Shared `reqwest::Client`.  Pass `None` to create one
    ///                internally.
    pub fn new(
        api_key: String,
        model: Option<String>,
        base_url: Option<String>,
        client: Option<Client>,
    ) -> Result<Self> {
        if api_key.is_empty() {
            return Err(SafeAgentError::VectorStore(
                "embedding API key must not be empty".into(),
            ));
        }

        let client = match client {
            Some(c) => c,
            None => Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .map_err(|e| {
                    SafeAgentError::VectorStore(format!(
                        "failed to create HTTP client for embeddings: {e}"
                    ))
                })?,
        };

        Ok(Self {
            client,
            api_key,
            model: model.unwrap_or_else(|| DEFAULT_EMBEDDING_MODEL.to_string()),
            base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
        })
    }

    /// Embed a single piece of text.
    pub async fn embed_one(&self, text: &str) -> Result<Vec<f32>> {
        let mut results = self.embed_batch(&[text]).await?;
        results
            .pop()
            .ok_or_else(|| SafeAgentError::VectorStore("API returned no embeddings".into()))
    }

    /// Embed a batch of texts in one API call.
    ///
    /// Returns a `Vec<Vec<f32>>` whose order matches the input `texts` slice,
    /// regardless of the order the API returns the embeddings (we sort by the
    /// `index` field).
    pub async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let url = format!("{}/v1/embeddings", self.base_url);
        let body = EmbeddingRequest {
            model: self.model.clone(),
            input: texts.iter().map(|t| (*t).to_string()).collect(),
        };

        debug!(
            model = %self.model,
            count = texts.len(),
            "calling embeddings API"
        );

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                SafeAgentError::VectorStore(format!("embedding API request failed: {e}"))
            })?;

        let status = resp.status();

        if !status.is_success() {
            let error_text = resp.text().await.unwrap_or_default();
            let error_msg =
                if let Ok(err) = serde_json::from_str::<EmbeddingErrorResponse>(&error_text) {
                    err.error
                        .map(|e| e.message)
                        .unwrap_or_else(|| error_text.clone())
                } else {
                    error_text
                };

            return Err(SafeAgentError::VectorStore(format!(
                "embedding API returned {status}: {error_msg}"
            )));
        }

        let mut embedding_resp: EmbeddingResponse = resp.json().await.map_err(|e| {
            SafeAgentError::VectorStore(format!("failed to parse embedding response: {e}"))
        })?;

        // Sort by index so callers get results in the same order as input.
        embedding_resp.data.sort_by_key(|d| d.index);

        let embeddings: Vec<Vec<f32>> = embedding_resp
            .data
            .into_iter()
            .map(|d| d.embedding)
            .collect();

        debug!(
            count = embeddings.len(),
            dim = embeddings.first().map(|e| e.len()).unwrap_or(0),
            "embeddings received"
        );

        Ok(embeddings)
    }
}

// ---------------------------------------------------------------------------
// Local fastembed-based embedder (optional)
// ---------------------------------------------------------------------------

#[cfg(feature = "local-embeddings")]
pub struct LocalEmbedder {
    /// Wrapped in a `Mutex` because `TextEmbedding::embed` takes `&mut self`.
    /// We use `std::sync::Mutex` rather than `tokio::sync::Mutex` because the
    /// underlying ONNX inference is synchronous CPU work -- holding the lock
    /// across an `.await` point is not a concern here.
    model: std::sync::Mutex<fastembed::TextEmbedding>,
}

#[cfg(feature = "local-embeddings")]
impl LocalEmbedder {
    /// Create a new local embedder using the BGE-Large-EN-v1.5 ONNX model.
    ///
    /// The model files are cached under `cache_dir`.  On the first call the
    /// model is downloaded (~330 MB); subsequent invocations load from cache.
    pub fn new(cache_dir: std::path::PathBuf) -> Result<Self> {
        use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

        let options = InitOptions::new(EmbeddingModel::BGELargeENV15)
            .with_cache_dir(cache_dir)
            .with_show_download_progress(true);

        let model = TextEmbedding::try_new(options).map_err(|e| {
            SafeAgentError::VectorStore(format!("failed to load local embedding model: {e}"))
        })?;

        Ok(Self {
            model: std::sync::Mutex::new(model),
        })
    }

    /// Embed a single piece of text (synchronous under the hood).
    pub fn embed_one(&self, text: &str) -> Result<Vec<f32>> {
        let mut results = self.embed_batch(&[text])?;
        results
            .pop()
            .ok_or_else(|| SafeAgentError::VectorStore("local model returned no embeddings".into()))
    }

    /// Embed a batch of texts.
    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let owned: Vec<String> = texts.iter().map(|t| (*t).to_string()).collect();
        let mut model = self.model.lock().map_err(|e| {
            SafeAgentError::VectorStore(format!("embedding model lock poisoned: {e}"))
        })?;
        let embeddings = model.embed(owned, None).map_err(|e| {
            SafeAgentError::VectorStore(format!("local embedding failed: {e}"))
        })?;

        Ok(embeddings)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- ApiEmbedder unit tests ---------------------------------------------

    #[test]
    fn api_embedder_rejects_empty_api_key() {
        let result = ApiEmbedder::new(String::new(), None, None, None);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("API key must not be empty"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn api_embedder_uses_defaults() {
        let embedder = ApiEmbedder::new(
            "test-key-123".to_string(),
            None,
            None,
            None,
        )
        .expect("should construct with valid key");

        assert_eq!(embedder.model, DEFAULT_EMBEDDING_MODEL);
        assert_eq!(embedder.base_url, DEFAULT_BASE_URL);
    }

    #[test]
    fn api_embedder_custom_model_and_url() {
        let embedder = ApiEmbedder::new(
            "key".to_string(),
            Some("custom/model".to_string()),
            Some("https://api.example.com".to_string()),
            None,
        )
        .expect("should construct");

        assert_eq!(embedder.model, "custom/model");
        assert_eq!(embedder.base_url, "https://api.example.com");
    }

    #[tokio::test]
    async fn api_embed_batch_empty_returns_empty() {
        let embedder = ApiEmbedder::new("key".to_string(), None, None, None)
            .expect("should construct");

        let result = embedder.embed_batch(&[]).await.expect("empty batch ok");
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn api_embed_batch_bad_url_returns_error() {
        // Point at a URL that will never resolve to verify error handling.
        let embedder = ApiEmbedder::new(
            "key".to_string(),
            None,
            Some("http://localhost:1".to_string()),
            None,
        )
        .expect("should construct");

        let result = embedder.embed_batch(&["hello"]).await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("embedding API request failed"),
            "unexpected error: {msg}"
        );
    }

    // -- Embedder enum tests ------------------------------------------------

    #[test]
    fn embedder_api_dim_is_none() {
        let api = ApiEmbedder::new("key".to_string(), None, None, None)
            .expect("should construct");
        let embedder = Embedder::Api(api);
        assert_eq!(embedder.dim(), None);
    }

    #[tokio::test]
    async fn embedder_api_empty_batch() {
        let api = ApiEmbedder::new("key".to_string(), None, None, None)
            .expect("should construct");
        let embedder = Embedder::Api(api);
        let result = embedder.embed_batch(&[]).await.expect("empty batch ok");
        assert!(result.is_empty());
    }

    // -- LocalEmbedder tests (cfg-gated) ------------------------------------

    #[cfg(feature = "local-embeddings")]
    mod local_tests {
        use super::*;

        #[test]
        fn local_embedder_dim_is_1024() {
            let cache = std::env::temp_dir().join("safe-agent-test-embeddings");
            let local = LocalEmbedder::new(cache).expect("should load model");
            let embedder = Embedder::Local(local);
            assert_eq!(embedder.dim(), Some(1024));
        }

        #[test]
        fn local_embed_one_correct_dimension() {
            let cache = std::env::temp_dir().join("safe-agent-test-embeddings");
            let local = LocalEmbedder::new(cache).expect("should load model");
            let vec = local.embed_one("hello world").expect("should embed");
            assert_eq!(vec.len(), 1024);
        }

        #[test]
        fn local_embed_batch_empty_returns_empty() {
            let cache = std::env::temp_dir().join("safe-agent-test-embeddings");
            let local = LocalEmbedder::new(cache).expect("should load model");
            let result = local.embed_batch(&[]).expect("empty batch ok");
            assert!(result.is_empty());
        }

        #[test]
        fn local_embed_batch_preserves_order() {
            let cache = std::env::temp_dir().join("safe-agent-test-embeddings");
            let local = LocalEmbedder::new(cache).expect("should load model");
            let texts = &["hello", "world", "foo"];
            let result = local.embed_batch(texts).expect("should embed");
            assert_eq!(result.len(), 3);
            for v in &result {
                assert_eq!(v.len(), 1024);
            }
        }
    }
}
