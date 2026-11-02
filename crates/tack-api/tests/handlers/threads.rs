//! Threads over HTTP (ADR 0034, decision 7): the operator opens, lists, writes in and closes
//! them; an agent credential reaches a thread only with `converse` on its project, and one kept
//! for a credential is that credential's alone.

use crate::common::{send, test_app_with_config, with_default_host};
use axum::Router;
use axum::http::StatusCode;
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::{AppState, router::build_router};
use tack_db::repo::execution::NewRunner;
use tack_protocol::{Clock, SystemClock};

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
    with_projects_and_profile(app).await
}

async fn with_projects_and_profile(app: Router) -> Setup {
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

/// A thread in a project with code, a default harness and one active runner, so a person's
/// message starts a turn.
async fn runnable() -> (Setup, String) {
    let repo = tack_test_support::setup_test_db().await;
    let workspace = tack_test_support::create_test_workspace(&repo).await;
    repo.register_runner(
        NewRunner {
            id: "runner-1",
            name: "Runner",
            credential_hash: "hash",
            labels: "{}",
            total_capacity: 1,
            available_capacity: 1,
            capability_snapshot: "{}",
            protocol_version: 1,
        },
        SystemClock.now(),
    )
    .await
    .unwrap();
    let (tx, _rx) = tokio::sync::broadcast::channel(16);
    let state = AppState {
        repo,
        config: AppConfig {
            api_token: Some("operator-token".into()),
            database_url: "sqlite::memory:".into(),
            ..AppConfig::default()
        },
        workspace_id: workspace,
        broadcast_tx: tx,
        event_bus: Default::default(),
        webhook: None,
        local_runner: None,
        tasks: Default::default(),
    };
    let s = with_projects_and_profile(with_default_host(build_router(state))).await;
    let (status, patched) = op(
        &s.app,
        "PATCH",
        &format!("/api/projects/{}", s.project),
        json!({"default_harness": "codex", "code_origin": "url", "repository": "https://example.test/c1.git"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{patched}");
    let (_, thread) = op(
        &s.app,
        "POST",
        "/api/threads",
        json!({"project_id": s.project, "profile": s.profile, "title": "Report"}),
    )
    .await;
    let id = thread["id"].as_str().unwrap().to_owned();
    (s, id)
}

#[tokio::test]
async fn an_agents_message_is_posted_and_starts_no_turn() {
    let (s, id) = runnable().await;
    let (talker, _) = issue(&s.app, "Talker", &s.project, &["read", "converse"]).await;
    let uri = format!("/api/threads/{id}/messages");

    let (status, posted) = agent(&s.app, &talker, "POST", &uri, json!({"body": "an alert"})).await;
    assert_eq!(status, StatusCode::OK, "{posted}");
    assert!(posted["turn"].is_null(), "{posted}");
    assert!(posted["notice"].is_null(), "{posted}");

    let (_, detail) = op(&s.app, "GET", &format!("/api/threads/{id}"), json!(null)).await;
    assert_eq!(detail["messages"].as_array().unwrap().len(), 1, "{detail}");
    assert_eq!(detail["messages"][0]["body"], "an alert");
    assert!(detail["turns"].as_array().unwrap().is_empty(), "{detail}");
    assert_eq!(detail["thread"]["turns"], 0, "{detail}");

    // The thread is not busy: a second report is accepted, and so is the person's message,
    // which starts the one turn.
    let (status, again) = agent(&s.app, &talker, "POST", &uri, json!({"body": "another"})).await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert!(again["turn"].is_null(), "{again}");
    let (status, person) = op(&s.app, "POST", &uri, json!({"body": "what happened?"})).await;
    assert_eq!(status, StatusCode::OK, "{person}");
    assert!(!person["turn"].is_null(), "{person}");
    let (_, detail) = op(&s.app, "GET", &format!("/api/threads/{id}"), json!(null)).await;
    assert_eq!(detail["turns"].as_array().unwrap().len(), 1, "{detail}");
}

#[tokio::test]
async fn an_agents_message_is_accepted_while_a_turn_is_pending() {
    let (s, id) = runnable().await;
    let (talker, _) = issue(&s.app, "Talker", &s.project, &["read", "converse"]).await;
    let uri = format!("/api/threads/{id}/messages");

    let (_, person) = op(&s.app, "POST", &uri, json!({"body": "go"})).await;
    assert!(!person["turn"].is_null(), "{person}");
    let (status, refused) = op(&s.app, "POST", &uri, json!({"body": "again"})).await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");

    let (status, posted) = agent(&s.app, &talker, "POST", &uri, json!({"body": "an alert"})).await;
    assert_eq!(status, StatusCode::OK, "{posted}");
    assert!(posted["turn"].is_null(), "{posted}");
}
