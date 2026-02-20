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

    #[error("messaging error: {0}")]
    Messaging(String),

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("2FA required: {0}")]
    TwoFactorRequired(String),

    #[error("cost limit exceeded: {0}")]
    CostLimitExceeded(String),

    #[error("PII detected: {0}")]
    PiiDetected(String),

    #[error("vector store error: {0}")]
    VectorStore(String),
}

pub type Result<T> = std::result::Result<T, SafeAgentError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_messages() {
        let cases: Vec<(SafeAgentError, &str)> = vec![
            (SafeAgentError::Config("bad key".into()), "config error: bad key"),
            (SafeAgentError::Llm("timeout".into()), "LLM error: timeout"),
            (SafeAgentError::SandboxViolation("path escape".into()), "sandbox violation: path escape"),
            (SafeAgentError::NetworkNotAllowed("localhost".into()), "network not allowed: localhost"),
            (SafeAgentError::RateLimited("too fast".into()), "rate limited: too fast"),
            (SafeAgentError::Approval("not found".into()), "approval error: not found"),
            (SafeAgentError::Agent("crashed".into()), "agent error: crashed"),
            (SafeAgentError::ToolNotFound("foo".into()), "tool not found: foo"),
            (SafeAgentError::ToolExecution("failed".into()), "tool execution error: failed"),
            (SafeAgentError::OAuth("expired".into()), "OAuth error: expired"),
            (SafeAgentError::Telegram("send fail".into()), "Telegram error: send fail"),
            (SafeAgentError::Messaging("offline".into()), "messaging error: offline"),
            (SafeAgentError::PermissionDenied("blocked".into()), "permission denied: blocked"),
            (SafeAgentError::TwoFactorRequired("exec rm".into()), "2FA required: exec rm"),
            (SafeAgentError::CostLimitExceeded("$5.00".into()), "cost limit exceeded: $5.00"),
            (SafeAgentError::PiiDetected("SSN".into()), "PII detected: SSN"),
            (SafeAgentError::VectorStore("embed failed".into()), "vector store error: embed failed"),
        ];
        for (err, expected) in cases {
            assert_eq!(err.to_string(), expected);
        }
    }

    #[test]
    fn error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
        let err: SafeAgentError = io_err.into();
        assert!(err.to_string().contains("gone"));
    }

    #[test]
    fn error_from_json() {
        let json_err = serde_json::from_str::<serde_json::Value>("{{bad").unwrap_err();
        let err: SafeAgentError = json_err.into();
        assert!(err.to_string().starts_with("JSON error:"));
    }

    #[test]
    fn error_is_debug() {
        let err = SafeAgentError::Config("test".into());
        let dbg = format!("{:?}", err);
        assert!(dbg.contains("Config"));
    }

    #[test]
    fn result_type_alias_works() {
        let ok: Result<i32> = Ok(42);
        assert_eq!(ok.unwrap(), 42);
        let err: Result<i32> = Err(SafeAgentError::Agent("fail".into()));
        assert!(err.is_err());
    }


    #[test]
    fn error_from_rusqlite() {
        let err = rusqlite::Connection::open_in_memory()
            .and_then(|c| c.execute("INVALID SQL", []))
            .unwrap_err();
        let wrapped: SafeAgentError = err.into();
        assert!(wrapped.to_string().contains("database error"));
    }
}
