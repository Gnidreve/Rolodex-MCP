use anyhow::Result;
use async_trait::async_trait;

mod discord;
mod email;
mod ntfy;
mod slack;
mod telegram;

/// A channel plugin encapsulates everything channel-specific: how ENV
/// configuration is loaded/checked and how a message is actually sent.
/// `config.rs`, `mcp_server.rs` and `main.rs` only know this trait (and
/// `ChannelDef` below), never a concrete email/Telegram/... - a new channel
/// is added as its own file, plus exactly one new line in `registry()`
/// below.
///
/// Project rule (see CONTRIBUTING.md): every channel is developed on its
/// own git branch and merged into `main` via pull request, without
/// touching main.rs/config.rs/mcp_server.rs.
#[async_trait]
pub trait Channel: Send + Sync {
    /// Builds the connection/access once for real, without sending - for
    /// the startup check. An error here stops the server from starting at
    /// all, instead of only surfacing on the first real send attempt.
    async fn test_connection(&self) -> Result<()>;

    /// Sends a message. `contact_name` is the display name from
    /// config.toml (some channels use it in the recipient, others ignore
    /// it - that's each channel's own decision).
    async fn send(&self, contact_name: &str, address: &str, subject: &str, body: &str) -> Result<()>;
}

/// Static metadata of a channel, independent of whether it's actually
/// configured right now (that's decided by `config.toml` at runtime).
pub struct ChannelDef {
    /// Field name in config.toml, e.g. "email" or "telegram_chat_id".
    pub config_key: &'static str,
    /// Segment in the tool name, e.g. "email" -> "send_via_email" /
    /// "list_email_contacts".
    pub slug: &'static str,
    /// For the human-readable tool title/description, e.g. "Email".
    pub display_name: &'static str,
    /// Checks the address/ID value from config.toml for plausibility (pure
    /// format, no network checks - that's what `Channel::test_connection`
    /// does).
    pub validate: fn(&str) -> Result<()>,
    /// Loads the ENV configuration for this channel. Only called by main.rs
    /// if at least one contact actually uses `config_key`.
    pub from_env: fn() -> Result<Box<dyn Channel>>,
}

/// All known channels. A new channel = its own file (on its own branch,
/// its own PR) + exactly one new line here.
pub fn registry() -> Vec<ChannelDef> {
    vec![
        email::channel_def(),
        ntfy::channel_def(),
        telegram::channel_def(),
        discord::channel_def(),
        slack::channel_def(),
    ]
}
