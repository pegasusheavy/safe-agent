pub mod prompts;

use std::path::PathBuf;

use llama_gguf::engine::{ChatEngine, Engine, EngineConfig};
use llama_gguf::huggingface::HfClient;
use tracing::info;

use crate::config::Config;
use crate::error::{Result, SafeAgentError};
use crate::security::SandboxedFs;

pub struct LlmEngine {
    chat_engine: ChatEngine,
    max_tokens: usize,
}

impl LlmEngine {
    pub fn load(config: &Config) -> Result<Self> {
        let model_path = config.resolved_model_path();

        if !model_path.exists() {
            return Err(SafeAgentError::Llm(format!(
                "model not found at {}. Run with --download-model to fetch it.",
                model_path.display()
            )));
        }

        info!(path = %model_path.display(), "loading LLM model");

        let engine_config = EngineConfig {
            model_path: model_path.to_string_lossy().into_owned(),
            tokenizer_path: None,
            temperature: config.llm.temperature,
            top_k: config.llm.top_k,
            top_p: config.llm.top_p,
            repeat_penalty: config.llm.repeat_penalty,
            max_tokens: config.llm.max_tokens,
            use_gpu: config.llm.use_gpu,
            seed: None,
        };

        let engine = Engine::load(engine_config)
            .map_err(|e| SafeAgentError::Llm(format!("failed to load model: {e}")))?;

        let system_prompt = prompts::system_prompt(&config.core_personality, &config.agent_name);
        let chat_engine = ChatEngine::new(engine, Some(system_prompt));
        let max_tokens = config.llm.max_tokens;

        info!("LLM model loaded successfully");

        Ok(Self {
            chat_engine,
            max_tokens,
        })
    }

    /// Generate a structured response from the LLM given a user message.
    /// Clears chat history before each call to prevent context accumulation across ticks.
    pub fn generate(&mut self, message: &str) -> Result<prompts::AgentReasoning> {
        // Reset chat history so each tick starts fresh (context is self-contained in the message).
        self.chat_engine.clear_history();

        let response = self
            .chat_engine
            .chat(message)
            .map_err(|e| SafeAgentError::Llm(format!("generation failed: {e}")))?;

        // Try to parse the JSON response, with cleanup for common issues
        let json_str = extract_json(&response);
        let reasoning: prompts::AgentReasoning = serde_json::from_str(json_str)
            .map_err(|e| SafeAgentError::Llm(format!("failed to parse LLM output as JSON: {e}\nRaw output: {response}")))?;

        Ok(reasoning)
    }

    pub fn clear_history(&mut self) {
        self.chat_engine.clear_history();
    }

    pub fn context_len(&self) -> usize {
        self.chat_engine.context_len()
    }
}

/// Extract JSON from a response that might contain markdown fences or extra text.
fn extract_json(text: &str) -> &str {
    let trimmed = text.trim();

    // Try to find JSON within markdown code fences
    if let Some(start) = trimmed.find("```json") {
        let after_fence = &trimmed[start + 7..];
        if let Some(end) = after_fence.find("```") {
            return after_fence[..end].trim();
        }
    }
    if let Some(start) = trimmed.find("```") {
        let after_fence = &trimmed[start + 3..];
        if let Some(end) = after_fence.find("```") {
            return after_fence[..end].trim();
        }
    }

    // Try to find a JSON object
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            return &trimmed[start..=end];
        }
    }

    trimmed
}

/// Download a model from HuggingFace into the sandboxed data directory.
pub fn download_model(config: &Config, sandbox: &SandboxedFs) -> Result<PathBuf> {
    let models_dir = std::path::Path::new("models");
    let target = sandbox.resolve(models_dir)?;
    std::fs::create_dir_all(&target)?;

    let hf = HfClient::with_cache_dir(target);

    info!(
        repo = %config.llm.hf_repo,
        filename = %config.llm.hf_filename,
        "downloading model"
    );

    let path = hf
        .download_file(&config.llm.hf_repo, &config.llm.hf_filename, true)
        .map_err(|e| SafeAgentError::Llm(format!("download failed: {e}")))?;

    Ok(path)
}
