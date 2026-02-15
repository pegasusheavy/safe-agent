# safe-agent

Sandboxed autonomous AI agent with tool execution, knowledge graph, and multi-interface control.

## Architecture

safe-agent is an OpenClaw-style autonomous agent system. A local LLM observes context, reasons about it, and proposes **tool calls** that flow through a human approval queue before execution. The operator controls the agent via a web dashboard or Telegram bot.

```
Telegram Bot ──┐
               ├──▶ Agent ──▶ LLM (local GGUF) ──▶ ToolCall proposals
Web Dashboard ─┘       │                                    │
                       │                             Approval Queue
                       │                                    │
                       ▼                             Tool Executor
                  Memory Manager                         │
                  ├─ Core                          Tool Registry
                  ├─ Conversation                  ├─ exec
                  ├─ Archival (FTS5)               ├─ read_file / write_file / edit_file
                  └─ Knowledge Graph               ├─ web_search / web_fetch
                                                   ├─ browser (CDP)
                                                   ├─ message
                                                   ├─ sessions_*
                                                   ├─ cron
                                                   ├─ memory_search / memory_get
                                                   ├─ knowledge_graph
                                                   ├─ google_calendar / google_drive / google_docs
                                                   └─ image
```

## Key Concepts

### Tool Trait

All tools implement a common trait:

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;
    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput>;
}
```

### ToolCall

The LLM proposes generic tool calls:

```rust
pub struct ToolCall {
    pub tool: String,        // "exec", "web_search", etc.
    pub params: serde_json::Value,
    pub reasoning: String,
}
```

These flow through the approval queue — the dashboard and Telegram bot show the tool name, params, and reasoning for the operator to approve/reject.

### Security Layers

- **SandboxedFs**: All file I/O confined to the data directory. Path traversal prevented.
- **Approval Queue**: All tool calls require human approval before execution.
- **exec tool**: Shell commands gated by approval; optional allowlist in config.
- **AllowlistedHttpClient**: Available for restricted HTTP access patterns.
- **Telegram auth**: Only configured chat IDs can control the bot.
- **Google OAuth**: Tokens stored in SQLite; client secrets from env vars only.

### Knowledge Graph

SQLite-based autonomous knowledge graph the agent grows as it learns. Supports:
- Typed nodes with labels, content, and confidence scores
- Typed edges with relations and weights
- Full-text search (FTS5) over nodes
- Recursive traversal via SQL CTEs
- Exposed to the LLM as the `knowledge_graph` tool

### Memory System

- **Core Memory**: Single-row personality that persists across restarts
- **Conversation Memory**: Rolling window of recent messages
- **Archival Memory**: Long-term storage with FTS5 search
- **Knowledge Graph**: Structured graph of facts and relationships

## Tech Stack

- **Language**: Rust (2021 edition)
- **LLM**: Local GGUF inference via `llama-gguf`
- **Database**: SQLite via `rusqlite` (WAL mode, FTS5, recursive CTEs)
- **Web**: `axum` + embedded HTML/CSS/JS dashboard
- **HTTP**: `reqwest` for outbound requests
- **Telegram**: `teloxide` for bot interface
- **Google OAuth**: `oauth2` crate + direct REST API calls
- **Browser**: `chromiumoxide` for CDP automation (scaffold)
- **Scheduling**: `tokio-cron-scheduler` for cron jobs
- **HTML to Markdown**: `htmd` for web_fetch

## Module Layout

```
src/
├── main.rs              # Entry point, CLI, tool registry setup
├── config.rs            # Configuration (TOML)
├── error.rs             # Error types
├── db.rs                # SQLite schema migrations
├── security.rs          # SandboxedFs, AllowlistedHttpClient
├── agent/
│   ├── mod.rs           # Agent struct, run loop
│   ├── tick.rs          # Tick cycle: observe → think → propose
│   ├── actions.rs       # ToolCall parsing and execution
│   └── reasoning.rs     # LLM context assembly
├── llm/
│   ├── mod.rs           # LlmEngine (load, generate, download)
│   └── prompts.rs       # System prompt, JSON schema, user message builder
├── memory/
│   ├── mod.rs           # MemoryManager, stats, activity log
│   ├── core.rs          # CoreMemory (personality)
│   ├── conversation.rs  # ConversationMemory (rolling window)
│   ├── archival.rs      # ArchivalMemory (FTS5)
│   └── knowledge.rs     # KnowledgeGraph (nodes, edges, traversal)
├── tools/
│   ├── mod.rs           # Tool trait, ToolRegistry, ToolCall, ToolOutput
│   ├── exec.rs          # Shell command execution
│   ├── process.rs       # Background process management
│   ├── file.rs          # Read, write, edit, apply_patch (sandboxed)
│   ├── web.rs           # DuckDuckGo search, URL fetch
│   ├── browser.rs       # Headless browser (CDP scaffold)
│   ├── message.rs       # Messaging platforms (scaffold)
│   ├── sessions.rs      # Multi-agent session coordination
│   ├── cron.rs          # Scheduled task management
│   ├── image.rs         # Image analysis (scaffold)
│   ├── memory.rs        # Archival memory search/get
│   ├── knowledge.rs     # Knowledge graph tool
│   └── google.rs        # Google Calendar, Drive, Docs tools
├── google/
│   ├── mod.rs           # Re-exports
│   ├── oauth.rs         # OAuth2 authorization flow
│   └── tokens.rs        # Token storage and refresh
├── telegram/
│   ├── mod.rs           # Bot setup and dispatcher
│   ├── commands.rs      # Command handlers
│   └── notifications.rs # Push notifications to operator
├── approval/
│   ├── mod.rs           # ApprovalQueue
│   └── types.rs         # PendingAction, ApprovalStatus
└── dashboard/
    ├── mod.rs           # HTTP server setup
    ├── routes.rs        # Route definitions
    ├── handlers.rs      # API endpoint handlers
    ├── sse.rs           # Server-sent events
    └── ui/
        ├── index.html   # Dashboard HTML
        ├── style.css    # Dashboard styles
        └── app.js       # Dashboard JavaScript
```

## Building & Running

```bash
# Build
cargo build --release

# Download the default model
./target/release/safe-agent --download-model

# Run with default config
./target/release/safe-agent

# Run with custom config
./target/release/safe-agent --config /path/to/config.toml

# Pre-flight check
./target/release/safe-agent --check
```

## Configuration

Config file: `~/.config/safe-agent/config.toml`

See `config.example.toml` for all options with defaults.

### Environment Variables

| Variable               | Required                  | Description                                |
|------------------------|---------------------------|--------------------------------------------|
| `TELEGRAM_BOT_TOKEN`   | Yes (if telegram enabled) | Telegram Bot API token from @BotFather     |
| `GOOGLE_CLIENT_ID`     | Yes (if google enabled)   | Google OAuth2 client ID                    |
| `GOOGLE_CLIENT_SECRET` | Yes (if google enabled)   | Google OAuth2 client secret                |
| `RUST_LOG`             | No                        | Tracing filter (default: info)             |

## Data Storage

All data is stored under `$XDG_DATA_HOME/safe-agent/` (typically `~/.local/share/safe-agent/`):
- `safe-agent.db` — SQLite database (conversation, memory, knowledge graph, approvals, stats)
- `models/` — Downloaded GGUF model files

## Development Guidelines

- All file operations go through `SandboxedFs` — no raw `std::fs` outside the sandbox
- New tools implement the `Tool` trait and register in `build_tool_registry()` in `main.rs`
- Tool calls always flow through the approval queue unless the agent auto-approves
- Keep tool implementations stateless where possible; shared state lives in `ToolContext`
- Secrets (API keys, tokens) come from environment variables, never config files
- The dashboard serves embedded static files (compiled into the binary)

## License

Copyright (c) 2026 Pegasus Heavy Industries LLC
Contact: pegasusheavyindustries@gmail.com
