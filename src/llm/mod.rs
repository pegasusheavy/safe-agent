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
    /// The active backend used for generation.
    active: Arc<dyn LlmBackend>,
    /// The key identifying the active backend.
    active_key: String,
    /// Registry of all available backends (built-in + plugins).
    pub plugins: LlmPluginRegistry,
}

impl LlmEngine {
    /// Build the engine from config.
    ///
    /// The backend is selected by `config.llm.backend` (overridable with the
    /// `LLM_BACKEND` environment variable).  Valid values: `"claude"`,
    /// `"codex"`, `"gemini"`, `"aider"`, `"openrouter"`, `"local"`.
    pub fn new(config: &Config) -> Result<Self> {
        let backend = std::env::var("LLM_BACKEND")
            .unwrap_or_else(|_| config.llm.backend.clone());

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

        // Select the active backend
        let active = match plugins.get(&backend) {
            Some(b) => {
                info!(backend = %backend, name = b.name(), "LLM backend selected");
                b
            }
            None => {
                #[cfg(not(feature = "local"))]
                if backend == "local" {
                    return Err(SafeAgentError::Config(
                        "LLM backend \"local\" requested but safe-agent was compiled without \
                         the `local` feature.  Rebuild with `--features local`."
                            .into(),
                    ));
                }

                return Err(SafeAgentError::Config(format!(
                    "unknown LLM backend \"{backend}\" — available: [{}]",
                    plugins.list().join(", "),
                )));
            }
        };

        Ok(Self {
            active,
            active_key: backend,
            plugins,
        })
    }

    /// List all available backend keys (built-in + plugins).
    pub fn available_backends(&self) -> Vec<String> {
        self.plugins.list()
    }

    /// Generate a response for the given generation context.
    ///
    /// Delegates to the active backend.  The context bundles the message,
    /// optional tool registry, and any prompt skills for this request.
    pub async fn generate(&self, ctx: &GenerateContext<'_>) -> Result<String> {
        self.active.generate(ctx).await
    }

    /// Return a human-readable description of the active backend.
    pub fn backend_info(&self) -> &str {
        self.active.name()
    }

    /// Return the key of the active backend.
    pub fn active_backend(&self) -> &str {
        &self.active_key
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
