# safe-agent — Roadmap

> What it takes to go from "cool project" to "indispensable agentic companion."

---

## 0. Critical Foundation — Structured Tool Calling ✅

~~The single biggest gap.~~ **Implemented.** The LLM now proposes tool calls
via `` ```tool_call `` fenced blocks in its response. The agent parses them,
routes through auto-approve or the approval queue, executes, feeds results
back, and loops up to `max_tool_turns` (default 5).

- [x] **Implement structured tool-call parsing from LLM output**
      `src/agent/tool_parse.rs` parses JSON tool invocations from LLM
      responses. `src/llm/prompts.rs` injects tool schemas and calling
      protocol into the system prompt for all 6 backends.
- [x] **Multi-turn tool chaining** — `handle_message` loops: call LLM →
      parse → execute auto-approved tools → feed results back → repeat
      until the LLM returns a final text reply or `max_tool_turns` hit.
- [x] **Auto-approve safe tools** — `auto_approve_tools` from config is
      checked for every tool call. Matching tools execute immediately;
      others go to the approval queue.
- [x] **Streaming tool progress** — structured SSE events (`thinking`,
      `tool_start`, `tool_result`, `approval_needed`, `turn_complete`,
      `error`) pushed to the dashboard live feed and messaging platforms.
      Dashboard shows a real-time "Tool Activity" panel on Overview with
      animated indicators. Chat tab shows contextual thinking/executing
      state. Typing indicators sent to Telegram/WhatsApp during tool
      execution. REST `/api/tool-events` endpoint for page-reload hydration.

---

## 1. Autonomy & Planning ✅

- [x] **Background goals** — goals and tasks are stored in the DB (`goals`
      + `goal_tasks` tables). The agent works on one task per tick, picking
      the highest-priority active goal's next actionable task. Tasks can
      have tool calls or free-form LLM objectives.
- [x] **Task decomposition** — the `goal` tool lets the LLM create goals,
      break them into subtasks with dependency chains, and track progress.
      Tasks execute in dependency order.
- [x] **Cron-driven autonomy** — the cron scheduler is now wired into the
      tick loop. Enabled cron jobs are checked every tick; due jobs are
      executed via `ToolRegistry`. Cron tool enabled by default.
- [x] **Self-reflection loop** — when a goal completes or fails, the LLM
      generates a self-reflection (stored on the goal), evaluating what
      went well and what to improve.
- [x] **Proactive notifications** — background goal progress and cron job
      results are pushed to all messaging platforms (Telegram/WhatsApp)
      automatically. Goal completions include the reflection.
- [x] **Goal persistence** — goals and tasks stored in SQLite, survive
      restarts. Dashboard "Goals" tab shows all goals with status filters,
      task progress bars, self-reflections, and pause/resume/cancel controls.

---

## 2. Memory & Learning

- [ ] **Embedding-based retrieval (RAG)** — replace FTS5 with vector
      embeddings for semantic memory search. Use a local embedding model
      or API.
- [ ] **Automatic memory extraction** — after each conversation, extract
      key facts, preferences, and commitments into archival memory
      without being told.
- [ ] **Episodic memory** — store *what happened* (tool results, decisions,
      outcomes) so the agent can learn from past actions.
- [ ] **User model** — build a structured profile of the user's
      preferences, schedule patterns, communication style, and priorities.
- [ ] **Knowledge graph auto-population** — when the agent learns new
      facts, add nodes/edges to the knowledge graph automatically.
- [ ] **Memory decay & consolidation** — older memories should be
      summarized and consolidated to keep context windows manageable.
- [ ] **Cross-session context** — when the user refers to "that thing
      from yesterday," the agent should be able to find it.

---

## 3. Perception & Multimodal

- [ ] **Vision (image tool)** — wire the placeholder image tool to a
      vision model (Claude vision, GPT-4o, local). Accept images from
      Telegram/WhatsApp and the dashboard.
- [ ] **Document understanding** — ingest PDFs, Word docs, spreadsheets.
      Extract text, tables, and structure.
- [ ] **Voice input** — accept voice messages from Telegram/WhatsApp,
      transcribe with Whisper (local or API), pass text to the LLM.
- [ ] **Voice output** — TTS for responses. Let the user choose between
      text and voice replies per-platform.
- [ ] **Screen/clipboard awareness** — optional desktop companion mode
      where the agent can see what the user is looking at.

---

## 4. Browser Automation

The `chromiumoxide` dependency is already in Cargo.toml but the browser
tool is a scaffold.

- [ ] **CDP integration** — launch a headless browser, navigate pages,
      extract content, fill forms, click buttons.
- [ ] **Authenticated browsing** — use stored OAuth tokens to log into
      web apps and perform actions.
- [ ] **Screenshot & visual grounding** — take screenshots, pass to
      vision model, click on elements by description.
- [ ] **Web scraping toolkit** — structured data extraction from pages
      with CSS/XPath selectors.
- [ ] **Bookmark & read-later** — save interesting pages to the knowledge
      graph for later retrieval.

---

## 5. Communication & Messaging

- [ ] **Email sending** — use OAuth tokens to send email (Gmail API,
      Microsoft Graph). Currently read-only.
- [ ] **Email monitoring** — watch inbox for important messages, summarize,
      and notify. Draft replies for approval.
- [ ] **SMS/iMessage bridge** — for users who don't use Telegram.
- [ ] **Matrix/Signal support** — privacy-focused messaging alternatives.
- [ ] **Slack workspace bot** — not just OAuth tokens, but an actual
      Slack bot presence the agent can operate.
- [ ] **Discord bot** — same as Slack; presence in Discord servers.
- [ ] **Rich messaging** — send images, files, formatted cards, and
      inline buttons (Telegram supports all of these).
- [ ] **Group chat awareness** — understand multi-user conversations,
      only respond when addressed or relevant.

---

## 6. Integrations & Actions

- [ ] **Calendar write access** — create, update, delete events. Currently
      read-only. Propose changes through the approval queue.
- [ ] **Smart home** — Home Assistant / HomeKit integration. "Turn off
      the lights," "set thermostat to 72."
- [ ] **Music control** — Spotify playback control via existing OAuth.
- [ ] **Git operations** — clone repos, create branches, make commits,
      open PRs using the GitHub OAuth token.
- [ ] **Note-taking** — Notion/Obsidian integration for structured notes.
- [ ] **File sync** — Dropbox/Drive upload and download for sharing files.
- [ ] **Finance tracking** — Plaid or bank API integration for expense
      monitoring and budgeting.
- [ ] **Location awareness** — "What's the weather?" "Find a restaurant
      nearby." Requires opt-in location sharing.
- [ ] **Webhooks & IFTTT** — expose an incoming webhook so external
      services can trigger the agent.

---

## 7. Security & Trust

- [x] **Capability-based permissions** — `SecurityConfig` with `blocked_tools`,
      `tool_capabilities` (per-tool operation allowlists), and `CapabilityChecker`
      that infers operations from tool params (e.g. exec command name, file read/write).
      Enforced in `handle_message` before every tool execution.
- [x] **Audit trail dashboard** — `audit_log` DB table with structured events
      (tool_call, approval, rate_limit, pii_detected, 2fa, permission_denied).
      `AuditLogger` with convenience methods. Dashboard Security tab with filterable
      audit log, summary stats, and per-event "Why?" explainability drill-down.
      API: `/api/security/audit`, `/api/security/audit/summary`, `/api/security/audit/{id}/explain`.
- [x] **Cost tracking** — `llm_usage` DB table tracking backend, model, token counts,
      and estimated USD cost per request. `CostTracker` with model-aware pricing
      (Claude, GPT, Gemini, Llama, DeepSeek, etc.). Daily cost limit with alerts.
      Dashboard shows today/month/all-time costs, token counts, and budget progress bar.
      API: `/api/security/cost`, `/api/security/cost/recent`.
- [x] **Rate limiting** — sliding-window `RateLimiter` (in-memory) with configurable
      per-minute and per-hour caps (default: 30/min, 300/hr). Returns `RateLimited`
      error. Checked before every tool execution in `handle_message`. Dashboard
      shows live usage bars. API: `/api/security/rate-limit`.
- [x] **Sensitive data detection** — `PiiScanner` detects SSNs, credit cards,
      API keys (sk-*, AKIA*), private keys, JWT tokens, passwords, and AWS keys.
      Scans LLM responses before sending; prepends a warning if PII found.
      Events logged to audit trail. Configurable via `pii_detection = true/false`.
- [x] **Explainability** — every tool call, approval, and security event is
      logged with reasoning, params, user context, and source. The `explain_action`
      method retrieves the causal chain (up to 10 related events within 1 minute)
      for any audit entry. Dashboard "Why?" button on each audit row opens a
      step-by-step reasoning modal.
- [x] **Multi-user support** — `users` DB table with UUID IDs, roles
      (admin/user/viewer), platform identity mapping (Telegram ID, WhatsApp
      JID, email). `UserManager` with full CRUD, authentication, and lookup
      by platform ID. `UserContext` threaded through `handle_message_as()`
      for per-user permission enforcement (viewers blocked from chat, only
      admins can approve). Per-user conversation memory isolation via
      `user_id` column on `conversation_history` (also added to
      `activity_log`, `audit_log`, `goals`, `pending_actions`).
      Dashboard login supports username+password (multi-user) alongside
      legacy single-password mode. JWT claims extended with `user_id` and
      `role`. SSO callback links email to existing users. Telegram and
      WhatsApp handlers look up users by platform ID before routing to
      agent. Dashboard shows current user identity in header, Settings tab
      includes full user management panel (create, edit roles, enable/
      disable, link platform IDs, delete). API: `/api/users` (CRUD).
- [x] **Dashboard 2FA & passkeys** — TOTP (RFC 6238) two-factor
      authentication with authenticator app QR code setup, 10 single-use
      recovery codes, and enable/disable flow requiring code verification.
      WebAuthn/FIDO2 passkey support via `webauthn-rs` for biometric and
      security-key 2FA. Challenge-token login flow: password auth returns
      a short-lived JWT; user must verify via TOTP, recovery code, or
      passkey assertion to receive the full session. DB: `totp_secret`,
      `totp_enabled`, `recovery_codes` columns on `users`; `passkeys`
      table. API: `/api/auth/2fa/{setup,enable,disable,status,verify}`,
      `/api/auth/passkey/{register,authenticate}/{start,finish}`,
      `/api/auth/passkeys` (list/delete). Dashboard Settings tab adds a
      Two-Factor Authentication panel; login overlay shows 2FA challenge
      step with passkey button, TOTP input, and recovery code fallback.
- [x] **PII encryption at rest** — auto-generates a 256-bit AES key on
      first launch (`<data_dir>/encryption.key`, 0600 perms). All PII
      fields in the `users` table are encrypted with AES-256-GCM before
      storage: `display_name`, `email`, `password_hash`, `telegram_id`,
      `whatsapp_id`, `totp_secret`, `recovery_codes`. Encrypted values
      use `ENC$<base64(nonce‖ciphertext)>` format; legacy plaintext is
      auto-migrated on startup via `migrate_encrypt_pii()`. Lookup fields
      (`email`, `telegram_id`, `whatsapp_id`) use HMAC-SHA-256 blind
      indexes in separate `*_blind` columns so SQL equality queries work
      without exposing plaintext. Key derivation separates the HMAC blind
      key from the AES key. Module: `src/crypto.rs` (`FieldEncryptor`).
- [x] **2FA for dangerous ops** — `TwoFactorManager` with configurable
      `require_2fa` tool list (default: `["exec"]`). Creates time-limited
      challenges (5 min TTL) that must be confirmed via dashboard before
      execution proceeds. Dashboard 2FA tab shows pending challenges with
      Confirm/Reject buttons. API: `/api/security/2fa`, `/api/security/2fa/{id}/confirm`,
      `/api/security/2fa/{id}/reject`.

---

## 8. Dashboard & UX

- [ ] **Mobile-responsive design** — the dashboard should be usable on
      a phone.
- [ ] **Real-time activity feed** — live-updating log of what the agent
      is doing right now (SSE events exist, wire them to the UI).
- [ ] **Conversation history browser** — searchable, filterable view of
      all past conversations across all platforms.
- [ ] **Approval notifications** — push notification or Telegram ping
      when the agent is waiting for approval.
- [ ] **Skill marketplace UI** — browse, install, and configure community
      skills from the dashboard.
- [ ] **Visual knowledge graph** — interactive force-directed graph
      visualization of the knowledge graph.
- [ ] **Agent persona editor** — edit personality, tone, and behavior
      rules from the dashboard (currently config-file only).
- [ ] **Dark/light theme toggle** — currently dark-only.
- [ ] **OAuth account management UX** — show token health, last refresh,
      expiry countdown, and re-auth flow.
- [ ] **PWA support** — installable as a mobile app with push
      notifications.

---

## 9. Multi-Agent & Sessions

- [ ] **Session system activation** — the sessions table and tools exist
      but are disabled by default. Enable and test multi-agent workflows.
- [ ] **Agent-to-agent delegation** — spawn sub-agents for parallel
      research, code review, or data processing.
- [ ] **Specialist personas** — different personality/prompt profiles
      for different tasks (coding, writing, research, personal assistant).
- [ ] **Collaborative planning** — agents discuss and refine a plan
      before executing.

---

## 10. Skill Ecosystem

- [ ] **Skill versioning** — track versions in skill.toml; support
      rollback.
- [ ] **Skill dependency management** — skills can declare dependencies
      on other skills.
- [ ] **Community skill registry** — a central catalog of shareable
      skills with install-from-URL.
- [x] **TypeScript/Node.js skills** — Node.js skill entrypoints (`.js`,
      `.mjs`, `.cjs`) with automatic `npm install` / `pnpm install` when a
      `package.json` is present.
- [x] **Rhai embedded skills** — `.rhai` entrypoints run in-process via
      `tokio::task::spawn_blocking` with a sandboxed API surface (HTTP,
      file I/O, env vars, Telegram, JSON, logging). Zero-overhead for
      lightweight automation.
- [x] **Python virtual environments** — `venv` field in `skill.toml`
      (`"auto"` | `"always"` | `"never"`). When enabled, the skill manager
      creates a `.venv/`, upgrades pip, installs `requirements.txt`, and
      runs the skill with the venv Python. `PYTHONUNBUFFERED=1` set by default.
- [x] **Skill lifecycle API** — `POST /api/skills/{name}/stop`,
      `/api/skills/{name}/start`, `/api/skills/{name}/restart`. Manually
      stopped skills are tracked and excluded from auto-reconciliation
      until explicitly restarted.
- [ ] **Skill sandboxing** — per-skill filesystem and network isolation
      (currently all skills share the same sandbox).
- [ ] **Skill health monitoring** — dashboard widget showing uptime,
      restart count, memory usage, and error rate per skill.
- [ ] **Hot reload** — detect skill file changes and restart without
      a full agent restart.

---

## 11. Deployment & Operations

- [x] **ARM64 Docker builds** — CI builds `linux/amd64,linux/arm64` via
      `docker/build-push-action` with Buildx. Dockerfile handles arch-aware
      ngrok download (`uname -m` → `amd64`/`arm64`). Supports Raspberry Pi,
      Apple Silicon servers, and AWS Graviton.
- [x] **Auto-update mechanism** — `/api/update/check` queries GitHub Releases
      API and compares semver. `/api/update/apply` runs `git pull --ff-only`
      + `cargo build --release` (or signals container restart for Docker).
      Dashboard Ops tab shows current/latest version, release notes, and
      one-click update button.
- [x] **Backup & restore** — `/api/backup` exports all agent data
      (core_memory, archival_memory, activity_log, cron_jobs, goals,
      goal_tasks, agent_stats) as a timestamped JSON file. `/api/restore`
      accepts the JSON and merges via INSERT OR REPLACE. Dashboard Ops tab
      has download and upload buttons.
- [x] **Health check endpoint** — `/healthz` (unauthenticated) returns
      200 OK with component health (database, agent, tools count, version)
      or 503 if unhealthy. Suitable for load balancers, Docker HEALTHCHECK,
      and Kubernetes liveness probes.
- [x] **Prometheus metrics** — `/metrics` (unauthenticated) exposes
      counters and gauges in OpenMetrics text format: agent info, paused
      state, tool count, tick/approve/reject totals, audit events, LLM
      cost/tokens, rate limiter state. Ready for Grafana/Prometheus scraping.
- [x] **Multi-node federation** — `FederationManager` with peer registry,
      heartbeat protocol, memory delta replication, and distributed task
      claiming. `[federation]` config section with `enabled`, `peers`,
      `advertise_address`, intervals. API: `/api/federation/status`,
      `/api/federation/peers`, `/api/federation/sync` (peer-to-peer),
      `/api/federation/heartbeat`, `/api/federation/claim`. Dashboard Ops
      tab shows node info, peer list, add/remove peers.
- [x] **Plugin architecture for LLM backends** — `LlmBackend` async trait
      with `name()` and `generate()`. `LlmPluginRegistry` for dynamic
      registration. All built-in backends (Claude, Codex, Gemini, Aider,
      OpenRouter, Local) implement the trait and auto-register. `LlmEngine`
      wraps the active backend + registry; supports `register_plugin()` and
      `switch_backend()` at runtime. API: `/api/llm/backends`.

---

## 12. Polish & Quality of Life

- [x] **Onboarding wizard** — first-run setup flow in the dashboard
      (set name, connect messaging, configure OAuth, test LLM).
      Full-screen 4-step wizard appears on first launch: (1) Welcome /
      Agent Identity — set `agent_name` and `core_personality`, (2) LLM
      Backend — pick from available backends with a "Test Connection"
      button that sends a test prompt, (3) Messaging — shows Telegram &
      WhatsApp status (skippable), (4) Review / Finish — summary with
      "Complete Setup" button. State is tracked in a new `metadata`
      key-value table; subsequent launches skip the wizard. Config
      changes are written to the TOML config file on disk. All
      onboarding endpoints are exempt from auth middleware so the wizard
      works before any user account exists.
- [ ] **Natural language config** — "Scruffy, start checking my email
      every hour" should create a cron + skill automatically.
- [ ] **Undo for everything** — not just file deletions (trash), but
      message edits, config changes, skill modifications.
- [ ] **Contextual help** — "What can you do?" returns a dynamic list
      based on what's configured and connected.
- [ ] **Conversation branching** — fork a conversation to explore
      different approaches without losing the original thread.
- [ ] **Notification preferences** — per-topic and per-severity
      notification routing (urgent → Telegram, FYI → dashboard only).
- [x] **Timezone & locale awareness** — respect the user's timezone for
      all date/time operations and responses. System-level default timezone
      and locale configurable via `config.toml` (`timezone = "America/New_York"`,
      `locale = "en-US"`).  Per-user overrides stored in the `users` table.
      The LLM system prompt includes the current local date/time so the agent
      gives time-aware responses (greetings, scheduling, etc.).  Dashboard
      Settings has a Timezone & Locale panel with browser auto-detection and
      a searchable IANA timezone picker.  All user-facing timestamps in the
      dashboard (activity log, audit trail, chat, knowledge graph, goals,
      memory panels) are formatted in the user's local timezone via a shared
      `time.ts` utility that parses both RFC 3339 and SQLite datetime formats.
      Backend API: `GET/POST /api/timezone`, `GET /api/timezones` (full list),
      `GET /api/timezone/convert` (server-side conversion).  Crate: `chrono-tz`.
- [ ] **i18n** — multi-language support for the dashboard and agent
      responses.

---

## 13. Website & Documentation

- [x] **GitHub Pages landing site** — Svelte 5 + SvelteKit static site
      in `web/` with Tailwind CSS 4 and FontAwesome 6. Sections: hero
      with `docker pull` command, features grid, 3-step quick start guide,
      architecture overview, skill system showcase, and get-involved links.
      Dark theme (slate-950/emerald accent). Docker-first messaging
      throughout. Deployed via `actions/deploy-pages@v4` on push to
      `develop` or `main`.
- [x] **GHCR container images** — GitHub Actions workflow builds and
      pushes multi-arch Docker images (amd64 + arm64) to
      `ghcr.io/pegasusheavy/safe-agent` on version tags (`v*`).
- [x] **Comprehensive Docker README** — full Quick Start section covering
      directory setup, `.env` configuration, `docker run` command with
      mount explanations, Docker Compose example, optional mounts (Claude
      CLI, ngrok, ACME), build-from-source, UID/GID matching, and ARM64.
- [x] **AI co-authorship policy** — `CONTRIBUTING.md` requires attestation
      of the AI model used and `Co-authored-by` git trailer for all
      AI-assisted contributions.
- [ ] **API reference** — auto-generated docs for all REST endpoints.
- [ ] **Skill authoring guide** — tutorial for writing Python, Node.js,
      Rhai, and shell skills with `skill.toml` examples.

---

*This list is ordered roughly by impact. Item 0 (structured tool calling)
has been implemented and unlocks almost everything else.*
