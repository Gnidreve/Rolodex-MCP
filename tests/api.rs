//! End-to-end integration test over the finished axum router
//! (`sendmail_mcp::build_router`) - covers healthcheck, bearer auth, and
//! the complete MCP roundtrip (tools/list, tools/call), without a real
//! TCP bind and without a real channel (email would need an SMTP server).
//! For that, this test registers a simple `MockChannel` under the test's
//! own slug "mock" - `sendmail_mcp` itself doesn't know this channel, it
//! uses exactly the same `Channel` interface as email/Telegram/...

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

use anyhow::Result;
use async_trait::async_trait;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use sendmail_mcp::channels::Channel;
use sendmail_mcp::config::ContactEntry;
use sendmail_mcp::mcp_server::ActiveChannel;

struct MockChannel;

#[async_trait]
impl Channel for MockChannel {
    async fn test_connection(&self) -> Result<()> {
        Ok(())
    }

    async fn send(&self, _contact_name: &str, _address: &str, _subject: &str, _body: &str) -> Result<()> {
        Ok(())
    }
}

fn test_app() -> axum::Router {
    let active_channels = vec![ActiveChannel {
        slug: "mock",
        display_name: "Mock",
    }];
    let mut contacts: HashMap<&'static str, Vec<ContactEntry>> = HashMap::new();
    contacts.insert(
        "mock",
        vec![ContactEntry {
            contact_name: "Test Contact".to_string(),
            contact_slug: "test_contact".to_string(),
            channel_slug: "mock",
            channel_display_name: "Mock",
            address: "irrelevant-for-the-mock".to_string(),
        }],
    );
    let mut senders: HashMap<&'static str, Box<dyn Channel>> = HashMap::new();
    senders.insert("mock", Box::new(MockChannel));
    sendmail_mcp::build_router(active_channels, Arc::new(RwLock::new(contacts)), senders, "test-token".to_string())
}

/// `logging::log_requests` extracts `ConnectInfo<SocketAddr>`, which in
/// real operation is set by `into_make_service_with_connect_info`. In an
/// in-process test without a real bind, the extension needs to be
/// inserted by hand, otherwise the extraction fails.
fn with_fake_connect_info(mut req: Request<Body>) -> Request<Body> {
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))));
    req
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    // Streamable HTTP responses come as an SSE event ("data: {...}\n\n").
    let json_part = text.strip_prefix("data: ").unwrap_or(&text).trim();
    serde_json::from_str(json_part).expect("valid JSON in the response")
}

#[tokio::test]
async fn health_check_is_unauthenticated() {
    let app = test_app();
    let response = app
        .oneshot(with_fake_connect_info(
            Request::builder().uri("/").body(Body::empty()).unwrap(),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_of(response).await;
    assert_eq!(body, serde_json::json!({ "status": "ok" }));
}

async fn body_of(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).expect("valid JSON")
}

#[tokio::test]
async fn mcp_endpoint_rejects_missing_bearer_token() {
    let app = test_app();
    let request = Request::builder()
        .method("POST")
        .uri("/")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .body(Body::from(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#))
        .unwrap();
    let response = app.oneshot(with_fake_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn tools_list_returns_the_mock_channel_tools() {
    let app = test_app();
    let request = Request::builder()
        .method("POST")
        .uri("/")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("authorization", "Bearer test-token")
        .body(Body::from(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#))
        .unwrap();
    let response = app.oneshot(with_fake_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let json = body_json(response).await;
    let tools = json["result"]["tools"].as_array().expect("tools array");
    assert_eq!(tools.len(), 2);
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"send_via_mock"));
    assert!(names.contains(&"list_mock_contacts"));
}

#[tokio::test]
async fn tools_call_invokes_the_mock_channel_and_reports_success() {
    let app = test_app();
    let request = Request::builder()
        .method("POST")
        .uri("/")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("authorization", "Bearer test-token")
        .body(Body::from(
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"send_via_mock","arguments":{"contact":"test_contact","subject":"Hello","body":"Test content"}}}"#,
        ))
        .unwrap();
    let response = app.oneshot(with_fake_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let json = body_json(response).await;
    assert_eq!(json["result"]["isError"], false);
    let text = json["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Test Contact"));
}

#[tokio::test]
async fn tools_call_rejects_unknown_contact() {
    let app = test_app();
    let request = Request::builder()
        .method("POST")
        .uri("/")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("authorization", "Bearer test-token")
        .body(Body::from(
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"send_via_mock","arguments":{"contact":"nobody","subject":"Hello","body":"Test content"}}}"#,
        ))
        .unwrap();
    let response = app.oneshot(with_fake_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let json = body_json(response).await;
    assert!(json["error"]["message"].as_str().unwrap().contains("unknown contact"));
}

#[tokio::test]
async fn list_contacts_tool_returns_the_configured_contact() {
    let app = test_app();
    let request = Request::builder()
        .method("POST")
        .uri("/")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("authorization", "Bearer test-token")
        .body(Body::from(
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"list_mock_contacts","arguments":{}}}"#,
        ))
        .unwrap();
    let response = app.oneshot(with_fake_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let json = body_json(response).await;
    let text = json["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("test_contact"));
    assert!(text.contains("Test Contact"));
}
