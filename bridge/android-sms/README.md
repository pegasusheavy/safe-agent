# Android SMS Bridge

Standalone Python HTTP server that bridges safe-agent to Android SMS via
[Termux:API](https://wiki.termux.com/wiki/Termux:API). No external
dependencies -- uses only the Python 3 standard library.

## Requirements

- Android phone
- [Termux](https://f-droid.org/packages/com.termux/) (install from F-Droid, **not** Google Play)
- [Termux:API](https://f-droid.org/packages/com.termux.api/) (also from F-Droid)
- SMS permissions granted to Termux:API (`termux-sms-list` and `termux-sms-send` must work)
- Python 3.8+ (install in Termux with `pkg install python`)

## Setup

1. Copy `server.py` to your phone (e.g. into `~/android-sms-bridge/`).
2. Grant SMS permissions:
   ```
   termux-sms-list -l 1      # triggers permission prompt
   termux-sms-send -n "+15555555555" "test"
   ```
3. Start the bridge:
   ```
   export WEBHOOK_URL="http://<safe-agent-ip>:3030/api/messaging/incoming"
   python server.py
   ```

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `PORT` | `3041` | Port the bridge listens on |
| `WEBHOOK_URL` | `http://127.0.0.1:3030/api/messaging/incoming` | safe-agent incoming message endpoint |
| `POLL_INTERVAL` | `3` | Seconds between inbox polls |
| `ALLOWED_IDS` | *(empty = all)* | Comma-separated phone numbers to accept (e.g. `+15551234567,+15559876543`) |

## How It Works

**Outbound (safe-agent -> SMS):** safe-agent POSTs to the bridge's
`/send` endpoint with `{"to": "+1...", "text": "..."}`. The bridge calls
`termux-sms-send` to dispatch the message.

**Inbound (SMS -> safe-agent):** A background thread polls
`termux-sms-list` every `POLL_INTERVAL` seconds, compares timestamps to
find new messages, and forwards them to safe-agent's webhook as:

```json
{
  "platform": "android_sms",
  "channel": "+15551234567",
  "sender": "+15551234567",
  "text": "message body"
}
```

**Status:** `GET /status` returns `{"state": "connected", "info": "..."}` or
`{"state": "disconnected", ...}` depending on whether Termux:API is
reachable.

## Network

safe-agent must be reachable from the phone. Common setups:

- **Same LAN:** Phone and server on the same Wi-Fi network. Set
  `WEBHOOK_URL` to the server's LAN IP.
- **Tailscale / WireGuard:** Use the tunnel IP for `WEBHOOK_URL`.
- **Reverse tunnel:** `ssh -R 3041:localhost:3041 user@server` to expose
  the bridge to the server.

safe-agent connects to the bridge at `http://<phone-ip>:3041/send`, so
the phone must also be reachable from the server (or tunneled).

## Troubleshooting

| Symptom | Fix |
|---|---|
| `[poll] error: [Errno 2] No such file or directory: 'termux-sms-list'` | Install Termux:API: `pkg install termux-api` |
| `termux-sms-list` hangs or returns empty | Open Termux:API app, grant SMS permissions in Android Settings > Apps > Termux:API > Permissions |
| Webhook fails with connection refused | Verify `WEBHOOK_URL` points to a reachable safe-agent instance |
| Messages from some numbers ignored | Check `ALLOWED_IDS` -- if set, only listed numbers are forwarded |
| Duplicate messages after restart | Normal on first poll after restart; the bridge re-anchors to the latest message timestamp |
