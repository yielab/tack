//! `GET /api/notifications/channels`: one line per configured channel, with how many deliveries
//! wait, how many were given up on and why. No channel configured, no line.

use crate::common;
use axum::http::StatusCode;
use chrono::Utc;
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::{AppState, router::build_router};
use tack_core::notification::{Key, Kind, Level, Notice};
use tack_db::Repository;

const TOKEN: &str = "channels-token";

async fn app(webhook_url: Option<&str>) -> (axum::Router, Repository, uuid::Uuid) {
    let repo = tack_test_support::setup_test_db().await;
    let workspace = tack_test_support::create_test_workspace(&repo).await;
    let project = tack_test_support::make_project(&repo, workspace).await.id;
    let mut config = AppConfig {
        api_token: Some(TOKEN.into()),
        database_url: "sqlite::memory:".into(),
        ..AppConfig::default()
    };
    config.loops.webhook_url = webhook_url.map(str::to_owned);
    let (tx, _rx) = tokio::sync::broadcast::channel(16);
    let state = AppState {
        repo: repo.clone(),
        config,
        workspace_id: workspace,
        broadcast_tx: tx,
        event_bus: Default::default(),
        notifications: common::inbox(&repo),
        local_runner: None,
        tasks: Default::default(),
    };
    (
        common::with_default_host(build_router(state)),
        repo,
        project,
    )
}

async fn channels(app: &axum::Router) -> Value {
    let (status, body) = common::send(
        app,
        "GET",
        "/api/notifications/channels",
        Value::Null,
        &[("authorization", &format!("Bearer {TOKEN}"))],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

#[tokio::test]
async fn no_channel_is_configured_so_none_is_listed() {
    let (app, _, _) = app(None).await;
    assert_eq!(channels(&app).await, json!([]));
}

#[tokio::test]
async fn a_webhook_with_nothing_queued_reports_zeros() {
    let (app, _, _) = app(Some("https://hook.example.test/tack")).await;
    assert_eq!(
        channels(&app).await,
        json!([{"name": "webhook", "pending": 0, "failed": 0, "last_error": null}])
    );
}

#[tokio::test]
async fn the_line_counts_what_waits_what_gave_up_and_says_why() {
    let (app, repo, project) = app(Some("https://hook.example.test/tack")).await;
    let now = Utc::now();
    let mut ids = Vec::new();
    for id in ["d-1", "d-2"] {
        let notice = Notice {
            kind: Kind::Decision,
            level: Level::NeedsYou,
            key: Key::decision(id),
            project_id: project,
            item_id: None,
            request_id: None,
            title: "A run is asking a question".into(),
            cause: "runner:local".into(),
        };
        let opened = repo
            .insert_open_notification(&notice, &["webhook".to_owned()], now)
            .await
            .unwrap()
            .unwrap();
        ids.push(opened.id);
    }
    repo.delivery_failed(
        ids[0],
        "webhook",
        None,
        "the webhook answered 404 Not Found",
    )
    .await
    .unwrap();

    assert_eq!(
        channels(&app).await,
        json!([{
            "name": "webhook",
            "pending": 1,
            "failed": 1,
            "last_error": "the webhook answered 404 Not Found",
        }])
    );
}

#[tokio::test]
async fn a_wrong_token_is_refused() {
    let (app, _, _) = app(None).await;
    let (status, _) = common::send(
        &app,
        "GET",
        "/api/notifications/channels",
        Value::Null,
        &[("authorization", "Bearer not-the-token")],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
