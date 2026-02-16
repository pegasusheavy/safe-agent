# safe-agent

Sandboxed autonomous AI agent with tool execution, knowledge graph, skill system, and multi-interface control.

## Architecture

safe-agent is an autonomous agent system. Claude Code CLI handles reasoning and tool use. The operator controls the agent via a Svelte web dashboard (with JWT authentication) or Telegram bot.

```
Telegram Bot ──┐
               ├──▶ Agent ──▶ Claude Code CLI ──▶ Tool execution
Web Dashboard ─┘       │                                │
(Svelte + JWT)         │                         Approval Queue
                       │                                │
                       ▼                         Tool Executor
                  Memory Manager                       │
                  ├─ Core                        Tool Registry
                  ├─ Conversation                ├─ exec
                  ├─ Archival (FTS5)             ├─ read_file / write_file / edit_file
                  └─ Knowledge Graph             ├─ web_search / web_fetch
                                                 ├─ browser (CDP)
                  Skill Manager                  ├─ message
                  ├─ Discovery (skill.toml)      ├─ sessions_*
                  ├─ Process groups              ├─ cron
                  ├─ Credential injection        ├─ memory_search / memory_get
                  └─ Auto-reconciliation         ├─ knowledge_graph
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
- **Dashboard JWT Auth**: `DASHBOARD_PASSWORD` and `JWT_SECRET` are **required** — the server will not start without them. Login issues HS256-signed HttpOnly cookies with 7-day expiry.
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

### Skill System

Skills are self-contained programs (typically Python) that the agent can create and manage on the fly. Each skill lives in its own directory under `$DATA_DIR/skills/<name>/` and includes a `skill.toml` manifest.

**Lifecycle:**
- Skills are discovered by scanning the skills directory for `skill.toml` files
- Daemon skills are started automatically and restarted if they crash
- Oneshot skills run once and exit
- Each skill runs in its own **Unix process group** for clean shutdown
- Reconciliation runs on every agent tick and after every message — deleted skill directories are detected immediately and their processes killed (SIGTERM → 2s grace → SIGKILL on the entire process group)

**Manifest (`skill.toml`):**
```toml
name = "my-skill"
description = "What this skill does"
skill_type = "daemon"   # or "oneshot"
enabled = true
entrypoint = "main.py"

[[credentials]]
name = "API_KEY"
label = "API Key"
description = "Third-party API key"
required = true
```

**Credentials:**
- Declared in `skill.toml` under `[[credentials]]`
- Configured via the dashboard UI or REST API
- Stored in `$SKILLS_DIR/credentials.json`
- Injected as environment variables when the skill process starts

**Environment variables passed to skills:**
- `SKILL_NAME`, `SKILL_DIR`, `SKILL_DATA_DIR`, `SKILLS_DIR`
- `TELEGRAM_BOT_TOKEN`, `TELEGRAM_CHAT_ID` (if configured)
- Any extra `[env]` vars from the manifest
- All stored credentials for that skill

## Tech Stack

- **Language**: Rust (2024 edition)
- **LLM**: Claude Code CLI (configurable profile and model via env vars)
- **Database**: SQLite via `rusqlite` (WAL mode, FTS5, recursive CTEs)
- **Web**: `axum` + Svelte 5 dashboard (compiled by Vite, embedded in binary)
- **Auth**: `jsonwebtoken` (HS256 JWT cookies)
- **HTTP**: `reqwest` for outbound requests
- **Telegram**: `teloxide` for bot interface
- **Google OAuth**: `oauth2` crate + direct REST API calls
- **Browser**: `chromiumoxide` for CDP automation (scaffold)
- **Scheduling**: `tokio-cron-scheduler` for cron jobs
- **HTML to Markdown**: `htmd` for web_fetch
- **Process management**: `libc` for Unix process group signals

## Module Layout

```
src/
├── main.rs              # Entry point, CLI, tool registry setup
├── config.rs            # Configuration (TOML)
├── error.rs             # Error types
├── db.rs                # SQLite schema migrations
├── security.rs          # SandboxedFs, AllowlistedHttpClient
├── agent/
│   ├── mod.rs           # Agent struct, run loop, skill reconciliation
│   ├── tick.rs          # Tick cycle: observe → think → propose
│   ├── actions.rs       # ToolCall parsing and execution
│   └── reasoning.rs     # LLM context assembly
├── llm/
│   ├── mod.rs           # LlmEngine (Claude Code CLI wrapper)
│   └── prompts.rs       # System prompt, JSON schema, user message builder
├── memory/
│   ├── mod.rs           # MemoryManager, stats, activity log
│   ├── core.rs          # CoreMemory (personality)
│   ├── conversation.rs  # ConversationMemory (rolling window)
│   ├── archival.rs      # ArchivalMemory (FTS5)
│   └── knowledge.rs     # KnowledgeGraph (nodes, edges, traversal)
├── skills/
│   ├── mod.rs           # Re-exports
│   └── manager.rs       # SkillManager: discovery, start, stop, credentials, reconciliation
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
    ├── routes.rs        # Route definitions, DashState, static file serving
    ├── auth.rs          # JWT middleware, login/logout/check endpoints
    ├── handlers.rs      # API endpoint handlers
    ├── sse.rs           # Server-sent events
    ├── ui/              # Build output (embedded in binary via include_str!)
    │   ├── index.html   # Dashboard HTML (Tailwind CSS 4 + Font Awesome CDN)
    │   ├── style.css    # Compiled CSS (from Vite)
    │   └── app.js       # Compiled JS (from Vite)
    └── frontend/        # Svelte 5 source (compiled by Vite → ui/)
        ├── index.html
        ├── vite.config.ts
        ├── svelte.config.js
        ├── vite-env.d.ts
        └── src/
            ├── main.ts          # Svelte mount point
            ├── app.css          # Custom CSS (scrollbars, badges, tabs)
            ├── App.svelte       # Root component (auth gate, tabs, layout)
            ├── lib/
            │   ├── types.ts     # TypeScript interfaces for API data
            │   ├── api.ts       # HTTP helper with 401 → auth redirect
            │   └── state.svelte.ts  # Shared reactive state ($state runes)
            └── components/
                ├── LoginOverlay.svelte   # Full-screen login form
                ├── Header.svelte         # Status bar, controls, logout
                ├── PendingActions.svelte  # Approval queue panel
                ├── ActivityLog.svelte     # Recent activity feed
                ├── MemoryPanel.svelte     # Core/conversation/archival tabs
                ├── StatsPanel.svelte      # Agent statistics
                ├── SkillsTab.svelte       # Skill list and management
                ├── SkillCard.svelte       # Individual skill card
                ├── CredentialRow.svelte   # Credential input row
                ├── KnowledgeTab.svelte    # Knowledge graph explorer
                └── ToolsTab.svelte        # Registered tools list
```

## Building & Running

```bash
# Install frontend dependencies
pnpm install

# Build the Svelte dashboard (outputs to src/dashboard/ui/)
pnpm run build:ui

# Build the Rust binary (embeds the compiled UI)
cargo build --release

# Run (requires DASHBOARD_PASSWORD and JWT_SECRET)
DASHBOARD_PASSWORD=mypass JWT_SECRET=mysecret ./target/release/safe-agent

# Run with custom config
./target/release/safe-agent --config /path/to/config.toml

# Pre-flight check
./target/release/safe-agent --check
```

### Docker

```bash
# Copy and fill in environment variables
cp .env.example .env

# Build and run
docker compose up -d --build

# View logs
docker compose logs -f safe-agent
```

## Configuration

Config file: `~/.config/safe-agent/config.toml`

See `config.example.toml` for all options with defaults.

### Environment Variables

| Variable               | Required | Description                                           |
|------------------------|----------|-------------------------------------------------------|
| `DASHBOARD_PASSWORD`   | **Yes**  | Password for the web dashboard (server won't start without it) |
| `JWT_SECRET`           | **Yes**  | Secret key for signing JWT cookies (server won't start without it) |
| `TELEGRAM_BOT_TOKEN`   | If telegram enabled | Telegram Bot API token from @BotFather     |
| `GOOGLE_CLIENT_ID`     | If google enabled   | Google OAuth2 client ID                    |
| `GOOGLE_CLIENT_SECRET` | If google enabled   | Google OAuth2 client secret                |
| `CLAUDE_BIN`           | No       | Path to the `claude` binary (default: `claude`)       |
| `CLAUDE_CONFIG_DIR`    | No       | Claude Code config directory for profile selection    |
| `CLAUDE_MODEL`         | No       | Model override: `sonnet`, `opus`, `haiku`             |
| `RUST_LOG`             | No       | Tracing filter (default: `info`)                      |

## Data Storage

All data is stored under `$XDG_DATA_HOME/safe-agent/` (typically `~/.local/share/safe-agent/`):
- `safe-agent.db` — SQLite database (conversation, memory, knowledge graph, approvals, stats)
- `skills/` — Skill directories (each with `skill.toml`, entrypoint, `skill.log`, `data/`)
- `skills/credentials.json` — Stored skill credentials

## Git Workflow

This project uses **git-flow** branching.

### Branches

- `main` — production-only. Never commit directly to `main`. It receives merges from `develop` when cutting a release.
- `develop` — integration branch. All day-to-day work lands here.
- `feature/<name>` — branched from `develop`, merged back into `develop` via PR.
- `release/<version>` — branched from `develop` when preparing a release. Final fixes go here, then it merges into both `main` (tagged) and `develop`.
- `hotfix/<name>` — branched from `main` for critical production fixes. Merges into both `main` (tagged) and `develop`.

### Commit Messages

Write commit messages as if Linus Torvalds is reviewing them.

- **Subject line**: imperative mood, under 72 characters, no trailing period. Describe *what* the commit does, not what you did. Prefix with a type tag: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `build:`, `chore:`.
- **Body** (when needed): separated by a blank line. Explain *why* the change was made, not *how* — the diff shows how. Wrap at 72 characters. Reference issues or prior commits when relevant.
- Do not write meaningless messages like "fix stuff", "update", or "WIP". Every commit in the history should be a self-contained, reviewable unit of work.
- Never use `--no-verify` to skip pre-commit or pre-push hooks. Fix the underlying issue instead.

```
feat: add web chat interface to dashboard

The agent was only reachable via Telegram. Add POST /api/chat
endpoint that calls the same handle_message() path and a ChatTab
Svelte component so the operator can message the agent directly
from the dashboard.
```

### Release Flow

1. Branch `release/vX.Y.Z` from `develop`.
2. Bump version numbers, update changelogs, fix release-blocking issues on the release branch.
3. Merge the release branch into `main` with `--no-ff`. Tag the merge commit `vX.Y.Z`.
4. Merge the release branch back into `develop` to pick up any last-minute fixes.
5. Delete the release branch.

```bash
git checkout develop
git checkout -b release/v0.2.0
# ... version bumps, final fixes ...
git checkout main
git merge --no-ff release/v0.2.0
git tag v0.2.0
git push origin main --tags
git checkout develop
git merge --no-ff release/v0.2.0
git push origin develop
git branch -d release/v0.2.0
```

## Development Guidelines

- All file operations go through `SandboxedFs` — no raw `std::fs` outside the sandbox
- New tools implement the `Tool` trait and register in `build_tool_registry()` in `main.rs`
- Tool calls always flow through the approval queue unless the agent auto-approves
- Keep tool implementations stateless where possible; shared state lives in `ToolContext`
- Secrets (API keys, tokens) come from environment variables, never config files
- The dashboard serves embedded static files (compiled into the binary via `include_str!`)
- Frontend changes require rebuilding: `pnpm run build:ui` then `cargo build`
- Skills run in their own Unix process groups — `stop_skill()` sends SIGTERM/SIGKILL to the group
- Skill reconciliation runs every tick and after every message — no conditional keyword matching
- `DASHBOARD_PASSWORD` and `JWT_SECRET` are mandatory — the server errors out on startup if either is missing

### Dashboard Development

The dashboard frontend is a Svelte 5 app in `src/dashboard/frontend/`:

```bash
# Dev server (hot reload, proxied to Rust backend)
pnpm run dev:ui

# Production build (outputs to src/dashboard/ui/)
pnpm run build:ui
```

Styling uses Tailwind CSS 4 (via browser CDN) with the Tailswatch Oxide theme and Font Awesome icons (also via CDN). The `index.html` in `ui/` includes the CDN `<script>` and `<link>` tags; Vite only bundles the Svelte JS and custom CSS.

### Adding a New Skill

Skills are created by the agent itself (via Claude) in response to user requests, but can also be created manually:

1. Create `$DATA_DIR/skills/<name>/` directory
2. Add `skill.toml` manifest (see Skill System section above)
3. Add entrypoint script (e.g., `main.py`)
4. Optionally add `requirements.txt` for Python dependencies
5. The skill will be discovered and started on the next reconciliation cycle

### Dashboard Authentication

Authentication is **mandatory** — the server refuses to start without `DASHBOARD_PASSWORD` and `JWT_SECRET`.

- `POST /api/auth/login` — Validates password, returns HS256-signed JWT in an `HttpOnly; SameSite=Strict` cookie (7-day expiry)
- `POST /api/auth/logout` — Clears the cookie
- `GET /api/auth/check` — Reports whether the current request has a valid JWT
- Middleware enforces auth on all API routes; static assets (`/`, `/style.css`, `/app.js`) and auth endpoints are exempt
- The Svelte app checks auth on mount and shows a login overlay when unauthenticated
- Any 401 response from `api.ts` resets auth state and shows the login screen

## License

Copyright (c) 2026 Pegasus Heavy Industries LLC
Contact: pegasusheavyindustries@gmail.com
