use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use tracing_subscriber::EnvFilter;

use sendmail_mcp::channels::{self, Channel};
use sendmail_mcp::config::load_contacts;
use sendmail_mcp::build_router;

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

    let config_path = std::env::var("CONFIG_PATH").unwrap_or_else(|_| "/app/config.toml".into());
    let tools = load_contacts(&PathBuf::from(&config_path))
        .with_context(|| format!("could not load contact list: {config_path}"))?;

    tracing::info!(count = tools.len(), "tools loaded");
    for t in &tools {
        tracing::info!(tool = %t.tool_name, name = %t.contact_name, "tool registered");
    }

    // Each channel is only loaded (and its required ENV variables only
    // demanded) if the contact book actually uses it - a pure Telegram
    // config shouldn't need SMTP_HOST, for example, and vice versa. This
    // loop doesn't know a single concrete channel by name - new channels
    // register themselves exclusively via channels::registry().
    let mut senders: HashMap<&'static str, Box<dyn Channel>> = HashMap::new();
    for def in channels::registry() {
        let uses_channel = tools.iter().any(|t| t.channel_slug == def.slug);
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
    }

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

    let app = build_router(tools, senders, bearer_token);

    let bind_addr: SocketAddr = std::env::var("MCP_BIND")
        .unwrap_or_else(|_| "0.0.0.0:8080".into())
        .parse()
        .context("MCP_BIND is not a valid socket address")?;

    tracing::info!(%bind_addr, "rolodex-mcp starting");
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await?;

    Ok(())
}
