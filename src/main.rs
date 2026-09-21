use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use tracing_subscriber::EnvFilter;

use sendmail_mcp::channels::{self, Channel};
use sendmail_mcp::config::load_contacts;
use sendmail_mcp::build_router;

/// `[2026-09-11 07:52:45]` statt tracing_subscribers Default
/// (`2026-09-11T07:52:45.251010Z`) - besser lesbar in Coolifys Log-Viewer.
struct BracketedUtcTime;

impl tracing_subscriber::fmt::time::FormatTime for BracketedUtcTime {
    fn format_time(&self, w: &mut tracing_subscriber::fmt::format::Writer<'_>) -> std::fmt::Result {
        write!(w, "[{}]", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Ohne RUST_LOG würde tracing_subscriber defaultmäßig nur ERROR loggen —
    // damit wären auch die info!()-Zeilen unten (Kontakte geladen, Requests)
    // unsichtbar. rmcp=warn unterdrückt rmcps eigenes internes Session-/
    // Response-Logging (Debug-Dump jeder Antwort) im Normalbetrieb - bei
    // Bedarf zieht RUST_LOG=debug (o.ä.) das wieder hoch.
    tracing_subscriber::fmt()
        .with_timer(BracketedUtcTime)
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,rmcp=warn")))
        .init();

    let config_path = std::env::var("CONFIG_PATH").unwrap_or_else(|_| "/app/config.toml".into());
    let tools = load_contacts(&PathBuf::from(&config_path))
        .with_context(|| format!("Kontaktliste konnte nicht geladen werden: {config_path}"))?;

    tracing::info!(count = tools.len(), "Tools geladen");
    for t in &tools {
        tracing::info!(tool = %t.tool_name, name = %t.contact_name, "Tool registriert");
    }

    // Jeder Kanal wird nur geladen (und seine Pflicht-ENV-Variablen nur
    // verlangt), wenn das Kontaktbuch ihn tatsächlich nutzt - eine reine
    // Telegram-Config soll z.B. keinen SMTP_HOST brauchen und umgekehrt.
    // Diese Schleife kennt keinen einzigen konkreten Kanal namentlich -
    // neue Kanäle registrieren sich ausschließlich über channels::registry().
    let mut senders: HashMap<&'static str, Box<dyn Channel>> = HashMap::new();
    for def in channels::registry() {
        let uses_channel = tools.iter().any(|t| t.channel_slug == def.slug);
        if !uses_channel {
            continue;
        }
        let sender = (def.from_env)()
            .with_context(|| format!("{}-Konfiguration unvollständig (siehe ENV-Variablen)", def.display_name))?;
        tracing::info!("Prüfe {}-Verbindung...", def.display_name);
        sender
            .test_connection()
            .await
            .with_context(|| format!("{}-Verbindung fehlgeschlagen - Server startet nicht", def.display_name))?;
        tracing::info!("{}-Verbindung OK", def.display_name);
        senders.insert(def.slug, sender);
    }

    let bearer_token = std::env::var("MCP_BEARER_TOKEN")
        .context("Pflicht-ENV-Variable MCP_BEARER_TOKEN ist nicht gesetzt")?;
    if !bearer_token.is_ascii() {
        // HTTP-Header-Werte müssen ASCII sein. Mit einem nicht-ASCII-Token
        // setzen viele HTTP-Clients den Authorization-Header gar nicht erst
        // (statt eines Fehlers) - der Server lehnt dann jeden Request mit
        // "Header fehlt" ab, was wie ein Client-Bug aussieht, aber ein
        // ungültiger Token war. Lieber hier hart und sofort abbrechen.
        bail!("MCP_BEARER_TOKEN enthält nicht-ASCII-Zeichen - HTTP-Header dürfen nur ASCII sein");
    }

    let app = build_router(tools, senders, bearer_token);

    let bind_addr: SocketAddr = std::env::var("MCP_BIND")
        .unwrap_or_else(|_| "0.0.0.0:8080".into())
        .parse()
        .context("MCP_BIND ist keine gültige Socket-Adresse")?;

    tracing::info!(%bind_addr, "sendmail-mcp startet");
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await?;

    Ok(())
}
