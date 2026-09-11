use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::logging::RejectReason;

/// Prüft den `Authorization: Bearer <token>`-Header gegen MCP_BEARER_TOKEN.
/// Wird ausschließlich vor dem POST-Handler (dem eigentlichen MCP-Traffic)
/// eingehängt — der Healthcheck auf GET / bleibt bewusst ungeschützt, damit
/// Docker/Coolify ihn ohne Token erreichen können.
pub async fn require_bearer_token(
    State(expected_token): State<String>,
    req: Request,
    next: Next,
) -> Response {
    let expected_header = format!("Bearer {expected_token}");
    let received = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    let reason = match received {
        None => Some("Authorization-Header fehlt"),
        Some(v) if v != expected_header => Some("Bearer-Token stimmt nicht überein"),
        Some(_) => None,
    };

    match reason {
        None => next.run(req).await,
        Some(reason) => {
            let mut response = StatusCode::UNAUTHORIZED.into_response();
            response.extensions_mut().insert(RejectReason(reason));
            response
        }
    }
}
