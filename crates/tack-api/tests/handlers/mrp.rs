//! HTTP tests for `GET /api/executions/{id}/attempts/{n}/mrp` and its two POSTs:
//! the pack is uploaded through the real runner artifact route with the MRP
//! media type, then read back parsed; the first review is the record; accepting
//! moves the item only under the `done_on_mrp_accepted` policy.

use crate::common;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tack_api::config::AppConfig;
use tack_api::{AppState, router::build_router};
use tack_db::{Repository, init_pool, migrations};
use tower::ServiceExt;
use uuid::Uuid;

const OPERATOR_TOKEN: &str = "e2-mrp-operator-token";
const BASE_REVISION: &str = "0123456789abcdef0123456789abcdef01234567";
const MRP_MEDIA_TYPE: &str = "application/vnd.tack.mrp+json";
const READY: &str = include_str!("../../../../docs/contracts/mrp-v1/fixtures/ready.json");

async fn setup(storage_root: &std::path::Path) -> (axum::Router, Repository, String) {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_all(&pool).await.expect("migrations");
    let workspace_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'E2 mrp review', '{}')",
    )
    .bind(workspace_id.to_string())
    .execute(&pool)
    .await
    .expect("workspace");
    let repo = Repository::new(pool);
    let (tx, _rx) = tokio::sync::broadcast::channel(16);
    let state = AppState {
        repo: repo.clone(),
        config: AppConfig {
            api_token: Some(OPERATOR_TOKEN.into()),
            database_url: "sqlite::memory:".into(),
            storage_dir: storage_root.to_string_lossy().into_owned(),
            ..AppConfig::default()
        },
        workspace_id,
        broadcast_tx: tx,
        webhook: None,
        local_runner: None,
    };
    let app = build_router(state);
    let project = repo
        .create_project(
            workspace_id,
            tack_core::models::CreateProject {
                name: "E2 mrp review".into(),
                description: None,
                project_type: tack_core::models::ProjectType::Software,
                template: None,
            },
        )
        .await
        .expect("project");
    let item = repo
        .create_item(
            project.id,
            "To Do",
            tack_core::models::CreateItem {
                title: "Review the merge-readiness pack".into(),
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
    (app, repo, item.id.to_string())
}

fn operator_headers() -> Vec<(&'static str, &'static str)> {
    vec![("authorization", "Bearer e2-mrp-operator-token")]
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn full_capabilities() -> Value {
    let now = chrono::Utc::now().to_rfc3339();
    json!({
        "reported_at": now,
        "labels": {"os": "linux"},
        "concurrency": {"total": 1, "available": 1},
        "harnesses": [{
            "harness_kind": "codex",
            "installed_version": "1.0.0",
            "probe_error": null,
            "probed_at": now,
            "model_combinations": [{
                "model_provider": "openai",
                "model_ids": ["opaque/model-e2"],
                "discovery": "reported"
            }],
        }],
        "features": {},
        "limits": {"event_payload_bytes_max": 65536, "artifact_content_bytes_max": 52428800},
    })
}

/// Enrolls a runner and returns (runner_id, bearer-auth-header-pair).
async fn enroll_runner(app: &axum::Router, name: &str) -> (String, [(String, String); 1]) {
    let (status, pending, _) = common::send_with_raw(
        app,
        "POST",
        "/api/runners/enrollment",
        json!({"name": name, "total_capacity": 1, "available_capacity": 1}),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{pending}");
    let runner_id = pending["runner_id"].as_str().unwrap().to_owned();
    let raw_token = pending["enrollment_token"].as_str().unwrap().to_owned();

    let (status, enrolled, _) = common::send_with_raw(
        app,
        "POST",
        "/api/runner/v1/enroll",
        json!({
            "protocol_version": 1,
            "enrollment_token": raw_token,
            "runner_name": name,
            "runner_version": "0.1.0",
            "capabilities": full_capabilities(),
        }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{enrolled}");
    let credential = enrolled["runner_credential"].as_str().unwrap().to_owned();
    (
        runner_id,
        [("authorization".to_string(), bearer(&credential))],
    )
}

fn headers_ref(owned: &[(String, String); 1]) -> Vec<(&str, &str)> {
    owned
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect()
}

/// `label` keeps the profile name unique per call, matching
/// `attempt_lists.rs`'s own helper — a test that stands up two separate
/// executions calls this twice.
async fn create_agent_profile(app: &axum::Router, label: &str) -> String {
    let (status, profile, _) = common::send_with_raw(
        app,
        "POST",
        "/api/agent-profiles",
        json!({"name": format!("E2 {label} profile"), "instructions": "work safely"}),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{profile}");
    profile["agent_profile_id"].as_str().unwrap().to_owned()
}

fn execution_request_body(
    item_id: &str,
    key: &str,
    runner_id: &str,
    agent_profile_id: &str,
    policy: Option<&str>,
) -> Value {
    json!({
        "item_id": item_id,
        "idempotency_key": key,
        "selector_kind": "exact_runner",
        "selector_id": runner_id,
        "agent_profile_id": agent_profile_id,
        "requested_harness_kind": "codex",
        "requested_model_provider": "openai",
        "requested_model_id": "opaque/model-e2",
        "agent_profile_snapshot": {"name": "profile", "instructions": "work safely", "tool_policy": {}, "timeout_seconds": 60, "budgets": {}},
        "repository_snapshot": {"kind": "git", "remote": "https://example.test/e2.git", "base_revision": BASE_REVISION, "subdirectory": null},
        "permission_policy": {"tools": ["shell"], "network": false},
        "timeout_seconds": 60,
        "budgets": {},
        "environment": {},
        "metadata": {},
        "status_map_policy_id": policy,
    })
}

/// One real, claimed attempt with everything a caller needs to act as its
/// owning runner afterward (report events, manifest/upload an artifact).
struct ClaimedAttempt {
    request_id: String,
    attempt_id: String,
    runner_id: String,
    fencing_token: i64,
    auth: [(String, String); 1],
}

/// Creates an execution request and claims it — enough state to report
/// events directly; artifacts additionally need [`accept_and_start`] before
/// they can be manifested. Returns everything a caller needs to act as the
/// owning runner afterward.
async fn request_and_claim(
    app: &axum::Router,
    item_id: &str,
    label: &str,
    policy: Option<&str>,
) -> ClaimedAttempt {
    let (runner_id, auth_owned) = enroll_runner(app, &format!("{label} runner")).await;
    let auth = headers_ref(&auth_owned);
    let agent_profile_id = create_agent_profile(app, label).await;

    let (status, created, _) = common::send_with_raw(
        app,
        "POST",
        "/api/executions",
        execution_request_body(item_id, label, &runner_id, &agent_profile_id, policy),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let request_id = created["request_id"].as_str().unwrap().to_owned();

    let (status, claimed, _) = common::send_with_raw(
        app,
        "POST",
        "/api/runner/v1/claim",
        json!({"protocol_version": 1, "runner_id": runner_id, "claim_request_id": format!("{label}-claim"), "available_capacity": 1, "wait_ms": 0}),
        &auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{claimed}");
    let attempt_id = claimed["lease"]["attempt_id"].as_str().unwrap().to_owned();
    let fencing_token = claimed["lease"]["fencing_token"].as_i64().unwrap();

    ClaimedAttempt {
        request_id,
        attempt_id,
        runner_id,
        fencing_token,
        auth: auth_owned,
    }
}

/// Transitions a claimed attempt through accept and start — required before
/// the artifact manifest route accepts anything (it rejects a merely
/// `leased` attempt with a named `conflict`, stricter than the repository's
/// own eligibility check).
async fn accept_and_start(app: &axum::Router, attempt: &ClaimedAttempt) {
    let auth = headers_ref(&attempt.auth);
    let (status, accepted, _) = common::send_with_raw(
        app,
        "POST",
        &format!("/api/runner/v1/attempts/{}/accept", attempt.attempt_id),
        json!({
            "protocol_version": 1,
            "runner_id": attempt.runner_id,
            "attempt_id": attempt.attempt_id,
            "fencing_token": attempt.fencing_token,
            "workspace_id": "ws-1",
            "base_revision": BASE_REVISION,
        }),
        &auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");

    let (status, started, _) = common::send_with_raw(
        app,
        "POST",
        &format!("/api/runner/v1/attempts/{}/start", attempt.attempt_id),
        json!({
            "protocol_version": 1,
            "runner_id": attempt.runner_id,
            "attempt_id": attempt.attempt_id,
            "fencing_token": attempt.fencing_token,
            "workspace_id": "ws-1",
            "base_revision": BASE_REVISION,
            "process_id": "pid-1",
        }),
        &auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{started}");
}

/// Manifests and uploads `ready.json` as the attempt's MRP artifact through the
/// real runner-protocol routes.
async fn upload_pack(app: &axum::Router, attempt: &ClaimedAttempt) {
    let auth = headers_ref(&attempt.auth);
    let content = READY.as_bytes();
    let (status, manifest, _) = common::send_with_raw(
        app,
        "POST",
        &format!("/api/runner/v1/attempts/{}/artifacts", attempt.attempt_id),
        json!({
            "protocol_version": 1,
            "runner_id": attempt.runner_id,
            "attempt_id": attempt.attempt_id,
            "fencing_token": attempt.fencing_token,
            "artifacts": [{
                "artifact_id": "mrp-1",
                "kind": "report",
                "name": "mrp.json",
                "media_type": MRP_MEDIA_TYPE,
                "size_bytes": content.len(),
                "sha256": sha256_hex(content),
                "content_disposition": "inline_upload",
                "metadata": {},
            }],
        }),
        &auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{manifest}");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!(
                    "/api/runner/v1/attempts/{}/artifacts/mrp-1/content",
                    attempt.attempt_id
                ))
                .header("authorization", attempt.auth[0].1.clone())
                .header("x-tack-fencing-token", attempt.fencing_token.to_string())
                .header("content-type", MRP_MEDIA_TYPE)
                .body(Body::from(content.to_vec()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
}

/// A claimed attempt with the pack uploaded; returns the app, the repository,
/// the item id and the MRP route base for attempt 1.
async fn with_pack(
    storage_root: &std::path::Path,
    policy: Option<&str>,
) -> (axum::Router, Repository, String, String) {
    let (app, repo, item_id) = setup(storage_root).await;
    let attempt = request_and_claim(&app, &item_id, "e2", policy).await;
    accept_and_start(&app, &attempt).await;
    upload_pack(&app, &attempt).await;
    let base = format!("/api/executions/{}/attempts/1/mrp", attempt.request_id);
    (app, repo, item_id, base)
}

async fn review(
    app: &axum::Router,
    base: &str,
    verdict: &str,
    reason: &str,
) -> (StatusCode, Value) {
    common::send(
        app,
        "POST",
        &format!("{base}/review"),
        json!({"verdict": verdict, "reason": reason}),
        &operator_headers(),
    )
    .await
}

async fn item_status(repo: &Repository, item_id: &str) -> String {
    repo.get_item(item_id.parse().unwrap())
        .await
        .unwrap()
        .unwrap()
        .status
}

#[tokio::test]
async fn get_returns_the_uploaded_pack_parsed() {
    let root = tempfile::tempdir().unwrap();
    let (app, _repo, _item, base) = with_pack(root.path(), None).await;

    let (status, body) = common::send(&app, "GET", &base, Value::Null, &operator_headers()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let fixture: Value = serde_json::from_str(READY).unwrap();
    assert_eq!(body["pack"]["evidence_sha256"], fixture["evidence_sha256"]);
    assert_eq!(
        body["pack"]["recommendation"]["decision"],
        fixture["recommendation"]["decision"]
    );
    assert!(body["review"].is_null(), "{body}");

    let (status, viewed) = common::send(
        &app,
        "POST",
        &format!("{base}/viewed"),
        Value::Null,
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{viewed}");
    let (_, again) = common::send(
        &app,
        "POST",
        &format!("{base}/viewed"),
        Value::Null,
        &operator_headers(),
    )
    .await;
    assert_eq!(
        again["viewed_at"], viewed["viewed_at"],
        "viewed is idempotent"
    );
}

#[tokio::test]
async fn a_review_with_a_blank_reason_is_400() {
    let root = tempfile::tempdir().unwrap();
    let (app, _repo, _item, base) = with_pack(root.path(), None).await;
    let (status, body) = review(&app, &base, "accept", "   ").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

#[tokio::test]
async fn a_second_review_is_409_and_the_first_stays() {
    let root = tempfile::tempdir().unwrap();
    let (app, _repo, _item, base) = with_pack(root.path(), None).await;
    let (status, body) = review(&app, &base, "reject", "tests are thin").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = review(&app, &base, "accept", "changed my mind").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let (_, got) = common::send(&app, "GET", &base, Value::Null, &operator_headers()).await;
    assert_eq!(got["review"]["verdict"], "reject");
    assert_eq!(got["review"]["reason"], "tests are thin");
}

#[tokio::test]
async fn accept_under_the_policy_moves_the_item() {
    let root = tempfile::tempdir().unwrap();
    let (app, repo, item_id, base) = with_pack(root.path(), Some("done_on_mrp_accepted")).await;
    assert_eq!(item_status(&repo, &item_id).await, "To Do");
    let (status, body) = review(&app, &base, "accept", "looks right").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(item_status(&repo, &item_id).await, "Done");
}

#[tokio::test]
async fn accept_without_the_policy_does_not_move_the_item() {
    let root = tempfile::tempdir().unwrap();
    let (app, repo, item_id, base) = with_pack(root.path(), None).await;
    let (status, body) = review(&app, &base, "accept", "looks right").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(item_status(&repo, &item_id).await, "To Do");
}
