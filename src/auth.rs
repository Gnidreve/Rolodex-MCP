use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::logging::RejectReason;

/// Checks the `Authorization: Bearer <token>` header against MCP_BEARER_TOKEN.
/// Hooked in exclusively in front of the POST handler (the actual MCP
/// traffic) - the healthcheck on GET / stays deliberately unprotected so
/// Docker/Coolify can reach it without a token.
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
        None => Some("Authorization header missing"),
        Some(v) if v != expected_header => Some("bearer token does not match"),
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
