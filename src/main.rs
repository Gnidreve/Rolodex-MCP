mod config;
mod mcp_server;
mod smtp;

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::{StreamableHttpServerConfig, StreamableHttpService};

use crate::config::load_contacts;
use crate::mcp_server::SendMailServer;
use crate::smtp::SmtpConfig;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let config_path = std::env::var("CONFIG_PATH").unwrap_or_else(|_| "/app/config.toml".into());
    let contacts = load_contacts(&PathBuf::from(&config_path))
        .with_context(|| format!("Kontaktliste konnte nicht geladen werden: {config_path}"))?;

    tracing::info!(count = contacts.len(), "Kontakte geladen");
    for c in &contacts {
        tracing::info!(tool = %c.tool_name, name = %c.name, "Tool registriert");
    }

    let smtp = SmtpConfig::from_env().context("SMTP-Konfiguration unvollständig (siehe ENV-Variablen)")?;

    let server = SendMailServer::new(contacts, smtp);

    // Streamable HTTP ist der von rmcp empfohlene HTTP-Transport (ersetzt das
    // alte zweigeteilte HTTP+SSE-Schema). Der Prozess selbst spricht nur
    // HTTP — TLS/HTTPS wird über einen vorgeschalteten Reverse Proxy
    // (Traefik/Caddy/nginx) terminiert, wie bei Docker-Compose-Deployments üblich.
    let service = StreamableHttpService::new(
        move || Ok(server.clone()),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default(),
    );

    let app = axum::Router::new().nest_service("/mcp", service);

    let bind_addr: SocketAddr = std::env::var("MCP_BIND")
        .unwrap_or_else(|_| "0.0.0.0:8080".into())
        .parse()
        .context("MCP_BIND ist keine gültige Socket-Adresse")?;

    tracing::info!(%bind_addr, "sendmail-mcp startet");
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
