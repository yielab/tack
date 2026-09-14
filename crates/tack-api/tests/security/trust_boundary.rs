//! The operator bearer-token boundary: a path that only looks like an
//! unauthenticated route (a suffix appended to `/api/projects`) still lands
//! behind `require_token`, `/api/health` stays open, the response carries a
//! CSP that disallows inline/executable content, and a WebSocket handshake
//! driven over a raw TCP connection authorizes via the subprotocol-embedded
//! credential with no token in the query string.

use crate::common;
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use std::net::SocketAddr;
use tack_api::config::AppConfig;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinHandle,
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

/// Binds a real TCP listener, serves `app` on a background task, and
/// returns its address plus the task handle — callers must `.abort()` it.
async fn spawn_real_server(app: Router) -> (SocketAddr, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test listener");
    let address = listener.local_addr().expect("test listener address");
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (address, server)
}

/// Creates a project over the real listener at `address`, bearer-authed
/// with `token` when given, and returns its id.
async fn create_project_via_listener(
    address: SocketAddr,
    token: Option<&str>,
    name: &str,
) -> String {
    let mut req = reqwest::Client::new()
        .post(format!("http://{address}/api/projects"))
        .json(&serde_json::json!({ "name": name, "project_type": "software" }));
    if let Some(t) = token {
        req = req.bearer_auth(t);
    }
    let project = req
        .send()
        .await
        .expect("create project over the real listener")
        .error_for_status()
        .expect("project creation must succeed")
        .json::<serde_json::Value>()
        .await
        .expect("project JSON");
    project["id"].as_str().expect("project ID").to_string()
}

/// The trimmed value of `name`'s header line in a raw HTTP `response`,
/// matched case-insensitively; panics if the header is absent.
fn response_header<'a>(response: &'a str, name: &str) -> &'a str {
    let prefix = format!("{name}:").to_ascii_lowercase();
    let line = response
        .lines()
        .find(|line| line.to_ascii_lowercase().starts_with(&prefix))
        .unwrap_or_else(|| panic!("response carries no {name} header: {response}"));
    line.split_once(':').expect("header has a colon").1.trim()
}

/// Sends a raw HTTP/WebSocket-handshake request over a fresh TCP connection
/// to `address` and returns whatever bytes come back within 2 seconds.
async fn raw_handshake(address: SocketAddr, request: &str) -> String {
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
    String::from_utf8_lossy(&buffer[..count]).into_owned()
}

#[tokio::test]
async fn suffix_lookalikes_stay_behind_bearer_gate() {
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
async fn handshake_credential_travels_in_subprotocol_not_query() {
    let (app, _) = common::test_app_with_config(protected_config()).await;
    let (address, server) = spawn_real_server(app).await;

    let project_id = create_project_via_listener(address, Some(API_TOKEN), "WebSocket test").await;

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

    let response = raw_handshake(address, &request).await;
    server.abort();

    assert!(
        response.starts_with("HTTP/1.1 101"),
        "subprotocol credential should authorize the upgrade, got: {response}"
    );
}

/// A raw handshake reader (this test, or `curl`) only ever checks the status
/// line — see the assertion above, which is the shape of every other
/// WebSocket check in this file. That shape cannot tell a spec-compliant
/// handshake apart from one a real browser refuses to use: RFC 6455 §4.1
/// requires a client that offered a subprotocol to fail the connection when
/// the response omits `Sec-WebSocket-Protocol`, and nothing about the status
/// line changes either way. This test closes that gap on the wire level by
/// parsing the response headers and asserting the exact selected value; the
/// browser side of the same gap (proving a real client actually stays
/// connected) is proved separately in
/// `frontend/e2e/board-websocket-subprotocol.spec.ts`, which no Rust-only
/// test can stand in for.
#[tokio::test]
async fn board_live_handshake_selects_tack_v1_subprotocol() {
    let (app, _) = common::test_app().await;
    let (address, server) = spawn_real_server(app).await;

    let project_id = create_project_via_listener(address, None, "WS subprotocol test").await;

    let request_target = format!("/api/projects/{project_id}/boards/live");
    let request = format!(
        "GET {request_target} HTTP/1.1\r\n\
         Host: {address}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
         Sec-WebSocket-Version: 13\r\n\
         Sec-WebSocket-Protocol: tack.v1\r\n\r\n"
    );

    let response = raw_handshake(address, &request).await;
    server.abort();

    assert!(
        response.starts_with("HTTP/1.1 101"),
        "handshake did not upgrade: {response}"
    );
    let value = response_header(&response, "sec-websocket-protocol");
    assert_eq!(
        value, "tack.v1",
        "response selected the wrong subprotocol: {response}"
    );
}

/// The default `AppConfig`, bound to `0.0.0.0` instead of loopback when
/// `non_loopback` is set.
fn config_for_bind(non_loopback: bool) -> AppConfig {
    if non_loopback {
        AppConfig {
            host: "0.0.0.0".into(),
            ..AppConfig::default()
        }
    } else {
        AppConfig::default()
    }
}

/// Starts `app` on a real loopback listener, creates a project through it,
/// then drives a raw WebSocket handshake against that project's `boards/live`
/// route carrying `origin` (when given). Returns the raw HTTP response text
/// so callers can assert on the status line. Shared by the pair of tests
/// below and `board_live_handshake_selects_the_tack_v1_subprotocol` above.
async fn board_live_handshake_response(app: axum::Router, origin: Option<&str>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test listener");
    let address = listener.local_addr().expect("test listener address");
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let project = reqwest::Client::new()
        .post(format!("http://{address}/api/projects"))
        .json(&serde_json::json!({ "name": "WS origin test", "project_type": "software" }))
        .send()
        .await
        .expect("create project over the real listener")
        .error_for_status()
        .expect("project creation must succeed")
        .json::<serde_json::Value>()
        .await
        .expect("project JSON");
    let project_id = project["id"].as_str().expect("project ID");

    let origin_line = origin
        .map(|o| format!("Origin: {o}\r\n"))
        .unwrap_or_default();
    let request = format!(
        "GET /api/projects/{project_id}/boards/live HTTP/1.1\r\n\
         Host: {address}\r\n\
         {origin_line}\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
         Sec-WebSocket-Version: 13\r\n\
         Sec-WebSocket-Protocol: tack.v1\r\n\r\n"
    );

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
    response
}

/// Origin-based authorization for the board's live handshake follows the
/// documented developer recipe (`tack serve` on its loopback default, then
/// `npm run dev` on `http://localhost:5173`, never listed in
/// `default_allowed_origins()`): a loopback-bound server recognizes any
/// loopback-hosted `Origin` as same-machine by address, not by string
/// prefix, and only on a loopback bind — never on `0.0.0.0`.
#[tokio::test]
async fn board_live_handshake_origin_authorization() {
    // (claim, origin, non_loopback_bind, authorized).
    let cases = [
        (
            "vite dev origin is authorized on a loopback bind",
            "http://localhost:5173",
            false,
            true,
        ),
        (
            "a hostname merely starting with 127. is not this machine",
            "http://127.attacker.example:5173",
            false,
            false,
        ),
        (
            "a bracketed IPv6 loopback literal is this machine",
            "http://[::1]:5173",
            false,
            true,
        ),
        (
            "a loopback origin gets no special treatment on a non-loopback bind",
            "http://localhost:5173",
            true,
            false,
        ),
    ];
    for (claim, origin, non_loopback_bind, authorized) in cases {
        let config = config_for_bind(non_loopback_bind);
        let (app, _) = common::test_app_with_config(config).await;
        let response = board_live_handshake_response(app, Some(origin)).await;
        assert_eq!(
            response.starts_with("HTTP/1.1 101"),
            authorized,
            "{claim}: {response}"
        );
    }
}
