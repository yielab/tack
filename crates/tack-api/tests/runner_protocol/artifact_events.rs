//! HTTP tests: artifact content upload/download and the redaction guarantee
//! over event/artifact payloads. Repository-level atomicity/retention
//! proofs live in `tack-db`'s `repository/event_artifact_retention.rs`;
//! the storage primitive's own unit tests live beside it in
//! `artifact_storage.rs`. This file proves the HTTP wiring end to end.

// `artifact_download::routes(...)` below is proven as its own,
// separately-constructed local router (never merged with the runner-only
// `runner_protocol::routes(...)` router), isolating this file's claims from
// the production router's own auth and mounting — that route is also
// mounted in the real production router and proven end to end by the
// `wiring` binary's `artifact.rs`.
use tack_api::handlers::runner_protocol;

use std::sync::{Arc, Mutex};

use crate::log_capture::{CaptureGuard, ensure_global_log_capture_installed};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use chrono::{DateTime, Duration, TimeZone, Utc};
use serde_json::{Value, json};
use tack_core::models::{CreateItem, CreateProject, ProjectType};
use tack_db::{
    Repository, init_pool, migrations,
    repo::execution::{
        EnrollmentToken, ExecutionClock, NewAgentProfile, NewExecutionRequest, NewRunner,
    },
};
use tower::ServiceExt;
use uuid::Uuid;

const REQUESTED_MODEL_PROVIDER: &str = "openai";
const REQUESTED_MODEL_ID: &str = "opaque/model-f2";

#[derive(Clone)]
struct FakeClock(Arc<Mutex<DateTime<Utc>>>);

impl FakeClock {
    fn new(start: DateTime<Utc>) -> Self {
        Self(Arc::new(Mutex::new(start)))
    }
}

impl ExecutionClock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap()
    }
}

/// Artifact storage root for one artifact-events test.
///
/// The `TempDir` removes the directory and everything under it when it drops,
/// so a failing assertion leaves nothing behind either.
fn temp_storage_root(label: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(label)
        .tempdir()
        .expect("temporary directory")
}

async fn setup() -> (Router, Repository, FakeClock, String, tempfile::TempDir) {
    // Must run before any handler code in this binary executes — see
    // `log_capture.rs`'s own doc comment for the `tracing` interest-cache
    // race this closes.
    ensure_global_log_capture_installed();
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_all(&pool).await.expect("migrations");
    let repo = Repository::new(pool);
    let workspace = Uuid::new_v4();
    sqlx::query("INSERT INTO workspaces (id,name,default_vocabulary) VALUES (?, 'F2', '{}')")
        .bind(workspace.to_string())
        .execute(repo.pool())
        .await
        .expect("workspace");
    let project = repo
        .create_project(
            workspace,
            CreateProject {
                name: "F2".into(),
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

    let clock = FakeClock::new(Utc.with_ymd_and_hms(2026, 8, 12, 12, 0, 0).unwrap());
    repo.create_agent_profile(
        NewAgentProfile {
            id: "profile-f2",
            name: "F2 Profile",
            instructions: "work safely",
            tool_policy: r#"{"mode":"safe"}"#,
            limits: r#"{"tokens":1000}"#,
        },
        &clock,
    )
    .await
    .expect("profile");

    let storage_root = temp_storage_root("root");
    let state = runner_protocol::RunnerProtocolState::new(repo.clone(), Arc::new(clock.clone()))
        .with_artifact_storage_root(storage_root.path().to_path_buf());
    let app = runner_protocol::routes(state, usize::MAX);
    (app, repo, clock, item.id.to_string(), storage_root)
}

#[allow(clippy::too_many_arguments)]
async fn enqueue_request(
    repo: &Repository,
    clock: &FakeClock,
    item_id: &str,
    runner_id: &str,
    agent_profile_id: &str,
    key: &str,
) -> String {
    let request_id = format!("exec_{}", Uuid::new_v4());
    let created_at = clock.now();
    let snapshot = json!({
        "request_id": request_id,
        "item_id": item_id,
        "idempotency_key": key,
        "created_by": {"source": "test", "subject_id": "f2-test"},
        "created_at": created_at.to_rfc3339(),
        "selector": {"kind": "exact_runner", "runner_id": runner_id},
        "agent_profile_id": agent_profile_id,
        "resolved_agent_profile": {"name":"F2 Profile","instructions":"work safely","tool_policy":{"mode":"safe"},"timeout_seconds":60,"budgets":{"tokens":1000}},
        "requested_harness_kind": "codex",
        "requested_model_provider": REQUESTED_MODEL_PROVIDER,
        "requested_model_id": REQUESTED_MODEL_ID,
        "repository": {"kind":"git","remote":"https://example.test/f2.git","base_revision":"abc123def456abc123def456abc123def456abc","subdirectory": Value::Null},
        "permission_policy": {"tools":["shell"],"network": false},
        "timeout_seconds": 60,
        "budgets": {"tokens": 1000},
        "status_map_policy_id": Value::Null,
        "environment": {},
        "metadata": {},
    });
    let snapshot_string = serde_json::to_string(&snapshot).unwrap();
    let root = snapshot.as_object().unwrap();
    let field_str = |name: &str| serde_json::to_string(&root[name]).unwrap();
    repo.enqueue_execution(
        NewExecutionRequest {
            id: &request_id,
            item_id,
            idempotency_scope: "test",
            idempotency_key: key,
            request_fingerprint: key,
            selector_kind: "exact_runner",
            selector_id: runner_id,
            agent_profile_id: Some(agent_profile_id),
            agent_profile_snapshot: &field_str("resolved_agent_profile"),
            requested_harness_kind: Some("codex"),
            requested_model_provider: Some(REQUESTED_MODEL_PROVIDER),
            requested_model_id: Some(REQUESTED_MODEL_ID),
            repository_snapshot: &field_str("repository"),
            permission_policy: &field_str("permission_policy"),
            timeout_seconds: Some(60),
            budgets: &field_str("budgets"),
            status_map_policy_id: None,
            environment: &field_str("environment"),
            metadata: &field_str("metadata"),
            request_snapshot: &snapshot_string,
        },
        clock,
    )
    .await
    .expect("enqueue");
    request_id
}

fn full_capabilities(reported_at: DateTime<Utc>, total: i64, available: i64) -> Value {
    json!({
        "reported_at": reported_at.to_rfc3339(),
        "labels": {"os": "linux"},
        "concurrency": {"total": total, "available": available},
        "harnesses": [{
            "harness_kind": "codex",
            "installed_version": "1.0.0",
            "probe_error": null,
            "probed_at": reported_at.to_rfc3339(),
            "model_combinations": [{
                "model_provider": REQUESTED_MODEL_PROVIDER,
                "model_ids": [REQUESTED_MODEL_ID],
                "discovery": "reported"
            }]
        }],
        "features": {},
        "limits": {"event_payload_bytes_max": 65536, "artifact_content_bytes_max": 52428800},
    })
}

async fn send_json(
    app: &Router,
    method: &str,
    uri: &str,
    body: String,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    for (name, value) in headers {
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
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, value)
}

/// Raw-bytes PUT for the artifact content endpoint. `extra_headers` carries
/// the fencing-token header and, optionally, `content-type`.
async fn put_content(
    app: &Router,
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

struct RunningAttempt {
    runner_id: String,
    credential: String,
    attempt_id: String,
    fencing_token: i64,
}

/// Enroll → claim → accept → start, entirely through the real HTTP handlers,
/// leaving the attempt in `running` — the state artifact/event writes
/// require.
async fn ready_running_attempt(
    app: &Router,
    repo: &Repository,
    clock: &FakeClock,
    item_id: &str,
    label: &str,
) -> RunningAttempt {
    let runner_id = format!("runner-{label}");
    let raw_token = format!("example_enrollment_token_{label}");
    let token_hash = runner_protocol::runner_auth::credential_hash(&raw_token);
    repo.create_pending_runner_and_issue_token(
        NewRunner {
            id: &runner_id,
            name: "F2 Runner",
            credential_hash: "pending:no-credential",
            labels: "{}",
            total_capacity: 2,
            available_capacity: 2,
            capability_snapshot: "{}",
            protocol_version: 1,
        },
        EnrollmentToken {
            id: &format!("tok-{label}"),
            runner_id: &runner_id,
            token_hash: &token_hash,
            expires_at: clock.now() + Duration::hours(1),
        },
        clock,
    )
    .await
    .expect("pending runner");

    let (status, enrolled) = send_json(
        app,
        "POST",
        "/enroll",
        json!({
            "protocol_version": 1,
            "enrollment_token": raw_token,
            "runner_name": "F2 Runner",
            "runner_version": "0.1.0",
            "capabilities": full_capabilities(clock.now(), 2, 2),
        })
        .to_string(),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{enrolled}");
    let credential = enrolled["runner_credential"].as_str().unwrap().to_owned();
    let auth_header = format!("Bearer {credential}");

    repo.create_agent_profile(
        NewAgentProfile {
            id: &format!("profile-{label}"),
            name: "F2 Profile",
            instructions: "work safely",
            tool_policy: r#"{"mode":"safe"}"#,
            limits: r#"{"tokens":1000}"#,
        },
        clock,
    )
    .await
    .ok();

    let request_id = enqueue_request(
        repo,
        clock,
        item_id,
        &runner_id,
        "profile-f2",
        &format!("key-{label}"),
    )
    .await;

    let (status, claimed) = send_json(
        app,
        "POST",
        "/claim",
        json!({
            "protocol_version": 1, "runner_id": runner_id, "claim_request_id": format!("claim-{label}"),
            "available_capacity": 1, "wait_ms": 1000,
        })
        .to_string(),
        &[("authorization", &auth_header)],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{claimed} for request {request_id}");
    let attempt_id = claimed["lease"]["attempt_id"].as_str().unwrap().to_owned();
    let fencing_token = claimed["lease"]["fencing_token"].as_i64().unwrap();

    let (status, _) = send_json(
        app,
        "POST",
        &format!("/attempts/{attempt_id}/accept"),
        json!({
            "protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token,
            "workspace_id": "ws-1", "base_revision": "abc123def456abc123def456abc123def456abc",
        })
        .to_string(),
        &[("authorization", &auth_header)],
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = send_json(
        app,
        "POST",
        &format!("/attempts/{attempt_id}/start"),
        json!({
            "protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token,
            "workspace_id": "ws-1", "base_revision": "abc123def456abc123def456abc123def456abc", "process_id": "pid-1",
        })
        .to_string(),
        &[("authorization", &auth_header)],
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    RunningAttempt {
        runner_id,
        credential,
        attempt_id,
        fencing_token,
    }
}

/// The router/repo, a running attempt ready for artifact/event writes, and
/// its storage root — the setup every test below starts from.
struct Fixture {
    app: Router,
    repo: Repository,
    storage_root_dir: tempfile::TempDir,
    attempt: RunningAttempt,
}

impl Fixture {
    async fn new(label: &str) -> Self {
        let (app, repo, clock, item_id, storage_root_dir) = setup().await;
        let attempt = ready_running_attempt(&app, &repo, &clock, &item_id, label).await;
        Fixture {
            app,
            repo,
            storage_root_dir,
            attempt,
        }
    }

    fn storage_root(&self) -> &std::path::Path {
        self.storage_root_dir.path()
    }

    async fn manifest(&self, artifact_id: &str, content: &[u8], media_type: Option<&str>) -> Value {
        manifest_artifact(&self.app, &self.attempt, artifact_id, content, media_type).await
    }

    async fn put(
        &self,
        uri: &str,
        body: Vec<u8>,
        extra_headers: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        put_content(&self.app, uri, body, extra_headers).await
    }

    fn auth(&self) -> (String, String) {
        auth_header(&self.attempt)
    }

    fn fence(&self) -> (String, String) {
        fencing_header(&self.attempt)
    }

    /// [`Fixture::auth`]/[`Fixture::fence`] as the `&[(&str, &str)]` header
    /// pair every content PUT sends.
    /// PUT `content` to `artifact_id`'s content endpoint with the correct
    /// auth/fencing headers plus whatever `extra_headers` a test needs
    /// (e.g. `content-type`, or a deliberately wrong fencing token).
    async fn upload(
        &self,
        artifact_id: &str,
        content: Vec<u8>,
        extra_headers: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        let (auth, fence) = (self.auth(), self.fence());
        let mut headers = vec![
            (auth.0.as_str(), auth.1.as_str()),
            (fence.0.as_str(), fence.1.as_str()),
        ];
        headers.extend_from_slice(extra_headers);
        self.put(&self.content_uri(artifact_id), content, &headers)
            .await
    }

    async fn cleanup(&self) {
        let _ = tokio::fs::remove_dir_all(self.storage_root()).await;
    }

    fn content_uri(&self, artifact_id: &str) -> String {
        content_uri(&self.attempt, artifact_id)
    }

    /// `(attempt_number, request_id)`, the two ids an operator-facing
    /// download URI is built from.
    async fn attempt_location(&self) -> (i64, String) {
        let attempt_id = &self.attempt.attempt_id;
        let attempt_number: i64 =
            sqlx::query_scalar("SELECT attempt_number FROM execution_attempts WHERE id=?")
                .bind(attempt_id)
                .fetch_one(self.repo.pool())
                .await
                .unwrap();
        let request_id: String =
            sqlx::query_scalar("SELECT request_id FROM execution_attempts WHERE id=?")
                .bind(attempt_id)
                .fetch_one(self.repo.pool())
                .await
                .unwrap();
        (attempt_number, request_id)
    }

    fn download_router(&self) -> Router {
        download_router(&self.repo, self.storage_root())
    }

    async fn stored_reference(&self, artifact_id: &str) -> Option<String> {
        sqlx::query_scalar(
            "SELECT content_reference FROM execution_artifacts WHERE attempt_id=? AND artifact_id=?",
        )
        .bind(&self.attempt.attempt_id)
        .bind(artifact_id)
        .fetch_one(self.repo.pool())
        .await
        .unwrap()
    }

    async fn attempt_dir_is_empty(&self) -> bool {
        let dir = self
            .storage_root()
            .join(hex_encode(&self.attempt.attempt_id));
        dir_is_empty_or_absent(&dir).await
    }
}

/// The operator-facing download sub-router, constructed locally per
/// `artifact_download.rs`'s own doc comment (never merged into the
/// runner-only protocol router).
fn download_router(repo: &Repository, storage_root: &std::path::Path) -> Router {
    let download_state = runner_protocol::artifact_download::ArtifactDownloadState {
        repo: repo.clone(),
        artifact_storage: Arc::new(runner_protocol::artifact_storage::ArtifactStorage::new(
            storage_root,
        )),
    };
    runner_protocol::artifact_download::routes(download_state)
}

/// `GET uri` against a download router, as the operator principal when
/// `as_operator` is set.
async fn get_download(app: &Router, uri: &str, as_operator: bool) -> axum::http::Response<Body> {
    let mut builder = Request::builder().method("GET").uri(uri);
    if as_operator {
        builder = builder.header("x-tack-principal", "operator-test");
    }
    app.clone()
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

async fn manifest_artifact(
    app: &Router,
    attempt: &RunningAttempt,
    artifact_id: &str,
    content: &[u8],
    media_type: Option<&str>,
) -> Value {
    let (status, response) = send_json(
        app,
        "POST",
        &format!("/attempts/{}/artifacts", attempt.attempt_id),
        json!({
            "protocol_version": 1,
            "runner_id": attempt.runner_id,
            "attempt_id": attempt.attempt_id,
            "fencing_token": attempt.fencing_token,
            "artifacts": [{
                "artifact_id": artifact_id,
                "kind": "patch",
                "name": "changes.patch",
                "media_type": media_type,
                "size_bytes": content.len(),
                "sha256": sha256_hex(content),
                "content_disposition": "inline_upload",
                "metadata": {},
            }],
        })
        .to_string(),
        &[("authorization", &format!("Bearer {}", attempt.credential))],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{response}");
    response
}

fn content_uri(attempt: &RunningAttempt, artifact_id: &str) -> String {
    format!(
        "/attempts/{}/artifacts/{artifact_id}/content",
        attempt.attempt_id
    )
}

fn fencing_header(attempt: &RunningAttempt) -> (String, String) {
    (
        "x-tack-fencing-token".to_string(),
        attempt.fencing_token.to_string(),
    )
}

fn auth_header(attempt: &RunningAttempt) -> (String, String) {
    (
        "authorization".to_string(),
        format!("Bearer {}", attempt.credential),
    )
}

async fn dir_is_empty_or_absent(path: &std::path::Path) -> bool {
    match tokio::fs::read_dir(path).await {
        Ok(mut entries) => entries.next_entry().await.unwrap().is_none(),
        Err(_) => true, // never created at all
    }
}

// ---------------------------------------------------------------------
// 1. Happy path: manifest, PUT content, commit, and a streamed operator
//    download of the exact same bytes.
// ---------------------------------------------------------------------

#[tokio::test]
async fn artifact_content_upload_commits_verified_state() {
    let fx = Fixture::new("roundtrip").await;
    let content = b"diff --git a/x b/x\n+hello\n".to_vec();
    fx.manifest("art-1", &content, Some("text/x-diff")).await;

    let (status, uploaded) = fx
        .upload("art-1", content.clone(), &[("content-type", "text/x-diff")])
        .await;
    assert_eq!(status, StatusCode::OK, "{uploaded}");
    assert_eq!(uploaded["state"], "content_verified");
    assert_eq!(uploaded["size_bytes"], content.len());

    let stored_reference: Option<String> = sqlx::query_scalar(
        "SELECT content_reference FROM execution_artifacts WHERE attempt_id=? AND artifact_id='art-1'",
    )
    .bind(&fx.attempt.attempt_id)
    .fetch_one(fx.repo.pool())
    .await
    .unwrap();
    assert!(stored_reference.is_some());
    fx.cleanup().await;
}

/// Via the operator-facing router, constructed locally per
/// `artifact_download.rs`'s own doc comment (never merged into the
/// runner-only protocol router `Fixture::upload` posts through).
#[tokio::test]
async fn operator_download_returns_the_uploaded_bytes_and_headers() {
    let fx = Fixture::new("download").await;
    let content = b"diff --git a/x b/x\n+hello\n".to_vec();
    fx.manifest("art-1", &content, Some("text/x-diff")).await;
    fx.upload("art-1", content.clone(), &[("content-type", "text/x-diff")])
        .await;

    let (attempt_number, request_id) = fx.attempt_location().await;
    let uri = format!("/executions/{request_id}/attempts/{attempt_number}/artifacts/art-1/content");
    let response = get_download(&fx.download_router(), &uri, true).await;
    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers().clone();
    let content_type = headers.get("content-type").unwrap().to_str().unwrap();
    assert_eq!(content_type, "text/x-diff");
    let disposition = headers
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(disposition.contains("changes.patch"));
    let downloaded = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    assert_eq!(downloaded.as_ref(), content.as_slice());
    fx.cleanup().await;
}

// ---------------------------------------------------------------------
// 2. Acceptance: checksum mismatch stages nothing.
// ---------------------------------------------------------------------

/// Load-bearing (proven by hand, not left in the tree): temporarily made
/// `put_artifact_content` call `set_execution_artifact_content_reference`
/// unconditionally, skipping the checksum-mismatch early return — this test
/// failed (a `content_reference` was committed despite the mismatch).
/// Reverted, and it passes again. Two cases isolate the checksum check from
/// the size check: mismatched length, and same length with wrong bytes.
struct ChecksumCase {
    label: &'static str,
    artifact_id: &'static str,
    declared: &'static [u8],
    wrong: &'static [u8],
    /// True isolates the checksum check from the size check (a
    /// mismatched-length wrong body could conflict on either).
    same_length: bool,
}

#[tokio::test]
async fn artifact_content_put_stages_nothing_on_checksum_mismatch() {
    let cases = [
        ChecksumCase {
            label: "checksum",
            artifact_id: "art-mismatch",
            declared: b"the real bytes",
            wrong: b"the WRONG bytes!!",
            same_length: false,
        },
        ChecksumCase {
            label: "checksum-samesize",
            artifact_id: "art-samesize",
            declared: b"AAAAAAAAAA",
            wrong: b"BBBBBBBBBB",
            same_length: true,
        },
    ];

    for case in cases {
        let fx = Fixture::new(case.label).await;
        fx.manifest(case.artifact_id, case.declared, None).await;
        let (status, response) = fx.upload(case.artifact_id, case.wrong.to_vec(), &[]).await;
        assert!(
            status == StatusCode::CONFLICT || status == StatusCode::PAYLOAD_TOO_LARGE,
            "{status}: {response}"
        );
        if case.same_length {
            assert_eq!(status, StatusCode::CONFLICT, "{response}");
            assert_eq!(response["error"]["code"], "artifact_checksum_mismatch");
            assert_eq!(response["error"]["retryable"], true);
        }
        assert_eq!(
            fx.stored_reference(case.artifact_id).await,
            None,
            "no content_reference committed"
        );
        assert!(
            fx.attempt_dir_is_empty().await,
            "no blob left on disk after a checksum mismatch"
        );
        fx.cleanup().await;
    }
}

// ---------------------------------------------------------------------
// 3. Acceptance: oversize / compression-bomb-style upload rejected.
// ---------------------------------------------------------------------

#[tokio::test]
async fn oversize_artifact_body_yields_413_and_stages_nothing() {
    let fx = Fixture::new("oversize").await;
    let declared_content = b"tiny".to_vec(); // manifest declares 4 bytes
    fx.manifest("art-oversize", &declared_content, None).await;

    // Actually sends far more than declared — the "compression bomb" shape:
    // a small declared size, a much larger real body.
    let bomb = vec![0u8; 5 * 1024 * 1024]; // 5 MiB actually sent vs. 4 bytes declared
    let (status, response) = fx.upload("art-oversize", bomb, &[]).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{response}");
    assert_eq!(response["error"]["code"], "payload_too_large");
    assert_eq!(fx.stored_reference("art-oversize").await, None);
    assert!(
        fx.attempt_dir_is_empty().await,
        "no partial bomb content may remain on disk"
    );
    fx.cleanup().await;
}

// ---------------------------------------------------------------------
// 4. Path traversal via crafted ids never escapes the storage root.
// ---------------------------------------------------------------------

#[tokio::test]
async fn single_crafted_traversal_id_stays_inside_storage_root() {
    let fx = Fixture::new("traversal").await;
    let content = b"safe content".to_vec();
    // The manifest step takes `artifact_id` from a JSON string field — no
    // URL decoding involved, so the literal traversal-shaped value is
    // exactly what gets stored.
    let malicious_artifact_id = "../../../etc/passwd";
    fx.manifest(malicious_artifact_id, &content, None).await;

    // The PUT URI, by contrast, must percent-encode the same literal value
    // into its `{artifact_id}` path segment (`axum::extract::Path` decodes
    // captured segments) so this test exercises the same request shape a
    // real client — or a genuine attacker attempting this traversal —
    // would have to send; a raw literal `/` would not even route here.
    let uri = format!(
        "/attempts/{}/artifacts/..%2f..%2f..%2fetc%2fpasswd/content",
        fx.attempt.attempt_id
    );
    let (auth, fence) = (fx.auth(), fx.fence());
    let headers = [
        (auth.0.as_str(), auth.1.as_str()),
        (fence.0.as_str(), fence.1.as_str()),
    ];
    let (status, response) = fx.put(&uri, content.clone(), &headers).await;
    assert_eq!(status, StatusCode::OK, "{response}");

    // Confirm the write landed strictly inside the canonical storage root.
    let canonical_root = tokio::fs::canonicalize(fx.storage_root()).await.unwrap();
    let content_reference = fx
        .stored_reference(malicious_artifact_id)
        .await
        .expect("content_reference must be set after a successful upload");
    let full_path = fx.storage_root().join(&content_reference);
    let canonical_full = tokio::fs::canonicalize(full_path.parent().unwrap())
        .await
        .unwrap();
    assert!(canonical_full.starts_with(&canonical_root));
    assert!(!content_reference.contains(".."));
    fx.cleanup().await;
}

// ---------------------------------------------------------------------
// 5. Immutability: a second upload for the same artifact_id is refused.
// ---------------------------------------------------------------------

#[tokio::test]
async fn content_is_immutable_once_verified() {
    let fx = Fixture::new("immutable").await;
    let content = b"first and only".to_vec();
    fx.manifest("art-immutable", &content, None).await;

    let (status, _) = fx.upload("art-immutable", content.clone(), &[]).await;
    assert_eq!(status, StatusCode::OK);

    // A byte-identical second upload is still refused — content is
    // recorded once, not "once per distinct value."
    let (status, response) = fx.upload("art-immutable", content, &[]).await;
    assert_eq!(status, StatusCode::CONFLICT, "{response}");
    fx.cleanup().await;
}

// ---------------------------------------------------------------------
// 6. Fencing: missing fencing token, content-type mismatch.
// ---------------------------------------------------------------------

#[tokio::test]
async fn missing_fencing_header_is_invalid_request() {
    let fx = Fixture::new("missing-fence").await;
    let content = b"content".to_vec();
    fx.manifest("art-missing-fence", &content, None).await;

    let auth = fx.auth();
    let headers = [(auth.0.as_str(), auth.1.as_str())];
    let (status, response) = fx
        .put(&fx.content_uri("art-missing-fence"), content, &headers)
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{response}");
    assert_eq!(response["error"]["code"], "invalid_request");
    fx.cleanup().await;
}

#[tokio::test]
async fn content_type_mismatch_with_declared_media_type_is_rejected() {
    let fx = Fixture::new("content-type").await;
    let content = b"diff content".to_vec();
    fx.manifest("art-ct", &content, Some("text/x-diff")).await;

    // "application/json" does not match the manifest's declared text/x-diff.
    let (status, response) = fx
        .upload("art-ct", content, &[("content-type", "application/json")])
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{response}");
    assert_eq!(response["error"]["code"], "invalid_request");
    fx.cleanup().await;
}

// ---------------------------------------------------------------------
// 7. Operator-download edge cases.
// ---------------------------------------------------------------------

#[tokio::test]
async fn unverified_manifest_download_is_named_conflict_not_404() {
    let fx = Fixture::new("download-unverified").await;
    fx.manifest("art-unverified", b"content", None).await;

    let (attempt_number, request_id) = fx.attempt_location().await;
    let uri = format!(
        "/executions/{request_id}/attempts/{attempt_number}/artifacts/art-unverified/content"
    );
    let response = get_download(&fx.download_router(), &uri, true).await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let bytes = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["error"]["code"], "conflict");
    fx.cleanup().await;
}

#[tokio::test]
async fn download_without_an_operator_principal_is_unauthorized() {
    let (_app, repo, _clock, _item_id, storage_root_dir) = setup().await;
    let download_app = download_router(&repo, storage_root_dir.path());
    let response = get_download(
        &download_app,
        "/executions/req-x/attempts/1/artifacts/art-x/content",
        false,
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn artifact_download_subrouter_404s_for_unknown_artifact() {
    let (_app, repo, _clock, _item_id, storage_root_dir) = setup().await;
    let download_app = download_router(&repo, storage_root_dir.path());
    let uri = "/executions/does-not-exist/attempts/1/artifacts/art-x/content";
    let response = get_download(&download_app, uri, true).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------
// 8. A real, moderately large upload proves the per-route body-limit
//    override actually took effect (the router-wide 4 MiB JSON ceiling
//    would otherwise reject this before the handler's own streaming logic
//    ever ran).
// ---------------------------------------------------------------------

#[tokio::test]
async fn upload_over_default_json_body_ceiling_still_succeeds() {
    let fx = Fixture::new("large").await;
    // 6 MiB: comfortably over the 4 MiB router-wide DefaultBodyLimit meant
    // for JSON control-plane bodies, comfortably under the 50 MiB protocol
    // ceiling.
    let large_content = vec![7u8; 6 * 1024 * 1024];
    fx.manifest("art-large", &large_content, None).await;

    let (status, response) = fx.upload("art-large", large_content.clone(), &[]).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["size_bytes"], large_content.len());
    fx.cleanup().await;
}

fn hex_encode(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

// ---------------------------------------------------------------------
// 9. Redaction: event payloads and artifact content never reach logs.
//
// Mirrors `lifecycle.rs`'s `logs_never_contain_raw_credentials_only_ids`
// rig exactly (a real, process-global `tracing_subscriber::fmt` subscriber,
// captured per-test via a thread-local so parallel tests in this binary
// cannot see each other's output) — see `log_capture.rs`'s doc comment for
// why a *global* default subscriber, not a thread-local `set_default`, is
// what actually closes the race.
// ---------------------------------------------------------------------

/// Distinctive markers unlikely to appear anywhere in genuine log
/// scaffolding (ids, status words, etc.), so their absence from captured
/// output is a meaningful, non-vacuous assertion.
const SECRET_EVENT_MARKER: &str = "SECRET_EVENT_PAYLOAD_MARKER_1f9c7";
const SECRET_ARTIFACT_MARKER: &[u8] = b"SECRET_ARTIFACT_BYTES_MARKER_9e21c";

/// Common tail: the capture rig must have observed real output (non-vacuous:
/// the runner id, not a secret, is expected in genuine handler logs), and
/// none of it may be `secret`.
fn assert_redacted(text: &str, runner_id: &str, secret: &str, secret_label: &str) {
    assert!(
        !text.is_empty(),
        "capture rig must have observed real log output"
    );
    assert!(
        !text.contains(secret),
        "{secret_label} leaked into logs:\n{text}"
    );
    assert!(
        text.contains(runner_id),
        "expected the runner id to appear in captured logs:\n{text}"
    );
}

#[tokio::test]
async fn logs_never_leak_event_payload_text_only_ids() {
    let fx = Fixture::new("redaction-events").await;
    let (guard, captured) = CaptureGuard::start();
    let events = json!({
        "protocol_version": 1, "runner_id": fx.attempt.runner_id, "attempt_id": fx.attempt.attempt_id,
        "fencing_token": fx.attempt.fencing_token, "previous_checkpoint": Value::Null,
        "checkpoint": "checkpoint-redaction",
        "events": [{"event_id": "evt-redaction", "sequence": 1, "occurred_at": Utc::now().to_rfc3339(),
            "source": "runner", "kind": "message", "payload": {"text": SECRET_EVENT_MARKER}}],
    })
    .to_string();
    let uri = format!("/attempts/{}/events", fx.attempt.attempt_id);
    let auth = format!("Bearer {}", fx.attempt.credential);
    let (status, batch) =
        send_json(&fx.app, "POST", &uri, events, &[("authorization", &auth)]).await;
    assert_eq!(status, StatusCode::OK, "{batch}");
    drop(guard);

    let text = String::from_utf8_lossy(&captured.lock().unwrap()).into_owned();
    assert_redacted(
        &text,
        &fx.attempt.runner_id,
        SECRET_EVENT_MARKER,
        "event payload text",
    );
    fx.cleanup().await;
}

#[tokio::test]
async fn logs_never_leak_artifact_content_bytes_only_ids() {
    let fx = Fixture::new("redaction-artifact").await;
    fx.manifest("art-redaction", SECRET_ARTIFACT_MARKER, None)
        .await;

    let (guard, captured) = CaptureGuard::start();
    let (status, uploaded) = fx
        .upload("art-redaction", SECRET_ARTIFACT_MARKER.to_vec(), &[])
        .await;
    assert_eq!(status, StatusCode::OK, "{uploaded}");
    drop(guard);

    let text = String::from_utf8_lossy(&captured.lock().unwrap()).into_owned();
    let secret = std::str::from_utf8(SECRET_ARTIFACT_MARKER).unwrap();
    assert_redacted(
        &text,
        &fx.attempt.runner_id,
        secret,
        "artifact content bytes",
    );
    fx.cleanup().await;
}

/// The error path's `details` (e.g. `{"artifact_id": ...}`) must stay
/// id-only too. Same length as the mismatch marker so this exercises a
/// pure checksum mismatch, not the oversize path.
#[tokio::test]
async fn logs_never_leak_mismatched_content_on_error_path_only_ids() {
    let fx = Fixture::new("redaction-mismatch").await;
    let mismatch_marker: &[u8] = b"SECRET_MISMATCH_BYTES_MARKER_44a1";
    let declared_but_never_sent = vec![0u8; mismatch_marker.len()];
    fx.manifest("art-redaction-2", &declared_but_never_sent, None)
        .await;

    let (guard, captured) = CaptureGuard::start();
    let (status, _) = fx
        .upload("art-redaction-2", mismatch_marker.to_vec(), &[])
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    drop(guard);

    let text = String::from_utf8_lossy(&captured.lock().unwrap()).into_owned();
    let secret = std::str::from_utf8(mismatch_marker).unwrap();
    assert_redacted(
        &text,
        &fx.attempt.runner_id,
        secret,
        "mismatched artifact content",
    );
    fx.cleanup().await;
}
