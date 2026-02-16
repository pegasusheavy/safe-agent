pub mod browser;
pub mod cron;
pub mod exec;
pub mod file;
pub mod image;
pub mod knowledge;
pub mod memory;
pub mod message;
pub mod process;
pub mod sessions;
pub mod web;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use rusqlite::Connection;
use tokio::sync::Mutex;

use crate::error::{Result, SafeAgentError};
use crate::security::SandboxedFs;

/// Output from a tool execution.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolOutput {
    pub success: bool,
    pub output: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

impl ToolOutput {
    pub fn ok(output: impl Into<String>) -> Self {
        Self {
            success: true,
            output: output.into(),
            metadata: None,
        }
    }

    pub fn error(output: impl Into<String>) -> Self {
        Self {
            success: false,
            output: output.into(),
            metadata: None,
        }
    }

    pub fn ok_with_meta(output: impl Into<String>, meta: serde_json::Value) -> Self {
        Self {
            success: true,
            output: output.into(),
            metadata: Some(meta),
        }
    }
}

/// A tool call proposed by the LLM.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolCall {
    pub tool: String,
    pub params: serde_json::Value,
    pub reasoning: String,
}

/// Shared context passed to tools during execution.
pub struct ToolContext {
    pub sandbox: SandboxedFs,
    pub db: Arc<Mutex<Connection>>,
    pub http_client: reqwest::Client,
    pub telegram_bot: Option<teloxide::Bot>,
    pub telegram_chat_id: Option<i64>,
}

/// The trait all tools implement.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Unique name of the tool (e.g. "exec", "web_search").
    fn name(&self) -> &str;

    /// Human-readable description for the LLM prompt.
    fn description(&self) -> &str;

    /// JSON Schema describing the tool's parameters.
    fn parameters_schema(&self) -> serde_json::Value;

    /// Execute the tool with the given parameters.
    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput>;
}

/// Registry of all available tools.
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool. Panics on duplicate names.
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        let name = tool.name().to_string();
        assert!(
            !self.tools.contains_key(&name),
            "duplicate tool name: {name}"
        );
        self.tools.insert(name, tool);
    }

    /// Look up a tool by name.
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    /// List all registered tools as (name, description) pairs.
    pub fn list(&self) -> Vec<(&str, &str)> {
        let mut items: Vec<_> = self
            .tools
            .values()
            .map(|t| (t.name(), t.description()))
            .collect();
        items.sort_by_key(|(name, _)| *name);
        items
    }

    /// Execute a tool by name.
    pub async fn execute(
        &self,
        name: &str,
        params: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolOutput> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| SafeAgentError::ToolNotFound(name.to_string()))?;
        tool.execute(params, ctx).await
    }

    /// Build a compact text representation of all tools for the LLM prompt.
    /// Only includes tool names and descriptions to minimize token usage.
    /// Full parameter schemas are available via `parameter_schema_for(name)`.
    pub fn schema_for_prompt(&self) -> String {
        let mut tools: Vec<_> = self.tools.values().collect();
        tools.sort_by_key(|t| t.name());

        let mut out = String::new();
        for tool in &tools {
            out.push_str(&format!("- {}: {}\n", tool.name(), tool.description()));
        }
        out
    }

    /// Number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}
