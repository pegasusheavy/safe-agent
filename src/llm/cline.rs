use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::error::{Result, SafeAgentError};
use crate::llm::context::GenerateContext;
use crate::llm::prompts;

/// LLM engine backed by the Cline CLI.
///
/// Invokes `cline` in non-interactive mode, passing the prompt as a message.
/// Cline CLI supports `--yes` for auto-approval of tool use.  Authentication
/// is handled by whatever API key Cline already has configured (OpenAI,
/// Anthropic, etc. via Cline's own config).
pub struct ClineEngine {
    cline_bin: String,
    model: Option<String>,
    personality: String,
    agent_name: String,
    timezone: String,
    timeout_secs: u64,
    work_dir: std::path::PathBuf,
}

impl ClineEngine {
    pub fn new(config: &Config) -> Result<Self> {
        let cline_bin = std::env::var("CLINE_BIN")
            .unwrap_or_else(|_| config.llm.cline_bin.clone());

        let model = std::env::var("CLINE_MODEL")
            .ok()
            .or_else(|| {
                if config.llm.cline_model.is_empty() {
                    None
                } else {
                    Some(config.llm.cline_model.clone())
                }
            });

        let timeout_secs = config.llm.timeout_secs;

        info!(
            cline_bin = %cline_bin,
            model = ?model,
            timeout_secs,
            "Cline CLI engine initialized"
        );

        Ok(Self {
            cline_bin,
            model,
            personality: config.core_personality.clone(),
            agent_name: config.agent_name.clone(),
            timezone: config.timezone.clone(),
            timeout_secs,
            work_dir: Config::data_dir(),
        })
    }

    /// Send a message to Cline and return the plain-text response.
    pub async fn generate(&self, ctx: &GenerateContext<'_>) -> Result<String> {
        let system_prompt = prompts::system_prompt(
            &self.personality,
            &self.agent_name,
            ctx.tools,
            Some(&self.timezone),
            ctx.prompt_skills,
        );
        let prompt = format!(
            "{}\n\n---\n\nThe user says: {}",
            system_prompt, ctx.message
        );

        let mut cmd = Command::new(&self.cline_bin);

        // Cline CLI accepts a prompt string and --yes for auto-approval
        cmd.arg("--yes")
            .arg(&prompt);

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
            "invoking cline CLI"
        );

        let child = cmd.spawn().map_err(|e| {
            SafeAgentError::Llm(format!(
                "failed to spawn cline CLI ({}): {e}",
                self.cline_bin
            ))
        })?;

        let output = if self.timeout_secs > 0 {
            let timeout = Duration::from_secs(self.timeout_secs);
            match tokio::time::timeout(timeout, child.wait_with_output()).await {
                Ok(result) => result.map_err(|e| {
                    SafeAgentError::Llm(format!("cline CLI failed: {e}"))
                })?,
                Err(_) => {
                    warn!(timeout_secs = self.timeout_secs, "cline CLI timed out");
                    return Err(SafeAgentError::Llm(format!(
                        "cline CLI timed out after {}s",
                        self.timeout_secs
                    )));
                }
            }
        } else {
            child.wait_with_output().await.map_err(|e| {
                SafeAgentError::Llm(format!("cline CLI failed: {e}"))
            })?
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(
                exit_code = ?output.status.code(),
                stderr = %stderr,
                "cline CLI exited with error"
            );
            return Err(SafeAgentError::Llm(format!(
                "cline CLI exited with {}: {}",
                output.status,
                stderr.trim()
            )));
        }

        let response = String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string();

        info!(response_len = response.len(), "cline CLI response received");

        if response.is_empty() {
            return Err(SafeAgentError::Llm(
                "cline CLI returned empty response".into(),
            ));
        }

        Ok(response)
    }
}
