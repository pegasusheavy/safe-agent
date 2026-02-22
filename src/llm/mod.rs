pub mod context;
pub mod prompts;

mod aider;
mod claude;
mod codex;
mod gemini;
mod openrouter;
#[cfg(feature = "local")]
mod local;

use std::collections::HashMap;
use std::sync::Arc;

use tracing::info;

use crate::config::Config;
use crate::error::{Result, SafeAgentError};

pub use context::GenerateContext;

// -- Plugin trait -----------------------------------------------------------

/// Trait that all LLM backends implement.  Allows dynamic dispatch so new
/// backends can be registered at runtime without compile-time feature flags.
#[async_trait::async_trait]
pub trait LlmBackend: Send + Sync {
    /// Human-readable name of this backend (e.g. "Claude CLI", "OpenRouter API").
    fn name(&self) -> &str;

    /// Generate a response for the given generation context.
    ///
    /// The context bundles the message, optional tool registry, and any
    /// prompt skills that should be injected into the system prompt.
    async fn generate(&self, ctx: &GenerateContext<'_>) -> Result<String>;
}

// -- Plugin registry --------------------------------------------------------

/// Registry of available LLM backends.  Built-in backends are registered
/// automatically; additional backends can be added via `register()`.
pub struct LlmPluginRegistry {
    backends: HashMap<String, Arc<dyn LlmBackend>>,
}

impl LlmPluginRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            backends: HashMap::new(),
        }
    }

    /// Register a new backend plugin.
    pub fn register(&mut self, key: &str, backend: Arc<dyn LlmBackend>) {
        info!(backend = key, name = backend.name(), "LLM plugin registered");
        self.backends.insert(key.to_string(), backend);
    }

    /// Get a registered backend by key.
    pub fn get(&self, key: &str) -> Option<Arc<dyn LlmBackend>> {
        self.backends.get(key).cloned()
    }

    /// List all registered backend keys.
    pub fn list(&self) -> Vec<String> {
        self.backends.keys().cloned().collect()
    }

}

// -- Trait implementations for built-in backends ----------------------------

#[async_trait::async_trait]
impl LlmBackend for claude::ClaudeEngine {
    fn name(&self) -> &str { "Claude CLI" }
    async fn generate(&self, ctx: &GenerateContext<'_>) -> Result<String> {
        self.generate(ctx).await
    }
}

#[async_trait::async_trait]
impl LlmBackend for codex::CodexEngine {
    fn name(&self) -> &str { "Codex CLI" }
    async fn generate(&self, ctx: &GenerateContext<'_>) -> Result<String> {
        self.generate(ctx).await
    }
}

#[async_trait::async_trait]
impl LlmBackend for gemini::GeminiEngine {
    fn name(&self) -> &str { "Gemini CLI" }
    async fn generate(&self, ctx: &GenerateContext<'_>) -> Result<String> {
        self.generate(ctx).await
    }
}

#[async_trait::async_trait]
impl LlmBackend for aider::AiderEngine {
    fn name(&self) -> &str { "Aider" }
    async fn generate(&self, ctx: &GenerateContext<'_>) -> Result<String> {
        self.generate(ctx).await
    }
}

#[async_trait::async_trait]
impl LlmBackend for openrouter::OpenRouterEngine {
    fn name(&self) -> &str { "OpenRouter API" }
    async fn generate(&self, ctx: &GenerateContext<'_>) -> Result<String> {
        self.generate(ctx).await
    }
}

#[cfg(feature = "local")]
#[async_trait::async_trait]
impl LlmBackend for local::LocalEngine {
    fn name(&self) -> &str { "local GGUF" }
    async fn generate(&self, ctx: &GenerateContext<'_>) -> Result<String> {
        self.generate(ctx).await
    }
}

// -- LlmEngine (wraps active backend + plugin registry) ---------------------

/// Unified LLM engine that dispatches to one of the registered backends.
///
/// Built-in backends:
/// - **Claude**      -- Claude Code CLI (default)
/// - **Codex**       -- OpenAI Codex CLI
/// - **Gemini**      -- Google Gemini CLI
/// - **Aider**       -- Aider multi-provider AI pair-programmer
/// - **OpenRouter**  -- OpenRouter API (hundreds of models via one API key)
/// - **Local**       -- local GGUF model via llama-gguf (requires `local` feature)
///
/// Additional backends can be registered at runtime via the plugin registry.
pub struct LlmEngine {
    /// Ordered failover chain: (key, backend). First is primary.
    chain: Vec<(String, Arc<dyn LlmBackend>)>,
    /// Registry of all available backends (built-in + plugins).
    pub plugins: LlmPluginRegistry,
}

impl LlmEngine {
    /// Build the engine from config.
    ///
    /// The failover chain is built from `config.llm.failover_chain` if
    /// non-empty, otherwise falls back to a single-element chain from
    /// `config.llm.backend` (overridable with `LLM_BACKEND` env var).
    ///
    /// Valid backend keys: `"claude"`, `"codex"`, `"gemini"`, `"aider"`,
    /// `"openrouter"`, `"local"`.
    pub fn new(config: &Config) -> Result<Self> {
        let mut plugins = LlmPluginRegistry::new();

        // Register all built-in backends that are configurable
        if let Ok(engine) = claude::ClaudeEngine::new(config) {
            plugins.register("claude", Arc::new(engine));
        }
        if let Ok(engine) = codex::CodexEngine::new(config) {
            plugins.register("codex", Arc::new(engine));
        }
        if let Ok(engine) = gemini::GeminiEngine::new(config) {
            plugins.register("gemini", Arc::new(engine));
        }
        if let Ok(engine) = aider::AiderEngine::new(config) {
            plugins.register("aider", Arc::new(engine));
        }
        if let Ok(engine) = openrouter::OpenRouterEngine::new(config) {
            plugins.register("openrouter", Arc::new(engine));
        }
        #[cfg(feature = "local")]
        if let Ok(engine) = local::LocalEngine::new(config) {
            plugins.register("local", Arc::new(engine));
        }

        // Build the failover chain
        let requested_keys: Vec<String> = if !config.llm.failover_chain.is_empty() {
            config.llm.failover_chain.clone()
        } else {
            let backend = std::env::var("LLM_BACKEND")
                .unwrap_or_else(|_| config.llm.backend.clone());
            vec![backend]
        };

        let mut chain: Vec<(String, Arc<dyn LlmBackend>)> = Vec::new();
        for key in &requested_keys {
            match plugins.get(key) {
                Some(b) => chain.push((key.clone(), b)),
                None => {
                    #[cfg(not(feature = "local"))]
                    if key == "local" {
                        tracing::warn!(
                            backend = %key,
                            "LLM backend \"local\" skipped: compiled without `local` feature"
                        );
                        continue;
                    }

                    tracing::warn!(
                        backend = %key,
                        available = %plugins.list().join(", "),
                        "LLM failover chain: unknown backend, skipping"
                    );
                }
            }
        }

        if chain.is_empty() {
            return Err(SafeAgentError::Config(format!(
                "no valid LLM backends in failover chain {:?} — available: [{}]",
                requested_keys,
                plugins.list().join(", "),
            )));
        }

        let chain_keys: Vec<&str> = chain.iter().map(|(k, _)| k.as_str()).collect();
        info!(chain = ?chain_keys, "LLM failover chain configured");

        Ok(Self { chain, plugins })
    }

    /// List all available backend keys (built-in + plugins).
    pub fn available_backends(&self) -> Vec<String> {
        self.plugins.list()
    }

    /// Generate a response by trying each backend in the failover chain.
    ///
    /// Walks the chain in order: on success returns immediately, on failure
    /// (error or empty response) logs a warning and tries the next backend.
    pub async fn generate(&self, ctx: &GenerateContext<'_>) -> Result<String> {
        let mut last_err = None;
        for (key, backend) in &self.chain {
            match backend.generate(ctx).await {
                Ok(response) if !response.trim().is_empty() => {
                    if key != &self.chain[0].0 {
                        tracing::warn!(
                            primary = %self.chain[0].0,
                            fallback = %key,
                            "LLM failover: primary failed, using fallback"
                        );
                    }
                    return Ok(response);
                }
                Ok(_empty) => {
                    tracing::warn!(backend = %key, "LLM backend returned empty response, trying next");
                    last_err = Some(SafeAgentError::Llm(format!("{key} returned empty response")));
                }
                Err(e) => {
                    tracing::warn!(backend = %key, err = %e, "LLM backend failed, trying next");
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| SafeAgentError::Llm("no backends configured".into())))
    }

    /// Return a human-readable description of the primary backend.
    pub fn backend_info(&self) -> &str {
        self.chain[0].1.name()
    }

    /// Return the key of the primary backend.
    pub fn active_backend(&self) -> &str {
        &self.chain[0].0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_registry() {
        let registry = LlmPluginRegistry::new();
        assert!(registry.get("test").is_none());
        assert!(registry.list().is_empty());
    }
}
