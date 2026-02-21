# iMessage Bridge for safe-agent

AppleScript-based iMessage/SMS relay bridge that connects safe-agent to macOS Messages.app. Sends outgoing messages via AppleScript and receives incoming messages by polling the local `chat.db` SQLite database.

## Requirements

- **macOS** (AppleScript and Messages.app are macOS-only)
- **Messages.app** signed into iCloud with an Apple ID
- **iPhone SMS relay** enabled (Settings > Messages > Text Message Forwarding on iPhone) for SMS support
- **Full Disk Access** granted to your terminal application (System Settings > Privacy & Security > Full Disk Access) so the bridge can read `~/Library/Messages/chat.db`
- **Node.js** >= 18

## Setup

```bash
cd bridge/imessage
npm install
```

Configure environment variables (see below), then start:

```bash
npm start
```

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `PORT` | `3040` | HTTP server listen port |
| `WEBHOOK_URL` | `http://127.0.0.1:3030/api/messaging/incoming` | safe-agent endpoint for incoming messages |
| `POLL_INTERVAL_MS` | `3000` | How often to poll chat.db for new messages (ms) |
| `ALLOWED_IDS` | _(empty = all)_ | Comma-separated phone numbers or iCloud emails to accept messages from |

Example:

```bash
PORT=3040 \
ALLOWED_IDS="+15551234567,friend@icloud.com" \
npm start
```

## How It Works

**Sending:** safe-agent POSTs to `http://localhost:3040/send` with `{ "to": "+15551234567", "text": "Hello" }`. The bridge executes an AppleScript command that tells Messages.app to send the message via iMessage (or SMS relay if the recipient is not on iMessage).

**Receiving:** The bridge opens `~/Library/Messages/chat.db` in read-only mode and polls for new rows in the `message` table every `POLL_INTERVAL_MS` milliseconds. New incoming messages are forwarded to `WEBHOOK_URL` as JSON.

## API

### `POST /send`

Send an outgoing iMessage/SMS.

```json
{ "to": "+15551234567", "text": "Hello from safe-agent" }
```

Returns `{ "ok": true }` on success, `{ "ok": false, "error": "..." }` on failure.

### `GET /status`

Check bridge health.

```json
{ "state": "connected", "info": "Messages.app=true, chat.db=true" }
```

## Troubleshooting

- **"failed to open chat.db"** -- Grant Full Disk Access to your terminal (Terminal.app, iTerm2, etc.) in System Settings > Privacy & Security > Full Disk Access. Restart the terminal after granting access.
- **Messages not sending** -- Ensure Messages.app is open and signed in. Try sending a message manually first to confirm it works.
- **SMS not working** -- On your iPhone, go to Settings > Messages > Text Message Forwarding and enable your Mac. Both devices must be signed into the same Apple ID.
- **AppleScript permission denied** -- The first time the bridge sends a message, macOS will prompt you to allow terminal access to Messages.app. Click "OK" to allow.
