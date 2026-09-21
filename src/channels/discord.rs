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

/// Anders als bei E-Mail/Telegram/ntfy gibt es bei Discord Incoming Webhooks
/// kein gemeinsames Server-Geheimnis in ENV - die Webhook-URL selbst enthält
/// das Secret und ist pro Kontakt/Channel verschieden (siehe IDEA.md). Sie
/// steht deshalb als Adresse in config.toml statt in .env; die Prüfung hier
/// ist rein formal (Format/Host), keine Netzwerkabfrage.
fn validate_webhook_url(url: &str) -> Result<()> {
    let parsed = reqwest::Url::parse(url).with_context(|| format!("keine gültige URL: '{url}'"))?;
    let host_ok = matches!(parsed.host_str(), Some("discord.com") | Some("discordapp.com") | Some("ptb.discord.com") | Some("canary.discord.com"));
    let path_ok = parsed.path().starts_with("/api/webhooks/");
    if parsed.scheme() != "https" || !host_ok || !path_ok {
        bail!("keine gültige Discord-Webhook-URL ('{url}') - erwartet wird https://discord.com/api/webhooks/<id>/<token>");
    }
    Ok(())
}

fn from_env() -> Result<Box<dyn Channel>> {
    Ok(Box::new(DiscordConfig { client: reqwest::Client::new() }))
}

/// Keine Felder - siehe Kommentar bei `validate_webhook_url`, warum Discord
/// (im Unterschied zu den anderen Kanälen) keine gemeinsame ENV-Konfiguration
/// braucht.
struct DiscordConfig {
    client: reqwest::Client,
}

#[derive(serde::Serialize)]
struct WebhookPayload<'a> {
    content: &'a str,
}

#[async_trait]
impl Channel for DiscordConfig {
    /// Es gibt keine gemeinsame Verbindung/kein gemeinsames Secret, das sich
    /// unabhängig von einer konkreten Kontakt-Webhook-URL prüfen ließe (siehe
    /// oben) - der Startup-Check ist deshalb ein bewusstes No-Op. Ob eine
    /// einzelne Webhook-URL tatsächlich funktioniert, zeigt sich erst beim
    /// ersten echten Versand an den jeweiligen Kontakt.
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
            .context("Discord-Versand fehlgeschlagen (Netzwerk)")?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            bail!("Discord-Versand fehlgeschlagen: HTTP {status} - {text}");
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
