//! The inbox over HTTP and over the live socket: what the list shows as notifications open,
//! settle and are read, and what `/live` tells while it happens. The rows are written through the
//! state's own `Inbox`, the way the loops that watch the board write them.

use std::net::SocketAddr;

use axum::http::StatusCode;
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_core::notification::{Key, Kind, Level, Notice};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{Duration, timeout};
use uuid::Uuid;

use crate::common;

fn notice(project: Uuid, kind: Kind, level: Level, key: Key) -> Notice {
    Notice {
        kind,
        level,
        key,
        project_id: project,
        item_id: None,
        request_id: None,
        title: "A run is asking a question".into(),
        cause: "runner:local".into(),
    }
}

fn question(project: Uuid, id: &str) -> Notice {
    notice(project, Kind::Decision, Level::NeedsYou, Key::decision(id))
}

fn finished(project: Uuid, id: &str) -> Notice {
    notice(project, Kind::RunFinished, Level::Info, Key::finished(id))
}

async fn get(app: &axum::Router, uri: &str) -> (StatusCode, Value) {
    common::send(app, "GET", uri, Value::Null, &[]).await
}

fn titles_and_open(list: &Value) -> (usize, u64) {
    (
        list["notifications"].as_array().unwrap().len(),
        list["open"].as_u64().unwrap(),
    )
}

#[tokio::test]
async fn listing_follows_a_notification_from_open_to_read_to_settled() {
    let (app, state, _) = common::test_app_with_state(AppConfig::default(), None).await;
    let project = common::create_project(&app, "Inbox", "software").await;
    let decision = state
        .notifications
        .open(&question(project, "d-1"))
        .await
        .unwrap()
        .unwrap();
    let info = state
        .notifications
        .open(&finished(project, "r-1"))
        .await
        .unwrap()
        .unwrap();

    // Open is the default: what asks something of the person, and an info nobody read.
    let (status, list) = get(&app, "/api/notifications").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(titles_and_open(&list), (2, 2));
    assert_eq!(list["notifications"][0]["kind"], "run_finished");
    assert_eq!(list["notifications"][1]["key"], "decision:d-1");

    // Reading an info settles it from the open list; reading twice, or an unknown id, changes
    // nothing and is not an error.
    for id in [info.id, info.id, Uuid::new_v4()] {
        let (status, _) = common::send(
            &app,
            "POST",
            &format!("/api/notifications/{id}/read"),
            Value::Null,
            &[],
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }
    let (_, list) = get(&app, "/api/notifications?state=open").await;
    assert_eq!(titles_and_open(&list), (1, 1));
    assert_eq!(list["notifications"][0]["id"], json!(decision.id));

    // `all` keeps what was read, with when.
    let (_, list) = get(&app, "/api/notifications?state=all").await;
    assert_eq!(titles_and_open(&list), (2, 1));
    let kept = &list["notifications"][0];
    assert_eq!(kept["id"], json!(info.id));
    assert!(kept["read_at"].is_string());

    // A needs-you stays open after it is read, until what it asks is answered.
    let (status, read) = common::send(
        &app,
        "POST",
        "/api/notifications/read-all",
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(read, json!({"read": 1}));
    let (_, list) = get(&app, "/api/notifications").await;
    assert_eq!(titles_and_open(&list), (1, 1));
    assert!(list["notifications"][0]["read_at"].is_string());

    assert!(
        state
            .notifications
            .close(&Key::decision("d-1"))
            .await
            .unwrap()
    );
    let (_, list) = get(&app, "/api/notifications").await;
    assert_eq!(titles_and_open(&list), (0, 0));
    let (_, list) = get(&app, "/api/notifications?state=all").await;
    assert_eq!(titles_and_open(&list), (2, 0));
    assert!(list["notifications"][1]["resolved_at"].is_string());
}

#[tokio::test]
async fn limit_cuts_the_list_but_not_the_count() {
    let (app, state, _) = common::test_app_with_state(AppConfig::default(), None).await;
    let project = common::create_project(&app, "Inbox", "software").await;
    for id in ["a", "b", "c"] {
        state
            .notifications
            .open(&question(project, id))
            .await
            .unwrap();
    }

    let (_, list) = get(&app, "/api/notifications?limit=2").await;
    assert_eq!(titles_and_open(&list), (2, 3));
    let (_, list) = get(&app, "/api/notifications?limit=0").await;
    assert_eq!(titles_and_open(&list), (0, 3));

    let (status, body) = get(&app, "/api/notifications?state=nothing").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (status, body) = get(&app, "/api/notifications?limit=many").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (status, body) = common::send(
        &app,
        "POST",
        "/api/notifications/not-an-id/read",
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

// ---------------------------------------------------------------------
// The live socket, driven over a raw connection: the test crate has no WebSocket client.
// ---------------------------------------------------------------------

/// A client after its handshake: the bytes read past it are kept for the frames.
struct Live {
    stream: TcpStream,
    pending: Vec<u8>,
}

impl Live {
    /// Sends the handshake `protocols` offers and returns the response head with the client.
    async fn connect(address: SocketAddr, protocols: &str) -> (String, Live) {
        let mut stream = TcpStream::connect(address).await.expect("connect");
        let request = format!(
            "GET /api/notifications/live HTTP/1.1\r\n\
             Host: {address}\r\n\
             Upgrade: websocket\r\n\
             Connection: Upgrade\r\n\
             Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
             Sec-WebSocket-Version: 13\r\n\
             Sec-WebSocket-Protocol: {protocols}\r\n\r\n"
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut live = Live {
            stream,
            pending: Vec::new(),
        };
        let end = loop {
            if let Some(end) = live.pending.windows(4).position(|w| w == b"\r\n\r\n") {
                break end + 4;
            }
            live.fill().await;
        };
        let head = String::from_utf8_lossy(&live.pending[..end]).into_owned();
        live.pending.drain(..end);
        (head, live)
    }

    async fn fill(&mut self) {
        let mut chunk = [0_u8; 4096];
        let count = timeout(Duration::from_secs(5), self.stream.read(&mut chunk))
            .await
            .expect("the socket said nothing in time")
            .expect("read");
        assert!(count > 0, "the socket closed");
        self.pending.extend_from_slice(&chunk[..count]);
    }

    /// The next text frame as JSON. The server's frames are never masked.
    async fn next(&mut self) -> Value {
        loop {
            if let Some((header, length)) = frame_size(&self.pending)
                && self.pending.len() >= header + length
            {
                assert_eq!(self.pending[0], 0x81, "a final text frame");
                let text = self.pending[header..header + length].to_vec();
                self.pending.drain(..header + length);
                return serde_json::from_slice(&text).expect("a JSON frame");
            }
            self.fill().await;
        }
    }
}

/// The header length and payload length of the frame at the start of `bytes`, once known.
fn frame_size(bytes: &[u8]) -> Option<(usize, usize)> {
    match *bytes.get(1)? & 0x7f {
        126 => Some((
            4,
            usize::from(*bytes.get(2)?) << 8 | usize::from(*bytes.get(3)?),
        )),
        127 => None,
        short => Some((2, usize::from(short))),
    }
}

async fn serve(app: axum::Router) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (address, server)
}

#[tokio::test]
async fn live_tells_the_count_on_connect_and_each_change_with_the_count_after_it() {
    let (app, state, _) = common::test_app_with_state(AppConfig::default(), None).await;
    let project = common::create_project(&app, "Inbox", "software").await;
    state
        .notifications
        .open(&question(project, "before"))
        .await
        .unwrap();
    let (address, server) = serve(app).await;

    let (head, mut live) = Live::connect(address, "tack.v1").await;
    assert!(head.starts_with("HTTP/1.1 101"), "{head}");
    assert!(
        head.to_ascii_lowercase()
            .contains("sec-websocket-protocol: tack.v1"),
        "{head}"
    );
    assert_eq!(live.next().await, json!({"open": 1}));

    let opened = state
        .notifications
        .open(&question(project, "d-1"))
        .await
        .unwrap()
        .unwrap();
    let told = live.next().await;
    assert_eq!(told["type"], "opened");
    assert_eq!(told["id"], json!(opened.id));
    assert_eq!(told["key"], "decision:d-1");
    assert_eq!(told["open"], 2);

    state
        .notifications
        .close(&Key::decision("d-1"))
        .await
        .unwrap();
    let told = live.next().await;
    assert_eq!(told["type"], "closed");
    assert_eq!(told["id"], json!(opened.id));
    assert_eq!(told["open"], 1);

    let info = state
        .notifications
        .open(&finished(project, "r-1"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(live.next().await["open"], 2);
    state.notifications.read(info.id).await.unwrap();
    let told = live.next().await;
    assert_eq!(told, json!({"type": "read", "id": info.id, "open": 1}));

    server.abort();
}

#[tokio::test]
async fn live_is_refused_without_the_token_and_accepted_with_it_in_the_subprotocol() {
    let token = "secret-token";
    let config = AppConfig {
        api_token: Some(token.into()),
        ..AppConfig::default()
    };
    let (app, _, _) = common::test_app_with_state(config, None).await;
    let (address, server) = serve(app).await;

    let (head, _) = Live::connect(address, "tack.v1").await;
    assert!(head.starts_with("HTTP/1.1 401"), "{head}");
    let (head, _) = Live::connect(address, "tack.v1, tack.auth.d3Jvbmc").await;
    assert!(head.starts_with("HTTP/1.1 401"), "{head}");

    // "secret-token" in unpadded base64url.
    let (head, mut live) = Live::connect(address, "tack.v1, tack.auth.c2VjcmV0LXRva2Vu").await;
    assert!(head.starts_with("HTTP/1.1 101"), "{head}");
    assert_eq!(live.next().await, json!({"open": 0}));

    server.abort();
}
