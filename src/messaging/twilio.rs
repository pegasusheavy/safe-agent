use async_trait::async_trait;
use tracing::debug;

use crate::error::{Result, SafeAgentError};

use super::MessagingBackend;

/// Twilio SMS backend — sends SMS directly via the Twilio REST API.
pub struct TwilioBackend {
    account_sid: String,
    auth_token: String,
    from_number: String,
    http: reqwest::Client,
}

impl TwilioBackend {
    pub fn new(account_sid: String, auth_token: String, from_number: String) -> Self {
        Self {
            account_sid,
            auth_token,
            from_number,
            http: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl MessagingBackend for TwilioBackend {
    fn platform_name(&self) -> &str {
        "twilio"
    }

    fn max_message_length(&self) -> usize {
        // Twilio concatenates up to 10 SMS segments (1600 chars).
        1600
    }

    async fn send_message(&self, channel: &str, text: &str) -> Result<()> {
        debug!(to = channel, "sending SMS via Twilio");

        let url = format!(
            "https://api.twilio.com/2010-04-01/Accounts/{}/Messages.json",
            self.account_sid
        );

        let resp = self
            .http
            .post(&url)
            .basic_auth(&self.account_sid, Some(&self.auth_token))
            .form(&[
                ("From", self.from_number.as_str()),
                ("To", channel),
                ("Body", text),
            ])
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
            .map_err(|e| SafeAgentError::Messaging(format!("twilio send failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(SafeAgentError::Messaging(format!(
                "twilio returned {status}: {body}"
            )));
        }

        Ok(())
    }

    async fn send_typing(&self, _channel: &str) -> Result<()> {
        // SMS has no typing indicator concept.
        Ok(())
    }
}
