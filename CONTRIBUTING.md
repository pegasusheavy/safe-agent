# Contributing to safe-agent

Thank you for your interest in contributing to safe-agent. This document covers everything you need to get started.

## Prerequisites

- **Rust** (2024 edition, stable toolchain)
- **Node.js** (managed via `nvm`) and **pnpm**
- **Docker** and **Docker Compose** (for deployment testing)
- A Claude Code CLI installation (for end-to-end testing with the LLM)

## Getting Started

```bash
# Clone the repository
git clone https://github.com/pegasusheavy/safe-agent.git
cd safe-agent

# Switch to the develop branch (all work starts here)
git checkout develop

# Install frontend dependencies
pnpm install

# Build the Svelte dashboard
pnpm run build:ui

# Build the Rust binary
cargo build

# Copy and configure environment
cp .env.example .env
# Edit .env with your DASHBOARD_PASSWORD, JWT_SECRET, TELEGRAM_BOT_TOKEN, etc.
```

## Git Workflow

This project uses **git-flow** branching. See `AGENTS.md` for the full specification.

### Quick Reference

| Branch | Purpose | Merges into |
|---|---|---|
| `main` | Production releases only | -- |
| `develop` | Integration branch | `main` (via release) |
| `feature/<name>` | New features and enhancements | `develop` |
| `release/<version>` | Release preparation | `main` + `develop` |
| `hotfix/<name>` | Critical production fixes | `main` + `develop` |

**Never commit directly to `main`.** All work goes through `develop`.

### Starting Work

```bash
git checkout develop
git pull origin develop
git checkout -b feature/my-feature
```

### Submitting Work

Push your feature branch and open a pull request targeting `develop`:

```bash
git push -u origin feature/my-feature
```

Then open a PR on GitHub from `feature/my-feature` into `develop`.

## Commit Messages

Write commit messages as if Linus Torvalds is reviewing them. Every commit should be a self-contained, reviewable unit of work.

### Format

```
<type>: <subject>

<body>
```

- **Subject line**: imperative mood, under 72 characters, no trailing period.
- **Body** (when needed): blank line after subject. Explain *why*, not *how*. Wrap at 72 characters.

### Type Tags

| Tag | Use for |
|---|---|
| `feat` | New features or capabilities |
| `fix` | Bug fixes |
| `refactor` | Code restructuring with no behavior change |
| `docs` | Documentation only |
| `test` | Adding or fixing tests |
| `build` | Build system, dependencies, Docker |
| `chore` | Maintenance tasks, CI, tooling |

### Examples

Good:

```
feat: add web chat interface to dashboard

The agent was only reachable via Telegram. Add POST /api/chat
endpoint that calls the same handle_message() path and a ChatTab
Svelte component so the operator can message the agent directly
from the dashboard.
```

```
fix: terminate orphaned skill processes on directory deletion

Skill reconciliation was only checking for new skill.toml files,
not for removed directories. Kill the entire process group
(SIGTERM then SIGKILL) when a tracked skill's directory disappears.
```

Bad:

```
update stuff
```

```
WIP
```

```
fix
```

### Rules

- Never use `--no-verify` to skip hooks. Fix the underlying issue instead.
- Do not squash unrelated changes into a single commit.
- Each commit should compile and pass linting on its own.

## Code Style

### Rust

- Run `cargo fmt` before committing.
- Run `cargo clippy` and fix all warnings. Use `#[allow(...)]` only with an explanation.
- Use `thiserror` for error types, `tracing` for logging (never `println!` or `eprintln!`).
- Prefer `Result<T>` over panics.
- Document public APIs with doc comments.
- All file I/O must go through `SandboxedFs` -- no raw `std::fs` outside the sandbox.
- Secrets come from environment variables, never config files.

### Frontend (Svelte / TypeScript)

- The dashboard is a Svelte 5 app in `src/dashboard/frontend/`.
- Use Svelte 5 runes (`$state`, `$effect`, `$derived`) -- not legacy stores.
- Use `untrack()` when writing to reactive state inside an `$effect` to avoid infinite loops.
- Style with Tailwind CSS 4 utility classes. Custom CSS goes in `app.css`.
- Type all API responses with interfaces in `lib/types.ts`.
- Use the `api()` helper from `lib/api.ts` for all HTTP requests (handles 401 redirect).

### Build

After changing frontend code:

```bash
pnpm run build:ui   # Compiles Svelte -> src/dashboard/ui/
cargo build         # Embeds compiled UI into the Rust binary
```

Both steps are required -- the Rust binary embeds the UI files at compile time via `include_str!`.

## Adding a New Tool

1. Create `src/tools/<name>.rs` implementing the `Tool` trait.
2. Register it in `build_tool_registry()` in `src/main.rs`.
3. Tool calls flow through the approval queue -- no direct execution.
4. Keep tool implementations stateless; shared state lives in `ToolContext`.

## Adding a New Dashboard Component

1. Create `src/dashboard/frontend/src/components/MyComponent.svelte`.
2. Import and render it from `App.svelte` (add a tab if it's a top-level view).
3. Add any new API response types to `lib/types.ts`.
4. If you need new backend endpoints, add handlers in `src/dashboard/handlers.rs` and register routes in `src/dashboard/routes.rs`.
5. Rebuild: `pnpm run build:ui && cargo build`.

## Testing

```bash
# Run all Rust tests
cargo test

# Run with a specific test
cargo test test_name

# Lint
cargo clippy
cargo fmt -- --check
```

There is no formal frontend test suite yet. Manual testing against the running dashboard is the current workflow.

## Pull Request Checklist

Before opening a PR, verify:

- [ ] `cargo fmt` produces no changes
- [ ] `cargo clippy` produces no warnings
- [ ] `cargo build` succeeds
- [ ] `pnpm run build:ui` succeeds (if frontend was changed)
- [ ] Commit messages follow the format described above
- [ ] PR targets `develop`, not `main`
- [ ] New tools/endpoints are documented in `AGENTS.md`

## Security

If you discover a security vulnerability, **do not** open a public issue. Email [pegasusheavyindustries@gmail.com](mailto:pegasusheavyindustries@gmail.com) directly.

## License

Copyright (c) 2026 Pegasus Heavy Industries LLC. By contributing, you agree that your contributions will be licensed under the same terms as the project.
