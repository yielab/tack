//! Shared infrastructure for the chaos/fencing/recovery adversarial suite,
//! split across `chaos_recovery.rs` (fencing, artifacts, replay, corrupt
//! rows) and `chaos_races.rs` (multi-runner/credential races, revocation)
//! once the combined file passed 1000 lines. Loaded via `#[path]` from both
//! — a second, independent copy of this module tree per the same pattern
//! `runner_protocol.rs`'s own doc comments describe, deliberately
//! self-contained rather than shared through `crate::` (matching the
//! established precedent in `wiring/artifact.rs`: each adversarial file
//! builds its own clean database and production router).

// Each of the two importing files uses a different subset of this shared
// surface (races vs. fencing/artifacts/replay/corruption); allowed here
// rather than per-item, matching `tests/common/mod.rs`'s own precedent for
// a helper module several independent test binaries draw from unevenly.
#![allow(dead_code)]

use crate::common;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use chrono::Utc;
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::{AppState, router::build_router};
use tack_db::{Repository, init_pool, migrations};
use tower::ServiceExt;
use uuid::Uuid;

pub const BASE_REVISION: &str = "abc123def456abc123def456abc123def456abc";

/// Storage root for one chaos test, removed with everything under it when the
/// guard drops — including on the panic an adversarial test is likeliest to hit.
pub fn distinctive_temp_dir(label: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(label)
        .tempdir()
        .expect("temporary directory")
}

pub async fn app_in_memory(storage_dir: &std::path::Path) -> (axum::Router, sqlx::SqlitePool) {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_all(&pool).await.expect("migrations");
    build_app(pool, storage_dir, "sqlite::memory:".to_string()).await
}

pub async fn app_file_backed(
    db_path: &std::path::Path,
    storage_dir: &std::path::Path,
) -> (axum::Router, sqlx::SqlitePool) {
    let url = format!("sqlite:{}?mode=rwc", db_path.to_string_lossy());
    let pool = init_pool(&url).await.expect("file-backed pool");
    migrations::run_all(&pool).await.expect("migrations");
    build_app(pool, storage_dir, url).await
}

async fn build_app(
    pool: sqlx::SqlitePool,
    storage_dir: &std::path::Path,
    database_url: String,
) -> (axum::Router, sqlx::SqlitePool) {
    let workspace_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'G2Audit', '{}')",
    )
    .bind(workspace_id.to_string())
    .execute(&pool)
    .await
    .expect("insert workspace");
    let (tx, _rx) = tokio::sync::broadcast::channel(16);
    let state = AppState {
        repo: Repository::new(pool.clone()),
        config: AppConfig {
            database_url,
            storage_dir: storage_dir.to_string_lossy().into_owned(),
            ..AppConfig::default()
        },
        workspace_id,
        broadcast_tx: tx,
        webhook: None,
        local_runner: None,
    };
    (build_router(state), pool)
}

pub async fn put_content(
    app: &axum::Router,
    uri: &str,
    body: Vec<u8>,
    extra_headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method("PUT").uri(uri);
    for (name, value) in extra_headers {
        builder = builder.header(*name, *value);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 64 * 1_048_576)
        .await
        .unwrap();
    let value: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
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
            "model_combinations": [{
                "model_provider": "openai",
                "model_ids": ["opaque/model-g2"],
                "discovery": "reported"
            }],
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
        json!({"name": "G2 audit", "project_type": "software"}),
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
            "requested_model_provider": "openai",
            "requested_model_id": "opaque/model-g2",
            "agent_profile_snapshot": {"name": "profile", "instructions": "work safely", "tool_policy": {}, "timeout_seconds": 60, "budgets": {}},
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

pub struct RunningAttempt {
    pub runner_id: String,
    pub credential: String,
    pub attempt_id: String,
    pub fencing_token: i64,
}

/// Enrolls one runner, creates one exact-runner-selected request, claims,
/// accepts and starts it — leaving the attempt `running`. Mirrors
/// `wiring/artifact.rs::ready_running_attempt`.
pub async fn ready_running_attempt(
    app: &axum::Router,
    item_id: &str,
    label: &str,
) -> RunningAttempt {
    let agent_profile_id = agent_profile(app, label).await;
    let runner = enroll_runner(app, &format!("G2 runner {label}")).await;
    let _request_id = create_execution_request(
        app,
        item_id,
        &format!("key-{label}"),
        "exact_runner",
        &runner.runner_id,
        &agent_profile_id,
    )
    .await;
    let (status, claimed) = common::claim_runner(
        app,
        &runner.runner_id,
        &runner.credential,
        &format!("claim-{label}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{claimed}");
    let attempt_id = claimed["lease"]["attempt_id"].as_str().unwrap().to_owned();
    let fencing_token = claimed["lease"]["fencing_token"].as_i64().unwrap();

    let (status, accepted) = common::send_large(
        app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_id}/accept"),
        json!({
            "protocol_version": 1, "runner_id": runner.runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token,
            "workspace_id": "ws-1", "base_revision": BASE_REVISION,
        }),
        &[("authorization", &auth(&runner.credential))],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");

    let (status, started) = common::send_large(
        app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_id}/start"),
        json!({
            "protocol_version": 1, "runner_id": runner.runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token,
            "workspace_id": "ws-1", "base_revision": BASE_REVISION, "process_id": "pid-1",
        }),
        &[("authorization", &auth(&runner.credential))],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{started}");

    RunningAttempt {
        runner_id: runner.runner_id,
        credential: runner.credential,
        attempt_id,
        fencing_token,
    }
}

/// Minimal, test-local percent-encoder for a URI path segment — avoids
/// pulling in an extra dependency just for this adversarial file. Encodes
/// every byte that is not an unreserved URI character, which is sufficient
/// (if wasteful) for the deliberately-malicious ids this file constructs.
pub fn percent_encode_path_segment(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() * 3);
    for byte in raw.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

pub async fn walk_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(mut entries) = tokio::fs::read_dir(&current).await else {
            continue;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if let Ok(meta) = entry.metadata().await
                && meta.is_dir()
            {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------
// Wire-body builders — every route this file drives repeats the same
// runner-protocol envelope with a few fields varying per call; factored out
// here so a test body carries only what it's proving, not the envelope.
// ---------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub fn artifact_manifest_body(
    runner_id: &str,
    attempt_id: &str,
    fencing_token: i64,
    artifact_id: &str,
    kind: &str,
    name: &str,
    size_bytes: u64,
    sha256: &str,
) -> Value {
    json!({
        "protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token,
        "artifacts": [{
            "artifact_id": artifact_id, "kind": kind, "name": name, "media_type": "text/plain",
            "size_bytes": size_bytes, "sha256": sha256, "content_disposition": "inline_upload", "metadata": {},
        }],
    })
}

pub async fn post_artifact_manifest(
    app: &axum::Router,
    attempt: &RunningAttempt,
    artifact_id: &str,
    kind: &str,
    name: &str,
    size_bytes: u64,
    sha256: &str,
) -> (StatusCode, Value) {
    let body = artifact_manifest_body(
        &attempt.runner_id,
        &attempt.attempt_id,
        attempt.fencing_token,
        artifact_id,
        kind,
        name,
        size_bytes,
        sha256,
    );
    let uri = format!("/api/runner/v1/attempts/{}/artifacts", attempt.attempt_id);
    common::send_large(
        app,
        "POST",
        &uri,
        body,
        &[("authorization", &auth(&attempt.credential))],
    )
    .await
}

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

pub fn decision_body(
    runner_id: &str,
    attempt_id: &str,
    fencing_token: i64,
    decision_id: &str,
) -> Value {
    json!({
        "protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token,
        "decision_id": decision_id, "kind": "tool_permission", "prompt": "Allow?",
        "options": [{"option_id": "allow", "label": "Allow"}],
        "expires_at": (Utc::now() + chrono::Duration::minutes(5)).to_rfc3339(), "metadata": {},
    })
}

pub fn recovery_body(
    runner_id: &str,
    attempt_id: &str,
    fencing_token: i64,
    recovery_key: &str,
) -> Value {
    json!({
        "protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token,
        "recovery_key": recovery_key, "observation": "process_stopped",
        "details": {"journal_state": "prepared", "process_observed": false},
    })
}

pub fn cancellation_body(
    runner_id: &str,
    attempt_id: &str,
    fencing_token: i64,
    cancellation_request_id: &str,
) -> Value {
    json!({
        "protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token,
        "cancellation_request_id": cancellation_request_id, "observation": "process_stopped",
        "observed_at": Utc::now().to_rfc3339(), "details": {"exit_code": 130, "signal": "SIGTERM"},
    })
}

#[allow(clippy::too_many_arguments)]
pub fn events_body(
    runner_id: &str,
    attempt_id: &str,
    fencing_token: i64,
    previous_checkpoint: Value,
    checkpoint: &str,
    event_id: &str,
) -> Value {
    json!({
        "protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token,
        "previous_checkpoint": previous_checkpoint, "checkpoint": checkpoint,
        "events": [{"event_id": event_id, "sequence": 1, "occurred_at": Utc::now().to_rfc3339(),
            "source": "runner", "kind": "progress", "payload": {}}],
    })
}

pub async fn post_events(
    app: &axum::Router,
    attempt: &RunningAttempt,
    hdr: &str,
    body: Value,
) -> (StatusCode, Value) {
    let uri = format!("/api/runner/v1/attempts/{}/events", attempt.attempt_id);
    common::send_large(app, "POST", &uri, body, &[("authorization", hdr)]).await
}
