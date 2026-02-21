use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::error::{Result, SafeAgentError};
use crate::llm::context::GenerateContext;
use crate::llm::prompts;

/// LLM engine backed by the OpenAI Codex CLI.
///
/// Invokes `codex exec` as a subprocess in non-interactive mode, passing the
/// prompt as stdin (via the `-` sentinel).  Progress streams to stderr while
/// only the final agent message prints to stdout.
///
/// Codex runs with `--sandbox danger-full-access` and `--ask-for-approval
/// never` so the agent can create skills, write files, and run commands just
/// like the Claude backend.  Sessions are ephemeral — no rollout files
/// persist.
pub struct CodexEngine {
    codex_bin: String,
    model: Option<String>,
    profile: Option<String>,
    personality: String,
    agent_name: String,
    timezone: String,
    timeout_secs: u64,
    /// Working directory for the CLI process.
    work_dir: std::path::PathBuf,
}

impl CodexEngine {
    pub fn new(config: &Config) -> Result<Self> {
        let codex_bin = std::env::var("CODEX_BIN")
            .unwrap_or_else(|_| config.llm.codex_bin.clone());

        let model = std::env::var("CODEX_MODEL")
            .ok()
            .or_else(|| {
                if config.llm.codex_model.is_empty() {
                    None
                } else {
                    Some(config.llm.codex_model.clone())
                }
            });

        let profile = std::env::var("CODEX_PROFILE")
            .ok()
            .or_else(|| {
                if config.llm.codex_profile.is_empty() {
                    None
                } else {
                    Some(config.llm.codex_profile.clone())
                }
            });

        let timeout_secs = config.llm.timeout_secs;

        info!(
            codex_bin = %codex_bin,
            model = ?model,
            profile = ?profile,
            timeout_secs,
            "Codex CLI engine initialized"
        );

        Ok(Self {
            codex_bin,
            model,
            profile,
            personality: config.core_personality.clone(),
            agent_name: config.agent_name.clone(),
            timezone: config.timezone.clone(),
            timeout_secs,
            work_dir: Config::data_dir(),
        })
    }

    /// Send a message to Codex and return the plain-text response.
    pub async fn generate(&self, ctx: &GenerateContext<'_>) -> Result<String> {
        let mut cmd = Command::new(&self.codex_bin);

        cmd.arg("exec")
            .arg("--sandbox").arg("danger-full-access")
            .arg("--ask-for-approval").arg("never")
            .arg("--skip-git-repo-check")
            .arg("--ephemeral");

        if let Some(model) = &self.model {
            cmd.arg("--model").arg(model);
        }

        if let Some(profile) = &self.profile {
            cmd.arg("--profile").arg(profile);
        }

        // Pass prompt via stdin using the `-` sentinel so we avoid
        // command-line length limits on large system prompts.
        cmd.arg("-");

        cmd.current_dir(&self.work_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let system_prompt = prompts::system_prompt(&self.personality, &self.agent_name, ctx.tools, Some(&self.timezone), ctx.prompt_skills);
        let prompt = format!(
            "{}\n\n---\n\nThe user says: {}",
            system_prompt, ctx.message
        );

        debug!(
            model = ?self.model,
            prompt_len = prompt.len(),
            "invoking codex exec"
        );

        let mut child = cmd.spawn().map_err(|e| {
            SafeAgentError::Llm(format!(
                "failed to spawn codex CLI ({}): {e}",
                self.codex_bin
            ))
        })?;

        // Write prompt to stdin then close it
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(prompt.as_bytes()).await.map_err(|e| {
                SafeAgentError::Llm(format!("failed to write to codex stdin: {e}"))
            })?;
        }

        // Wait for the process to finish, with an optional timeout.
        let output = if self.timeout_secs > 0 {
            let timeout = Duration::from_secs(self.timeout_secs);
            match tokio::time::timeout(timeout, child.wait_with_output()).await {
                Ok(result) => result.map_err(|e| {
                    SafeAgentError::Llm(format!("codex CLI failed: {e}"))
                })?,
                Err(_) => {
                    warn!(timeout_secs = self.timeout_secs, "codex CLI timed out");
                    return Err(SafeAgentError::Llm(format!(
                        "codex CLI timed out after {}s",
                        self.timeout_secs
                    )));
                }
            }
        } else {
            child.wait_with_output().await.map_err(|e| {
                SafeAgentError::Llm(format!("codex CLI failed: {e}"))
            })?
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(
                exit_code = ?output.status.code(),
                stderr = %stderr,
                "codex CLI exited with error"
            );
            return Err(SafeAgentError::Llm(format!(
                "codex CLI exited with {}: {}",
                output.status,
                stderr.trim()
            )));
        }

        let response = String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string();

        info!(response_len = response.len(), "codex CLI response received");

        if response.is_empty() {
            return Err(SafeAgentError::Llm(
                "codex CLI returned empty response".into(),
            ));
        }

        Ok(response)
    }
}
