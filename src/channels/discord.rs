use anyhow::{bail, Context, Result};
use async_trait::async_trait;

use super::{Channel, ChannelDef};

pub fn channel_def() -> ChannelDef {
    ChannelDef {
        config_key: "discord_webhook_url",
        slug: "discord",
        display_name: "Discord",
        validate: validate_webhook_url,
        from_env,
    }
}

/// Unlike email/Telegram/ntfy, Discord Incoming Webhooks have no shared
/// server secret in ENV - the webhook URL itself contains the secret and
/// is different per contact/channel (see CHANNELS.md). It therefore lives
/// as an address in config.toml instead of in .env; the check here is
/// purely formal (format/host), no network call.
fn validate_webhook_url(url: &str) -> Result<()> {
    let parsed = reqwest::Url::parse(url).with_context(|| format!("not a valid URL: '{url}'"))?;
    let host_ok = matches!(parsed.host_str(), Some("discord.com") | Some("discordapp.com") | Some("ptb.discord.com") | Some("canary.discord.com"));
    let path_ok = parsed.path().starts_with("/api/webhooks/");
    if parsed.scheme() != "https" || !host_ok || !path_ok {
        bail!("not a valid Discord webhook URL ('{url}') - expected https://discord.com/api/webhooks/<id>/<token>");
    }
    Ok(())
}

fn from_env() -> Result<Box<dyn Channel>> {
    Ok(Box::new(DiscordConfig { client: reqwest::Client::new() }))
}

/// No fields - see the comment on `validate_webhook_url` for why Discord
/// (unlike the other channels) needs no shared ENV configuration.
struct DiscordConfig {
    client: reqwest::Client,
}

#[derive(serde::Serialize)]
struct WebhookPayload<'a> {
    content: &'a str,
}

#[async_trait]
impl Channel for DiscordConfig {
    /// There is no shared connection/shared secret that could be checked
    /// independently of a specific contact's webhook URL (see above) - the
    /// startup check is therefore a deliberate no-op. Whether an
    /// individual webhook URL actually works only shows up on the first
    /// real send attempt to that particular contact.
    async fn test_connection(&self) -> Result<()> {
        Ok(())
    }

    async fn send(&self, _contact_name: &str, webhook_url: &str, subject: &str, body: &str) -> Result<()> {
        let content = format!("**{subject}**\n\n{body}");
        let response = self
            .client
            .post(webhook_url)
            .json(&WebhookPayload { content: &content })
            .send()
            .await
            .context("Discord send failed (network)")?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            bail!("Discord send failed: HTTP {status} - {text}");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_discord_webhook_urls() {
        assert!(validate_webhook_url("https://discord.com/api/webhooks/123456789/abcDEF-token").is_ok());
        assert!(validate_webhook_url("https://discordapp.com/api/webhooks/123456789/abcDEF-token").is_ok());
    }

    #[test]
    fn rejects_non_discord_or_malformed_urls() {
        assert!(validate_webhook_url("").is_err());
        assert!(validate_webhook_url("not a url").is_err());
        assert!(validate_webhook_url("http://discord.com/api/webhooks/1/token").is_err());
        assert!(validate_webhook_url("https://evil.example.com/api/webhooks/1/token").is_err());
        assert!(validate_webhook_url("https://discord.com/some/other/path").is_err());
    }
}
