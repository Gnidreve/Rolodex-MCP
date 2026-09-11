use std::net::SocketAddr;
use std::time::Instant;

use axum::extract::{ConnectInfo, Request};
use axum::middleware::Next;
use axum::response::Response;

/// Wird von auth.rs auf abgelehnte Responses gesetzt, damit die Logzeile den
/// Ablehnungsgrund mit ausgibt, statt eine eigene WARN-Zeile zu schreiben.
#[derive(Clone)]
pub struct RejectReason(pub &'static str);

/// Eine Logzeile pro Request statt tower-http's Default (zwei Zeilen +
/// rmcp's eigenes internes Session-/Response-Logging). Client-IP kommt aus
/// X-Forwarded-For (von Traefik gesetzt), mit Fallback auf die direkte
/// TCP-Peer-Adresse für den Fall, dass mal kein Proxy davorhängt (z.B. lokal).
pub async fn log_requests(ConnectInfo(peer): ConnectInfo<SocketAddr>, req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let ip = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(str::trim)
        .map(str::to_string)
        .unwrap_or_else(|| peer.ip().to_string());

    let start = Instant::now();
    let response = next.run(req).await;
    let elapsed_ms = start.elapsed().as_millis();
    let status = response.status();

    let reason = response.extensions().get::<RejectReason>().map(|r| format!(" [{}]", r.0));

    let line = format!(
        "{ip:<15} {method:<6} {path:<20} -> {} ({elapsed_ms}ms){}",
        status.as_u16(),
        reason.unwrap_or_default(),
    );

    if status.is_client_error() || status.is_server_error() {
        tracing::warn!("{line}");
    } else {
        tracing::info!("{line}");
    }

    response
}
