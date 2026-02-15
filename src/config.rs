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

    #[serde(default)]
    pub llm: LlmConfig,

    #[serde(default)]
    pub tools: ToolsConfig,

    #[serde(default)]
    pub telegram: TelegramConfig,

    #[serde(default)]
    pub google: GoogleConfig,

    #[serde(default)]
    pub sessions: SessionsConfig,
}

// -- LLM -----------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct LlmConfig {
    #[serde(default)]
    pub model_path: String,

    #[serde(default = "default_temperature")]
    pub temperature: f32,

    #[serde(default = "default_top_k")]
    pub top_k: usize,

    #[serde(default = "default_top_p")]
    pub top_p: f32,

    #[serde(default = "default_repeat_penalty")]
    pub repeat_penalty: f32,

    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,

    #[serde(default)]
    pub use_gpu: bool,

    #[serde(default = "default_hf_repo")]
    pub hf_repo: String,

    #[serde(default = "default_hf_filename")]
    pub hf_filename: String,
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
    #[serde(default)]
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

// -- Google --------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct GoogleConfig {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default = "default_google_redirect_uri")]
    pub redirect_uri: String,
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
    50
}
fn default_approval_expiry_secs() -> u64 {
    3600
}
fn default_temperature() -> f32 {
    0.7
}
fn default_top_k() -> usize {
    40
}
fn default_top_p() -> f32 {
    0.9
}
fn default_repeat_penalty() -> f32 {
    1.1
}
fn default_max_tokens() -> usize {
    64
}
fn default_hf_repo() -> String {
    "Qwen/Qwen2.5-0.5B-Instruct-GGUF".to_string()
}
fn default_hf_filename() -> String {
    "qwen2.5-0.5b-instruct-q4_k_m.gguf".to_string()
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
fn default_google_redirect_uri() -> String {
    "http://localhost:3030/auth/google/callback".to_string()
}
fn default_sessions_max_agents() -> usize {
    10
}

// -- Default impls -------------------------------------------------------

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            model_path: String::new(),
            temperature: default_temperature(),
            top_k: default_top_k(),
            top_p: default_top_p(),
            repeat_penalty: default_repeat_penalty(),
            max_tokens: default_max_tokens(),
            use_gpu: false,
            hf_repo: default_hf_repo(),
            hf_filename: default_hf_filename(),
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

impl Default for GoogleConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            redirect_uri: default_google_redirect_uri(),
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
            llm: LlmConfig::default(),
            tools: ToolsConfig::default(),
            telegram: TelegramConfig::default(),
            google: GoogleConfig::default(),
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

    /// Get the Google OAuth2 client ID from the environment.
    pub fn google_client_id() -> Result<String> {
        std::env::var("GOOGLE_CLIENT_ID")
            .map_err(|_| SafeAgentError::Config("GOOGLE_CLIENT_ID environment variable not set".into()))
    }

    /// Get the Google OAuth2 client secret from the environment.
    pub fn google_client_secret() -> Result<String> {
        std::env::var("GOOGLE_CLIENT_SECRET")
            .map_err(|_| SafeAgentError::Config("GOOGLE_CLIENT_SECRET environment variable not set".into()))
    }

    /// Resolve the model path: explicit config > HfClient cache layout > flat fallback
    pub fn resolved_model_path(&self) -> PathBuf {
        if !self.llm.model_path.is_empty() {
            return PathBuf::from(&self.llm.model_path);
        }

        // HfClient caches to <cache_dir>/<owner>--<repo>/<filename>
        let cache_subdir = self.llm.hf_repo.replace('/', "--");
        let hf_path = Self::data_dir()
            .join("models")
            .join(&cache_subdir)
            .join(&self.llm.hf_filename);
        if hf_path.exists() {
            return hf_path;
        }

        // Flat fallback
        Self::data_dir().join("models").join(&self.llm.hf_filename)
    }

    /// Generate the default config file contents.
    pub fn default_config_contents() -> &'static str {
        include_str!("../config.example.toml")
    }
}
