//! Change sets over HTTP (ADR 0034, decisions 3 and 4): an agent with the scope `propose`
//! proposes; the project's rung decides what applies at once; a person lists, applies, edits and
//! rejects; and what is refused is refused whole with every reason.

use crate::common::{send, test_app_with_config};
use axum::Router;
use axum::http::StatusCode;
use serde_json::{Value, json};
use tack_api::config::AppConfig;

const OPERATOR: &str = "Bearer operator-token";

async fn op(app: &Router, method: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send(app, method, uri, body, &[("authorization", OPERATOR)]).await
}

async fn agent(
    app: &Router,
    secret: &str,
    method: &str,
    uri: &str,
    body: Value,
) -> (StatusCode, Value) {
    send(
        app,
        method,
        uri,
        body,
        &[("authorization", &format!("Bearer {secret}"))],
    )
    .await
}

struct Setup {
    app: Router,
    project: String,
    item: String,
}

async fn setup() -> Setup {
    let (app, _) = test_app_with_config(AppConfig {
        api_token: Some("operator-token".into()),
        ..AppConfig::default()
    })
    .await;
    let (_, project) = op(
        &app,
        "POST",
        "/api/projects",
        json!({"name": "My SaaS", "project_type": "software"}),
    )
    .await;
    let project = project["id"].as_str().unwrap().to_owned();
    let (_, item) = op(
        &app,
        "POST",
        &format!("/api/projects/{project}/items"),
        json!({"title": "Existing"}),
    )
    .await;
    let item = item["id"].as_str().unwrap().to_owned();
    Setup { app, project, item }
}

async fn issue(app: &Router, project: &str, capabilities: &[&str]) -> String {
    let (status, issued) = op(
        app,
        "POST",
        "/api/agent-credentials",
        json!({"name": "Planner", "scopes": [{"projects": [project], "capabilities": capabilities}]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{issued}");
    issued["secret"].as_str().unwrap().to_owned()
}

fn create(title: &str) -> Value {
    json!({"op": "create", "title": title, "acceptance": ["it works"]})
}

#[tokio::test]
async fn the_default_rung_makes_an_agents_change_set_wait_for_a_person() {
    let s = setup().await;
    let secret = issue(&s.app, &s.project, &["read", "act"]).await;
    let uri = format!("/api/projects/{}/change-sets", s.project);
    let (status, proposed) = agent(
        &s.app,
        &secret,
        "POST",
        &uri,
        json!({
            "reason": "The launch needs it",
            "idempotency_key": "k-1",
            "operations": [
                create("Write the docs"),
                {"op": "comment", "item": 0, "text": "start here"},
                {"op": "priority", "item": s.item, "priority": "high"},
            ],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{proposed}");
    assert_eq!(proposed["made"], true);
    assert_eq!(proposed["applied_now"], json!([]));
    assert_eq!(proposed["change_set"]["status"], "waiting");
    assert_eq!(proposed["change_set"]["principal_name"], "Planner");
    let id = proposed["change_set"]["id"].as_str().unwrap().to_owned();

    // The same key is the first answer again.
    let (_, again) = agent(
        &s.app,
        &secret,
        "POST",
        &uri,
        json!({"reason": "other", "idempotency_key": "k-1", "operations": [create("Else")]}),
    )
    .await;
    assert_eq!(again["made"], false);
    assert_eq!(again["change_set"]["id"], id.as_str());

    // The person finds it among the waiting ones, edits one operation and applies the lot.
    let (_, waiting) = op(&s.app, "GET", &format!("{uri}?status=waiting"), Value::Null).await;
    assert_eq!(waiting.as_array().unwrap().len(), 1);
    let (status, _) = op(
        &s.app,
        "PATCH",
        &format!("/api/change-sets/{id}/operations/0"),
        json!({"operation": create("Write the user guide")}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, applied) = op(
        &s.app,
        "POST",
        &format!("/api/change-sets/{id}/apply"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{applied}");
    assert_eq!(applied["status"], "applied");
    let made = applied["ops"][0]["result"]["item"].as_str().unwrap();
    let (_, tree) = op(
        &s.app,
        "GET",
        &format!("/api/projects/{}/items/tree", s.project),
        Value::Null,
    )
    .await;
    assert!(tree.to_string().contains("Write the user guide"), "{tree}");
    let (_, card) = op(&s.app, "GET", &format!("/api/items/{made}"), Value::Null).await;
    assert_eq!(card["item"]["created_by"], "Planner");
    let (_, existing) = op(
        &s.app,
        "GET",
        &format!("/api/items/{}", s.item),
        Value::Null,
    )
    .await;
    assert_eq!(existing["item"]["priority"], "high");
}

#[tokio::test]
async fn at_the_act_rung_the_rest_applies_at_once_and_creates_wait() {
    let s = setup().await;
    let (status, project) = op(
        &s.app,
        "PATCH",
        &format!("/api/projects/{}", s.project),
        json!({"agents_on_board": {"level": "act", "creates_per_change_set": 0, "creates_per_day": 0}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{project}");
    assert_eq!(project["agents_on_board"]["level"], "act");
    let secret = issue(&s.app, &s.project, &["act"]).await;
    let (status, proposed) = agent(
        &s.app,
        &secret,
        "POST",
        &format!("/api/projects/{}/change-sets", s.project),
        json!({
            "reason": "Triage",
            "operations": [
                create("A new thing"),
                {"op": "move", "item": s.item, "status": "To Do"},
            ],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{proposed}");
    assert_eq!(proposed["applied_now"], json!([1]));
    assert_eq!(proposed["change_set"]["status"], "partly");
    let (_, existing) = op(
        &s.app,
        "GET",
        &format!("/api/items/{}", s.item),
        Value::Null,
    )
    .await;
    assert_eq!(existing["item"]["status"], "To Do");
}

#[tokio::test]
async fn a_set_with_a_cycle_a_missing_status_and_a_full_column_is_refused_whole() {
    let s = setup().await;
    let (_, other) = op(
        &s.app,
        "POST",
        &format!("/api/projects/{}/items", s.project),
        json!({"title": "Other"}),
    )
    .await;
    let other = other["id"].as_str().unwrap();
    let secret = issue(&s.app, &s.project, &["propose"]).await;
    let (status, refused) = agent(
        &s.app,
        &secret,
        "POST",
        &format!("/api/projects/{}/change-sets", s.project),
        json!({
            "reason": "Two reasons",
            "operations": [
                {"op": "depend", "blocker": s.item, "blocked": other},
                {"op": "depend", "blocker": other, "blocked": s.item},
                {"op": "move", "item": s.item, "status": "Nowhere"},
            ],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
    let message = refused["error"]["message"].as_str().unwrap();
    assert!(message.contains("2 reasons"), "{message}");
    assert!(
        message.contains("loop") && message.contains("Nowhere"),
        "{message}"
    );
    let (_, listed) = op(
        &s.app,
        "GET",
        &format!("/api/projects/{}/change-sets", s.project),
        Value::Null,
    )
    .await;
    assert_eq!(listed, json!([]));
}

#[tokio::test]
async fn a_credential_without_propose_cannot_propose_and_none_may_decide() {
    let s = setup().await;
    let reader = issue(&s.app, &s.project, &["read"]).await;
    let uri = format!("/api/projects/{}/change-sets", s.project);
    let body = json!({"reason": "r", "operations": [create("x")]});
    let (status, refused) = agent(&s.app, &reader, "POST", &uri, body.clone()).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");
    assert!(
        refused["error"]["message"]
            .as_str()
            .unwrap()
            .contains("`propose`")
    );

    let proposer = issue_named(&s.app, &s.project, "Proposer", &["read", "propose"]).await;
    let (status, proposed) = agent(&s.app, &proposer, "POST", &uri, body).await;
    assert_eq!(status, StatusCode::OK, "{proposed}");
    let id = proposed["change_set"]["id"].as_str().unwrap();
    // It can read the change set it proposed, but not apply it, reject it or edit it.
    let (status, _) = agent(
        &s.app,
        &proposer,
        "GET",
        &format!("/api/change-sets/{id}"),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    for (method, uri, body) in [
        ("POST", format!("/api/change-sets/{id}/apply"), json!({})),
        (
            "POST",
            format!("/api/change-sets/{id}/reject"),
            json!({"reason": "x"}),
        ),
        (
            "PATCH",
            format!("/api/change-sets/{id}/operations/0"),
            json!({"operation": create("y")}),
        ),
    ] {
        let (status, _) = agent(&s.app, &proposer, method, &uri, body).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}");
    }

    let (status, rejected) = op(
        &s.app,
        "POST",
        &format!("/api/change-sets/{id}/reject"),
        json!({"reason": "Not now"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{rejected}");
    assert_eq!(rejected["status"], "rejected");
    assert_eq!(rejected["rejection"], "Not now");
}

async fn issue_named(app: &Router, project: &str, name: &str, capabilities: &[&str]) -> String {
    let (_, issued) = op(
        app,
        "POST",
        "/api/agent-credentials",
        json!({"name": name, "scopes": [{"projects": [project], "capabilities": capabilities}]}),
    )
    .await;
    issued["secret"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn an_agent_cannot_read_the_change_sets_of_a_project_it_cannot_see() {
    let s = setup().await;
    let (_, other) = op(
        &s.app,
        "POST",
        "/api/projects",
        json!({"name": "Other", "project_type": "software"}),
    )
    .await;
    let other = other["id"].as_str().unwrap().to_owned();
    let secret = issue(&s.app, &other, &["read", "propose"]).await;
    let (status, _) = agent(
        &s.app,
        &secret,
        "GET",
        &format!("/api/projects/{}/change-sets", s.project),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}
