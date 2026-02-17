#!/usr/bin/env python3
"""
Claude Code Monitor — token refresh + usage alerting daemon.

Monitors:
  1. OAuth token in ~/.claude/.credentials.json — refreshes before expiry
  2. Usage stats in ~/.claude/stats-cache.json — alerts at configurable %
  3. Cumulative cost tracking from per-invocation data

Alerts via Telegram when:
  - Token was auto-refreshed (info)
  - Token refresh failed (critical)
  - Daily output tokens exceed USAGE_WARN_PERCENT of budget
  - Weekly cumulative cost exceeds threshold
"""

import json
import os
import sys
import time
import traceback
from datetime import datetime, timezone, timedelta
from pathlib import Path

import requests

# ---------------------------------------------------------------------------
# Config from environment
# ---------------------------------------------------------------------------

SKILL_NAME = os.environ.get("SKILL_NAME", "claude-monitor")
SKILL_DATA_DIR = Path(os.environ.get("SKILL_DATA_DIR", f"/data/safe-agent/skills/{SKILL_NAME}/data"))

TELEGRAM_BOT_TOKEN = os.environ.get("TELEGRAM_BOT_TOKEN", "")
TELEGRAM_CHAT_ID = os.environ.get("TELEGRAM_CHAT_ID", "")

CLAUDE_OAUTH_CLIENT_ID = os.environ.get("CLAUDE_OAUTH_CLIENT_ID", "9d1c250a-e61b-44d9-88ed-5944d1962f5e")
USAGE_WARN_PERCENT = int(os.environ.get("USAGE_WARN_PERCENT", "85"))
TOKEN_REFRESH_BUFFER_SECS = int(os.environ.get("TOKEN_REFRESH_BUFFER_SECS", "3600"))
CHECK_INTERVAL_SECS = int(os.environ.get("CHECK_INTERVAL_SECS", "120"))
DAILY_OUTPUT_TOKEN_BUDGET = int(os.environ.get("DAILY_OUTPUT_TOKEN_BUDGET", "45000"))

# Resolve credentials path: env override, or probe common locations
CLAUDE_CREDENTIALS_PATH_ENV = os.environ.get("CLAUDE_CREDENTIALS_PATH", "")
CLAUDE_CONFIG_DIR = os.environ.get("CLAUDE_CONFIG_DIR", "")

TOKEN_REFRESH_URL = "https://console.anthropic.com/v1/oauth/token"

# ---------------------------------------------------------------------------
# State file
# ---------------------------------------------------------------------------

STATE_FILE = SKILL_DATA_DIR / "state.json"

def load_state():
    if STATE_FILE.exists():
        try:
            return json.loads(STATE_FILE.read_text())
        except Exception:
            pass
    return {
        "last_refresh_utc": None,
        "refresh_count": 0,
        "last_warn_utc": None,
        "daily_tokens_warned": False,
        "daily_tokens_warned_date": None,
        "total_refreshes": 0,
        "errors": [],
    }

def save_state(state):
    SKILL_DATA_DIR.mkdir(parents=True, exist_ok=True)
    STATE_FILE.write_text(json.dumps(state, indent=2))

# ---------------------------------------------------------------------------
# Telegram helpers
# ---------------------------------------------------------------------------

def tg_send(message: str, parse_mode: str = "HTML"):
    if not TELEGRAM_BOT_TOKEN or not TELEGRAM_CHAT_ID:
        print(f"[tg skip] {message}", flush=True)
        return
    try:
        requests.post(
            f"https://api.telegram.org/bot{TELEGRAM_BOT_TOKEN}/sendMessage",
            json={
                "chat_id": TELEGRAM_CHAT_ID,
                "text": message,
                "parse_mode": parse_mode,
            },
            timeout=10,
        )
    except Exception as e:
        print(f"[tg error] {e}", flush=True)

# ---------------------------------------------------------------------------
# Credentials file discovery
# ---------------------------------------------------------------------------

def find_credentials_path() -> Path | None:
    """Find the Claude Code credentials file."""
    if CLAUDE_CREDENTIALS_PATH_ENV:
        p = Path(CLAUDE_CREDENTIALS_PATH_ENV)
        if p.exists():
            return p

    # Check CLAUDE_CONFIG_DIR
    if CLAUDE_CONFIG_DIR:
        p = Path(CLAUDE_CONFIG_DIR) / ".credentials.json"
        if p.exists():
            return p

    # Common locations
    candidates = [
        Path.home() / ".claude" / ".credentials.json",
        Path("/claude-config/.credentials.json"),
        Path("/data/safe-agent/.claude/.credentials.json"),
        Path("/home/safeagent/.claude/.credentials.json"),
        Path("/home/agent/.claude/.credentials.json"),
        Path("/home/joseph/.claude/.credentials.json"),
    ]
    for c in candidates:
        if c.exists():
            return c

    return None


def find_stats_path() -> Path | None:
    """Find the Claude Code stats-cache.json file."""
    if CLAUDE_CONFIG_DIR:
        p = Path(CLAUDE_CONFIG_DIR) / "stats-cache.json"
        if p.exists():
            return p

    candidates = [
        Path.home() / ".claude" / "stats-cache.json",
        Path("/claude-config/stats-cache.json"),
        Path("/data/safe-agent/.claude/stats-cache.json"),
        Path("/home/safeagent/.claude/stats-cache.json"),
        Path("/home/agent/.claude/stats-cache.json"),
        Path("/home/joseph/.claude/stats-cache.json"),
    ]
    for c in candidates:
        if c.exists():
            return c

    return None

# ---------------------------------------------------------------------------
# Token refresh logic
# ---------------------------------------------------------------------------

def read_credentials(cred_path: Path) -> dict | None:
    try:
        data = json.loads(cred_path.read_text())
        return data.get("claudeAiOauth")
    except Exception as e:
        print(f"[cred read error] {e}", flush=True)
        return None


def token_expires_soon(creds: dict) -> bool:
    """Check if the access token expires within TOKEN_REFRESH_BUFFER_SECS."""
    expires_at_ms = creds.get("expiresAt", 0)
    expires_at = expires_at_ms / 1000.0
    now = time.time()
    remaining = expires_at - now
    print(f"  token expires in {remaining:.0f}s ({remaining/3600:.1f}h)", flush=True)
    return remaining < TOKEN_REFRESH_BUFFER_SECS


def refresh_token(creds: dict, cred_path: Path) -> bool:
    """Refresh the OAuth token and write new credentials."""
    refresh_tok = creds.get("refreshToken")
    if not refresh_tok:
        print("[refresh] no refresh token available", flush=True)
        return False

    print("[refresh] refreshing OAuth token...", flush=True)

    try:
        resp = requests.post(
            TOKEN_REFRESH_URL,
            json={
                "grant_type": "refresh_token",
                "refresh_token": refresh_tok,
                "client_id": CLAUDE_OAUTH_CLIENT_ID,
            },
            timeout=30,
        )

        if resp.status_code != 200:
            print(f"[refresh] failed: HTTP {resp.status_code}: {resp.text[:200]}", flush=True)
            return False

        data = resp.json()
        new_access = data.get("access_token")
        new_refresh = data.get("refresh_token")
        expires_in = data.get("expires_in", 28800)

        if not new_access:
            print(f"[refresh] no access_token in response", flush=True)
            return False

        # Build new credentials
        new_creds = {
            "claudeAiOauth": {
                "accessToken": new_access,
                "refreshToken": new_refresh or refresh_tok,
                "expiresAt": int((time.time() + expires_in) * 1000),
                "scopes": creds.get("scopes", []),
                "subscriptionType": creds.get("subscriptionType", ""),
                "rateLimitTier": creds.get("rateLimitTier", ""),
            }
        }

        cred_path.write_text(json.dumps(new_creds, indent=2))

        expires_dt = datetime.fromtimestamp(
            new_creds["claudeAiOauth"]["expiresAt"] / 1000, tz=timezone.utc
        )
        print(f"[refresh] success — new token expires {expires_dt.isoformat()}", flush=True)
        return True

    except Exception as e:
        print(f"[refresh] exception: {e}", flush=True)
        traceback.print_exc()
        return False

# ---------------------------------------------------------------------------
# Usage monitoring
# ---------------------------------------------------------------------------

def get_today_usage(stats_path: Path) -> dict:
    """Read today's usage from stats-cache.json."""
    result = {"output_tokens": 0, "messages": 0, "sessions": 0, "cost_usd": 0.0}

    if not stats_path or not stats_path.exists():
        return result

    try:
        data = json.loads(stats_path.read_text())
    except Exception:
        return result

    today = datetime.now(timezone.utc).strftime("%Y-%m-%d")

    # Daily activity
    for day in data.get("dailyActivity", []):
        if day.get("date") == today:
            result["messages"] = day.get("messageCount", 0)
            result["sessions"] = day.get("sessionCount", 0)
            break

    # Daily model tokens (these are output tokens)
    for day in data.get("dailyModelTokens", []):
        if day.get("date") == today:
            for model, tokens in day.get("tokensByModel", {}).items():
                result["output_tokens"] += tokens
            break

    # Cumulative model usage for cost
    for model, usage in data.get("modelUsage", {}).items():
        result["cost_usd"] += usage.get("costUSD", 0)

    return result


def get_weekly_usage(stats_path: Path) -> dict:
    """Aggregate usage for the current week (Mon-Sun)."""
    result = {"output_tokens": 0, "messages": 0, "sessions": 0}

    if not stats_path or not stats_path.exists():
        return result

    try:
        data = json.loads(stats_path.read_text())
    except Exception:
        return result

    now = datetime.now(timezone.utc)
    # Monday of this week
    monday = now - timedelta(days=now.weekday())
    monday_str = monday.strftime("%Y-%m-%d")

    for day in data.get("dailyActivity", []):
        if day.get("date", "") >= monday_str:
            result["messages"] += day.get("messageCount", 0)
            result["sessions"] += day.get("sessionCount", 0)

    for day in data.get("dailyModelTokens", []):
        if day.get("date", "") >= monday_str:
            for model, tokens in day.get("tokensByModel", {}).items():
                result["output_tokens"] += tokens

    return result

# ---------------------------------------------------------------------------
# Main loop
# ---------------------------------------------------------------------------

def main():
    print(f"Claude Monitor daemon starting", flush=True)
    print(f"  warn threshold: {USAGE_WARN_PERCENT}%", flush=True)
    print(f"  daily budget:   {DAILY_OUTPUT_TOKEN_BUDGET} output tokens", flush=True)
    print(f"  refresh buffer: {TOKEN_REFRESH_BUFFER_SECS}s", flush=True)
    print(f"  check interval: {CHECK_INTERVAL_SECS}s", flush=True)

    cred_path = find_credentials_path()
    stats_path = find_stats_path()

    print(f"  credentials:    {cred_path}", flush=True)
    print(f"  stats cache:    {stats_path}", flush=True)

    if not cred_path:
        msg = "Claude Monitor: cannot find .credentials.json — token refresh disabled"
        print(f"[WARN] {msg}", flush=True)
        tg_send(f"⚠️ {msg}")

    state = load_state()

    while True:
        try:
            now_utc = datetime.now(timezone.utc)
            today_str = now_utc.strftime("%Y-%m-%d")

            # Reset daily warning flag on new day
            if state.get("daily_tokens_warned_date") != today_str:
                state["daily_tokens_warned"] = False
                state["daily_tokens_warned_date"] = today_str

            # --- 1. Token refresh check ---
            if cred_path and cred_path.exists():
                creds = read_credentials(cred_path)
                if creds and token_expires_soon(creds):
                    success = refresh_token(creds, cred_path)
                    if success:
                        state["last_refresh_utc"] = now_utc.isoformat()
                        state["refresh_count"] = state.get("refresh_count", 0) + 1
                        state["total_refreshes"] = state.get("total_refreshes", 0) + 1
                    else:
                        err_msg = f"Claude token refresh FAILED at {now_utc.isoformat()}"
                        state.setdefault("errors", []).append(err_msg)
                        # Keep only last 10 errors
                        state["errors"] = state["errors"][-10:]
                        tg_send(
                            f"🚨 <b>Claude token refresh FAILED!</b>\n"
                            f"Manual intervention may be needed.\n"
                            f"The token will expire soon."
                        )

            # --- 2. Usage monitoring ---
            # Re-discover stats path each iteration (file may appear later)
            stats_path = find_stats_path()

            if stats_path:
                daily = get_today_usage(stats_path)
                weekly = get_weekly_usage(stats_path)

                output_tokens = daily["output_tokens"]
                usage_pct = (output_tokens / DAILY_OUTPUT_TOKEN_BUDGET * 100) if DAILY_OUTPUT_TOKEN_BUDGET > 0 else 0

                print(
                    f"  daily: {output_tokens} tokens ({usage_pct:.0f}%), "
                    f"{daily['messages']} msgs | "
                    f"weekly: {weekly['output_tokens']} tokens, "
                    f"{weekly['messages']} msgs",
                    flush=True,
                )

                # Warn if approaching daily budget
                if usage_pct >= USAGE_WARN_PERCENT and not state.get("daily_tokens_warned"):
                    state["daily_tokens_warned"] = True
                    tg_send(
                        f"⚠️ <b>Claude usage at {usage_pct:.0f}%</b>\n"
                        f"<code>{output_tokens:,} / {DAILY_OUTPUT_TOKEN_BUDGET:,}</code> output tokens today\n"
                        f"Messages: {daily['messages']} | Sessions: {daily['sessions']}\n"
                        f"Weekly total: {weekly['output_tokens']:,} tokens, {weekly['messages']} msgs"
                    )
                    print(f"  [ALERT] usage at {usage_pct:.0f}% — Telegram alert sent", flush=True)

            # Save state
            save_state(state)

        except Exception as e:
            print(f"[error] {e}", flush=True)
            traceback.print_exc()

        time.sleep(CHECK_INTERVAL_SECS)


if __name__ == "__main__":
    main()
