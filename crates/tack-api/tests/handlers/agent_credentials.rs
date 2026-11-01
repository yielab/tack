//! An agent credential is a principal with scopes (ADR 0034, decision 2): it reads only its
//! projects, cannot resolve a decision or reach an operator route, signs its writes with its
//! name, and is refused once rotated, revoked or expired. The operator's token is unchanged.

use crate::common::{send, test_app_with_config};
use axum::Router;
use axum::http::StatusCode;
use serde_json::{Value, json};
use tack_api::config::AppConfig;

const OPERATOR: &str = "Bearer operator-token";

struct Setup {
    app: Router,
    mine: String,
    other: String,
    item_in_mine: String,
    item_in_other: String,
}

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

async fn setup() -> Setup {
    let (app, _) = test_app_with_config(AppConfig {
        api_token: Some("operator-token".into()),
        ..AppConfig::default()
    })
    .await;
    let mut ids = Vec::new();
    for name in ["Mine", "Other"] {
        let (status, project) = op(
            &app,
            "POST",
            "/api/projects",
            json!({"name": name, "project_type": "software"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{project}");
        let project_id = project["id"].as_str().unwrap().to_owned();
        let (status, item) = op(
            &app,
            "POST",
            &format!("/api/projects/{project_id}/items"),
            json!({"title": format!("Task of {name}")}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{item}");
        ids.push((project_id, item["id"].as_str().unwrap().to_owned()));
    }
    let (other, item_in_other) = ids.pop().unwrap();
    let (mine, item_in_mine) = ids.pop().unwrap();
    Setup {
        app,
        mine,
        other,
        item_in_mine,
        item_in_other,
    }
}

async fn issue(
    app: &Router,
    name: &str,
    projects: &[&str],
    capabilities: &[&str],
) -> (String, String) {
    let (status, issued) = op(
        app,
        "POST",
        "/api/agent-credentials",
        json!({"name": name, "scopes": [{"projects": projects, "capabilities": capabilities}]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{issued}");
    (
        issued["credential"]["id"].as_str().unwrap().to_owned(),
        issued["secret"].as_str().unwrap().to_owned(),
    )
}

#[tokio::test]
async fn an_agent_credential_reads_only_its_projects() {
    let s = setup().await;
    let (_, secret) = issue(&s.app, "Reviewer", &[&s.mine], &["read"]).await;

    let (status, projects) = agent(&s.app, &secret, "GET", "/api/projects", Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    let ids: Vec<&str> = projects
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec![s.mine.as_str()]);

    let (status, _) = agent(
        &s.app,
        &secret,
        "GET",
        &format!("/api/projects/{}/items", s.mine),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, item) = agent(
        &s.app,
        &secret,
        "GET",
        &format!("/api/items/{}", s.item_in_mine),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(item["item"]["title"], "Task of Mine");

    // The other project: 403 naming the scope it lacks, for the project and for its item.
    let (status, body) = agent(
        &s.app,
        &secret,
        "GET",
        &format!("/api/projects/{}/items", s.other),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let message = body["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("`read`") && message.contains(&s.other),
        "{message}"
    );
    let (status, _) = agent(
        &s.app,
        &secret,
        "GET",
        &format!("/api/items/{}", s.item_in_other),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // An item that does not exist answers the same, so the answer does not tell which.
    let (status, _) = agent(
        &s.app,
        &secret,
        "GET",
        &format!("/api/items/{}", uuid::Uuid::new_v4()),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn an_agent_changes_the_board_only_through_a_change_set_and_signs_it() {
    let s = setup().await;
    let (_, reader) = issue(&s.app, "Reader", &[&s.mine], &["read"]).await;
    let sets = format!("/api/projects/{}/change-sets", s.mine);
    let proposal = json!({
        "reason": "Triage",
        "operations": [{"op": "priority", "item": s.item_in_mine, "priority": "high"}],
    });
    let (status, body) = agent(&s.app, &reader, "POST", &sets, proposal.clone()).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("`propose`")
    );

    let (_, secret) = issue(&s.app, "Planner bot", &[&s.mine], &["read", "act"]).await;

    // Every direct write answers 403 and names the change-set route, even with `act`.
    for (method, uri, body) in [
        (
            "POST",
            format!("/api/items/{}/comments", s.item_in_mine),
            json!({"content": "looks done", "author": "Santiago"}),
        ),
        (
            "POST",
            format!("/api/projects/{}/items", s.mine),
            json!({"title": "From the agent"}),
        ),
        (
            "PATCH",
            format!("/api/items/{}", s.item_in_mine),
            json!({"priority": "high"}),
        ),
        (
            "POST",
            format!("/api/items/{}/dependencies", s.item_in_mine),
            json!({"target_item_id": s.item_in_other, "dependency_type": "blocks"}),
        ),
    ] {
        let (status, refused) = agent(&s.app, &secret, method, &uri, body).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}: {refused}");
        assert!(
            refused["error"]["message"]
                .as_str()
                .unwrap()
                .contains("change-sets"),
            "{refused}"
        );
    }

    // At the Act rung the same changes go through a change set and carry the credential's name.
    let (status, _) = op(
        &s.app,
        "PATCH",
        &format!("/api/projects/{}", s.mine),
        json!({"agents_on_board": {"level": "act", "creates_per_change_set": 0, "creates_per_day": 0}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, second) = op(
        &s.app,
        "POST",
        &format!("/api/projects/{}/items", s.mine),
        json!({"title": "Second"}),
    )
    .await;
    let (status, proposed) = agent(
        &s.app,
        &secret,
        "POST",
        &sets,
        json!({
            "reason": "Triage",
            "operations": [
                {"op": "priority", "item": s.item_in_mine, "priority": "high"},
                {"op": "comment", "item": s.item_in_mine, "text": "looks done"},
                {"op": "depend", "blocker": s.item_in_mine, "blocked": second["id"]},
            ],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{proposed}");
    assert_eq!(proposed["applied_now"], json!([0, 1, 2]));
    let (_, read_back) = agent(
        &s.app,
        &secret,
        "GET",
        &format!("/api/items/{}", s.item_in_mine),
        Value::Null,
    )
    .await;
    assert_eq!(read_back["item"]["updated_by"], "Planner bot");
    let (_, comments) = agent(
        &s.app,
        &secret,
        "GET",
        &format!("/api/items/{}/comments", s.item_in_mine),
        Value::Null,
    )
    .await;
    assert_eq!(comments[0]["author"], "Planner bot");
    assert!(
        comments[0]["principal"]
            .as_str()
            .unwrap()
            .starts_with("agent:")
    );
    let (_, deps) = agent(
        &s.app,
        &secret,
        "GET",
        &format!("/api/items/{}/dependencies", s.item_in_mine),
        Value::Null,
    )
    .await;
    assert_eq!(deps[0]["created_by"], "Planner bot", "{deps}");

    // A change set cannot reach a project outside the credential's scope.
    let (status, _) = agent(
        &s.app,
        &secret,
        "POST",
        &format!("/api/projects/{}/change-sets", s.other),
        proposal,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // The operator's own comment keeps the author it typed, and is recorded as the operator.
    let (status, theirs) = op(
        &s.app,
        "POST",
        &format!("/api/items/{}/comments", s.item_in_mine),
        json!({"content": "agreed", "author": "Santiago"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(theirs["author"], "Santiago");
    assert!(
        theirs["principal"]
            .as_str()
            .unwrap()
            .starts_with("operator:")
    );
}

#[tokio::test]
async fn an_agent_cannot_resolve_a_decision_or_reach_an_operator_route() {
    let s = setup().await;
    let (_, secret) = issue(
        &s.app,
        "Everything",
        &[&s.mine, &s.other],
        &["read", "propose", "act", "converse"],
    )
    .await;
    for (method, uri) in [
        (
            "POST",
            "/api/attempts/att_1/decisions/dec_1/resolve".to_string(),
        ),
        (
            "POST",
            "/api/attempts/att_1/decisions/dec_1/viewed".to_string(),
        ),
        ("POST", "/api/executions".to_string()),
        ("GET", "/api/executions".to_string()),
        ("GET", "/api/agent-credentials".to_string()),
        ("POST", "/api/agent-credentials".to_string()),
        ("DELETE", format!("/api/projects/{}", s.mine)),
        ("DELETE", format!("/api/items/{}", s.item_in_mine)),
        ("GET", "/api/backup".to_string()),
    ] {
        let (status, body) = agent(
            &s.app,
            &secret,
            method,
            &uri,
            json!({"name": "x", "scopes": []}),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}: {body}");
    }
}

#[tokio::test]
async fn the_operators_token_behaves_as_before() {
    let s = setup().await;
    let (status, projects) = op(&s.app, "GET", "/api/projects", Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(projects.as_array().unwrap().len(), 2);
    let (status, _) = op(&s.app, "GET", "/api/agent-credentials", Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = send(&s.app, "GET", "/api/projects", Value::Null, &[]).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = send(
        &s.app,
        "GET",
        "/api/projects",
        Value::Null,
        &[("authorization", "Bearer wrong")],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn rotate_replaces_the_secret_and_revoke_ends_the_credential() {
    let s = setup().await;
    let (id, secret) = issue(&s.app, "Rotating", &[&s.mine], &["read"]).await;
    let (status, _) = agent(&s.app, &secret, "GET", "/api/projects", Value::Null).await;
    assert_eq!(status, StatusCode::OK);

    let (status, rotated) = op(
        &s.app,
        "POST",
        &format!("/api/agent-credentials/{id}/rotate"),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{rotated}");
    let fresh = rotated["secret"].as_str().unwrap().to_owned();
    assert_ne!(fresh, secret);
    let (status, _) = agent(&s.app, &secret, "GET", "/api/projects", Value::Null).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = agent(&s.app, &fresh, "GET", "/api/projects", Value::Null).await;
    assert_eq!(status, StatusCode::OK);

    let (status, revoked) = op(
        &s.app,
        "POST",
        &format!("/api/agent-credentials/{id}/revoke"),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(revoked["revoked_at"].is_string());
    let (status, _) = agent(&s.app, &fresh, "GET", "/api/projects", Value::Null).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = op(
        &s.app,
        "POST",
        &format!("/api/agent-credentials/{id}/rotate"),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // The list never shows a secret or its hash, and shows the last use.
    let (_, list) = op(&s.app, "GET", "/api/agent-credentials", Value::Null).await;
    let text = list.to_string();
    assert!(
        !text.contains("tack_agent_") && !text.contains("secret"),
        "{text}"
    );
    assert_eq!(list[0]["name"], "Rotating");
}

#[tokio::test]
async fn issuing_refuses_a_bad_name_scope_or_project_and_keeps_names_unique() {
    let s = setup().await;
    for body in [
        json!({"name": "", "scopes": [{"projects": [s.mine], "capabilities": ["read"]}]}),
        json!({"name": "Operator", "scopes": [{"projects": [s.mine], "capabilities": ["read"]}]}),
        json!({"name": "A", "scopes": []}),
        json!({"name": "A", "scopes": [{"projects": [], "capabilities": ["read"]}]}),
        json!({"name": "A", "scopes": [{"projects": [s.mine], "capabilities": []}]}),
        json!({"name": "A", "scopes": [{"projects": [uuid::Uuid::new_v4()], "capabilities": ["read"]}]}),
    ] {
        let (status, answer) = op(&s.app, "POST", "/api/agent-credentials", body.clone()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}: {answer}");
    }
    let (id, _) = issue(&s.app, "Same", &[&s.mine], &["read"]).await;
    let (status, _) = op(
        &s.app,
        "POST",
        "/api/agent-credentials",
        json!({"name": "Same", "scopes": [{"projects": [s.mine], "capabilities": ["read"]}]}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    // A revoked name is free again.
    op(
        &s.app,
        "POST",
        &format!("/api/agent-credentials/{id}/revoke"),
        Value::Null,
    )
    .await;
    issue(&s.app, "Same", &[&s.mine], &["read"]).await;
}

#[tokio::test]
async fn a_credential_for_every_project_reaches_a_project_made_later() {
    let s = setup().await;
    let (status, issued) = op(
        &s.app,
        "POST",
        "/api/agent-credentials",
        json!({"name": "All", "scopes": [{"all_projects": true, "capabilities": ["read"]}]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{issued}");
    let secret = issued["secret"].as_str().unwrap().to_owned();
    let (_, later) = op(
        &s.app,
        "POST",
        "/api/projects",
        json!({"name": "Later", "project_type": "software"}),
    )
    .await;
    let (status, projects) = agent(&s.app, &secret, "GET", "/api/projects", Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(projects.as_array().unwrap().len(), 3);
    let (status, _) = agent(
        &s.app,
        &secret,
        "GET",
        &format!("/api/projects/{}", later["id"].as_str().unwrap()),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}
