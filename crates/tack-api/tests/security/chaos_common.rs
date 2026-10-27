//! Shared infrastructure for the chaos/fencing/recovery adversarial suite,
//! split across `chaos_recovery.rs` (fencing, artifacts, replay, corrupt
//! rows) and `chaos_races.rs` (multi-runner/credential races, revocation)
//! once the combined file passed 1000 lines. Loaded via `#[path]` from both
//! — a second, independent copy of this module tree per the same pattern
//! `runner_protocol.rs`'s own doc comments describe, deliberately
//! self-contained rather than shared through `crate::` (matching the
//! established precedent in `wiring/artifact.rs`: each adversarial file
//! builds its own clean database and production router).

// What only one of the two importing files uses lives in that file; what is
// here is drawn on by both.

use crate::common;
use axum::http::StatusCode;
use chrono::Utc;
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::{AppState, router::build_router};
use tack_db::Repository;
use tack_test_support::{create_test_workspace, setup_test_db};

pub const BASE_REVISION: &str = "abc123def456abc123def456abc123def456abc";

/// Storage root for one chaos test, removed with everything under it when the
/// guard drops — including on the panic an adversarial test is likeliest to hit.
pub fn distinctive_temp_dir(label: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(label)
        .tempdir()
        .expect("temporary directory")
}

pub async fn app_in_memory(storage_dir: &std::path::Path) -> (axum::Router, Repository) {
    build_app(
        setup_test_db().await,
        storage_dir,
        "sqlite::memory:".to_string(),
    )
    .await
}

pub async fn build_app(
    repo: Repository,
    storage_dir: &std::path::Path,
    database_url: String,
) -> (axum::Router, Repository) {
    let workspace_id = create_test_workspace(&repo).await;
    let (tx, _rx) = tokio::sync::broadcast::channel(16);
    let state = AppState {
        repo: repo.clone(),
        config: AppConfig {
            database_url,
            storage_dir: storage_dir.to_string_lossy().into_owned(),
            ..AppConfig::default()
        },
        workspace_id,
        broadcast_tx: tx,
        webhook: None,
        local_runner: None,
        tasks: Default::default(),
    };
    (crate::common::with_default_host(build_router(state)), repo)
}

pub fn full_capabilities() -> Value {
    let now = Utc::now().to_rfc3339();
    json!({
        "reported_at": now,
        "labels": {"os": "linux"},
        "concurrency": {"total": 1, "available": 1},
        "harnesses": [{
            "harness_kind": "codex",
            "installed_version": "1.2.3",
            "probe_error": null,
            "probed_at": now,
            "requires_model": true,
        }],
        "features": {},
        "limits": {"event_payload_bytes_max": 65536, "artifact_content_bytes_max": 52428800},
    })
}

pub async fn create_project_and_item(app: &axum::Router) -> String {
    let (status, project) = common::send_large(
        app,
        "POST",
        "/api/projects",
        json!({"name": "G2 audit", "project_type": "software", "code_origin": "url", "repository": "https://example.test/g2.git"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{project}");
    let project_id = project["id"].as_str().unwrap().to_owned();
    let (status, item) = common::send_large(
        app,
        "POST",
        &format!("/api/projects/{project_id}/items"),
        json!({"title": "G2 adversarial item"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{item}");
    item["id"].as_str().unwrap().to_owned()
}

pub async fn agent_profile(app: &axum::Router, label: &str) -> String {
    let (status, profile) = common::send_large(
        app,
        "POST",
        "/api/agent-profiles",
        json!({"name": format!("{label} profile"), "instructions": "work safely"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{profile}");
    profile["agent_profile_id"].as_str().unwrap().to_owned()
}

pub struct EnrolledRunner {
    pub runner_id: String,
    pub credential: String,
}

pub async fn enroll_runner(app: &axum::Router, name: &str) -> EnrolledRunner {
    let (status, pending) = common::send_large(
        app,
        "POST",
        "/api/runners/enrollment",
        json!({"name": name, "total_capacity": 1, "available_capacity": 1}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{pending}");
    let runner_id = pending["runner_id"].as_str().unwrap().to_owned();
    let raw_enrollment_token = pending["enrollment_token"].as_str().unwrap().to_owned();

    let (status, enrolled) = common::send_large(
        app,
        "POST",
        "/api/runner/v1/enroll",
        json!({
            "protocol_version": 1,
            "enrollment_token": raw_enrollment_token,
            "runner_name": name,
            "runner_version": "0.1.0",
            "capabilities": full_capabilities(),
        }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{enrolled}");
    let credential = enrolled["runner_credential"].as_str().unwrap().to_owned();
    EnrolledRunner {
        runner_id,
        credential,
    }
}

pub fn auth(credential: &str) -> String {
    format!("Bearer {credential}")
}

pub async fn create_execution_request(
    app: &axum::Router,
    item_id: &str,
    key: &str,
    selector_kind: &str,
    selector_id: &str,
    agent_profile_id: &str,
) -> String {
    let (status, created) = common::send_large(
        app,
        "POST",
        "/api/executions",
        json!({
            "item_id": item_id,
            "idempotency_key": key,
            "selector_kind": selector_kind,
            "selector_id": selector_id,
            "agent_profile_id": agent_profile_id,
            "requested_harness_kind": "codex",
            "requested_model": "opaque/model-g2",
            "repository_snapshot": {"kind": "git", "remote": "https://example.test/g2.git", "base_revision": BASE_REVISION, "subdirectory": null},
            "permission_policy": {"tools": ["shell"], "network": false},
            "timeout_seconds": 60,
            "budgets": {},
            "environment": {},
            "metadata": {},
        }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    created["request_id"].as_str().unwrap().to_owned()
}

// ---------------------------------------------------------------------
// Wire-body builders — every route this file drives repeats the same
// runner-protocol envelope with a few fields varying per call; factored out
// here so a test body carries only what it's proving, not the envelope.
// ---------------------------------------------------------------------

pub fn heartbeat_body(
    runner_id: &str,
    heartbeat_id: &str,
    capacity: i64,
    active_attempts: Value,
) -> Value {
    json!({
        "protocol_version": 1, "runner_id": runner_id, "heartbeat_id": heartbeat_id,
        "sent_at": Utc::now().to_rfc3339(), "available_capacity": capacity, "active_attempts": active_attempts,
    })
}

pub fn active_attempt_entry(attempt_id: &str, fencing_token: i64) -> Value {
    json!({
        "attempt_id": attempt_id, "fencing_token": fencing_token, "state": "running",
        "journal_state": "process_observed_running", "last_event_checkpoint": Value::Null,
    })
}
