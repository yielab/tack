//! III-C1 card-local HTTP tests. C5 owns global-router registration.

#[path = "../src/handlers/executions.rs"]
mod executions;
#[path = "../src/handlers/runner_admin.rs"]
mod runner_admin;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use chrono::Utc;
use tack_core::models::{CreateItem, CreateProject, ProjectType};
use tack_db::{
    Repository, init_pool, migrations,
    repo::execution::{NewRunner, SystemExecutionClock},
};
use tower::ServiceExt;
use uuid::Uuid;

async fn setup() -> (axum::Router, Repository, String) {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_all(&pool).await.expect("migrations");
    let repo = Repository::new(pool);
    let workspace = Uuid::new_v4();
    sqlx::query("INSERT INTO workspaces (id,name,default_vocabulary) VALUES (?, 'C1', '{}')")
        .bind(workspace.to_string())
        .execute(repo.pool())
        .await
        .expect("workspace");
    let project = repo
        .create_project(
            workspace,
            CreateProject {
                name: "C1".into(),
                description: None,
                project_type: ProjectType::Software,
                template: None,
            },
        )
        .await
        .expect("project");
    let item = repo
        .create_item(
            project.id,
            "To Do",
            CreateItem {
                title: "I".into(),
                description: None,
                item_type: None,
                parent_id: None,
                priority: None,
                estimate: None,
                estimate_unit: None,
                tags: None,
                due_date: None,
                sprint_id: None,
                assignee: None,
            },
        )
        .await
        .expect("item");
    let clock = SystemExecutionClock;
    repo.register_runner(
        NewRunner {
            id: "runner-active",
            name: "Active",
            credential_hash: "hash-only",
            labels: "{}",
            total_capacity: 1,
            available_capacity: 1,
            capability_snapshot: "{}",
            protocol_version: 1,
        },
        &clock,
    )
    .await
    .expect("runner");
    let state = executions::OperatorExecutionState::new(repo.clone());
    let app = executions::routes(state.clone()).merge(runner_admin::routes(state));
    (app, repo, item.id.to_string())
}

fn create_body(item_id: &str) -> String {
    serde_json::json!({"item_id":item_id,"idempotency_key":"same-key","selector_kind":"exact_runner","selector_id":"runner-active"}).to_string()
}
async fn send(
    app: &axum::Router,
    method: &str,
    uri: &str,
    body: String,
) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let value =
        serde_json::from_slice(&to_bytes(response.into_body(), 131072).await.unwrap()).unwrap();
    (status, value)
}

#[tokio::test]
async fn duplicate_create_replays_same_request_and_revoked_runner_is_rejected() {
    let (app, repo, item_id) = setup().await;
    let (first_status, first) = send(&app, "POST", "/executions", create_body(&item_id)).await;
    let (second_status, second) = send(&app, "POST", "/executions", create_body(&item_id)).await;
    assert_eq!(first_status, StatusCode::OK);
    assert_eq!(second_status, StatusCode::OK);
    assert_eq!(first["request_id"], second["request_id"]);
    assert_eq!(second["replayed"], true);

    sqlx::query("UPDATE agent_runners SET state='revoked', revoked_at=? WHERE id='runner-active'")
        .bind(Utc::now().to_rfc3339())
        .execute(repo.pool())
        .await
        .unwrap();
    let (status, body) = send(&app, "POST", "/executions", serde_json::json!({"item_id":item_id,"idempotency_key":"new-key","selector_kind":"exact_runner","selector_id":"runner-active"}).to_string()).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "runner_revoked");
}

#[tokio::test]
async fn cancellation_is_requested_not_terminal() {
    let (app, repo, item_id) = setup().await;
    let (_, created) = send(&app, "POST", "/executions", create_body(&item_id)).await;
    let request_id = created["request_id"].as_str().unwrap();
    let (status, body) = send(
        &app,
        "POST",
        &format!("/executions/{request_id}/cancel"),
        "{}".into(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["state"], "cancellation_requested");
    let state: String = sqlx::query_scalar("SELECT state FROM execution_requests WHERE id=?")
        .bind(request_id)
        .fetch_one(repo.pool())
        .await
        .unwrap();
    assert_eq!(state, "queued");
}

#[tokio::test]
async fn only_needs_operator_can_be_requeued_and_recovery_is_audited() {
    let (app, repo, item_id) = setup().await;
    let (_, created) = send(&app, "POST", "/executions", create_body(&item_id)).await;
    let request_id = created["request_id"].as_str().unwrap().to_owned();
    let now = Utc::now().to_rfc3339();
    sqlx::query("INSERT INTO execution_attempts (id,request_id,attempt_number,runner_id,fencing_token,state,lease_issued_at,lease_expires_at,created_at,updated_at) VALUES ('attempt-1',?,1,'runner-active',1,'lost',?,?,?,?)")
        .bind(&request_id).bind(&now).bind(&now).bind(&now).bind(&now).execute(repo.pool()).await.unwrap();
    let (denied, _) = send(
        &app,
        "POST",
        &format!("/executions/{request_id}/requeue"),
        r#"{"reason":"operator reviewed"}"#.into(),
    )
    .await;
    assert_eq!(denied, StatusCode::CONFLICT);
    sqlx::query("UPDATE execution_attempts SET state='needs_operator' WHERE id='attempt-1'")
        .execute(repo.pool())
        .await
        .unwrap();
    let (allowed, body) = send(
        &app,
        "POST",
        &format!("/executions/{request_id}/requeue"),
        r#"{"reason":"operator reviewed"}"#.into(),
    )
    .await;
    assert_eq!(allowed, StatusCode::OK);
    assert_eq!(body["state"], "queued");
    let audits: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM execution_events WHERE attempt_id='attempt-1' AND kind='requeue_confirmed'").fetch_one(repo.pool()).await.unwrap();
    assert_eq!(audits, 1);
}
