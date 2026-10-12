//! A raw body states its type, and an import is never trusted.
//!
//! Every route that reads a structured or binary body from raw bytes checks
//! the declared `Content-Type` before it parses anything, and an imported
//! item that claims the `manual` source is recorded as a JSON import instead.

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::common;

async fn post(app: &Router, uri: &str, content_type: &str, body: &str) -> StatusCode {
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(uri)
                .header("Content-Type", content_type)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let _ = to_bytes(res.into_body(), 1 << 20).await;
    status
}

/// A `text/plain` body is refused with 415 on every raw-body route, and the
/// route's own type gets past the check to its handler (any status but 415).
#[tokio::test]
async fn raw_body_routes_refuse_text_plain_with_415() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    let routes: Vec<(String, &str, &str)> = vec![
        (
            "/api/projects/outline".into(),
            "application/json",
            r#"{"project":{"name":"Outline"},"items":[{"title":"A"}]}"#,
        ),
        (
            format!("/api/projects/{pid}/outline"),
            "application/json",
            r#"{"items":[{"title":"B"}]}"#,
        ),
        ("/api/projects/import".into(), "application/json", "{}"),
        (
            "/api/projects/import-csv?name=x".into(),
            "text/csv",
            "title\nA\n",
        ),
        (
            format!("/api/projects/{pid}/import-csv"),
            "text/csv",
            "title\nA\n",
        ),
        (
            "/api/restore".into(),
            "application/octet-stream",
            "not a database",
        ),
    ];
    for (uri, correct, body) in routes {
        let refused = post(&app, &uri, "text/plain", body).await;
        assert_eq!(
            refused,
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "{uri} text/plain"
        );
        let accepted = post(&app, &uri, correct, body).await;
        assert_ne!(
            accepted,
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "{uri} {correct}"
        );
    }
}

/// An import whose item claims `"source": "manual"` reads back as
/// `json_import`: a payload cannot make its own items trusted.
#[tokio::test]
async fn import_payload_manual_source_lands_as_json_import() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "Source", "software").await;
    common::create_item(&app, pid, "Claimed trusted").await;
    let (status, export) = common::send(
        &app,
        "GET",
        &format!("/api/projects/{pid}/export?format=json"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(export["items"][0]["source"], "manual", "{export}");

    let (fresh, _) = common::test_app().await;
    let (status, out) = common::send(&fresh, "POST", "/api/projects/import", export, &[]).await;
    assert_eq!(status, StatusCode::OK, "{out}");
    let new_pid = out["project"]["id"].as_str().unwrap();
    let (_, items) = common::send(
        &fresh,
        "GET",
        &format!("/api/projects/{new_pid}/items"),
        Value::Null,
        &[],
    )
    .await;
    let new_iid = items["data"][0]["id"].as_str().unwrap();
    let (status, got) = common::send(
        &fresh,
        "GET",
        &format!("/api/items/{new_iid}"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{got}");
    assert_eq!(got["item"]["source"], json!("json_import"), "{got}");
}
