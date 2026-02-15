use serde::{Deserialize, Serialize};

/// The structured JSON output the LLM should produce on each tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentReasoning {
    pub internal_monologue: String,
    #[serde(default)]
    pub proposed_actions: Vec<serde_json::Value>,
    #[serde(default)]
    pub memory_updates: Vec<MemoryUpdate>,
    #[serde(default)]
    pub knowledge_updates: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryUpdate {
    pub category: String,
    pub content: String,
}

pub const JSON_SCHEMA: &str = r#"{
  "type": "object",
  "required": ["internal_monologue", "proposed_actions", "memory_updates"],
  "properties": {
    "internal_monologue": { "type": "string" },
    "proposed_actions": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["tool", "reasoning"],
        "properties": {
          "tool": { "type": "string", "description": "Name of the tool to invoke" },
          "params": { "type": "object", "description": "Tool-specific parameters" },
          "reasoning": { "type": "string", "description": "Why this action should be taken" }
        }
      }
    },
    "memory_updates": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["category", "content"],
        "properties": {
          "category": { "type": "string" },
          "content": { "type": "string" }
        }
      }
    },
    "knowledge_updates": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["action"],
        "properties": {
          "action": { "type": "string", "enum": ["add_node", "add_edge", "update_node", "remove_node"] },
          "label": { "type": "string" },
          "node_type": { "type": "string" },
          "content": { "type": "string" },
          "confidence": { "type": "number" },
          "source": { "type": "string" },
          "target": { "type": "string" },
          "relation": { "type": "string" },
          "node_id": { "type": "integer" }
        }
      }
    }
  }
}"#;

pub fn system_prompt(personality: &str, agent_name: &str) -> String {
    let personality_line = if personality.is_empty() {
        String::new()
    } else {
        format!("\nPersonality: {personality}\n")
    };

    format!(
        r#"You are {agent_name}, an autonomous AI agent.{personality_line}
Respond with JSON only. Schema:
{JSON_SCHEMA}

Rules: propose 0-3 actions per tick. All tool calls need operator approval. Use "internal_monologue" for reasoning."#
    )
}

pub fn build_user_message(
    context_summary: &str,
    conversation_context: &str,
    archival_context: &str,
    stats: &str,
    tool_schema: &str,
) -> String {
    let mut msg = String::new();

    if !context_summary.is_empty() {
        msg.push_str("Context: ");
        msg.push_str(context_summary);
        msg.push('\n');
    }

    if !conversation_context.is_empty() {
        msg.push_str("Conversation:\n");
        msg.push_str(conversation_context);
        msg.push('\n');
    }

    if !archival_context.is_empty() {
        msg.push_str("Memories:\n");
        msg.push_str(archival_context);
        msg.push('\n');
    }

    if !stats.is_empty() {
        msg.push_str("Stats: ");
        msg.push_str(stats);
        msg.push('\n');
    }

    if !tool_schema.is_empty() {
        msg.push_str("Tools:\n");
        msg.push_str(tool_schema);
    }

    msg.push_str("\nRespond with JSON. Propose tool calls if useful, otherwise empty proposed_actions.\n");
    msg
}
