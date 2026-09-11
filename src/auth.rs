use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

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
    let authorized = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|v| v == expected_header)
        .unwrap_or(false);

    if authorized {
        next.run(req).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}
