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

/// Telegram chat IDs are numbers (groups/supergroups have negative IDs) -
/// not the @username.
fn validate_chat_id(chat_id: &str) -> Result<()> {
    if chat_id.parse::<i64>().is_err() {
        bail!("not a valid telegram_chat_id ('{chat_id}') - must be a number (groups/supergroups have negative IDs)");
    }
    Ok(())
}

fn from_env() -> Result<Box<dyn Channel>> {
    let bot_token = env_required("TELEGRAM_BOT_TOKEN")?;
    Ok(Box::new(TelegramConfig { bot_token }))
}

fn env_required(key: &str) -> Result<String> {
    std::env::var(key).with_context(|| format!("required ENV variable {key} is not set"))
}

/// Bot token comes from ENV, never from config.toml - config.toml stays
/// exclusively the contact book (see config.rs).
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
    /// Checks the bot token once against the Telegram API (`getMe`),
    /// without sending a message.
    async fn test_connection(&self) -> Result<()> {
        let url = format!("https://api.telegram.org/bot{}/getMe", self.bot_token);
        let response = reqwest::get(&url)
            .await
            .context("Telegram connection test failed (network)")?;
        let status = response.status();
        let body: TelegramApiResponse<serde_json::Value> = response
            .json()
            .await
            .context("Telegram response to getMe not readable")?;

        if !status.is_success() || !body.ok {
            bail!(
                "Telegram connection test failed: {}",
                body.description.unwrap_or_else(|| format!("HTTP {status}"))
            );
        }
        Ok(())
    }

    async fn send(&self, _contact_name: &str, chat_id: &str, subject: &str, body: &str) -> Result<()> {
        let url = format!("https://api.telegram.org/bot{}/sendMessage", self.bot_token);
        // Telegram has no separate subject field - subject is set as a
        // bold first line before the actual text.
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
            .context("Telegram send failed (network)")?;

        let status = response.status();
        let resp_body: TelegramApiResponse<serde_json::Value> =
            response.json().await.context("Telegram response not readable")?;

        if !status.is_success() || !resp_body.ok {
            bail!(
                "Telegram send failed: {}",
                resp_body.description.unwrap_or_else(|| format!("HTTP {status}"))
            );
        }
        Ok(())
    }
}

/// MarkdownV2 requires a fixed set of special characters to be escaped,
/// otherwise the Telegram API rejects the message outright.
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
        assert_eq!(escape_markdown("Hello (World)!"), r"Hello \(World\)\!");
        assert_eq!(escape_markdown("100% done."), r"100% done\.");
    }

    #[test]
    fn validates_numeric_chat_ids_only() {
        assert!(validate_chat_id("123456789").is_ok());
        assert!(validate_chat_id("-1001234567890").is_ok());
        assert!(validate_chat_id("@max_mustermann").is_err());
    }
}
