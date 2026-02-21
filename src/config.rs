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

    /// Default timezone for the system (IANA name, e.g. "America/New_York").
    /// Per-user overrides take precedence.  Defaults to "UTC".
    #[serde(default = "default_timezone")]
    pub timezone: String,

    /// Default locale for date/number formatting (BCP 47 tag, e.g. "en-US").
    /// Per-user overrides take precedence.  Defaults to "en-US".
    #[serde(default = "default_locale")]
    pub locale: String,

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

    /// Maximum number of tool-call round-trips per user message before the
    /// agent returns whatever it has.  Prevents infinite tool-call loops.
    #[serde(default = "default_max_tool_turns")]
    pub max_tool_turns: usize,

    #[serde(default)]
    pub llm: LlmConfig,

    #[serde(default)]
    pub tools: ToolsConfig,

    #[serde(default)]
    pub dashboard: DashboardConfig,

    #[serde(default)]
    pub telegram: TelegramConfig,

    #[serde(default)]
    pub whatsapp: WhatsAppConfig,

    #[serde(default)]
    pub sessions: SessionsConfig,

    #[serde(default)]
    pub tunnel: TunnelConfig,

    #[serde(default)]
    pub tls: TlsConfig,

    #[serde(default)]
    pub security: SecurityConfig,

    #[serde(default)]
    pub federation: FederationConfig,

    #[serde(default)]
    pub plugins: PluginsConfig,
}

// -- Federation --------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct FederationConfig {
    /// Enable multi-node federation.
    #[serde(default)]
    pub enabled: bool,

    /// Display name for this node (defaults to agent_name).
    #[serde(default)]
    pub node_name: String,

    /// Advertised address of this node (e.g. "http://host:3031").
    /// Peers use this to connect back.
    #[serde(default)]
    pub advertise_address: String,

}

impl Default for FederationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            node_name: String::new(),
            advertise_address: String::new(),
        }
    }
}

// -- Security ----------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct SecurityConfig {
    /// Tools that are completely blocked (never executable).
    #[serde(default)]
    pub blocked_tools: Vec<String>,

    /// Tools that require 2FA (confirmation on a second channel) before execution.
    #[serde(default = "default_2fa_tools")]
    pub require_2fa: Vec<String>,

    /// Maximum tool calls per minute (0 = unlimited).
    #[serde(default = "default_rate_limit_per_minute")]
    pub rate_limit_per_minute: u32,

    /// Maximum tool calls per hour (0 = unlimited).
    #[serde(default = "default_rate_limit_per_hour")]
    pub rate_limit_per_hour: u32,

    /// Maximum estimated LLM cost per day in USD (0.0 = unlimited).
    #[serde(default)]
    pub daily_cost_limit_usd: f64,

    /// Enable PII/sensitive data detection in LLM responses.
    #[serde(default = "default_true")]
    pub pii_detection: bool,

    /// Capability restrictions per tool. Keys are tool names, values are
    /// lists of allowed operations/capabilities.
    /// e.g. { "exec" = ["echo", "ls", "cat"], "file" = ["read"] }
    #[serde(default)]
    pub tool_capabilities: std::collections::HashMap<String, Vec<String>>,
}

// -- LLM -----------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct LlmConfig {
    /// Backend to use: "claude" (default), "codex", "gemini", "aider",
    /// "openrouter", or "local".
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

    // -- OpenRouter settings (backend = "openrouter") --

    /// OpenRouter API key.
    /// Can be overridden with `OPENROUTER_API_KEY` env var.
    #[serde(default)]
    pub openrouter_api_key: String,

    /// OpenRouter model identifier (e.g. "anthropic/claude-sonnet-4",
    /// "openai/gpt-4o", "google/gemini-2.5-pro", "meta-llama/llama-4-maverick").
    /// Can be overridden with `OPENROUTER_MODEL` env var.
    #[serde(default)]
    pub openrouter_model: String,

    /// OpenRouter API base URL (default: "https://openrouter.ai/api/v1").
    /// Can be overridden with `OPENROUTER_BASE_URL` env var.
    #[serde(default)]
    pub openrouter_base_url: String,

    /// Max tokens for OpenRouter completions (0 = use general max_tokens).
    #[serde(default)]
    pub openrouter_max_tokens: usize,

    /// Site URL sent as HTTP-Referer for OpenRouter analytics.
    /// Can be overridden with `OPENROUTER_SITE_URL` env var.
    #[serde(default)]
    pub openrouter_site_url: String,

    /// App name sent as X-Title for OpenRouter dashboard identification.
    /// Can be overridden with `OPENROUTER_APP_NAME` env var.
    #[serde(default)]
    pub openrouter_app_name: String,

    // -- Local model settings (backend = "local") --

    /// Path to a GGUF model file for local inference.
    /// Can be overridden with the `MODEL_PATH` env var.
    #[serde(default)]
    pub model_path: String,

    /// Temperature for local sampling (0.0 = greedy).
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    /// Top-P (nucleus) sampling for local model.
    #[serde(default = "default_top_p")]
    pub top_p: f32,

    /// Maximum tokens to generate per response.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,

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

    #[serde(default = "default_exec_timeout")]
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WebToolConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default = "default_web_max_results")]
    pub max_results: usize,
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
    #[serde(default = "default_true")]
    pub enabled: bool,
}

// -- Dashboard -----------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct DashboardConfig {
    /// Whether password-based login is enabled (default: true).
    /// Set to false to require SSO-only login.
    #[serde(default = "default_true")]
    pub password_enabled: bool,

    /// SSO providers enabled for dashboard login.
    /// Use provider IDs from the OAuth registry: "google", "github",
    /// "microsoft", "discord", etc.
    #[serde(default)]
    pub sso_providers: Vec<String>,

    /// Email addresses allowed to sign in via SSO.
    /// Empty means any authenticated SSO user is allowed.
    #[serde(default)]
    pub sso_allowed_emails: Vec<String>,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            password_enabled: true,
            sso_providers: Vec::new(),
            sso_allowed_emails: Vec::new(),
        }
    }
}

// -- Telegram ------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct TelegramConfig {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default)]
    pub allowed_chat_ids: Vec<i64>,
}

// -- WhatsApp ------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct WhatsAppConfig {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default = "default_whatsapp_bridge_port")]
    pub bridge_port: u16,

    /// The dashboard port used to construct the webhook URL that the
    /// bridge POSTs incoming messages to.
    #[serde(default = "default_whatsapp_webhook_port")]
    pub webhook_port: u16,

    #[serde(default)]
    pub allowed_numbers: Vec<String>,
}

fn default_whatsapp_bridge_port() -> u16 {
    3033
}

fn default_whatsapp_webhook_port() -> u16 {
    3030
}

impl Default for WhatsAppConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bridge_port: default_whatsapp_bridge_port(),
            webhook_port: default_whatsapp_webhook_port(),
            allowed_numbers: Vec::new(),
        }
    }
}

// -- Sessions ------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct SessionsConfig {
    #[serde(default)]
    pub enabled: bool,
}

// -- Plugins -------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct PluginsConfig {
    /// Global plugin directory (default: ~/.config/safe-agent/plugins).
    /// Empty string means use the default path.
    #[serde(default)]
    pub global_dir: String,

    /// Project-local plugin directory (default: .safe-agent/plugins).
    /// Relative to the working directory. Empty means use default.
    #[serde(default)]
    pub project_dir: String,

    /// Plugin names to explicitly disable.
    #[serde(default)]
    pub disabled: Vec<String>,
}

impl Default for PluginsConfig {
    fn default() -> Self {
        Self {
            global_dir: String::new(),
            project_dir: String::new(),
            disabled: Vec::new(),
        }
    }
}

// -- TLS / ACME ----------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct TlsConfig {
    /// Enable ACME (Let's Encrypt) automatic certificate management.
    /// When enabled, `acme_domains` and `acme_email` are required.
    /// Can be overridden with `ACME_ENABLED=true`.
    #[serde(default)]
    pub acme_enabled: bool,

    /// Domain name(s) for the certificate.
    /// Can be overridden with `ACME_DOMAIN`.
    #[serde(default)]
    pub acme_domains: Vec<String>,

    /// Contact email for Let's Encrypt (e.g. "mailto:admin@example.com").
    /// Can be overridden with `ACME_EMAIL`.
    #[serde(default)]
    pub acme_email: String,

    /// Use the Let's Encrypt production CA (true) or staging (false).
    /// Staging is useful for testing — it doesn't enforce rate limits.
    /// Can be overridden with `ACME_PRODUCTION=true`.
    #[serde(default)]
    pub acme_production: bool,

    /// Directory to cache ACME account keys and certificates.
    /// Defaults to `$XDG_DATA_HOME/safe-agent/acme-cache`.
    #[serde(default)]
    pub acme_cache_dir: String,

    /// Port for the HTTPS listener (default: 443).
    /// Can be overridden with `ACME_PORT`.
    #[serde(default = "default_acme_port")]
    pub acme_port: u16,
}

// -- Tunnel (ngrok) ------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct TunnelConfig {
    /// Enable the ngrok tunnel on startup.
    #[serde(default)]
    pub enabled: bool,

    /// Path to the `ngrok` binary.
    /// Can be overridden with `NGROK_BIN`.
    #[serde(default = "default_ngrok_bin")]
    pub ngrok_bin: String,

    /// Ngrok auth token.
    /// Can be overridden with `NGROK_AUTHTOKEN` (preferred).
    #[serde(default)]
    pub authtoken: String,

    /// Ngrok static/reserved domain (e.g. "myapp.ngrok-free.app").
    /// Empty = use random subdomain.
    /// Can be overridden with `NGROK_DOMAIN`.
    #[serde(default)]
    pub domain: String,

    /// Local port for ngrok's inspection API (default 4040).
    #[serde(default = "default_ngrok_inspect_port")]
    pub inspect_port: u16,

    /// How often to poll the ngrok API for the tunnel URL (seconds).
    #[serde(default = "default_ngrok_poll_interval")]
    pub poll_interval_secs: u64,
}

// -- Defaults ------------------------------------------------------------

fn default_agent_name() -> String {
    "safe-agent".to_string()
}
fn default_timezone() -> String {
    "UTC".to_string()
}
fn default_locale() -> String {
    "en-US".to_string()
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
    vec![
        "message".to_string(),
        "memory_search".to_string(),
        "memory_get".to_string(),
        "goal".to_string(),
    ]
}
fn default_max_tool_turns() -> usize {
    5
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
fn default_top_p() -> f32 {
    0.95
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
fn default_web_max_results() -> usize {
    10
}
fn default_acme_port() -> u16 {
    443
}
fn default_ngrok_bin() -> String {
    "ngrok".to_string()
}
fn default_ngrok_inspect_port() -> u16 {
    4040
}
fn default_ngrok_poll_interval() -> u64 {
    15
}
fn default_2fa_tools() -> Vec<String> {
    vec![
        "exec".to_string(),
    ]
}
fn default_rate_limit_per_minute() -> u32 {
    30
}
fn default_rate_limit_per_hour() -> u32 {
    300
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
            openrouter_api_key: String::new(),
            openrouter_model: String::new(),
            openrouter_base_url: String::new(),
            openrouter_max_tokens: 0,
            openrouter_site_url: String::new(),
            openrouter_app_name: String::new(),
            model_path: String::new(),
            temperature: default_temperature(),
            top_p: default_top_p(),
            max_tokens: default_max_tokens(),
        }
    }
}

impl Default for ExecToolConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            timeout_secs: default_exec_timeout(),
        }
    }
}

impl Default for WebToolConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_results: default_web_max_results(),
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
            enabled: true,
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
        }
    }
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            acme_enabled: false,
            acme_domains: Vec::new(),
            acme_email: String::new(),
            acme_production: false,
            acme_cache_dir: String::new(),
            acme_port: default_acme_port(),
        }
    }
}

impl Default for TunnelConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            ngrok_bin: default_ngrok_bin(),
            authtoken: String::new(),
            domain: String::new(),
            inspect_port: default_ngrok_inspect_port(),
            poll_interval_secs: default_ngrok_poll_interval(),
        }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            blocked_tools: Vec::new(),
            require_2fa: default_2fa_tools(),
            rate_limit_per_minute: default_rate_limit_per_minute(),
            rate_limit_per_hour: default_rate_limit_per_hour(),
            daily_cost_limit_usd: 0.0,
            pii_detection: true,
            tool_capabilities: std::collections::HashMap::new(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            agent_name: default_agent_name(),
            core_personality: String::new(),
            timezone: default_timezone(),
            locale: default_locale(),
            dashboard_bind: default_dashboard_bind(),
            tick_interval_secs: default_tick_interval_secs(),
            conversation_window: default_conversation_window(),
            approval_expiry_secs: default_approval_expiry_secs(),
            auto_approve_tools: default_auto_approve_tools(),
            max_tool_turns: default_max_tool_turns(),
            llm: LlmConfig::default(),
            tools: ToolsConfig::default(),
            dashboard: DashboardConfig::default(),
            telegram: TelegramConfig::default(),
            whatsapp: WhatsAppConfig::default(),
            sessions: SessionsConfig::default(),
            tunnel: TunnelConfig::default(),
            tls: TlsConfig::default(),
            security: SecurityConfig::default(),
            federation: FederationConfig::default(),
            plugins: PluginsConfig::default(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_expected_values() {
        let c = Config::default();
        assert_eq!(c.agent_name, "safe-agent");
        assert_eq!(c.dashboard_bind, "127.0.0.1:3030");
        assert_eq!(c.tick_interval_secs, 120);
        assert_eq!(c.conversation_window, 5);
        assert_eq!(c.approval_expiry_secs, 3600);
        assert_eq!(c.max_tool_turns, 5);
        assert!(c.core_personality.is_empty());
    }

    #[test]
    fn test_default_auto_approve_tools() {
        let c = Config::default();
        assert!(c.auto_approve_tools.contains(&"message".to_string()));
        assert!(c.auto_approve_tools.contains(&"memory_search".to_string()));
        assert!(c.auto_approve_tools.contains(&"memory_get".to_string()));
        assert!(c.auto_approve_tools.contains(&"goal".to_string()));
        assert_eq!(c.auto_approve_tools.len(), 4);
    }

    #[test]
    fn default_llm_config() {
        let llm = LlmConfig::default();
        assert_eq!(llm.backend, "claude");
        assert_eq!(llm.claude_bin, "claude");
        assert_eq!(llm.model, "sonnet");
        assert_eq!(llm.max_turns, 10);
        assert_eq!(llm.timeout_secs, 120);
        assert!((llm.temperature - 0.7).abs() < 0.001);
        assert!((llm.top_p - 0.95).abs() < 0.001);
        assert_eq!(llm.max_tokens, 2048);
    }

    #[test]
    fn default_tools_config() {
        let tools = ToolsConfig::default();
        assert!(tools.exec.enabled);
        assert_eq!(tools.exec.timeout_secs, 30);
        assert!(tools.web.enabled);
        assert_eq!(tools.web.max_results, 10);
        assert!(!tools.browser.enabled);
        assert!(tools.browser.headless);
        assert!(!tools.message.enabled);
        assert!(tools.cron.enabled);
    }

    #[test]
    fn default_dashboard_config() {
        let d = DashboardConfig::default();
        assert!(d.password_enabled);
        assert!(d.sso_providers.is_empty());
        assert!(d.sso_allowed_emails.is_empty());
    }

    #[test]
    fn default_telegram_config() {
        let t = TelegramConfig::default();
        assert!(!t.enabled);
        assert!(t.allowed_chat_ids.is_empty());
    }

    #[test]
    fn default_whatsapp_config() {
        let w = WhatsAppConfig::default();
        assert!(!w.enabled);
        assert_eq!(w.bridge_port, 3033);
        assert_eq!(w.webhook_port, 3030);
        assert!(w.allowed_numbers.is_empty());
    }

    #[test]
    fn default_sessions_config() {
        let s = SessionsConfig::default();
        assert!(!s.enabled);
    }

    #[test]
    fn default_tls_config() {
        let t = TlsConfig::default();
        assert!(!t.acme_enabled);
        assert!(t.acme_domains.is_empty());
        assert!(t.acme_email.is_empty());
        assert!(!t.acme_production);
        assert_eq!(t.acme_port, 443);
    }

    #[test]
    fn default_tunnel_config() {
        let t = TunnelConfig::default();
        assert!(!t.enabled);
        assert_eq!(t.ngrok_bin, "ngrok");
        assert!(t.authtoken.is_empty());
        assert!(t.domain.is_empty());
        assert_eq!(t.inspect_port, 4040);
        assert_eq!(t.poll_interval_secs, 15);
    }

    #[test]
    fn parse_minimal_toml() {
        let toml_str = r#"agent_name = "TestBot""#;
        let c: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(c.agent_name, "TestBot");
        assert_eq!(c.dashboard_bind, "127.0.0.1:3030");
        assert_eq!(c.max_tool_turns, 5);
    }

    #[test]
    fn parse_llm_section() {
        let toml_str = r#"
        [llm]
        backend = "openrouter"
        model = "opus"
        max_turns = 20
        temperature = 0.5
        "#;
        let c: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(c.llm.backend, "openrouter");
        assert_eq!(c.llm.model, "opus");
        assert_eq!(c.llm.max_turns, 20);
        assert!((c.llm.temperature - 0.5).abs() < 0.001);
    }

    #[test]
    fn parse_tools_section() {
        let toml_str = r#"
        [tools.exec]
        enabled = false
        timeout_secs = 60
        "#;
        let c: Config = toml::from_str(toml_str).unwrap();
        assert!(!c.tools.exec.enabled);
        assert_eq!(c.tools.exec.timeout_secs, 60);
    }

    #[test]
    fn parse_dashboard_sso() {
        let toml_str = r#"
        [dashboard]
        password_enabled = false
        sso_providers = ["google", "github"]
        sso_allowed_emails = ["admin@example.com"]
        "#;
        let c: Config = toml::from_str(toml_str).unwrap();
        assert!(!c.dashboard.password_enabled);
        assert_eq!(c.dashboard.sso_providers, vec!["google", "github"]);
        assert_eq!(c.dashboard.sso_allowed_emails, vec!["admin@example.com"]);
    }

    #[test]
    fn load_nonexistent_returns_defaults() {
        let c = Config::load(Some(Path::new("/tmp/nonexistent-safe-agent-test.toml"))).unwrap();
        assert_eq!(c.agent_name, "safe-agent");
    }

    #[test]
    fn load_invalid_toml_returns_error() {
        let path = std::env::temp_dir().join("bad-safe-agent.toml");
        std::fs::write(&path, "this is not valid %%% toml").unwrap();
        let result = Config::load(Some(&path));
        assert!(result.is_err());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn default_config_path_has_safe_agent() {
        let path = Config::default_config_path();
        assert!(path.to_string_lossy().contains("safe-agent"));
        assert!(path.to_string_lossy().contains("config.toml"));
    }

    #[test]
    fn data_dir_has_safe_agent() {
        let path = Config::data_dir();
        assert!(path.to_string_lossy().contains("safe-agent"));
    }

    #[test]
    fn telegram_bot_token_without_env_var_errors() {
        unsafe { std::env::remove_var("TELEGRAM_BOT_TOKEN"); }
        assert!(Config::telegram_bot_token().is_err());
    }

    #[test]
    fn default_config_contents_is_non_empty() {
        let contents = Config::default_config_contents();
        assert!(!contents.is_empty());
    }

    #[test]
    fn default_plugins_config() {
        let c = Config::default();
        assert!(c.plugins.global_dir.is_empty());
        assert!(c.plugins.project_dir.is_empty());
        assert!(c.plugins.disabled.is_empty());
    }

    #[test]
    fn parse_plugins_section() {
        let toml_str = r#"
        [plugins]
        global_dir = "/home/user/.config/safe-agent/plugins"
        project_dir = ".safe-agent/plugins"
        disabled = ["broken-plugin"]
        "#;
        let c: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(c.plugins.global_dir, "/home/user/.config/safe-agent/plugins");
        assert_eq!(c.plugins.project_dir, ".safe-agent/plugins");
        assert_eq!(c.plugins.disabled, vec!["broken-plugin"]);
    }
}
