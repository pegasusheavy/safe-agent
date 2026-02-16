# safe-agent

Sandboxed autonomous AI agent with tool execution, knowledge graph, skill system, and multi-interface control.

safe-agent pairs a pluggable LLM backend (Claude Code CLI, OpenAI Codex CLI, or a local GGUF model via llama-gguf) with a human-in-the-loop approval queue so an AI agent can observe, reason, and act -- but only with your permission. Control it from a Svelte web dashboard or a Telegram bot. The agent can teach itself new skills on the fly and grow its own knowledge graph over time.

## Features

- **Human-gated tool execution** -- the agent proposes actions; you approve or reject them from the dashboard or Telegram before anything runs.
- **Web dashboard** -- Svelte 5 UI with JWT authentication for monitoring status, managing approvals, chatting with the agent, browsing memory/knowledge, and configuring skills.
- **Telegram bot** -- full bidirectional control: send messages, approve/reject actions, force ticks, search memory.
- **Skill system** -- the agent can create, deploy, and manage its own long-running services (Python daemons, oneshot scripts) with credential injection and process group lifecycle management.
- **Knowledge graph** -- SQLite-backed graph of typed nodes and edges that the agent grows autonomously. Full-text search via FTS5 and recursive traversal via CTEs.
- **Memory hierarchy** -- core personality, rolling conversation window, archival long-term storage with full-text search.
- **Sandboxed file I/O** -- all file operations are confined to the data directory with path traversal prevention.
- **13 built-in tools** -- shell exec, file read/write/edit, web search, URL fetch, browser automation (CDP), messaging, cron scheduling, memory search, knowledge graph, Google Calendar/Drive/Docs, image analysis.

## Quick Start

### Docker (recommended)

```bash
git clone https://github.com/pegasusheavy/safe-agent.git
cd safe-agent
cp .env.example .env
# Edit .env -- at minimum set DASHBOARD_PASSWORD, JWT_SECRET, and TELEGRAM_BOT_TOKEN
docker compose up -d --build
```

The dashboard will be available at `http://localhost:3031`.

### From Source

Requires Rust (stable), Node.js, and pnpm.

```bash
git clone https://github.com/pegasusheavy/safe-agent.git
cd safe-agent

# Frontend
pnpm install
pnpm run build:ui

# Backend (Claude-only, default)
cargo build --release

# Backend (with local GGUF model support)
cargo build --release --features local

# Run with Claude (default)
cp .env.example .env
# Edit .env with your values
source .env && ./target/release/safe-agent

# Run with OpenAI Codex
LLM_BACKEND=codex source .env && ./target/release/safe-agent

# Run with a local model
LLM_BACKEND=local MODEL_PATH=/path/to/model.gguf \
  source .env && ./target/release/safe-agent
```

## Configuration

Runtime configuration lives in a TOML file (default: `~/.config/safe-agent/config.toml`). See [`config.example.toml`](config.example.toml) for all options.

Secrets are loaded from environment variables, never config files. See [`.env.example`](.env.example) for the full list.

### Required Environment Variables

| Variable | Description |
|---|---|
| `DASHBOARD_PASSWORD` | Password for the web dashboard. The server will not start without it. |
| `JWT_SECRET` | Secret key for signing session cookies. The server will not start without it. |

### Optional Environment Variables

| Variable | Description |
|---|---|
| `LLM_BACKEND` | `claude` (default), `codex`, or `local` (local requires `--features local`) |
| `CLAUDE_BIN` | Path to the `claude` binary (default: `claude`) |
| `CLAUDE_CONFIG_DIR` | Claude Code config directory for profile selection |
| `CLAUDE_MODEL` | Model override: `sonnet`, `opus`, `haiku` |
| `CODEX_BIN` | Path to the `codex` binary (default: `codex`) |
| `CODEX_MODEL` | Model override: `gpt-5-codex`, `o3`, etc. |
| `CODEX_PROFILE` | Codex config profile from `~/.codex/config.toml` |
| `CODEX_API_KEY` | OpenAI API key (uses saved auth if unset) |
| `MODEL_PATH` | Path to a `.gguf` model file (required when `LLM_BACKEND=local`) |
| `TELEGRAM_BOT_TOKEN` | Telegram Bot API token (from @BotFather) |
| `GOOGLE_CLIENT_ID` | Google OAuth2 client ID |
| `GOOGLE_CLIENT_SECRET` | Google OAuth2 client secret |
| `RUST_LOG` | Tracing filter (default: `info`) |

## Architecture

```
Telegram Bot ──┐
               ├──▶ Agent ──▶ LLM Engine ──▶ Tool execution
Web Dashboard ─┘       │       ├─ Claude CLI (Anthropic)
(Svelte + JWT)         │       ├─ Codex CLI  (OpenAI)
                       │       └─ llama-gguf (local, optional)
                       │                                │
                       ▼                         Approval Queue
                  Memory Manager                       │
                  ├─ Core                        Tool Executor
                  ├─ Conversation                      │
                  ├─ Archival (FTS5)             Tool Registry
                  └─ Knowledge Graph             ├─ exec
                                                 ├─ read_file / write_file / edit_file
                  Skill Manager                  ├─ web_search / web_fetch
                  ├─ Discovery (skill.toml)      ├─ browser (CDP)
                  ├─ Process groups              ├─ message
                  ├─ Credential injection        ├─ sessions_*
                  └─ Auto-reconciliation         ├─ cron
                                                 ├─ memory_search / memory_get
                                                 ├─ knowledge_graph
                                                 ├─ google_calendar / google_drive / google_docs
                                                 └─ image
```

For detailed architecture documentation, module layout, and development guidelines, see [`AGENTS.md`](AGENTS.md).

## Tech Stack

- **Rust** (2024 edition) -- backend, tool execution, agent loop
- **LLM** -- Claude Code CLI, OpenAI Codex CLI, or llama-gguf (local GGUF models, optional)
- **SQLite** -- conversation, memory, knowledge graph, approvals (WAL mode, FTS5)
- **Svelte 5 + Vite + Tailwind CSS 4** -- dashboard frontend (compiled and embedded in the binary)
- **axum** -- HTTP server and API
- **teloxide** -- Telegram bot framework
- **tokio** -- async runtime

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for the git workflow, commit message standards, code style guidelines, and pull request checklist.

## License

Copyright (c) 2026 Pegasus Heavy Industries LLC

Contact: [pegasusheavyindustries@gmail.com](mailto:pegasusheavyindustries@gmail.com)
