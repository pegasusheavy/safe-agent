use std::sync::{Arc, Mutex};

use llama_gguf::{ChatEngine, Engine, EngineConfig};
use tracing::info;

use crate::config::Config;
use crate::error::{Result, SafeAgentError};
use crate::llm::prompts;

/// LLM engine backed by a local GGUF model via llama-gguf.
///
/// Loads the model once at startup and keeps a `ChatEngine` that accumulates
/// conversation history in its KV cache.  Generation is CPU/GPU bound, so
/// every call is dispatched to Tokio's blocking thread pool.
pub struct LocalEngine {
    chat: Arc<Mutex<ChatEngine>>,
    model_path: String,
}

impl LocalEngine {
    pub fn new(config: &Config) -> Result<Self> {
        let model_path = std::env::var("MODEL_PATH")
            .unwrap_or_else(|_| config.llm.model_path.clone());

        if model_path.is_empty() {
            return Err(SafeAgentError::Config(
                "LLM backend \"local\" requires a model path.  Set `llm.model_path` \
                 in config.toml or the `MODEL_PATH` environment variable."
                    .into(),
            ));
        }

        let engine_config = EngineConfig {
            model_path: model_path.clone(),
            temperature: config.llm.temperature,
            top_k: config.llm.top_k,
            top_p: config.llm.top_p,
            repeat_penalty: config.llm.repeat_penalty,
            max_tokens: config.llm.max_tokens,
            use_gpu: config.llm.use_gpu,
            ..Default::default()
        };

        if config.llm.context_length > 0 {
            info!(
                context_length = config.llm.context_length,
                "context_length override requested (applied at model level if supported)"
            );
        }

        info!(
            model = %model_path,
            temperature = config.llm.temperature,
            top_k = config.llm.top_k,
            top_p = config.llm.top_p,
            max_tokens = config.llm.max_tokens,
            context_length = config.llm.context_length,
            use_gpu = config.llm.use_gpu,
            "loading local GGUF model"
        );

        let engine = Engine::load(engine_config).map_err(|e| {
            SafeAgentError::Llm(format!("failed to load GGUF model: {e}"))
        })?;

        let system_prompt = prompts::system_prompt(
            &config.core_personality,
            &config.agent_name,
        );

        info!(
            chat_template = ?engine.chat_template(),
            vocab_size = engine.model_config().vocab_size,
            max_seq_len = engine.model_config().max_seq_len,
            "local model loaded"
        );

        let chat = ChatEngine::new(engine, Some(system_prompt));

        Ok(Self {
            chat: Arc::new(Mutex::new(chat)),
            model_path,
        })
    }

    /// Generate a response by running inference on the blocking thread pool.
    pub async fn generate(&self, message: &str) -> Result<String> {
        let chat = Arc::clone(&self.chat);
        let msg = message.to_string();

        let response = tokio::task::spawn_blocking(move || {
            let mut engine = chat.lock().map_err(|e| {
                SafeAgentError::Llm(format!("chat engine lock poisoned: {e}"))
            })?;
            engine.chat(&msg).map_err(|e| {
                SafeAgentError::Llm(format!("local inference failed: {e}"))
            })
        })
        .await
        .map_err(|e| SafeAgentError::Llm(format!("blocking task join error: {e}")))??;

        if response.is_empty() {
            return Err(SafeAgentError::Llm(
                "local model returned empty response".into(),
            ));
        }

        info!(
            model = %self.model_path,
            response_len = response.len(),
            "local model response received"
        );

        Ok(response)
    }
}
