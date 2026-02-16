pub mod prompts;

mod claude;
mod codex;
#[cfg(feature = "local")]
mod local;

use tracing::info;

use crate::config::Config;
use crate::error::{Result, SafeAgentError};

/// Unified LLM engine that dispatches to one of three backends:
///
/// - **Claude** -- Claude Code CLI (default)
/// - **Codex**  -- OpenAI Codex CLI
/// - **Local**  -- local GGUF model via llama-gguf (requires `local` feature)
pub enum LlmEngine {
    Claude(claude::ClaudeEngine),
    Codex(codex::CodexEngine),
    #[cfg(feature = "local")]
    Local(local::LocalEngine),
}

impl LlmEngine {
    /// Build the engine from config.
    ///
    /// The backend is selected by `config.llm.backend` (overridable with the
    /// `LLM_BACKEND` environment variable).  Valid values: `"claude"`,
    /// `"codex"`, `"local"`.
    pub fn new(config: &Config) -> Result<Self> {
        let backend = std::env::var("LLM_BACKEND")
            .unwrap_or_else(|_| config.llm.backend.clone());

        match backend.as_str() {
            "claude" => {
                info!("LLM backend: Claude CLI");
                Ok(Self::Claude(claude::ClaudeEngine::new(config)?))
            }
            "codex" => {
                info!("LLM backend: Codex CLI");
                Ok(Self::Codex(codex::CodexEngine::new(config)?))
            }
            #[cfg(feature = "local")]
            "local" => {
                info!("LLM backend: local GGUF model");
                Ok(Self::Local(local::LocalEngine::new(config)?))
            }
            #[cfg(not(feature = "local"))]
            "local" => Err(SafeAgentError::Config(
                "LLM backend \"local\" requested but safe-agent was compiled without \
                 the `local` feature.  Rebuild with `--features local`."
                    .into(),
            )),
            other => Err(SafeAgentError::Config(format!(
                "unknown LLM backend \"{other}\" (valid: \"claude\", \"codex\", \"local\")"
            ))),
        }
    }

    /// Generate a response for the given user message.
    pub async fn generate(&self, message: &str) -> Result<String> {
        match self {
            Self::Claude(engine) => engine.generate(message).await,
            Self::Codex(engine) => engine.generate(message).await,
            #[cfg(feature = "local")]
            Self::Local(engine) => engine.generate(message).await,
        }
    }

    /// Return a human-readable description of the active backend.
    pub fn backend_info(&self) -> &str {
        match self {
            Self::Claude(_) => "Claude CLI",
            Self::Codex(_) => "Codex CLI",
            #[cfg(feature = "local")]
            Self::Local(_) => "local GGUF",
        }
    }
}
