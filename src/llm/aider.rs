use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::error::{Result, SafeAgentError};
use crate::llm::context::GenerateContext;
use crate::llm::prompts;

/// LLM engine backed by Aider, the open-source AI pair-programming tool.
///
/// Invokes `aider` in scripting mode with `--message`, captures the response
/// from stdout, then exits.  Aider supports many LLM providers (OpenAI,
/// Anthropic, Google, etc.) and selects the model via `--model`.
///
/// Runs with `--yes` (auto-confirm), `--no-auto-commits` (safeclaw manages
/// its own persistence), and `--no-stream` for clean stdout capture.
///
/// The caller's existing provider API keys (OPENAI_API_KEY,
/// ANTHROPIC_API_KEY, etc.) are inherited from the environment.
pub struct AiderEngine {
    aider_bin: String,
    model: Option<String>,
    personality: String,
    agent_name: String,
    timezone: String,
    locale: String,
    timeout_secs: u64,
    /// Working directory for the CLI process.
    work_dir: std::path::PathBuf,
}

impl AiderEngine {
    pub fn new(config: &Config) -> Result<Self> {
        let aider_bin = std::env::var("AIDER_BIN")
            .unwrap_or_else(|_| config.llm.aider_bin.clone());

        let model = std::env::var("AIDER_MODEL")
            .ok()
            .or_else(|| {
                if config.llm.aider_model.is_empty() {
                    None
                } else {
                    Some(config.llm.aider_model.clone())
                }
            });

        let timeout_secs = config.llm.timeout_secs;

        info!(
            aider_bin = %aider_bin,
            model = ?model,
            timeout_secs,
            "Aider engine initialized"
        );

        Ok(Self {
            aider_bin,
            model,
            personality: config.core_personality.clone(),
            agent_name: config.agent_name.clone(),
            timezone: config.timezone.clone(),
            locale: config.locale.clone(),
            timeout_secs,
            work_dir: Config::data_dir(),
        })
    }

    /// Send a message to Aider and return the response text.
    pub async fn generate(&self, ctx: &GenerateContext<'_>) -> Result<String> {
        let system_prompt = prompts::system_prompt(&self.personality, &self.agent_name, ctx.tools, Some(&self.timezone), Some(&self.locale), ctx.prompt_skills);
        let prompt = format!(
            "{}\n\n---\n\nThe user says: {}",
            system_prompt, ctx.message
        );

        let mut cmd = Command::new(&self.aider_bin);

        cmd.arg("--message").arg(&prompt)
            .arg("--yes")
            .arg("--no-auto-commits")
            .arg("--no-stream")
            .arg("--no-git");

        if let Some(model) = &self.model {
            cmd.arg("--model").arg(model);
        }

        cmd.current_dir(&self.work_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        debug!(
            model = ?self.model,
            prompt_len = prompt.len(),
            "invoking aider"
        );

        let child = cmd.spawn().map_err(|e| {
            SafeAgentError::Llm(format!(
                "failed to spawn aider ({}): {e}",
                self.aider_bin
            ))
        })?;

        let output = if self.timeout_secs > 0 {
            let timeout = Duration::from_secs(self.timeout_secs);
            match tokio::time::timeout(timeout, child.wait_with_output()).await {
                Ok(result) => result.map_err(|e| {
                    SafeAgentError::Llm(format!("aider failed: {e}"))
                })?,
                Err(_) => {
                    warn!(timeout_secs = self.timeout_secs, "aider timed out");
                    return Err(SafeAgentError::Llm(format!(
                        "aider timed out after {}s",
                        self.timeout_secs
                    )));
                }
            }
        } else {
            child.wait_with_output().await.map_err(|e| {
                SafeAgentError::Llm(format!("aider failed: {e}"))
            })?
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(
                exit_code = ?output.status.code(),
                stderr = %stderr,
                "aider exited with error"
            );
            return Err(SafeAgentError::Llm(format!(
                "aider exited with {}: {}",
                output.status,
                stderr.trim()
            )));
        }

        let response = String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string();

        info!(response_len = response.len(), "aider response received");

        if response.is_empty() {
            return Err(SafeAgentError::Llm(
                "aider returned empty response".into(),
            ));
        }

        Ok(response)
    }
}
