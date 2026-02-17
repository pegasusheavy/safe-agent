/**
 * WhatsApp bridge for safe-agent using @whiskeysockets/baileys.
 *
 * Environment variables:
 *   PORT          – HTTP API port (default 3033)
 *   AUTH_DIR      – Directory to persist session auth state
 *   WEBHOOK_URL   – URL to POST incoming messages to (the agent webhook)
 *   ALLOWED_NUMBERS – Comma-separated list of allowed phone numbers (E.164)
 */

const {
  default: makeWASocket,
  useMultiFileAuthState,
  DisconnectReason,
  fetchLatestBaileysVersion,
  makeCacheableSignalKeyStore,
} = require("@whiskeysockets/baileys");
const express = require("express");
const QRCode = require("qrcode");
const pino = require("pino");
const path = require("path");
const fs = require("fs");

const PORT = parseInt(process.env.PORT || "3033", 10);
const AUTH_DIR = process.env.AUTH_DIR || path.join(__dirname, "auth");
const WEBHOOK_URL =
  process.env.WEBHOOK_URL || "http://127.0.0.1:3030/api/messaging/incoming";
const ALLOWED_NUMBERS = (process.env.ALLOWED_NUMBERS || "")
  .split(",")
  .map((n) => n.trim())
  .filter(Boolean);

const logger = pino({ level: "info" });

let sock = null;
let currentQR = null;
let connectionState = "disconnected";
let connectedNumber = null;

// ---------------------------------------------------------------------------
// Express HTTP API
// ---------------------------------------------------------------------------

const app = express();
app.use(express.json());

// POST /send  { to, text }
app.post("/send", async (req, res) => {
  try {
    const { to, text } = req.body;
    if (!to || !text) {
      return res.status(400).json({ error: "missing 'to' or 'text'" });
    }

    if (!sock) {
      return res.status(503).json({ error: "not connected" });
    }

    // Ensure the JID is formatted correctly
    const jid = to.includes("@") ? to : `${to.replace(/\+/g, "")}@s.whatsapp.net`;

    await sock.sendMessage(jid, { text });
    res.json({ ok: true });
  } catch (err) {
    logger.error({ err }, "send failed");
    res.status(500).json({ error: err.message });
  }
});

// GET /status
app.get("/status", (req, res) => {
  res.json({
    state: connectionState,
    qr: currentQR,
    number: connectedNumber,
    allowedNumbers: ALLOWED_NUMBERS,
  });
});

// GET /qr  — returns base64 PNG of the QR code
app.get("/qr", async (req, res) => {
  if (!currentQR) {
    return res.status(404).json({ error: "no QR code available" });
  }
  try {
    const png = await QRCode.toDataURL(currentQR);
    res.json({ qr: png });
  } catch (err) {
    res.status(500).json({ error: err.message });
  }
});

app.listen(PORT, () => {
  logger.info({ port: PORT }, "whatsapp bridge HTTP server started");
});

// ---------------------------------------------------------------------------
// Baileys connection
// ---------------------------------------------------------------------------

async function connectWhatsApp() {
  fs.mkdirSync(AUTH_DIR, { recursive: true });

  const { state, saveCreds } = await useMultiFileAuthState(AUTH_DIR);
  const { version } = await fetchLatestBaileysVersion();

  sock = makeWASocket({
    version,
    logger: pino({ level: "warn" }),
    auth: {
      creds: state.creds,
      keys: makeCacheableSignalKeyStore(state.keys, pino({ level: "warn" })),
    },
    generateHighQualityLinkPreview: false,
  });

  // Save credentials on update
  sock.ev.on("creds.update", saveCreds);

  // Connection state events
  sock.ev.on("connection.update", async (update) => {
    const { connection, lastDisconnect, qr } = update;

    if (qr) {
      currentQR = qr;
      connectionState = "pairing";
      logger.info("QR code received — scan with WhatsApp to pair");
    }

    if (connection === "open") {
      currentQR = null;
      connectionState = "connected";
      connectedNumber = sock.user?.id?.split(":")[0] || null;
      logger.info({ number: connectedNumber }, "whatsapp connected");
    }

    if (connection === "close") {
      connectionState = "disconnected";
      const reason = lastDisconnect?.error?.output?.statusCode;
      const shouldReconnect = reason !== DisconnectReason.loggedOut;
      logger.info(
        { reason, shouldReconnect },
        "connection closed"
      );
      if (shouldReconnect) {
        setTimeout(connectWhatsApp, 3000);
      } else {
        logger.warn("logged out — delete auth dir and rescan QR");
      }
    }
  });

  // Incoming messages
  sock.ev.on("messages.upsert", async ({ messages, type: msgType }) => {
    if (msgType !== "notify") return;

    for (const msg of messages) {
      if (msg.key.fromMe) continue;

      const sender = msg.key.remoteJid;
      if (!sender || sender.endsWith("@g.us")) continue; // skip group messages

      const senderNumber = "+" + sender.split("@")[0];
      const text =
        msg.message?.conversation ||
        msg.message?.extendedTextMessage?.text ||
        "";

      if (!text) continue;

      // Authorization
      if (
        ALLOWED_NUMBERS.length > 0 &&
        !ALLOWED_NUMBERS.includes(senderNumber)
      ) {
        logger.warn({ sender: senderNumber }, "unauthorized sender");
        await sock.sendMessage(sender, {
          text: "⛔ Unauthorized. Your number is not in the allowed list.",
        });
        continue;
      }

      logger.info({ sender: senderNumber, text }, "incoming message");

      // Forward to the agent webhook
      try {
        const resp = await fetch(WEBHOOK_URL, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            platform: "whatsapp",
            channel: sender,
            sender: senderNumber,
            text,
          }),
        });

        if (!resp.ok) {
          const body = await resp.text();
          logger.error(
            { status: resp.status, body },
            "webhook returned error"
          );
        }
      } catch (err) {
        logger.error({ err }, "failed to call webhook");
      }
    }
  });
}

connectWhatsApp().catch((err) => {
  logger.error({ err }, "fatal error connecting to whatsapp");
  process.exit(1);
});
