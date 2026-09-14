//! Every orchestration-gated route across dispatch, auto-dispatch and
//! reporting returns 409 `orchestration_disabled` while the effective
//! setting is off, regardless of which endpoint family exposes it.

use crate::common;

use axum::Router;
use axum::http::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

async fn off(app: &Router, method: &str, uri: &str, body: Value) {
    let (status, json) = common::send(app, method, uri, body, &[]).await;
    assert_eq!(status, StatusCode::CONFLICT, "{method} {uri}");
    assert_eq!(
        json["error"]["code"], "orchestration_disabled",
        "{method} {uri}"
    );
}

#[tokio::test]
async fn every_orch_gated_route_returns_disabled_when_off() {
    let (app, _) = common::test_app().await; // orch_enable defaults to false
    let f = Uuid::new_v4();
    let item_dispatch = format!("/api/items/{f}/dispatch");
    let project_dispatch = format!("/api/projects/{f}/orch-dispatch");
    let sprint_dispatch = format!("/api/sprints/{f}/dispatch");
    let sprint_dry_run = format!("/api/sprints/{f}/dispatch/dry-run");
    let item_activity = format!("/api/items/{f}/agent-activity");
    let project_activity = format!("/api/projects/{f}/agent-activity");
    let project_budget = format!("/api/projects/{f}/orch-budget");
    let project_policy = format!("/api/projects/{f}/orch-policy");
    let variables = json!({"variables": {}});
    let grant = json!({"action": "grant"});

    off(&app, "POST", &item_dispatch, Value::Null).await;
    off(&app, "POST", &project_dispatch, variables).await;
    off(&app, "POST", &sprint_dispatch, Value::Null).await;
    off(&app, "GET", &sprint_dry_run, Value::Null).await;
    off(&app, "GET", &item_activity, Value::Null).await;
    off(&app, "GET", &project_activity, Value::Null).await;
    off(&app, "GET", "/api/approvals", Value::Null).await;
    off(&app, "POST", "/api/approvals/apr-1", grant).await;
    off(&app, "GET", &project_budget, Value::Null).await;
    off(&app, "GET", &project_policy, Value::Null).await;
    off(&app, "GET", "/api/orch-runs/run-whatever", Value::Null).await;
}
