//! Integrationstests für die Bearer-Token-Middleware
//! (`sendmail_mcp::auth::require_bearer_token`) - gegen einen minimalen
//! axum-Router in-process getestet, ohne echten TCP-Bind.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware;
use axum::routing::get;
use axum::Router;
use tower::ServiceExt;

fn protected_app(token: &str) -> Router {
    Router::new()
        .route("/", get(|| async { "ok" }))
        .layer(middleware::from_fn_with_state(
            token.to_string(),
            sendmail_mcp::auth::require_bearer_token,
        ))
}

#[tokio::test]
async fn rejects_request_without_authorization_header() {
    let app = protected_app("secret-token");
    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn rejects_wrong_bearer_token() {
    let app = protected_app("secret-token");
    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header("Authorization", "Bearer falsches-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn accepts_correct_bearer_token() {
    let app = protected_app("secret-token");
    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header("Authorization", "Bearer secret-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
