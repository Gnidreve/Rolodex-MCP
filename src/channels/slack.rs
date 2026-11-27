use anyhow::{bail, Context, Result};
use async_trait::async_trait;

use super::{Channel, ChannelDef};

pub fn channel_def() -> ChannelDef {
    ChannelDef {
        config_key: "slack_webhook_url",
        slug: "slack",
        display_name: "Slack",
        validate: validate_webhook_url,
        from_env,
    }
}

/// Like Discord, Slack Incoming Webhooks have no shared server secret in
/// ENV - the webhook URL itself contains the secret and is different per
/// contact/channel (see CHANNELS.md). It therefore lives as an address in
/// config.toml instead of in .env; the check here is purely formal
/// (format/host), no network call.
fn validate_webhook_url(url: &str) -> Result<()> {
    let parsed = reqwest::Url::parse(url).with_context(|| format!("not a valid URL: '{url}'"))?;
    let host_ok = parsed.host_str() == Some("hooks.slack.com");
    let path_ok = parsed.path().starts_with("/services/");
    if parsed.scheme() != "https" || !host_ok || !path_ok {
        bail!("not a valid Slack webhook URL ('{url}') - expected https://hooks.slack.com/services/<...>");
    }
    Ok(())
}

fn from_env() -> Result<Box<dyn Channel>> {
    Ok(Box::new(SlackConfig { client: reqwest::Client::new() }))
}

/// No fields - see the comment on `validate_webhook_url` for why Slack
/// (unlike email/Telegram/ntfy) needs no shared ENV configuration.
struct SlackConfig {
    client: reqwest::Client,
}

#[derive(serde::Serialize)]
struct WebhookPayload<'a> {
    text: &'a str,
}

#[async_trait]
impl Channel for SlackConfig {
    /// There is no shared connection/shared secret that could be checked
    /// independently of a specific contact's webhook URL (see above).
    /// Unlike Discord, a GET on a Slack webhook URL doesn't return any
    /// useful metadata anyway (Slack only accepts POST there) - the
    /// startup check is therefore likewise a deliberate no-op. Whether an
    /// individual webhook URL actually works only shows up on the first
    /// real send attempt to that particular contact.
    async fn test_connection(&self) -> Result<()> {
        Ok(())
    }

    async fn send(&self, _contact_name: &str, webhook_url: &str, subject: &str, body: &str) -> Result<()> {
        let text = format!("*{subject}*\n\n{body}");
        let response = self
            .client
            .post(webhook_url)
            .json(&WebhookPayload { text: &text })
            .send()
            .await
            .context("Slack send failed (network)")?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            bail!("Slack send failed: HTTP {status} - {text}");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_slack_webhook_urls() {
        assert!(validate_webhook_url("https://hooks.slack.com/services/some-team/some-bot/some-token").is_ok());
    }

    #[test]
    fn rejects_non_slack_or_malformed_urls() {
        assert!(validate_webhook_url("").is_err());
        assert!(validate_webhook_url("not a url").is_err());
        assert!(validate_webhook_url("http://hooks.slack.com/services/T0/B0/X").is_err());
        assert!(validate_webhook_url("https://evil.example.com/services/T0/B0/X").is_err());
        assert!(validate_webhook_url("https://hooks.slack.com/some/other/path").is_err());
    }
}
