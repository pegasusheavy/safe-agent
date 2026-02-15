use teloxide::prelude::*;
use tracing::{debug, error};

/// Send a notification to the operator's Telegram chat.
pub async fn notify(bot: &Bot, chat_id: ChatId, message: &str) {
    debug!(chat_id = %chat_id, "sending telegram notification");
    if let Err(e) = bot.send_message(chat_id, message).await {
        error!("failed to send telegram notification: {e}");
    }
}

/// Notify about a new pending action.
pub async fn notify_pending_action(
    bot: &Bot,
    chat_id: ChatId,
    action_id: &str,
    tool_name: &str,
    reasoning: &str,
) {
    let msg = format!(
        "🔔 *New pending action*\nID: `{action_id}`\nTool: `{tool_name}`\nReason: _{reasoning}_\n\nUse /approve {action_id} or /reject {action_id}",
    );
    notify(bot, chat_id, &msg).await;
}

/// Notify about a tool execution result.
pub async fn notify_result(
    bot: &Bot,
    chat_id: ChatId,
    tool_name: &str,
    success: bool,
    output: &str,
) {
    let icon = if success { "✅" } else { "❌" };
    let truncated = if output.len() > 500 {
        format!("{}...", &output[..500])
    } else {
        output.to_string()
    };
    let msg = format!("{icon} *{tool_name}*\n```\n{truncated}\n```");
    notify(bot, chat_id, &msg).await;
}

/// Notify about an agent error.
pub async fn notify_error(bot: &Bot, chat_id: ChatId, error: &str) {
    let msg = format!("🚨 *Agent error*\n{error}");
    notify(bot, chat_id, &msg).await;
}
