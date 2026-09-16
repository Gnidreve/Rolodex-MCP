use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::{Channel, ChannelDef};

pub fn channel_def() -> ChannelDef {
    ChannelDef {
        config_key: "telegram_chat_id",
        slug: "telegram",
        display_name: "Telegram",
        validate: validate_chat_id,
        from_env,
    }
}

/// Telegram-Chat-IDs sind Zahlen (Gruppen/Supergruppen haben negative IDs) -
/// nicht der @username.
fn validate_chat_id(chat_id: &str) -> Result<()> {
    if chat_id.parse::<i64>().is_err() {
        bail!("keine gültige telegram_chat_id ('{chat_id}') - muss eine Zahl sein (Gruppen/Supergruppen haben negative IDs)");
    }
    Ok(())
}

fn from_env() -> Result<Box<dyn Channel>> {
    let bot_token = env_required("TELEGRAM_BOT_TOKEN")?;
    Ok(Box::new(TelegramConfig { bot_token }))
}

fn env_required(key: &str) -> Result<String> {
    std::env::var(key).with_context(|| format!("Pflicht-ENV-Variable {key} ist nicht gesetzt"))
}

/// Bot-Token kommt aus ENV, nie aus config.toml - config.toml bleibt
/// ausschließlich das Kontaktbuch (siehe config.rs).
#[derive(Debug, Clone)]
struct TelegramConfig {
    bot_token: String,
}

#[derive(Debug, Deserialize)]
struct TelegramApiResponse<T> {
    ok: bool,
    #[serde(default)]
    description: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    result: Option<T>,
}

#[async_trait]
impl Channel for TelegramConfig {
    /// Prüft den Bot-Token einmal gegen die Telegram-API (`getMe`), ohne
    /// eine Nachricht zu verschicken.
    async fn test_connection(&self) -> Result<()> {
        let url = format!("https://api.telegram.org/bot{}/getMe", self.bot_token);
        let response = reqwest::get(&url)
            .await
            .context("Telegram-Verbindungstest fehlgeschlagen (Netzwerk)")?;
        let status = response.status();
        let body: TelegramApiResponse<serde_json::Value> = response
            .json()
            .await
            .context("Telegram-Antwort auf getMe nicht lesbar")?;

        if !status.is_success() || !body.ok {
            bail!(
                "Telegram-Verbindungstest fehlgeschlagen: {}",
                body.description.unwrap_or_else(|| format!("HTTP {status}"))
            );
        }
        Ok(())
    }

    async fn send(&self, _contact_name: &str, chat_id: &str, subject: &str, body: &str) -> Result<()> {
        let url = format!("https://api.telegram.org/bot{}/sendMessage", self.bot_token);
        // Telegram kennt keinen separaten Betreff - subject wird als fette
        // erste Zeile vor den eigentlichen Text gesetzt.
        let text = format!("*{}*\n\n{}", escape_markdown(subject), escape_markdown(body));

        let client = reqwest::Client::new();
        let response = client
            .post(&url)
            .json(&json!({
                "chat_id": chat_id,
                "text": text,
                "parse_mode": "MarkdownV2",
            }))
            .send()
            .await
            .context("Telegram-Versand fehlgeschlagen (Netzwerk)")?;

        let status = response.status();
        let resp_body: TelegramApiResponse<serde_json::Value> =
            response.json().await.context("Telegram-Antwort nicht lesbar")?;

        if !status.is_success() || !resp_body.ok {
            bail!(
                "Telegram-Versand fehlgeschlagen: {}",
                resp_body.description.unwrap_or_else(|| format!("HTTP {status}"))
            );
        }
        Ok(())
    }
}

/// MarkdownV2 verlangt, dass eine feste Menge an Sonderzeichen escaped wird,
/// sonst lehnt die Telegram-API die Nachricht komplett ab.
/// https://core.telegram.org/bots/api#markdownv2-style
fn escape_markdown(text: &str) -> String {
    const SPECIAL: &[char] = &[
        '_', '*', '[', ']', '(', ')', '~', '`', '>', '#', '+', '-', '=', '|', '{', '}', '.', '!',
    ];
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if SPECIAL.contains(&ch) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_markdown_special_chars() {
        assert_eq!(escape_markdown("Hallo (Welt)!"), r"Hallo \(Welt\)\!");
        assert_eq!(escape_markdown("100% fertig."), r"100% fertig\.");
    }

    #[test]
    fn validates_numeric_chat_ids_only() {
        assert!(validate_chat_id("123456789").is_ok());
        assert!(validate_chat_id("-1001234567890").is_ok());
        assert!(validate_chat_id("@max_mustermann").is_err());
    }
}
