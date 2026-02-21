use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::error::{Result, SafeAgentError};
use crate::llm::context::GenerateContext;
use crate::llm::prompts;

/// LLM engine backed by the Claude Code CLI.
///
/// Invokes `claude` as a subprocess in print mode (`-p`), piping the prompt
/// via stdin.  The `CLAUDE_CONFIG_DIR` environment variable selects which
/// authenticated profile to use, and `--model` picks the model.
pub struct ClaudeEngine {
    claude_bin: String,
    model: String,
    config_dir: Option<String>,
    personality: String,
    agent_name: String,
    timezone: String,
    max_turns: u32,
    timeout_secs: u64,
    /// Working directory for the CLI process.  Set to the data directory so
    /// that the CLI picks up the managed CLAUDE.md file.
    work_dir: std::path::PathBuf,
}

impl ClaudeEngine {
    pub fn new(config: &Config) -> Result<Self> {
        let claude_bin = std::env::var("CLAUDE_BIN")
            .unwrap_or_else(|_| config.llm.claude_bin.clone());

        let config_dir = std::env::var("CLAUDE_CONFIG_DIR")
            .ok()
            .or_else(|| {
                if config.llm.claude_config_dir.is_empty() {
                    None
                } else {
                    Some(config.llm.claude_config_dir.clone())
                }
            });

        let model = std::env::var("CLAUDE_MODEL")
            .unwrap_or_else(|_| config.llm.model.clone());

        let max_turns = config.llm.max_turns;
        let timeout_secs = config.llm.timeout_secs;

        info!(
            claude_bin = %claude_bin,
            model = %model,
            config_dir = ?config_dir,
            max_turns,
            timeout_secs,
            "Claude CLI engine initialized"
        );

        Ok(Self {
            claude_bin,
            model,
            config_dir,
            personality: config.core_personality.clone(),
            agent_name: config.agent_name.clone(),
            timezone: config.timezone.clone(),
            max_turns,
            timeout_secs,
            work_dir: Config::data_dir(),
        })
    }

    /// Send a message to Claude and return the plain-text response.
    pub async fn generate(&self, ctx: &GenerateContext<'_>) -> Result<String> {
        let system_prompt = prompts::system_prompt(&self.personality, &self.agent_name, ctx.tools, Some(&self.timezone), ctx.prompt_skills);
        let mut cmd = Command::new(&self.claude_bin);

        cmd.arg("-p")
            .arg("--output-format").arg("text")
            .arg("--model").arg(&self.model)
            .arg("--max-turns").arg(self.max_turns.to_string())
            .arg("--dangerously-skip-permissions")
            .arg("--append-system-prompt").arg(&system_prompt);

        if let Some(dir) = &self.config_dir {
            cmd.env("CLAUDE_CONFIG_DIR", dir);
        }

        cmd.current_dir(&self.work_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let message = ctx.message;
        debug!(model = %self.model, prompt_len = message.len(), max_turns = self.max_turns, "invoking claude CLI");

        let mut child = cmd.spawn().map_err(|e| {
            SafeAgentError::Llm(format!(
                "failed to spawn claude CLI ({}): {e}",
                self.claude_bin
            ))
        })?;

        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(message.as_bytes()).await.map_err(|e| {
                SafeAgentError::Llm(format!("failed to write to claude stdin: {e}"))
            })?;
        }

        let output = if self.timeout_secs > 0 {
            let timeout = Duration::from_secs(self.timeout_secs);
            match tokio::time::timeout(timeout, child.wait_with_output()).await {
                Ok(result) => result.map_err(|e| {
                    SafeAgentError::Llm(format!("claude CLI failed: {e}"))
                })?,
                Err(_) => {
                    warn!(timeout_secs = self.timeout_secs, "claude CLI timed out");
                    return Err(SafeAgentError::Llm(format!(
                        "claude CLI timed out after {}s",
                        self.timeout_secs
                    )));
                }
            }
        } else {
            child.wait_with_output().await.map_err(|e| {
                SafeAgentError::Llm(format!("claude CLI failed: {e}"))
            })?
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(
                exit_code = ?output.status.code(),
                stderr = %stderr,
                "claude CLI exited with error"
            );
            return Err(SafeAgentError::Llm(format!(
                "claude CLI exited with {}: {}",
                output.status,
                stderr.trim()
            )));
        }

        let response = String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string();

        info!(response_len = response.len(), "claude CLI response received");

        if response.is_empty() {
            return Err(SafeAgentError::Llm(
                "claude CLI returned empty response".into(),
            ));
        }

        Ok(response)
    }
}
