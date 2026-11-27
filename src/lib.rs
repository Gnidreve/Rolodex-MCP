pub mod auth;
pub mod channels;
pub mod config;
pub mod logging;
pub mod mcp_server;
pub mod reload;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use axum::middleware;
use axum::routing::{get, post_service};
use axum::Json;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::{StreamableHttpServerConfig, StreamableHttpService};
use serde_json::json;

use crate::channels::Channel;
use crate::config::ContactEntry;
use crate::mcp_server::{ActiveChannel, SendMailServer};

/// Builds the finished axum router (healthcheck on GET /, MCP endpoint on
/// POST /) from an already-loaded tool/sender configuration. Used by
/// main.rs to start the real server, and by the integration tests under
/// tests/ to test in-process without an actual network bind. `contacts` is
/// shared with `reload::spawn` by the caller so config.toml changes are
/// picked up live, without rebuilding the router.
pub fn build_router(
    active_channels: Vec<ActiveChannel>,
    contacts: Arc<RwLock<HashMap<&'static str, Vec<ContactEntry>>>>,
    senders: HashMap<&'static str, Box<dyn Channel>>,
    bearer_token: String,
) -> axum::Router {
    let server = SendMailServer::new(active_channels, contacts, senders);

    // Streamable HTTP is rmcp's recommended HTTP transport (replaces the
    // old two-part HTTP+SSE scheme). stateful_mode: false because we need
    // GET on the same route as a healthcheck - with stateful_mode: true
    // rmcp reserves GET for SSE session resumption.
    let service = StreamableHttpService::new(
        move || Ok(server.clone()),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig {
            stateful_mode: false,
            ..Default::default()
        },
    );

    // Everything on "/": GET returns an unauthenticated healthcheck, POST is
    // the actual MCP endpoint and requires the bearer token.
    let health = get(|| async { Json(json!({ "status": "ok" })) });
    let mcp_route =
        post_service(service).layer(middleware::from_fn_with_state(bearer_token, auth::require_bearer_token));

    axum::Router::new()
        .route("/", health.merge(mcp_route))
        .layer(middleware::from_fn(logging::log_requests))
}
