pub mod auth;
pub mod channels;
pub mod config;
pub mod logging;
pub mod mcp_server;

use std::collections::HashMap;

use axum::middleware;
use axum::routing::{get, post_service};
use axum::Json;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::{StreamableHttpServerConfig, StreamableHttpService};
use serde_json::json;

use crate::channels::Channel;
use crate::config::ChannelTool;
use crate::mcp_server::SendMailServer;

/// Baut den fertigen axum-Router (Healthcheck auf GET /, MCP-Endpunkt auf
/// POST /) aus einer bereits geladenen Tool-/Sender-Konfiguration. Von
/// main.rs zum Starten des echten Servers genutzt, von den Integrationstests
/// unter tests/ zum In-Process-Testen ohne echten Netzwerk-Bind.
pub fn build_router(
    tools: Vec<ChannelTool>,
    senders: HashMap<&'static str, Box<dyn Channel>>,
    bearer_token: String,
) -> axum::Router {
    let server = SendMailServer::new(tools, senders);

    // Streamable HTTP ist der von rmcp empfohlene HTTP-Transport (ersetzt das
    // alte zweigeteilte HTTP+SSE-Schema). stateful_mode: false, weil wir GET
    // auf derselben Route als Healthcheck brauchen - bei stateful_mode: true
    // reserviert rmcp GET für die SSE-Session-Resumption.
    let service = StreamableHttpService::new(
        move || Ok(server.clone()),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig {
            stateful_mode: false,
            ..Default::default()
        },
    );

    // Alles auf "/": GET liefert einen ungeschützten Healthcheck, POST ist
    // der eigentliche MCP-Endpunkt und verlangt den Bearer-Token.
    let health = get(|| async { Json(json!({ "status": "ok" })) });
    let mcp_route =
        post_service(service).layer(middleware::from_fn_with_state(bearer_token, auth::require_bearer_token));

    axum::Router::new()
        .route("/", health.merge(mcp_route))
        .layer(middleware::from_fn(logging::log_requests))
}
