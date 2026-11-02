//! Threads over HTTP (ADR 0034, decision 7): the operator opens, lists, writes in and closes
//! them; an agent credential reaches a thread only with `converse` on its project, and one kept
//! for a credential is that credential's alone.

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
    profile: String,
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
    let (status, profile) = op(
        &app,
        "POST",
        "/api/agent-profiles",
        json!({"name": "Manager profile", "instructions": "help"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{profile}");
    Setup {
        app,
        project: project["id"].as_str().unwrap().to_owned(),
        profile: profile["agent_profile_id"].as_str().unwrap().to_owned(),
    }
}

async fn issue(app: &Router, name: &str, project: &str, capabilities: &[&str]) -> (String, String) {
    let (status, issued) = op(
        app,
        "POST",
        "/api/agent-credentials",
        json!({"name": name, "scopes": [{"projects": [project], "capabilities": capabilities}]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{issued}");
    (
        issued["secret"].as_str().unwrap().to_owned(),
        issued["credential"]["id"]
            .as_str()
            .or_else(|| issued["id"].as_str())
            .unwrap()
            .to_owned(),
    )
}

#[tokio::test]
async fn the_operator_opens_lists_writes_in_and_closes_a_thread() {
    let s = setup().await;
    let (status, refused) = op(
        &s.app,
        "POST",
        "/api/threads",
        json!({"project_id": s.project, "profile": "no-such-profile", "title": "t"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");

    let (status, thread) = op(
        &s.app,
        "POST",
        "/api/threads",
        json!({"project_id": s.project, "profile": s.profile, "title": "What blocks the launch?"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{thread}");
    let id = thread["id"].as_str().unwrap().to_owned();
    assert_eq!(thread["status"], "open");

    let (_, listed) = op(
        &s.app,
        "GET",
        &format!("/api/threads?project_id={}&open=true", s.project),
        json!(null),
    )
    .await;
    assert_eq!(listed.as_array().unwrap().len(), 1);

    // The project has no agent to run on, so the turn cannot start; the thread says so.
    let (status, posted) = op(
        &s.app,
        "POST",
        &format!("/api/threads/{id}/messages"),
        json!({"body": "hello"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{posted}");
    assert!(
        posted["message"]["principal"]
            .as_str()
            .unwrap()
            .starts_with("operator:")
    );
    assert!(posted["turn"].is_null());
    assert!(
        posted["notice"]["body"]
            .as_str()
            .unwrap()
            .contains("could not start")
    );

    let (_, detail) = op(&s.app, "GET", &format!("/api/threads/{id}"), json!(null)).await;
    assert_eq!(detail["messages"].as_array().unwrap().len(), 2);

    let (status, closed) = op(
        &s.app,
        "POST",
        &format!("/api/threads/{id}/close"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(closed["status"], "closed");
    let (status, _) = op(
        &s.app,
        "POST",
        &format!("/api/threads/{id}/messages"),
        json!({"body": "again"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn an_agent_needs_converse_on_the_threads_project() {
    let s = setup().await;
    let (_, thread) = op(
        &s.app,
        "POST",
        "/api/threads",
        json!({"project_id": s.project, "profile": s.profile, "title": "t"}),
    )
    .await;
    let id = thread["id"].as_str().unwrap();

    let (reader, _) = issue(&s.app, "Reader", &s.project, &["read", "propose"]).await;
    for (method, uri, body) in [
        ("GET", format!("/api/threads/{id}"), json!(null)),
        (
            "POST",
            format!("/api/threads/{id}/messages"),
            json!({"body": "hi"}),
        ),
    ] {
        let (status, refused) = agent(&s.app, &reader, method, &uri, body).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");
        assert!(refused.to_string().contains("`converse`"), "{refused}");
    }

    let (talker, _) = issue(&s.app, "Talker", &s.project, &["read", "converse"]).await;
    let (status, detail) = agent(
        &s.app,
        &talker,
        "GET",
        &format!("/api/threads/{id}"),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{detail}");
    let (status, posted) = agent(
        &s.app,
        &talker,
        "POST",
        &format!("/api/threads/{id}/messages"),
        json!({"body": "a report"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{posted}");
    assert!(
        posted["message"]["principal"]
            .as_str()
            .unwrap()
            .starts_with("agent:")
    );

    // Opening and closing stay the operator's.
    let (status, _) = agent(
        &s.app,
        &talker,
        "POST",
        "/api/threads",
        json!({"project_id": s.project, "profile": s.profile, "title": "x"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = agent(
        &s.app,
        &talker,
        "POST",
        &format!("/api/threads/{id}/close"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_thread_kept_for_one_credential_is_that_credentials_alone() {
    let s = setup().await;
    let (mine, mine_id) = issue(&s.app, "Mine", &s.project, &["read", "converse"]).await;
    let (other, _) = issue(&s.app, "Other", &s.project, &["read", "converse"]).await;
    let (status, thread) = op(
        &s.app,
        "POST",
        "/api/threads",
        json!({"project_id": s.project, "credential_id": mine_id, "profile": s.profile, "title": "t"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{thread}");
    let uri = format!("/api/threads/{}", thread["id"].as_str().unwrap());
    let (status, _) = agent(&s.app, &mine, "GET", &uri, json!(null)).await;
    assert_eq!(status, StatusCode::OK);
    let (status, refused) = agent(&s.app, &other, "GET", &uri, json!(null)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");
}
