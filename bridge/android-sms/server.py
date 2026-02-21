#!/usr/bin/env python3
"""Android SMS bridge for safe-agent via Termux:API."""

import json
import subprocess
import time
import threading
from http.server import HTTPServer, BaseHTTPRequestHandler
from urllib.request import urlopen, Request
from urllib.error import URLError
import os

PORT = int(os.environ.get("PORT", "3041"))
WEBHOOK_URL = os.environ.get(
    "WEBHOOK_URL", "http://127.0.0.1:3030/api/messaging/incoming"
)
POLL_INTERVAL = int(os.environ.get("POLL_INTERVAL", "3"))
ALLOWED_IDS = [
    s.strip()
    for s in os.environ.get("ALLOWED_IDS", "").split(",")
    if s.strip()
]

last_seen_ts = None


def send_sms(to: str, text: str):
    """Send an SMS using Termux:API."""
    subprocess.run(
        ["termux-sms-send", "-n", to, text],
        check=True,
        timeout=30,
    )


def poll_incoming():
    """Poll for new SMS messages and forward to safe-agent."""
    global last_seen_ts

    try:
        result = subprocess.run(
            ["termux-sms-list", "-l", "10", "-t", "inbox"],
            capture_output=True,
            text=True,
            timeout=10,
        )
        messages = json.loads(result.stdout)
    except Exception as e:
        print(f"[poll] error: {e}")
        return

    if last_seen_ts is None and messages:
        # First run — just record the latest timestamp
        last_seen_ts = messages[0].get("received", "")
        return

    for msg in reversed(messages):
        ts = msg.get("received", "")
        if last_seen_ts is not None and ts <= last_seen_ts:
            continue

        sender = msg.get("number", "")
        body = msg.get("body", "")
        last_seen_ts = ts

        if ALLOWED_IDS and sender not in ALLOWED_IDS:
            continue

        print(f"[incoming] from={sender} text={body[:50]}")

        payload = json.dumps({
            "platform": "android_sms",
            "channel": sender,
            "sender": sender,
            "text": body,
        }).encode()

        try:
            req = Request(
                WEBHOOK_URL,
                data=payload,
                headers={"Content-Type": "application/json"},
            )
            urlopen(req, timeout=10)
        except URLError as e:
            print(f"[webhook] failed: {e}")


def poll_loop():
    while True:
        poll_incoming()
        time.sleep(POLL_INTERVAL)


class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        if self.path == "/send":
            length = int(self.headers.get("Content-Length", 0))
            body = json.loads(self.rfile.read(length))
            to = body.get("to", "")
            text = body.get("text", "")
            if not to or not text:
                self.send_response(400)
                self.end_headers()
                self.wfile.write(
                    json.dumps({"ok": False, "error": "missing to or text"}).encode()
                )
                return
            try:
                send_sms(to, text)
                self.send_response(200)
                self.end_headers()
                self.wfile.write(json.dumps({"ok": True}).encode())
            except Exception as e:
                self.send_response(500)
                self.end_headers()
                self.wfile.write(
                    json.dumps({"ok": False, "error": str(e)}).encode()
                )
        else:
            self.send_response(404)
            self.end_headers()

    def do_GET(self):
        if self.path == "/status":
            try:
                subprocess.run(
                    ["termux-sms-list", "-l", "1"],
                    capture_output=True,
                    timeout=5,
                    check=True,
                )
                state = "connected"
                info = "Termux:API available"
            except Exception:
                state = "disconnected"
                info = "Termux:API unavailable"

            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(
                json.dumps({"state": state, "info": info}).encode()
            )
        else:
            self.send_response(404)
            self.end_headers()

    def log_message(self, format, *args):
        print(f"[http] {format % args}")


if __name__ == "__main__":
    print(f"[android-sms-bridge] port={PORT} webhook={WEBHOOK_URL}")
    print(f"[android-sms-bridge] allowed IDs: {', '.join(ALLOWED_IDS) or '(all)'}")

    threading.Thread(target=poll_loop, daemon=True).start()

    server = HTTPServer(("0.0.0.0", PORT), Handler)
    server.serve_forever()
