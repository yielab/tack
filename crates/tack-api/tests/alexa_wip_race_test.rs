//! Test for card R3: `handlers::alexa::
//! complete_task` — the voice "mark done" path — used to do the exact same
//! unguarded `count_items_by_status` read followed by an unlocked
//! `Repository::update_item` write that card R2 fixed on the dispatch path
//! (`crates/tack-api/tests/wip_limit_race_test.rs`) and this card fixed on
//! the board-drag path (`crates/tack-api/tests/board_drag_wip_race_test.rs`).
//!
//! Two concurrent Alexa "mark done" requests completing *different* items
//! into the same WIP-limited "Done" column could each observe "under the
//! limit" and both commit. This drives `N` genuinely concurrent
//! `CompleteTaskIntent` requests — `N` distinct items, all open, all
//! resolved by a unique title — through the real `POST /api/alexa` HTTP path
//! and asserts the "Done" column's final count never exceeds its configured
//! limit.
//!
//! No preset workflow ships with a WIP limit on its Done column (by design —
//! see `tack-core/src/workflow.rs`'s presets), but nothing stops an operator
//! from configuring one, and `handlers::alexa` has its own dedicated
//! `msg_wip_limit` spoken response specifically for this case — so this test
//! configures a small custom workflow with `wip_limit: 5` on "Done" via
//! `PATCH /api/projects/{id}`.

mod common;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tower::ServiceExt;

const SKILL_ID: &str = "amzn1.ask.skill.test-0000";

fn alexa_config() -> AppConfig {
    AppConfig {
        alexa_skill_id: Some(SKILL_ID.into()),
        ..AppConfig::default()
    }
}

fn envelope(request: Value) -> Value {
    let mut request = request;
    request["requestId"] = json!("EdwRequestId.test");
    request["timestamp"] = json!(chrono::Utc::now().to_rfc3339());
    json!({
        "version": "1.0",
        "session": { "application": { "applicationId": SKILL_ID } },
        "context": { "System": { "application": { "applicationId": SKILL_ID } } },
        "request": request,
    })
}

fn complete_task_request(title: &str) -> Value {
    envelope(json!({
        "type": "IntentRequest",
        "intent": {
            "name": "CompleteTaskIntent",
            "slots": { "title": { "name": "title", "value": title } },
        },
    }))
}

async fn req(app: &Router, method: Method, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    let body = match body {
        Some(v) => {
            builder = builder.header("content-type", "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    let res = app
        .clone()
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), 64 * 1024)
        .await
        .unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

/// A minimal custom workflow: "Todo" (initial) -> "Done", with a WIP limit
/// of `limit` on "Done" and no explicit-transition restriction, so any open
/// item can be completed directly.
fn custom_workflow_with_done_wip_limit(limit: usize) -> Value {
    json!({
        "workflow_type": "custom",
        "statuses": [
            {"name": "Todo", "category": "todo", "wip_limit": null, "order": 0},
            {"name": "Done", "category": "done", "wip_limit": limit, "order": 1},
        ],
        "transitions": null,
    })
}

async fn create_project_with_wip_limited_done(app: &Router, limit: usize) -> String {
    let (status, body) = req(
        app,
        Method::POST,
        "/api/projects",
        Some(json!({"name": "Alexa WIP Race Test", "project_type": "software"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    let project_id = body["id"].as_str().unwrap().to_string();

    let (status, body) = req(
        app,
        Method::PATCH,
        &format!("/api/projects/{project_id}"),
        Some(json!({"workflow": custom_workflow_with_done_wip_limit(limit)})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");

    project_id
}

async fn create_item(app: &Router, project_id: &str, title: &str) {
    let (status, body) = req(
        app,
        Method::POST,
        &format!("/api/projects/{project_id}/items"),
        Some(json!({"title": title})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
}

async fn count_items_in_status(app: &Router, project_id: &str, status: &str) -> usize {
    let (code, body) = req(
        app,
        Method::GET,
        &format!("/api/projects/{project_id}/items?per_page=100"),
        None,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{body:?}");
    body["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["status"] == status)
        .count()
}

/// `N` distinct items, all open ("Todo"), each completed by its own
/// concurrent `CompleteTaskIntent` request into the same WIP-limited "Done"
/// column (limit 5). However many of the `N` requests win the race, the
/// column must never end up holding more than its configured limit.
#[tokio::test(flavor = "multi_thread", worker_threads = 16)]
async fn concurrent_alexa_completions_into_the_same_wip_limited_column_never_exceed_the_limit() {
    const N: usize = 12;
    const WIP_LIMIT: usize = 5;

    let (app, _) = common::test_app_with_config(alexa_config()).await;
    let project_id = create_project_with_wip_limited_done(&app, WIP_LIMIT).await;

    let titles: Vec<String> = (0..N).map(|i| format!("alexa item {i}")).collect();
    for t in &titles {
        create_item(&app, &project_id, t).await;
    }

    let handles: Vec<_> = titles
        .iter()
        .cloned()
        .map(|title| {
            let app = app.clone();
            tokio::spawn(async move {
                req(
                    &app,
                    Method::POST,
                    "/api/alexa",
                    Some(complete_task_request(&title)),
                )
                .await
                .0
            })
        })
        .collect();

    for h in handles {
        // Alexa always answers 200 + spoken text, even for a rejected WIP
        // limit or an invalid transition (TODO.md: "User-level problems ...
        // are answered with HTTP 200 + spoken text").
        assert_eq!(h.await.expect("alexa task panicked"), StatusCode::OK);
    }

    let done_count = count_items_in_status(&app, &project_id, "Done").await;
    assert!(
        done_count <= WIP_LIMIT,
        "WIP limit for 'Done' is {WIP_LIMIT}, but {done_count} of {N} concurrently Alexa-\
         completed items ended up there — handlers::alexa::complete_task's WIP-limit check and \
         its status write are not atomic against concurrent writers into the same column"
    );

    let todo_count = count_items_in_status(&app, &project_id, "Todo").await;
    assert_eq!(
        done_count + todo_count,
        N,
        "every item must be in exactly one of Todo/Done — none lost, none duplicated"
    );
}
