use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::error::{Result, SafeAgentError};
use crate::llm::prompts;
use crate::tools::ToolRegistry;

/// LLM engine backed by the Google Gemini CLI.
///
/// Invokes `gemini` in non-interactive mode with `--prompt` (`-p`), returning
/// the plain-text response from stdout.  The CLI runs with `--approval-mode
/// yolo` so the agent can execute tools and edit files without interactive
/// approval, mirroring the behaviour of the Claude and Codex backends.
///
/// Authentication uses whichever credentials the Gemini CLI already has
/// configured (Google AI Studio key via `GEMINI_API_KEY` / `GOOGLE_API_KEY`,
/// or OAuth login).
pub struct GeminiEngine {
    gemini_bin: String,
    model: Option<String>,
    personality: String,
    agent_name: String,
    timezone: String,
    timeout_secs: u64,
}

impl GeminiEngine {
    pub fn new(config: &Config) -> Result<Self> {
        let gemini_bin = std::env::var("GEMINI_BIN")
            .unwrap_or_else(|_| config.llm.gemini_bin.clone());

        let model = std::env::var("GEMINI_MODEL")
            .ok()
            .or_else(|| {
                if config.llm.gemini_model.is_empty() {
                    None
                } else {
                    Some(config.llm.gemini_model.clone())
                }
            });

        let timeout_secs = config.llm.timeout_secs;

        info!(
            gemini_bin = %gemini_bin,
            model = ?model,
            timeout_secs,
            "Gemini CLI engine initialized"
        );

        Ok(Self {
            gemini_bin,
            model,
            personality: config.core_personality.clone(),
            agent_name: config.agent_name.clone(),
            timezone: config.timezone.clone(),
            timeout_secs,
        })
    }

    /// Send a message to Gemini and return the plain-text response.
    pub async fn generate(&self, message: &str, tools: Option<&ToolRegistry>) -> Result<String> {
        let system_prompt = prompts::system_prompt(&self.personality, &self.agent_name, tools, Some(&self.timezone));
        let prompt = format!(
            "{}\n\n---\n\nThe user says: {}",
            system_prompt, message
        );

        let mut cmd = Command::new(&self.gemini_bin);

        cmd.arg("--prompt").arg(&prompt)
            .arg("--output-format").arg("text")
            .arg("--sandbox")
            .arg("--approval-mode").arg("yolo");

        if let Some(model) = &self.model {
            cmd.arg("--model").arg(model);
        }

        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        debug!(
            model = ?self.model,
            prompt_len = prompt.len(),
            "invoking gemini CLI"
        );

        let child = cmd.spawn().map_err(|e| {
            SafeAgentError::Llm(format!(
                "failed to spawn gemini CLI ({}): {e}",
                self.gemini_bin
            ))
        })?;

        let output = if self.timeout_secs > 0 {
            let timeout = Duration::from_secs(self.timeout_secs);
            match tokio::time::timeout(timeout, child.wait_with_output()).await {
                Ok(result) => result.map_err(|e| {
                    SafeAgentError::Llm(format!("gemini CLI failed: {e}"))
                })?,
                Err(_) => {
                    warn!(timeout_secs = self.timeout_secs, "gemini CLI timed out");
                    return Err(SafeAgentError::Llm(format!(
                        "gemini CLI timed out after {}s",
                        self.timeout_secs
                    )));
                }
            }
        } else {
            child.wait_with_output().await.map_err(|e| {
                SafeAgentError::Llm(format!("gemini CLI failed: {e}"))
            })?
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(
                exit_code = ?output.status.code(),
                stderr = %stderr,
                "gemini CLI exited with error"
            );
            return Err(SafeAgentError::Llm(format!(
                "gemini CLI exited with {}: {}",
                output.status,
                stderr.trim()
            )));
        }

        let response = String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string();

        info!(response_len = response.len(), "gemini CLI response received");

        if response.is_empty() {
            return Err(SafeAgentError::Llm(
                "gemini CLI returned empty response".into(),
            ));
        }

        Ok(response)
    }
}
