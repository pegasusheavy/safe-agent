use tracing::{debug, error, info, warn};

use crate::error::Result;
use crate::tools::ToolCall;

use super::Agent;

impl Agent {
    /// One full agent cycle: gather context → think → propose tool calls → update memory.
    pub async fn tick(&self) -> Result<()> {
        info!("tick starting");

        // Gather context (no external feed to fetch — context comes from conversation, memory, knowledge)
        let context_summary = self.gather_context().await;

        // Get stats summary
        let stats = self.memory.get_stats().await?;
        let stats_summary = format!(
            "Ticks: {}, Actions: {}, Approved: {}, Rejected: {}",
            stats.total_ticks, stats.total_actions, stats.total_approved, stats.total_rejected,
        );

        // Think: call LLM with tool list and context
        let reasoning = {
            let tool_schema = self.tools.schema_for_prompt();
            match super::reasoning::think(
                &self.llm,
                &self.memory,
                &context_summary,
                &stats_summary,
                &tool_schema,
            )
            .await
            {
                Ok(r) => r,
                Err(e) => {
                    error!("LLM reasoning failed: {e}");
                    self.memory
                        .log_activity("tick", "LLM reasoning failed", Some(&e.to_string()), "error")
                        .await?;
                    return Ok(());
                }
            }
        };

        // Log the internal monologue as a conversation entry
        self.memory
            .conversation
            .append("assistant", &reasoning.internal_monologue)
            .await?;

        // Process memory updates
        for update in &reasoning.memory_updates {
            debug!(category = %update.category, "storing memory update");
            self.memory
                .archival
                .archive(&update.content, &update.category)
                .await?;
        }

        // Process knowledge updates
        for kg_update in &reasoning.knowledge_updates {
            debug!(action = kg_update.get("action").and_then(|v| v.as_str()).unwrap_or("?"), "knowledge update");
            // Knowledge updates are proposed as tool calls that go through approval
            let call = ToolCall {
                tool: "knowledge_graph".to_string(),
                params: kg_update.clone(),
                reasoning: "Knowledge graph update from reasoning".to_string(),
            };
            let action_json = serde_json::to_value(&call)?;
            let id = self
                .approval_queue
                .propose(action_json, &call.reasoning, &context_summary)
                .await?;
            info!(id, "proposed knowledge update");
        }

        // Propose tool calls to the approval queue
        for proposed in &reasoning.proposed_actions {
            let call = ToolCall {
                tool: proposed
                    .get("tool")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                params: proposed
                    .get("params")
                    .cloned()
                    .unwrap_or_default(),
                reasoning: proposed
                    .get("reasoning")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            };

            let action_json = serde_json::to_value(&call)?;
            let summary = format!("{}({})", call.tool, call.params);
            let id = self
                .approval_queue
                .propose(action_json, &call.reasoning, &context_summary)
                .await?;
            info!(id, tool = %call.tool, "proposed action");
            self.memory
                .log_activity("propose", &summary, Some(&call.reasoning), "pending")
                .await?;
            self.notify_update();
        }

        // Expire stale pending actions
        let expired = self.approval_queue.expire_stale().await?;
        if expired > 0 {
            info!(count = expired, "expired stale actions");
        }

        // Record tick
        self.memory.record_tick().await?;

        info!(
            actions_proposed = reasoning.proposed_actions.len(),
            memory_updates = reasoning.memory_updates.len(),
            knowledge_updates = reasoning.knowledge_updates.len(),
            "tick complete"
        );

        Ok(())
    }

    /// Gather context from conversation history, archival memory, and knowledge graph.
    async fn gather_context(&self) -> String {
        let mut ctx = String::new();

        // Recent conversation
        if let Ok(conv) = self.memory.conversation.format_for_prompt().await {
            if !conv.is_empty() {
                ctx.push_str("Recent conversation:\n");
                ctx.push_str(&conv);
                ctx.push_str("\n\n");
            }
        }

        // Knowledge graph stats
        let kg = crate::memory::knowledge::KnowledgeGraph::new(self.ctx.db.clone());
        if let Ok((nodes, edges)) = kg.stats().await {
            if nodes > 0 {
                ctx.push_str(&format!(
                    "Knowledge graph: {nodes} nodes, {edges} edges\n\n"
                ));
            }
        }

        if ctx.is_empty() {
            "No specific context available. Awaiting instructions.".to_string()
        } else {
            ctx
        }
    }

    /// Execute approved actions from the queue.
    pub async fn execute_approved(&self) -> Result<()> {
        while let Some(pending) = self.approval_queue.next_approved().await? {
            let call: ToolCall = match serde_json::from_value(pending.action.clone()) {
                Ok(c) => c,
                Err(e) => {
                    warn!("failed to deserialize tool call: {e}");
                    self.approval_queue.mark_executed(&pending.id, false).await?;
                    continue;
                }
            };

            info!(id = %pending.id, tool = %call.tool, "executing approved tool call");

            match super::actions::execute_tool_call(&self.tools, &self.ctx, &call).await {
                Ok(output) => {
                    let success_str = if output.success { "ok" } else { "error" };
                    info!(id = %pending.id, success = output.success, "tool executed");
                    self.approval_queue
                        .mark_executed(&pending.id, output.success)
                        .await?;
                    self.memory.record_action().await?;
                    self.memory
                        .log_activity(
                            "execute",
                            &format!("{}({})", call.tool, call.params),
                            Some(&output.output),
                            success_str,
                        )
                        .await?;
                    self.memory
                        .conversation
                        .append("system", &format!("Tool {} result: {}", call.tool, output.output))
                        .await?;
                    self.notify_update();
                }
                Err(e) => {
                    error!(id = %pending.id, error = %e, "tool execution failed");
                    self.approval_queue.mark_executed(&pending.id, false).await?;
                    self.memory
                        .log_activity(
                            "execute",
                            &format!("{}({})", call.tool, call.params),
                            Some(&e.to_string()),
                            "error",
                        )
                        .await?;
                    self.notify_update();
                }
            }
        }
        Ok(())
    }
}
