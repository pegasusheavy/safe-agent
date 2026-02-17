# safe-agent

Sandboxed autonomous AI agent with tool execution, knowledge graph, skill system, and multi-interface control.

safe-agent pairs a pluggable LLM backend (Claude Code CLI, OpenAI Codex CLI, Google Gemini CLI, Aider, OpenRouter, or a local GGUF model via llama-gguf) with a human-in-the-loop approval queue so an AI agent can observe, reason, and act -- but only with your permission. Control it from a Svelte web dashboard or a Telegram bot. The agent can teach itself new skills on the fly and grow its own knowledge graph over time.

## Features

- **Human-gated tool execution** -- the agent proposes actions; you approve or reject them from the dashboard or Telegram before anything runs.
- **Web dashboard** -- Svelte 5 UI with JWT authentication for monitoring status, managing approvals, chatting with the agent, browsing memory/knowledge, and configuring skills.
- **Telegram bot** -- full bidirectional control: send messages, approve/reject actions, force ticks, search memory.
- **Skill system** -- the agent can create, deploy, and manage its own long-running services (Python daemons, oneshot scripts) with credential injection and process group lifecycle management.
- **Skill extensions** -- skills can register custom API endpoints via Rhai scripts and provide custom HTML/JS/CSS panels in the dashboard, similar to Mattermost plugins.
- **Multi-provider OAuth** -- built-in OAuth 2.0 flows for Google, Microsoft, GitHub, Slack, Discord, Spotify, Dropbox, Twitter/X, LinkedIn, and Notion with multi-account support.
- **Knowledge graph** -- SQLite-backed graph of typed nodes and edges that the agent grows autonomously. Full-text search via FTS5 and recursive traversal via CTEs.
- **Memory hierarchy** -- core personality, rolling conversation window, archival long-term storage with full-text search.
- **Sandboxed file I/O** -- all file operations are confined to the data directory with path traversal prevention.
- **10 built-in tools** -- shell exec, file read/write/edit, web search, URL fetch, browser automation (CDP), messaging, cron scheduling, memory search, knowledge graph, image analysis.
- **Automatic HTTPS** -- built-in Let's Encrypt certificate provisioning via ACME TLS-ALPN-01. Set `ACME_ENABLED=true` with your domain and email; the container aborts if the certificate cannot be obtained.
- **Ngrok tunnel** -- optional public exposure via ngrok for OAuth callbacks and external integrations. Set `NGROK_AUTHTOKEN` and the tunnel starts automatically, broadcasting `TUNNEL_URL` / `PUBLIC_URL` to all skills.

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

# Run with OpenRouter (any model via API)
LLM_BACKEND=openrouter OPENROUTER_API_KEY=sk-or-... \
  source .env && ./target/release/safe-agent

# Run with OpenAI Codex
LLM_BACKEND=codex source .env && ./target/release/safe-agent

# Run with Google Gemini
LLM_BACKEND=gemini source .env && ./target/release/safe-agent

# Run with Aider (any provider)
LLM_BACKEND=aider AIDER_MODEL=gpt-4o source .env && ./target/release/safe-agent

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
| `LLM_BACKEND` | `claude` (default), `codex`, `gemini`, `aider`, `openrouter`, or `local` |
| `CLAUDE_BIN` | Path to the `claude` binary (default: `claude`) |
| `CLAUDE_CONFIG_DIR` | Claude Code config directory for profile selection |
| `CLAUDE_MODEL` | Model override: `sonnet`, `opus`, `haiku` |
| `CODEX_BIN` | Path to the `codex` binary (default: `codex`) |
| `CODEX_MODEL` | Model override: `gpt-5-codex`, `o3`, etc. |
| `CODEX_PROFILE` | Codex config profile from `~/.codex/config.toml` |
| `CODEX_API_KEY` | OpenAI API key (uses saved auth if unset) |
| `GEMINI_BIN` | Path to the `gemini` binary (default: `gemini`) |
| `GEMINI_MODEL` | Model override: `gemini-2.5-pro`, `gemini-2.5-flash` |
| `GEMINI_API_KEY` | Google AI Studio API key (uses saved auth if unset) |
| `AIDER_BIN` | Path to the `aider` binary (default: `aider`) |
| `AIDER_MODEL` | Model string: `gpt-4o`, `claude-3.5-sonnet`, etc. |
| `OPENROUTER_API_KEY` | OpenRouter API key |
| `OPENROUTER_MODEL` | OpenRouter model ID (default: `anthropic/claude-sonnet-4`) |
| `MODEL_PATH` | Path to a `.gguf` model file (required when `LLM_BACKEND=local`) |
| `TELEGRAM_BOT_TOKEN` | Telegram Bot API token (from @BotFather) |
| `ACME_ENABLED` | Set to `true` to enable automatic HTTPS via Let's Encrypt |
| `ACME_DOMAIN` | Domain(s) for the certificate (required when ACME enabled) |
| `ACME_EMAIL` | Contact email for Let's Encrypt (required when ACME enabled) |
| `ACME_PRODUCTION` | `true` for production CA, `false` for staging (default) |
| `NGROK_AUTHTOKEN` | ngrok auth token -- setting this auto-enables the tunnel |
| `NGROK_DOMAIN` | Static ngrok domain (e.g. `myapp.ngrok-free.app`) |
| `RUST_LOG` | Tracing filter (default: `info`) |

### OAuth Provider Variables

Each provider needs a client ID and secret. Set only the providers you want to use.

| Provider | Client ID | Client Secret |
|---|---|---|
| Google | `GOOGLE_CLIENT_ID` | `GOOGLE_CLIENT_SECRET` |
| Microsoft | `MICROSOFT_CLIENT_ID` | `MICROSOFT_CLIENT_SECRET` |
| GitHub | `GITHUB_CLIENT_ID` | `GITHUB_CLIENT_SECRET` |
| Slack | `SLACK_CLIENT_ID` | `SLACK_CLIENT_SECRET` |
| Discord | `DISCORD_CLIENT_ID` | `DISCORD_CLIENT_SECRET` |
| Spotify | `SPOTIFY_CLIENT_ID` | `SPOTIFY_CLIENT_SECRET` |
| Dropbox | `DROPBOX_CLIENT_ID` | `DROPBOX_CLIENT_SECRET` |
| Twitter/X | `TWITTER_CLIENT_ID` | `TWITTER_CLIENT_SECRET` |
| LinkedIn | `LINKEDIN_CLIENT_ID` | `LINKEDIN_CLIENT_SECRET` |
| Notion | `NOTION_CLIENT_ID` | `NOTION_CLIENT_SECRET` |

## Architecture

```
Telegram Bot ──┐
               ├──▶ Agent ──▶ LLM Engine ──▶ Tool execution
Web Dashboard ─┘       │       ├─ Claude CLI   (Anthropic)
(Svelte + JWT)         │       ├─ Codex CLI    (OpenAI)
                       │       ├─ Gemini CLI   (Google)
                       │       ├─ Aider        (multi-provider)
                       │       ├─ OpenRouter   (API, any model)
                       │       └─ llama-gguf   (local, optional)
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
                  ├─ Extension engine (Rhai)     ├─ cron
                  └─ Auto-reconciliation         ├─ memory_search / memory_get
                                                 ├─ knowledge_graph
                  OAuth Manager                  └─ image
                  ├─ 10 providers
                  ├─ Multi-account
                  └─ Token refresh
```

For detailed architecture documentation, module layout, and development guidelines, see [`AGENTS.md`](AGENTS.md).

## Tech Stack

- **Rust** (2024 edition) -- backend, tool execution, agent loop
- **LLM** -- Claude Code CLI, OpenAI Codex CLI, Google Gemini CLI, Aider (multi-provider), OpenRouter (API), or llama-gguf (local GGUF models, optional)
- **SQLite** -- conversation, memory, knowledge graph, approvals, OAuth tokens (WAL mode, FTS5)
- **Svelte 5 + Vite + Tailwind CSS 4** -- dashboard frontend (compiled and embedded in the binary)
- **axum** -- HTTP server and API
- **Rhai** -- embedded scripting engine for skill extensions
- **teloxide** -- Telegram bot framework
- **tokio** -- async runtime

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for the git workflow, commit message standards, code style guidelines, and pull request checklist.

## License

Copyright (c) 2026 Pegasus Heavy Industries LLC

Contact: [pegasusheavyindustries@gmail.com](mailto:pegasusheavyindustries@gmail.com)
