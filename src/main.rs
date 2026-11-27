use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use tracing_subscriber::EnvFilter;

use sendmail_mcp::channels::{self, Channel};
use sendmail_mcp::config::{group_by_channel, load_contacts};
use sendmail_mcp::mcp_server::ActiveChannel;
use sendmail_mcp::{build_router, reload};

/// `[2026-09-11 07:52:45]` instead of tracing_subscriber's default
/// (`2026-09-11T07:52:45.251010Z`) - easier to read in Coolify's log viewer.
struct BracketedUtcTime;

impl tracing_subscriber::fmt::time::FormatTime for BracketedUtcTime {
    fn format_time(&self, w: &mut tracing_subscriber::fmt::format::Writer<'_>) -> std::fmt::Result {
        write!(w, "[{}]", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Without RUST_LOG, tracing_subscriber would default to logging only
    // ERROR - that would also hide the info!() lines below (contacts
    // loaded, requests). rmcp=warn suppresses rmcp's own internal
    // session/response logging (a debug dump of every response) in normal
    // operation - set RUST_LOG=debug (or similar) to bring it back if needed.
    tracing_subscriber::fmt()
        .with_timer(BracketedUtcTime)
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,rmcp=warn")))
        .init();

    let config_path = PathBuf::from(std::env::var("CONFIG_PATH").unwrap_or_else(|_| "/app/config.toml".into()));
    let entries = load_contacts(&config_path)
        .with_context(|| format!("could not load contact list: {}", config_path.display()))?;

    tracing::info!(count = entries.len(), "contact-channel entries loaded");

    // Each channel is only loaded (and its required ENV variables only
    // demanded) if the contact book actually uses it - a pure Telegram
    // config shouldn't need SMTP_HOST, for example, and vice versa. This
    // loop doesn't know a single concrete channel by name - new channels
    // register themselves exclusively via channels::registry().
    let mut senders: HashMap<&'static str, Box<dyn Channel>> = HashMap::new();
    let mut active_channels: Vec<ActiveChannel> = Vec::new();
    for def in channels::registry() {
        let uses_channel = entries.iter().any(|e| e.channel_slug == def.slug);
        if !uses_channel {
            continue;
        }
        let sender = (def.from_env)()
            .with_context(|| format!("{} configuration incomplete (see ENV variables)", def.display_name))?;
        tracing::info!("checking {} connection...", def.display_name);
        sender
            .test_connection()
            .await
            .with_context(|| format!("{} connection failed - server will not start", def.display_name))?;
        tracing::info!("{} connection OK", def.display_name);
        senders.insert(def.slug, sender);
        active_channels.push(ActiveChannel {
            slug: def.slug,
            display_name: def.display_name,
        });
    }

    let active_slugs: Vec<&'static str> = active_channels.iter().map(|c| c.slug).collect();
    let mut grouped = group_by_channel(entries);
    grouped.retain(|slug, _| active_slugs.contains(slug));
    for ch in &active_channels {
        let count = grouped.get(ch.slug).map(Vec::len).unwrap_or(0);
        tracing::info!(channel = ch.slug, contacts = count, "channel active");
    }
    let contacts = Arc::new(RwLock::new(grouped));

    reload::spawn(config_path, active_slugs, contacts.clone(), Duration::from_secs(5));

    let bearer_token = std::env::var("MCP_BEARER_TOKEN")
        .context("required ENV variable MCP_BEARER_TOKEN is not set")?;
    if !bearer_token.is_ascii() {
        // HTTP header values must be ASCII. With a non-ASCII token, many
        // HTTP clients silently don't set the Authorization header at all
        // (instead of raising an error) - the server then rejects every
        // request with "header missing", which looks like a client bug but
        // was actually an invalid token. Better to fail hard and
        // immediately here.
        bail!("MCP_BEARER_TOKEN contains non-ASCII characters - HTTP headers must be ASCII only");
    }

    let app = build_router(active_channels, contacts, senders, bearer_token);

    let bind_addr: SocketAddr = std::env::var("MCP_BIND")
        .unwrap_or_else(|_| "0.0.0.0:8080".into())
        .parse()
        .context("MCP_BIND is not a valid socket address")?;

    tracing::info!(%bind_addr, "rolodex-mcp starting");
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await?;

    Ok(())
}
