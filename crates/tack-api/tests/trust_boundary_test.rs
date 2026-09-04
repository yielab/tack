mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use tack_api::config::AppConfig;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::{Duration, timeout},
};
use tower::ServiceExt;

const API_TOKEN: &str = "secret-token";
const SPLIT_ORIGIN: &str = "https://app.example.test";

fn protected_config() -> AppConfig {
    AppConfig {
        api_token: Some(API_TOKEN.into()),
        allowed_origins: vec![SPLIT_ORIGIN.into()],
        ..AppConfig::default()
    }
}

#[tokio::test]
async fn suffix_lookalikes_stay_behind_the_bearer_gate() {
    let (app, _) = common::test_app_with_config(protected_config()).await;
    for uri in ["/api/projects/health", "/api/projects/openapi.json"] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{uri}");
    }

    let health = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);
}

#[tokio::test]
async fn csp_disallows_executable_content() {
    let (app, _) = common::test_app().await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let csp = response
        .headers()
        .get(header::CONTENT_SECURITY_POLICY)
        .and_then(|value| value.to_str().ok())
        .expect("CSP response header");
    assert!(csp.contains("script-src 'self'"));
    assert!(csp.contains("object-src 'none'"));
}

#[tokio::test]
async fn split_origin_websocket_handshake_accepts_subprotocol_credential_without_query_token() {
    let (app, _) = common::test_app_with_config(protected_config()).await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test listener");
    let address = listener.local_addr().expect("test listener address");
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let project = reqwest::Client::new()
        .post(format!("http://{address}/api/projects"))
        .bearer_auth(API_TOKEN)
        .json(&serde_json::json!({ "name": "WebSocket test", "project_type": "software" }))
        .send()
        .await
        .expect("create project over the real listener")
        .error_for_status()
        .expect("project creation must succeed")
        .json::<serde_json::Value>()
        .await
        .expect("project JSON");
    let project_id = project["id"].as_str().expect("project ID");

    let request_target = format!("/api/projects/{project_id}/boards/live");
    let request = format!(
        "GET {request_target} HTTP/1.1\r\n\
         Host: {address}\r\n\
         Origin: {SPLIT_ORIGIN}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
         Sec-WebSocket-Version: 13\r\n\
         Sec-WebSocket-Protocol: tack.v1, tack.auth.c2VjcmV0LXRva2Vu\r\n\r\n"
    );
    assert!(!request_target.contains('?'));
    assert!(!request.contains("Authorization:"));

    let mut stream = TcpStream::connect(address)
        .await
        .expect("connect WebSocket client");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("send browser-style WebSocket handshake");
    let mut buffer = [0_u8; 4096];
    let count = timeout(Duration::from_secs(2), stream.read(&mut buffer))
        .await
        .expect("WebSocket handshake timed out")
        .expect("read WebSocket handshake");
    let response = String::from_utf8_lossy(&buffer[..count]).into_owned();
    server.abort();

    assert!(
        response.starts_with("HTTP/1.1 101"),
        "subprotocol credential should authorize the upgrade, got: {response}"
    );
}
