//! End-to-End-Integrationstest über den fertigen axum-Router
//! (`sendmail_mcp::build_router`) - deckt Healthcheck, Bearer-Auth und den
//! kompletten MCP-Roundtrip (tools/list, tools/call) ab, ohne echten
//! TCP-Bind und ohne einen echten Kanal (E-Mail bräuchte einen SMTP-Server).
//! Dafür registriert dieser Test einen simplen `MockChannel` unter dem
//! Test-eigenen Slug "mock" - `sendmail_mcp` selbst kennt diesen Kanal nicht,
//! er nutzt exakt dieselbe `Channel`-Schnittstelle wie E-Mail/Telegram/...

use std::collections::HashMap;
use std::net::SocketAddr;

use anyhow::Result;
use async_trait::async_trait;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use sendmail_mcp::channels::Channel;
use sendmail_mcp::config::ChannelTool;

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
    let tools = vec![ChannelTool {
        contact_name: "Test Kontakt".to_string(),
        channel_slug: "mock",
        channel_display_name: "Mock",
        address: "irrelevant-fuer-den-mock".to_string(),
        tool_name: "send_to_test_kontakt_via_mock".to_string(),
        title: "Test Kontakt — Mock".to_string(),
    }];
    let mut senders: HashMap<&'static str, Box<dyn Channel>> = HashMap::new();
    senders.insert("mock", Box::new(MockChannel));
    sendmail_mcp::build_router(tools, senders, "test-token".to_string())
}

/// `logging::log_requests` extrahiert `ConnectInfo<SocketAddr>`, das im
/// echten Betrieb von `into_make_service_with_connect_info` gesetzt wird.
/// In einem In-Process-Test ohne echten Bind muss die Extension von Hand
/// rein, sonst schlägt die Extraktion fehl.
fn with_fake_connect_info(mut req: Request<Body>) -> Request<Body> {
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))));
    req
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    // Streamable-HTTP-Antworten kommen als SSE-Event ("data: {...}\n\n").
    let json_part = text.strip_prefix("data: ").unwrap_or(&text).trim();
    serde_json::from_str(json_part).expect("gültiges JSON in der Antwort")
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
    serde_json::from_slice(&bytes).expect("gültiges JSON")
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
async fn tools_list_returns_the_configured_mock_tool() {
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
    let tools = json["result"]["tools"].as_array().expect("tools-Array");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["name"], "send_to_test_kontakt_via_mock");
    assert_eq!(tools[0]["title"], "Test Kontakt — Mock");
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
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"send_to_test_kontakt_via_mock","arguments":{"subject":"Hallo","body":"Testinhalt"}}}"#,
        ))
        .unwrap();
    let response = app.oneshot(with_fake_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let json = body_json(response).await;
    assert_eq!(json["result"]["isError"], false);
    let text = json["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Test Kontakt"));
}
