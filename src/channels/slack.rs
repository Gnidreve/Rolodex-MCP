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

/// Wie bei Discord gibt es bei Slack Incoming Webhooks kein gemeinsames
/// Server-Geheimnis in ENV - die Webhook-URL selbst enthält das Secret und
/// ist pro Kontakt/Channel verschieden (siehe IDEA.md). Sie steht deshalb
/// als Adresse in config.toml statt in .env; die Prüfung hier ist rein
/// formal (Format/Host), keine Netzwerkabfrage.
fn validate_webhook_url(url: &str) -> Result<()> {
    let parsed = reqwest::Url::parse(url).with_context(|| format!("keine gültige URL: '{url}'"))?;
    let host_ok = parsed.host_str() == Some("hooks.slack.com");
    let path_ok = parsed.path().starts_with("/services/");
    if parsed.scheme() != "https" || !host_ok || !path_ok {
        bail!("keine gültige Slack-Webhook-URL ('{url}') - erwartet wird https://hooks.slack.com/services/<...>");
    }
    Ok(())
}

fn from_env() -> Result<Box<dyn Channel>> {
    Ok(Box::new(SlackConfig { client: reqwest::Client::new() }))
}

/// Keine Felder - siehe Kommentar bei `validate_webhook_url`, warum Slack
/// (im Unterschied zu E-Mail/Telegram/ntfy) keine gemeinsame
/// ENV-Konfiguration braucht.
struct SlackConfig {
    client: reqwest::Client,
}

#[derive(serde::Serialize)]
struct WebhookPayload<'a> {
    text: &'a str,
}

#[async_trait]
impl Channel for SlackConfig {
    /// Es gibt keine gemeinsame Verbindung/kein gemeinsames Secret, das sich
    /// unabhängig von einer konkreten Kontakt-Webhook-URL prüfen ließe (siehe
    /// oben). Anders als bei Discord liefert ein GET auf eine Slack-Webhook-
    /// URL ohnehin keine brauchbare Metadaten-Antwort (Slack erwartet dort
    /// ausschließlich POST) - der Startup-Check ist deshalb ebenfalls ein
    /// bewusstes No-Op. Ob eine einzelne Webhook-URL tatsächlich
    /// funktioniert, zeigt sich erst beim ersten echten Versand an den
    /// jeweiligen Kontakt.
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
            .context("Slack-Versand fehlgeschlagen (Netzwerk)")?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            bail!("Slack-Versand fehlgeschlagen: HTTP {status} - {text}");
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
