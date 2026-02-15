use thiserror::Error;

#[derive(Error, Debug)]
pub enum SafeAgentError {
    #[error("config error: {0}")]
    Config(String),

    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("LLM error: {0}")]
    Llm(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("sandbox violation: {0}")]
    SandboxViolation(String),

    #[error("network not allowed: {0}")]
    NetworkNotAllowed(String),

    #[error("rate limited: {0}")]
    RateLimited(String),

    #[error("approval error: {0}")]
    Approval(String),

    #[error("agent error: {0}")]
    Agent(String),

    #[error("tool not found: {0}")]
    ToolNotFound(String),

    #[error("tool execution error: {0}")]
    ToolExecution(String),

    #[error("OAuth error: {0}")]
    OAuth(String),

    #[error("Telegram error: {0}")]
    Telegram(String),
}

pub type Result<T> = std::result::Result<T, SafeAgentError>;
