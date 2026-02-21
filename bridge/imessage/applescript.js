const { execFile } = require("child_process");

/**
 * Send a message via Messages.app using AppleScript.
 * @param {string} to - Phone number or iCloud email
 * @param {string} text - Message body
 * @returns {Promise<void>}
 */
function sendMessage(to, text) {
  // Escape double quotes and backslashes in the text for AppleScript
  const escaped = text.replace(/\\/g, "\\\\").replace(/"/g, '\\"');

  const script = `
    tell application "Messages"
      set targetService to 1st account whose service type = iMessage
      set targetBuddy to participant targetService handle "${to}"
      send "${escaped}" to targetBuddy
    end tell
  `;

  return new Promise((resolve, reject) => {
    execFile("osascript", ["-e", script], (err, stdout, stderr) => {
      if (err) {
        reject(new Error(`AppleScript failed: ${stderr || err.message}`));
      } else {
        resolve();
      }
    });
  });
}

/**
 * Check if Messages.app is running.
 * @returns {Promise<boolean>}
 */
function isMessagesRunning() {
  const script = `
    tell application "System Events"
      return (name of processes) contains "Messages"
    end tell
  `;

  return new Promise((resolve) => {
    execFile("osascript", ["-e", script], (err, stdout) => {
      resolve(!err && stdout.trim() === "true");
    });
  });
}

module.exports = { sendMessage, isMessagesRunning };
