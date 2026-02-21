const http = require("http");
const path = require("path");
const os = require("os");
const { sendMessage, isMessagesRunning } = require("./applescript");

// --- Configuration from environment ---
const PORT = parseInt(process.env.PORT || "3040", 10);
const WEBHOOK_URL = process.env.WEBHOOK_URL || "http://127.0.0.1:3030/api/messaging/incoming";
const POLL_INTERVAL_MS = parseInt(process.env.POLL_INTERVAL_MS || "3000", 10);
const ALLOWED_IDS = (process.env.ALLOWED_IDS || "")
  .split(",")
  .map((s) => s.trim())
  .filter(Boolean);

// --- Chat.db polling state ---
const CHAT_DB_PATH = path.join(
  os.homedir(),
  "Library",
  "Messages",
  "chat.db"
);
let lastRowId = 0;
let db = null;

function initDb() {
  try {
    const Database = require("better-sqlite3");
    db = new Database(CHAT_DB_PATH, { readonly: true });
    // Get the current max ROWID so we only process new messages
    const row = db.prepare("SELECT MAX(ROWID) as max_id FROM message").get();
    lastRowId = row?.max_id || 0;
    console.log(`[chat.db] initialized, starting from ROWID ${lastRowId}`);
  } catch (err) {
    console.error(`[chat.db] failed to open: ${err.message}`);
    console.error("Ensure Full Disk Access is granted to Terminal/Node.");
  }
}

function pollNewMessages() {
  if (!db) return;

  try {
    const rows = db
      .prepare(
        `SELECT m.ROWID, m.text, m.is_from_me,
                COALESCE(h.id, '') as sender
         FROM message m
         LEFT JOIN handle h ON m.handle_id = h.ROWID
         WHERE m.ROWID > ? AND m.is_from_me = 0 AND m.text IS NOT NULL
         ORDER BY m.ROWID ASC
         LIMIT 50`
      )
      .all(lastRowId);

    for (const row of rows) {
      lastRowId = row.ROWID;

      // Skip if not from an allowed sender
      if (ALLOWED_IDS.length > 0 && !ALLOWED_IDS.includes(row.sender)) {
        continue;
      }

      console.log(`[incoming] from=${row.sender} text=${row.text.slice(0, 50)}`);

      // POST to safe-agent webhook
      const payload = JSON.stringify({
        platform: "imessage",
        channel: row.sender,
        sender: row.sender,
        text: row.text,
      });

      fetch(WEBHOOK_URL, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: payload,
      }).catch((err) => {
        console.error(`[webhook] failed to POST: ${err.message}`);
      });
    }
  } catch (err) {
    console.error(`[poll] error: ${err.message}`);
  }
}

// --- HTTP Server ---
function parseBody(req) {
  return new Promise((resolve, reject) => {
    let data = "";
    req.on("data", (chunk) => (data += chunk));
    req.on("end", () => {
      try {
        resolve(JSON.parse(data));
      } catch {
        reject(new Error("Invalid JSON"));
      }
    });
  });
}

const server = http.createServer(async (req, res) => {
  // POST /send
  if (req.method === "POST" && req.url === "/send") {
    try {
      const { to, text } = await parseBody(req);
      if (!to || !text) {
        res.writeHead(400, { "Content-Type": "application/json" });
        return res.end(JSON.stringify({ ok: false, error: "missing to or text" }));
      }
      await sendMessage(to, text);
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ ok: true }));
    } catch (err) {
      console.error(`[send] error: ${err.message}`);
      res.writeHead(500, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ ok: false, error: err.message }));
    }
    return;
  }

  // GET /status
  if (req.method === "GET" && req.url === "/status") {
    const running = await isMessagesRunning();
    const dbOk = db !== null;
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(
      JSON.stringify({
        state: running && dbOk ? "connected" : "disconnected",
        info: `Messages.app=${running}, chat.db=${dbOk}`,
      })
    );
    return;
  }

  res.writeHead(404);
  res.end("not found");
});

// --- Start ---
server.listen(PORT, () => {
  console.log(`[imessage-bridge] listening on port ${PORT}`);
  console.log(`[imessage-bridge] webhook URL: ${WEBHOOK_URL}`);
  console.log(`[imessage-bridge] allowed IDs: ${ALLOWED_IDS.join(", ") || "(all)"}`);

  initDb();
  setInterval(pollNewMessages, POLL_INTERVAL_MS);
});
