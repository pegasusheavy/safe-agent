use serde::Deserialize;
use std::path::{Path, PathBuf};
use tracing::info;

use crate::error::{Result, SafeAgentError};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_agent_name")]
    pub agent_name: String,

    #[serde(default)]
    pub core_personality: String,

    #[serde(default = "default_dashboard_bind")]
    pub dashboard_bind: String,

    #[serde(default = "default_tick_interval_secs")]
    pub tick_interval_secs: u64,

    #[serde(default = "default_conversation_window")]
    pub conversation_window: usize,

    #[serde(default = "default_approval_expiry_secs")]
    pub approval_expiry_secs: u64,

    #[serde(default = "default_auto_approve_tools")]
    pub auto_approve_tools: Vec<String>,

    #[serde(default)]
    pub llm: LlmConfig,

    #[serde(default)]
    pub tools: ToolsConfig,

    #[serde(default)]
    pub telegram: TelegramConfig,

    #[serde(default)]
    pub sessions: SessionsConfig,
}

// -- LLM -----------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct LlmConfig {
    /// Backend to use: "claude" (default), "codex", "gemini", "aider", or "local".
    /// Can be overridden with the `LLM_BACKEND` env var.
    #[serde(default = "default_backend")]
    pub backend: String,

    // -- Claude CLI settings (backend = "claude") --

    /// Path to the `claude` binary (default: "claude").
    /// Can be overridden with the `CLAUDE_BIN` env var.
    #[serde(default = "default_claude_bin")]
    pub claude_bin: String,

    /// Claude Code config directory for profile selection.
    /// Can be overridden with the `CLAUDE_CONFIG_DIR` env var.
    #[serde(default)]
    pub claude_config_dir: String,

    /// Model to use (e.g. "sonnet", "opus", "haiku").
    /// Can be overridden with the `CLAUDE_MODEL` env var.
    #[serde(default = "default_model")]
    pub model: String,

    /// Maximum tool-use turns per Claude CLI invocation.
    #[serde(default = "default_max_turns")]
    pub max_turns: u32,

    /// Process timeout in seconds (0 = no timeout).
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,

    // -- Codex CLI settings (backend = "codex") --

    /// Path to the `codex` binary (default: "codex").
    /// Can be overridden with the `CODEX_BIN` env var.
    #[serde(default = "default_codex_bin")]
    pub codex_bin: String,

    /// Codex model override (e.g. "gpt-5-codex", "o3").
    /// Can be overridden with the `CODEX_MODEL` env var.
    #[serde(default)]
    pub codex_model: String,

    /// Codex config profile name (from `~/.codex/config.toml`).
    /// Can be overridden with the `CODEX_PROFILE` env var.
    #[serde(default)]
    pub codex_profile: String,

    // -- Gemini CLI settings (backend = "gemini") --

    /// Path to the `gemini` binary (default: "gemini").
    /// Can be overridden with the `GEMINI_BIN` env var.
    #[serde(default = "default_gemini_bin")]
    pub gemini_bin: String,

    /// Gemini model override (e.g. "gemini-2.5-pro").
    /// Can be overridden with the `GEMINI_MODEL` env var.
    #[serde(default)]
    pub gemini_model: String,

    // -- Aider settings (backend = "aider") --

    /// Path to the `aider` binary (default: "aider").
    /// Can be overridden with the `AIDER_BIN` env var.
    #[serde(default = "default_aider_bin")]
    pub aider_bin: String,

    /// Aider model string (e.g. "gpt-4o", "claude-3.5-sonnet").
    /// Can be overridden with the `AIDER_MODEL` env var.
    #[serde(default)]
    pub aider_model: String,

    // -- Local model settings (backend = "local") --

    /// Path to a GGUF model file for local inference.
    /// Can be overridden with the `MODEL_PATH` env var.
    #[serde(default)]
    pub model_path: String,

    /// Temperature for local sampling (0.0 = greedy).
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    /// Top-K sampling for local model (0 = disabled).
    #[serde(default = "default_top_k")]
    pub top_k: usize,

    /// Top-P (nucleus) sampling for local model.
    #[serde(default = "default_top_p")]
    pub top_p: f32,

    /// Repetition penalty for local model (1.0 = none).
    #[serde(default = "default_repeat_penalty")]
    pub repeat_penalty: f32,

    /// Maximum tokens to generate per response.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,

    /// Maximum context length for the local model.
    /// Caps KV cache size; 0 = use model default.
    #[serde(default)]
    pub context_length: usize,

    /// Use GPU acceleration for local model (requires cuda feature).
    #[serde(default)]
    pub use_gpu: bool,
}

// -- Tools ---------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ToolsConfig {
    #[serde(default)]
    pub exec: ExecToolConfig,

    #[serde(default)]
    pub web: WebToolConfig,

    #[serde(default)]
    pub browser: BrowserToolConfig,

    #[serde(default)]
    pub message: MessageToolConfig,

    #[serde(default)]
    pub cron: CronToolConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExecToolConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default)]
    pub allowed_commands: Vec<String>,

    #[serde(default = "default_exec_timeout")]
    pub timeout_secs: u64,

    #[serde(default = "default_exec_security")]
    pub security: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WebToolConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default = "default_true")]
    pub safe_search: bool,

    #[serde(default = "default_web_max_results")]
    pub max_results: usize,

    #[serde(default)]
    pub allowed_domains: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BrowserToolConfig {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default = "default_true")]
    pub headless: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MessageToolConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CronToolConfig {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default = "default_cron_max_jobs")]
    pub max_jobs: usize,
}

// -- Telegram ------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct TelegramConfig {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default)]
    pub allowed_chat_ids: Vec<i64>,
}

// -- Sessions ------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct SessionsConfig {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default = "default_sessions_max_agents")]
    pub max_agents: usize,
}

// -- Defaults ------------------------------------------------------------

fn default_agent_name() -> String {
    "safe-agent".to_string()
}
fn default_dashboard_bind() -> String {
    "127.0.0.1:3030".to_string()
}
fn default_tick_interval_secs() -> u64 {
    120
}
fn default_conversation_window() -> usize {
    5
}
fn default_approval_expiry_secs() -> u64 {
    3600
}
fn default_auto_approve_tools() -> Vec<String> {
    vec!["message".to_string(), "memory_search".to_string(), "memory_get".to_string()]
}
fn default_backend() -> String {
    "claude".to_string()
}
fn default_claude_bin() -> String {
    "claude".to_string()
}
fn default_codex_bin() -> String {
    "codex".to_string()
}
fn default_gemini_bin() -> String {
    "gemini".to_string()
}
fn default_aider_bin() -> String {
    "aider".to_string()
}
fn default_model() -> String {
    "sonnet".to_string()
}
fn default_max_turns() -> u32 {
    10
}
fn default_timeout_secs() -> u64 {
    120
}
fn default_temperature() -> f32 {
    0.7
}
fn default_top_k() -> usize {
    40
}
fn default_top_p() -> f32 {
    0.95
}
fn default_repeat_penalty() -> f32 {
    1.1
}
fn default_max_tokens() -> usize {
    2048
}
fn default_true() -> bool {
    true
}
fn default_exec_timeout() -> u64 {
    30
}
fn default_exec_security() -> String {
    "approval".to_string()
}
fn default_web_max_results() -> usize {
    10
}
fn default_cron_max_jobs() -> usize {
    50
}
fn default_sessions_max_agents() -> usize {
    10
}

// -- Default impls -------------------------------------------------------

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            backend: default_backend(),
            claude_bin: default_claude_bin(),
            claude_config_dir: String::new(),
            model: default_model(),
            max_turns: default_max_turns(),
            timeout_secs: default_timeout_secs(),
            codex_bin: default_codex_bin(),
            codex_model: String::new(),
            codex_profile: String::new(),
            gemini_bin: default_gemini_bin(),
            gemini_model: String::new(),
            aider_bin: default_aider_bin(),
            aider_model: String::new(),
            model_path: String::new(),
            temperature: default_temperature(),
            top_k: default_top_k(),
            top_p: default_top_p(),
            repeat_penalty: default_repeat_penalty(),
            max_tokens: default_max_tokens(),
            context_length: 0,
            use_gpu: false,
        }
    }
}

impl Default for ExecToolConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            allowed_commands: Vec::new(),
            timeout_secs: default_exec_timeout(),
            security: default_exec_security(),
        }
    }
}

impl Default for WebToolConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            safe_search: true,
            max_results: default_web_max_results(),
            allowed_domains: Vec::new(),
        }
    }
}

impl Default for BrowserToolConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            headless: true,
        }
    }
}

impl Default for MessageToolConfig {
    fn default() -> Self {
        Self { enabled: false }
    }
}

impl Default for CronToolConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_jobs: default_cron_max_jobs(),
        }
    }
}

impl Default for TelegramConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            allowed_chat_ids: Vec::new(),
        }
    }
}

impl Default for SessionsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_agents: default_sessions_max_agents(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            agent_name: default_agent_name(),
            core_personality: String::new(),
            dashboard_bind: default_dashboard_bind(),
            tick_interval_secs: default_tick_interval_secs(),
            conversation_window: default_conversation_window(),
            approval_expiry_secs: default_approval_expiry_secs(),
            auto_approve_tools: default_auto_approve_tools(),
            llm: LlmConfig::default(),
            tools: ToolsConfig::default(),
            telegram: TelegramConfig::default(),
            sessions: SessionsConfig::default(),
        }
    }
}

// -- Config impl ---------------------------------------------------------

impl Config {
    /// Load config from the given path, or the default XDG config location.
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let config_path = match path {
            Some(p) => p.to_path_buf(),
            None => Self::default_config_path(),
        };

        let config = if config_path.exists() {
            info!("loading config from {}", config_path.display());
            let contents = std::fs::read_to_string(&config_path).map_err(SafeAgentError::Io)?;
            toml::from_str(&contents)
                .map_err(|e| SafeAgentError::Config(format!("parse error: {e}")))?
        } else {
            info!("no config file found, using defaults");
            Config::default()
        };

        Ok(config)
    }

    /// Returns the default config file path: `$XDG_CONFIG_HOME/safe-agent/config.toml`
    pub fn default_config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from(".config"))
            .join("safe-agent")
            .join("config.toml")
    }

    /// Returns the data directory: `$XDG_DATA_HOME/safe-agent/`
    pub fn data_dir() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from(".local/share"))
            .join("safe-agent")
    }

    /// Get the Telegram bot token from the environment.
    pub fn telegram_bot_token() -> Result<String> {
        std::env::var("TELEGRAM_BOT_TOKEN")
            .map_err(|_| SafeAgentError::Config("TELEGRAM_BOT_TOKEN environment variable not set".into()))
    }

    /// Generate the default config file contents.
    pub fn default_config_contents() -> &'static str {
        include_str!("../config.example.toml")
    }
}
