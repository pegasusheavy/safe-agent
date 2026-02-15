use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::debug;

use crate::error::{Result, SafeAgentError};
use crate::llm::prompts::{self, AgentReasoning};
use crate::llm::LlmEngine;
use crate::memory::MemoryManager;

/// Assemble the full context and call the LLM.
/// The LLM inference is CPU/GPU-bound, so it runs on a blocking thread
/// to avoid starving the async runtime (which handles Telegram, dashboard, etc.).
pub async fn think(
    llm: &Arc<Mutex<LlmEngine>>,
    memory: &MemoryManager,
    context_summary: &str,
    stats_summary: &str,
    tool_schema: &str,
) -> Result<AgentReasoning> {
    // Gather context from memory
    let conversation_context = memory.conversation.format_for_prompt().await?;

    // Search archival memory for relevant context
    let archival_context = if !context_summary.is_empty() {
        let query = context_summary.chars().take(100).collect::<String>();
        match memory.archival.search(&query, 5).await {
            Ok(entries) => {
                let mut ctx = String::new();
                for entry in &entries {
                    ctx.push_str(&format!(
                        "[{}] {}: {}\n",
                        entry.created_at, entry.category, entry.content
                    ));
                }
                ctx
            }
            Err(_) => String::new(),
        }
    } else {
        String::new()
    };

    let user_message = prompts::build_user_message(
        context_summary,
        &conversation_context,
        &archival_context,
        stats_summary,
        tool_schema,
    );

    debug!(context_len = user_message.len(), "calling LLM");

    // Run LLM inference on a blocking thread so we don't starve the async runtime.
    let llm_clone = llm.clone();
    let reasoning = tokio::task::spawn_blocking(move || {
        let mut engine = llm_clone.blocking_lock();
        engine.generate(&user_message)
    })
    .await
    .map_err(|e| SafeAgentError::Llm(format!("LLM task panicked: {e}")))??;

    debug!(
        monologue_len = reasoning.internal_monologue.len(),
        num_actions = reasoning.proposed_actions.len(),
        num_memory_updates = reasoning.memory_updates.len(),
        num_knowledge_updates = reasoning.knowledge_updates.len(),
        "LLM reasoning complete"
    );

    Ok(reasoning)
}
