//! Runner-protocol claim-through-completion attempt lifecycle over HTTP:
//! claim, accept/start, heartbeat, fencing, events, decisions, artifacts,
//! completion and recovery. Enrollment, refresh/credential-rotation and the
//! operator/runner auth non-substitution proof live in `enrollment.rs` —
//! split out once this file passed 1000 lines, since those claims never
//! touch an execution attempt at all. Each test builds its own router
//! directly from `runner_protocol::routes`, bypassing the production
//! router.

use tack_api::handlers::runner_protocol;

use std::sync::{Arc, Mutex};

use crate::common::send_str_strict as send;
use crate::log_capture::ensure_global_log_capture_installed;

use axum::{Router, http::StatusCode};
use chrono::{DateTime, Duration, TimeZone, Utc};
use serde_json::{Value, json};
use tack_core::models::{CreateItem, CreateProject, ProjectType};
use tack_db::{
    Repository, init_pool, migrations,
    repo::execution::{ExecutionClock, NewAgentProfile, NewExecutionRequest, NewRunner},
};
use uuid::Uuid;

const RUNNER_ID: &str = "runner-c2";
const RUNNER_CREDENTIAL: &str = "raw-test-runner-credential";
/// The explicit model every `enqueue_request()` call requests, matching
/// `full_capabilities()`'s declared combination — see the scheduler
/// wiring note on both functions.
const REQUESTED_MODEL_PROVIDER: &str = "openai";
const REQUESTED_MODEL_ID: &str = "opaque/model-c2";

// ---------------------------------------------------------------------
// Fake clock: every lease/heartbeat/expiry test below injects time rather
// than sleeping (rule 9).
// ---------------------------------------------------------------------

#[derive(Clone)]
struct FakeClock(Arc<Mutex<DateTime<Utc>>>);

impl FakeClock {
    fn new(start: DateTime<Utc>) -> Self {
        Self(Arc::new(Mutex::new(start)))
    }

    fn advance(&self, delta: Duration) {
        let mut guard = self.0.lock().unwrap();
        *guard += delta;
    }
}

impl ExecutionClock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap()
    }
}

// ---------------------------------------------------------------------
// Fixture: the router, repo, fake clock and a ready item, plus short
// methods for the runner-protocol calls this file repeats and the
// row-count/state queries the writes-nothing assertions check. Every
// method here corresponds 1:1 to an attempt-lifecycle route this suite
// exercises against `RUNNER_ID`/`RUNNER_CREDENTIAL`; enrollment and
// credential rotation live in `enrollment.rs`.
// ---------------------------------------------------------------------

struct Fixture {
    app: Router,
    repo: Repository,
    clock: FakeClock,
    item_id: String,
}

impl Fixture {
    async fn new() -> Self {
        // Must run before anything else in every test (see `log_capture.rs`'s
        // doc comment): it closes the race window between this binary's
        // tests by making sure no test's HTTP request can reach production
        // handler code before the one global `tracing` subscriber this
        // binary ever installs is in place.
        ensure_global_log_capture_installed();
        let pool = init_pool("sqlite::memory:").await.expect("pool");
        migrations::run_all(&pool).await.expect("migrations");
        let repo = Repository::new(pool);
        let workspace = Uuid::new_v4();
        sqlx::query("INSERT INTO workspaces (id,name,default_vocabulary) VALUES (?, 'C2', '{}')")
            .bind(workspace.to_string())
            .execute(repo.pool())
            .await
            .expect("workspace");
        let project = repo
            .create_project(
                workspace,
                CreateProject {
                    name: "C2".into(),
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

        let clock = FakeClock::new(Utc.with_ymd_and_hms(2026, 8, 6, 12, 0, 0).unwrap());
        let credential_hash = runner_protocol::runner_auth::credential_hash(RUNNER_CREDENTIAL);
        // `tack_orch::scheduler` is wired into the real claim path, so
        // `RUNNER_ID` must declare a real harness/model combination — an
        // empty `"{}"` snapshot would make every claim in this file
        // eligibility-reject before ever reaching the fencing/idempotency/
        // replay behavior these tests actually exist to prove.
        let capability_snapshot = full_capabilities(clock.now(), 2, 2).to_string();
        repo.register_runner(
            NewRunner {
                id: RUNNER_ID,
                name: "C2 Runner",
                credential_hash: &credential_hash,
                labels: "{}",
                total_capacity: 2,
                available_capacity: 2,
                capability_snapshot: &capability_snapshot,
                protocol_version: 1,
            },
            &clock,
        )
        .await
        .expect("runner");
        repo.create_agent_profile(
            NewAgentProfile {
                id: "profile-c2",
                name: "C2 Profile",
                instructions: "work safely",
                tool_policy: r#"{"mode":"safe"}"#,
                limits: r#"{"tokens":1000}"#,
            },
            &clock,
        )
        .await
        .expect("profile");

        // This router is tested in isolation from any operator config, so
        // `usize::MAX` here means "no additional global-config restriction"
        // — the effective limit still collapses to the fixed 4 MiB protocol
        // ceiling via `effective_body_limit_bytes`. The min-of-configured-
        // and-ceiling precedence itself is proven against the real
        // production router in `handlers/production_router.rs`, not here.
        let state =
            runner_protocol::RunnerProtocolState::new(repo.clone(), Arc::new(clock.clone()));
        let app = runner_protocol::routes(state, usize::MAX);
        Fixture {
            app,
            repo,
            clock,
            item_id: item.id.to_string(),
        }
    }

    async fn post(&self, uri: &str, body: String) -> (StatusCode, Value) {
        send(
            &self.app,
            "POST",
            uri,
            body,
            &[("authorization", &format!("Bearer {RUNNER_CREDENTIAL}"))],
        )
        .await
    }

    /// Enqueue a fresh execution request selecting `RUNNER_ID`.
    async fn enqueue(&self, key: &str) -> String {
        enqueue_request(
            &self.repo,
            &self.clock,
            &self.item_id,
            RUNNER_ID,
            "profile-c2",
            key,
        )
        .await
    }

    async fn claim(&self, claim_request_id: &str) -> Value {
        let (_, body) = self
            .post(
                "/claim",
                json!({
                    "protocol_version": 1, "runner_id": RUNNER_ID, "claim_request_id": claim_request_id,
                    "available_capacity": 2, "wait_ms": 1000,
                })
                .to_string(),
            )
            .await;
        body
    }

    /// Enqueue and claim in one step: `(request_id, attempt_id, fencing_token)`.
    async fn enqueue_and_claim(&self, key: &str, claim_request_id: &str) -> (String, String, i64) {
        let request_id = self.enqueue(key).await;
        let claimed = self.claim(claim_request_id).await;
        let attempt_id = claimed["lease"]["attempt_id"].as_str().unwrap().to_owned();
        let fencing_token = claimed["lease"]["fencing_token"].as_i64().unwrap();
        (request_id, attempt_id, fencing_token)
    }

    /// Enqueue, claim, accept and start: the attempt is left `running`, the
    /// state events/decisions/artifacts/completion require.
    async fn ready_running(&self, key: &str, claim_request_id: &str) -> (String, String, i64) {
        let (request_id, attempt_id, fencing_token) =
            self.enqueue_and_claim(key, claim_request_id).await;
        self.accept(&attempt_id, fencing_token).await;
        self.start(&attempt_id, fencing_token, "pid-1").await;
        (request_id, attempt_id, fencing_token)
    }

    async fn accept(&self, attempt_id: &str, fencing_token: i64) -> (StatusCode, Value) {
        self.post(
            &format!("/attempts/{attempt_id}/accept"),
            json!({
                "protocol_version": 1, "runner_id": RUNNER_ID, "attempt_id": attempt_id, "fencing_token": fencing_token,
                "workspace_id": "ws-1", "base_revision": "abc123def456abc123def456abc123def456abc",
            })
            .to_string(),
        )
        .await
    }

    async fn start(
        &self,
        attempt_id: &str,
        fencing_token: i64,
        process_id: &str,
    ) -> (StatusCode, Value) {
        self.post(
            &format!("/attempts/{attempt_id}/start"),
            json!({
                "protocol_version": 1, "runner_id": RUNNER_ID, "attempt_id": attempt_id, "fencing_token": fencing_token,
                "workspace_id": "ws-1", "base_revision": "abc123def456abc123def456abc123def456abc",
                "process_id": process_id,
            })
            .to_string(),
        )
        .await
    }

    async fn events(
        &self,
        attempt_id: &str,
        fencing_token: i64,
        checkpoint: &str,
        previous_checkpoint: Value,
        events: Value,
    ) -> (StatusCode, Value) {
        self.post(
            &format!("/attempts/{attempt_id}/events"),
            json!({
                "protocol_version": 1, "runner_id": RUNNER_ID, "attempt_id": attempt_id, "fencing_token": fencing_token,
                "previous_checkpoint": previous_checkpoint, "checkpoint": checkpoint, "events": events,
            })
            .to_string(),
        )
        .await
    }

    async fn decision(
        &self,
        attempt_id: &str,
        fencing_token: i64,
        decision_id: &str,
        prompt: &str,
    ) -> (StatusCode, Value) {
        self.post(
            &format!("/attempts/{attempt_id}/decisions"),
            json!({
                "protocol_version": 1, "runner_id": RUNNER_ID, "attempt_id": attempt_id, "fencing_token": fencing_token,
                "decision_id": decision_id, "kind": "tool_permission", "prompt": prompt,
                "options": [{"option_id":"allow_once","label":"Allow once"},{"option_id":"deny","label":"Deny"}],
                "expires_at": Value::Null, "metadata": {"tool": "cargo test"},
            })
            .to_string(),
        )
        .await
    }

    async fn poll(&self, attempt_id: &str, fencing_token: i64, after: &str) -> (StatusCode, Value) {
        self.post(
            &format!("/attempts/{attempt_id}/decisions/poll"),
            json!({
                "protocol_version": 1, "runner_id": RUNNER_ID, "attempt_id": attempt_id,
                "fencing_token": fencing_token, "after": after,
            })
            .to_string(),
        )
        .await
    }

    /// Resolve `decision_id` directly at the row level, standing in for the
    /// operator decision-resolution endpoint this file does not exercise.
    async fn resolve_decision(&self, attempt_id: &str, decision_id: &str, option_id: &str) {
        sqlx::query(
            "UPDATE execution_decisions SET state='resolved', answer=?, resolved_at=?, resolved_by=?, updated_at=? WHERE attempt_id=? AND decision_id=?",
        )
        .bind(json!({"option_id": option_id, "text": Value::Null}).to_string())
        .bind(self.clock.now().to_rfc3339())
        .bind(json!({"kind": "operator", "subject_id": "local-admin"}).to_string())
        .bind(self.clock.now().to_rfc3339())
        .bind(attempt_id)
        .bind(decision_id)
        .execute(self.repo.pool())
        .await
        .unwrap();
    }

    async fn artifact(
        &self,
        attempt_id: &str,
        fencing_token: i64,
        artifact_id: &str,
        sha256: &str,
    ) -> (StatusCode, Value) {
        self.post(
            &format!("/attempts/{attempt_id}/artifacts"),
            artifact_body(RUNNER_ID, attempt_id, fencing_token, artifact_id, sha256),
        )
        .await
    }

    /// The raw completion body [`Fixture::complete_default`] sends: a fixed
    /// 5-minute duration and no checkpoint, for tests that don't care about
    /// those values but need the raw string to tamper with.
    fn default_completion_body(
        &self,
        attempt_id: &str,
        fencing_token: i64,
        completion_id: &str,
    ) -> String {
        let started_at = self.clock.now() - Duration::minutes(5);
        completion_body(
            RUNNER_ID,
            attempt_id,
            fencing_token,
            completion_id,
            started_at,
            self.clock.now(),
            None,
        )
    }

    async fn complete_default(
        &self,
        attempt_id: &str,
        fencing_token: i64,
        completion_id: &str,
    ) -> (StatusCode, Value) {
        self.post(
            &format!("/attempts/{attempt_id}/completion"),
            self.default_completion_body(attempt_id, fencing_token, completion_id),
        )
        .await
    }

    async fn heartbeat(&self, heartbeat_id: &str, capacity: i64) -> (StatusCode, Value) {
        self.post(
            "/heartbeat",
            json!({
                "protocol_version": 1, "runner_id": RUNNER_ID, "heartbeat_id": heartbeat_id,
                "sent_at": self.clock.now().to_rfc3339(), "available_capacity": capacity, "active_attempts": [],
            })
            .to_string(),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn recovery_observation(
        &self,
        attempt_id: &str,
        fencing_token: i64,
        observation: &str,
        journal_state: &str,
        process_observed: bool,
    ) -> (StatusCode, Value) {
        self.post(
            &format!("/attempts/{attempt_id}/recovery-observation"),
            json!({
                "protocol_version": 1, "runner_id": RUNNER_ID, "attempt_id": attempt_id, "fencing_token": fencing_token,
                "recovery_key": format!("recovery:{attempt_id}:{fencing_token}:{observation}"),
                "observation": observation,
                "details": {"journal_state": journal_state, "process_observed": process_observed},
            })
            .to_string(),
        )
        .await
    }

    async fn count(&self, table: &str, attempt_id: &str) -> i64 {
        sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT COUNT(*) FROM {table} WHERE attempt_id=?"
        )))
        .bind(attempt_id)
        .fetch_one(self.repo.pool())
        .await
        .unwrap()
    }

    async fn attempt_state(&self, attempt_id: &str) -> String {
        sqlx::query_scalar("SELECT state FROM execution_attempts WHERE id=?")
            .bind(attempt_id)
            .fetch_one(self.repo.pool())
            .await
            .unwrap()
    }

    async fn request_state(&self, request_id: &str) -> String {
        sqlx::query_scalar("SELECT state FROM execution_requests WHERE id=?")
            .bind(request_id)
            .fetch_one(self.repo.pool())
            .await
            .unwrap()
    }

    async fn checkpoint(&self, attempt_id: &str) -> Option<String> {
        sqlx::query_scalar("SELECT event_checkpoint FROM execution_attempts WHERE id=?")
            .bind(attempt_id)
            .fetch_one(self.repo.pool())
            .await
            .unwrap()
    }

    async fn available_capacity(&self) -> i64 {
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id=?")
            .bind(RUNNER_ID)
            .fetch_one(self.repo.pool())
            .await
            .unwrap()
    }
}

/// Enqueues an execution request selecting `runner_id`, matching the frozen
/// snapshot shape exactly (mirrors what `create_execution` normalizes).
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
        "created_by": {"source": "test", "subject_id": "c2-test"},
        "created_at": created_at.to_rfc3339(),
        "selector": {"kind": "exact_runner", "runner_id": runner_id},
        "agent_profile_id": agent_profile_id,
        "resolved_agent_profile": {"name":"C2 Profile","instructions":"work safely","tool_policy":{"mode":"safe"},"timeout_seconds":60,"budgets":{"tokens":1000}},
        "requested_harness_kind": "codex",
        // Explicit, matching `full_capabilities()`'s declared combination —
        // see `REQUESTED_MODEL_PROVIDER`/`REQUESTED_MODEL_ID`'s doc comment.
        "requested_model_provider": REQUESTED_MODEL_PROVIDER,
        "requested_model_id": REQUESTED_MODEL_ID,
        "repository": {"kind":"git","remote":"https://example.test/c2.git","base_revision":"abc123def456abc123def456abc123def456abc","subdirectory": Value::Null},
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

/// One runner-reported event, wrapped in the single-element array shape
/// every `Fixture::events` call sends.
fn one_event(event_id: &str, sequence: i64, occurred_at: DateTime<Utc>, payload: &Value) -> Value {
    json!([{
        "event_id": event_id, "sequence": sequence, "occurred_at": occurred_at.to_rfc3339(),
        "source": "runner", "kind": "progress", "payload": payload,
    }])
}

/// `harnesses` declares "codex"/"openai"/`REQUESTED_MODEL_ID` — every
/// `enqueue_request()` call in this file requests exactly that pair (see
/// its own doc comment) so the real scheduler finds this
/// runner eligible, the same way a real runner's declared capabilities
/// would need to match a real request for a claim to succeed.
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

fn artifact_body(
    runner_id: &str,
    attempt_id: &str,
    fencing_token: i64,
    artifact_id: &str,
    sha256: &str,
) -> String {
    json!({
        "protocol_version": 1,
        "runner_id": runner_id,
        "attempt_id": attempt_id,
        "fencing_token": fencing_token,
        "artifacts": [{
            "artifact_id": artifact_id,
            "kind": "patch",
            "name": "changes.patch",
            "media_type": "text/x-diff",
            "size_bytes": 12,
            "sha256": sha256,
            "content_disposition": "inline_upload",
            "metadata": {"base_revision": "abc123"},
        }],
    })
    .to_string()
}

#[allow(clippy::too_many_arguments)]
fn completion_body(
    runner_id: &str,
    attempt_id: &str,
    fencing_token: i64,
    completion_id: &str,
    started_at: DateTime<Utc>,
    ended_at: DateTime<Utc>,
    final_event_checkpoint: Option<&str>,
) -> String {
    json!({
        "protocol_version": 1,
        "runner_id": runner_id,
        "attempt_id": attempt_id,
        "fencing_token": fencing_token,
        "completion_id": completion_id,
        "terminal_state": "succeeded",
        "terminal_reason": {"code": "completed", "message": "Harness exited successfully"},
        "actual_execution": {
            "harness_kind": "codex",
            "harness_version": "1.2.3",
            "model_provider": "openai",
            "model_id": "opaque/model-alpha",
            "model_observation_source": "harness_reported",
            "capability_snapshot": {
                "cancel": {"support": "supported", "reason": null},
                "resume": {"support": "unsupported", "reason": "no resumable session contract"},
                "decisions": {"support": "supported", "reason": null},
                "artifacts": {"support": "supported", "reason": null},
                "usage": {"support": "advisory", "reason": "usage may be absent"},
            },
            "workspace_id": "ws-1",
            "base_revision": "abc123def456abc123def456abc123def456abc",
            "started_at": started_at.to_rfc3339(),
            "ended_at": ended_at.to_rfc3339(),
        },
        "usage": {
            "tokens_in": {"value": 1234, "source": "measured"},
            "tokens_out": {"value": 456, "source": "measured"},
            "duration_ms": {"value": 295000, "source": "measured"},
            "cost_usd": {"value": null, "source": "not_measured"},
        },
        "final_event_checkpoint": final_event_checkpoint,
    })
    .to_string()
}

// ---------------------------------------------------------------------
// 1. Accept is idempotent (an exact replay returns the original commit),
//    and start transitions a `preparing` attempt to `running`.
// ---------------------------------------------------------------------

#[tokio::test]
async fn accept_is_idempotent_and_start_transitions_to_running() {
    let fx = Fixture::new().await;
    let (_, attempt_id, fencing_token) = fx.enqueue_and_claim("accept-key", "claim-accept").await;

    let (status, accepted) = fx.accept(&attempt_id, fencing_token).await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    assert_eq!(accepted["state"], "preparing");
    assert_eq!(accepted["replayed"], false);
    let (status, replayed) = fx.accept(&attempt_id, fencing_token).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replayed["replayed"], true);
    assert_eq!(replayed["committed_at"], accepted["committed_at"]);

    let (status, started) = fx.start(&attempt_id, fencing_token, "pid-123").await;
    assert_eq!(status, StatusCode::OK, "{started}");
    assert_eq!(started["state"], "running");
}

// ---------------------------------------------------------------------
// 2. Stale/expired fence writes nothing and returns `stale_lease`.
// ---------------------------------------------------------------------

#[tokio::test]
async fn stale_and_expired_fence_write_nothing() {
    let fx = Fixture::new().await;
    let (_, attempt_id, fencing_token) = fx.enqueue_and_claim("stale-key", "claim-stale").await;

    let (status, body) = fx
        .events(
            &attempt_id,
            fencing_token + 1,
            "cp-1",
            Value::Null,
            json!([]),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "stale_lease");
    assert_eq!(
        fx.count("execution_events", &attempt_id).await,
        0,
        "wrong fence writes nothing"
    );

    // Expired lease, correct fencing token: never heartbeat, just advance
    // the fake clock past `lease_duration_seconds` (60s).
    fx.clock.advance(Duration::seconds(61));
    let (status, body) = fx
        .events(&attempt_id, fencing_token, "cp-1", Value::Null, json!([]))
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "stale_lease");
    assert_eq!(
        fx.count("execution_events", &attempt_id).await,
        0,
        "expired lease writes nothing"
    );
    assert_eq!(
        fx.checkpoint(&attempt_id).await,
        None,
        "attempt row is untouched"
    );
}

// ---------------------------------------------------------------------
// 3. Heartbeat replay returns the original success; a same-id retry with
//    different content is a stable, distinct conflict code. (The
//    completion analogue of this claim is pinned once, in
//    `completion_replay_changed_content_is_idempotency_conflict` below —
//    not duplicated here.)
// ---------------------------------------------------------------------

#[tokio::test]
async fn heartbeat_replay_succeeds_conflicting_retry_rejected() {
    let fx = Fixture::new().await;
    fx.enqueue_and_claim("replay-key", "claim-replay").await;
    let (status, first) = fx.heartbeat("hb-replay", 1).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    let (status, replay) = fx.heartbeat("hb-replay", 1).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        replay["accepted_at"], first["accepted_at"],
        "exact replay returns the original success"
    );
    let (status, conflicting) = fx.heartbeat("hb-replay", 2).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(conflicting["error"]["code"], "idempotency_conflict");
}

// ---------------------------------------------------------------------
// 4. Oversized event batch writes nothing at all.
// ---------------------------------------------------------------------

#[tokio::test]
async fn oversized_event_batch_writes_nothing() {
    let fx = Fixture::new().await;
    let (_, attempt_id, fencing_token) = fx
        .enqueue_and_claim("oversized-key", "claim-oversized")
        .await;

    // Over `event_batch_count_max` (100 tiny events), and over the whole-body
    // byte cap (`json_body_bytes_max` == `event_batch_bytes_max`, both 1
    // MiB, one oversized-payload event) — both reject with 413 and write
    // nothing.
    let too_many_events: Vec<Value> = (0..101)
        .map(|i| json!({"event_id": format!("evt-{i}"), "sequence": i, "occurred_at": fx.clock.now().to_rfc3339(), "source":"runner","kind":"progress","payload":{}}))
        .collect();
    let huge_event = json!([{"event_id":"evt-huge","sequence":1,"occurred_at":fx.clock.now().to_rfc3339(),"source":"runner","kind":"progress","payload":{"blob":"x".repeat(2*1_048_576)}}]);

    for (events, limit_name) in [
        (json!(too_many_events), Some("event_batch_count_max")),
        (huge_event, None),
    ] {
        let (status, body) = fx
            .events(&attempt_id, fencing_token, "cp-1", Value::Null, events)
            .await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{body}");
        assert_eq!(body["error"]["code"], "payload_too_large");
        if let Some(limit_name) = limit_name {
            assert_eq!(body["error"]["details"]["limit"], limit_name);
        }
    }
    assert_eq!(
        fx.count("execution_events", &attempt_id).await,
        0,
        "an oversized batch writes nothing, not just a 413"
    );
    assert_eq!(fx.checkpoint(&attempt_id).await, None);
}

// ---------------------------------------------------------------------
// 5. Reusing a decision_id/artifact_id with different content is an
//    idempotency conflict (a compensating check for the
//    `ON CONFLICT DO NOTHING` inserts). Two claims, same shape, split so
//    each stays a single-concern test.
// ---------------------------------------------------------------------

#[tokio::test]
async fn decision_id_reuse_different_content_is_idempotency_conflict() {
    let fx = Fixture::new().await;
    let (_, attempt_id, fencing_token) = fx.ready_running("reuse-key", "claim-reuse").await;

    let (status, first) = fx
        .decision(&attempt_id, fencing_token, "dec-reuse", "Allow A?")
        .await;
    assert_eq!(status, StatusCode::OK, "{first}");
    let (status, exact_replay) = fx
        .decision(&attempt_id, fencing_token, "dec-reuse", "Allow A?")
        .await;
    assert_eq!(status, StatusCode::OK, "an exact replay is not a conflict");
    assert_eq!(exact_replay["created_at"], first["created_at"]);
    let (status, conflict) = fx
        .decision(&attempt_id, fencing_token, "dec-reuse", "Allow B?")
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(conflict["error"]["code"], "idempotency_conflict");

    let stored_prompt: String = sqlx::query_scalar(
        "SELECT prompt FROM execution_decisions WHERE attempt_id=? AND decision_id='dec-reuse'",
    )
    .bind(&attempt_id)
    .fetch_one(fx.repo.pool())
    .await
    .unwrap();
    assert_eq!(
        stored_prompt, "Allow A?",
        "the conflicting retry did not overwrite the original"
    );
}

#[tokio::test]
async fn artifact_id_reuse_different_content_is_idempotency_conflict() {
    let fx = Fixture::new().await;
    let (_, attempt_id, fencing_token) = fx.ready_running("reuse-art-key", "claim-reuse-art").await;
    let sha_a = "a".repeat(64);
    let sha_b = "b".repeat(64);

    let (status, _) = fx
        .artifact(&attempt_id, fencing_token, "art-reuse", &sha_a)
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, conflict) = fx
        .artifact(&attempt_id, fencing_token, "art-reuse", &sha_b)
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(conflict["error"]["code"], "idempotency_conflict");

    let stored_sha: String = sqlx::query_scalar(
        "SELECT sha256 FROM execution_artifacts WHERE attempt_id=? AND artifact_id='art-reuse'",
    )
    .bind(&attempt_id)
    .fetch_one(fx.repo.pool())
    .await
    .unwrap();
    assert_eq!(stored_sha, sha_a);
}

// ---------------------------------------------------------------------
// 6. Event batch happy path: commits the checkpoint and persists events.
// ---------------------------------------------------------------------

#[tokio::test]
async fn event_batch_commits_checkpoint_and_persists_events() {
    let fx = Fixture::new().await;
    let (_, attempt_id, fencing_token) = fx.enqueue_and_claim("events-key", "claim-events").await;
    let events = json!([{
        "event_id": "evt-1", "sequence": 1, "occurred_at": fx.clock.now().to_rfc3339(),
        "source": "runner", "kind": "progress", "payload": {"phase": "testing"},
    }]);

    let (status, batch) = fx
        .events(
            &attempt_id,
            fencing_token,
            "checkpoint-0001",
            Value::Null,
            events,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{batch}");
    assert_eq!(batch["accepted_event_ids"], json!(["evt-1"]));
    assert_eq!(batch["committed_checkpoint"], "checkpoint-0001");
    assert_eq!(fx.count("execution_events", &attempt_id).await, 1);
}

// ---------------------------------------------------------------------
// 7. Decision poll returns the pending decision, then the resolved
//     answer once it is resolved, advancing `next_after` each time.
// ---------------------------------------------------------------------

#[tokio::test]
async fn decision_poll_returns_pending_then_resolved() {
    let fx = Fixture::new().await;
    let (_, attempt_id, fencing_token) = fx.ready_running("decision-key", "claim-decision").await;
    let (status, decision) = fx
        .decision(&attempt_id, fencing_token, "dec-1", "Allow?")
        .await;
    assert_eq!(status, StatusCode::OK, "{decision}");
    assert_eq!(decision["state"], "pending");
    let created_at = decision["created_at"].as_str().unwrap().to_owned();

    let before = (fx.clock.now() - Duration::hours(1)).to_rfc3339();
    let (status, first_poll) = fx.poll(&attempt_id, fencing_token, &before).await;
    assert_eq!(status, StatusCode::OK, "{first_poll}");
    assert_eq!(first_poll["decisions"][0]["decision_id"], "dec-1");
    assert_eq!(first_poll["decisions"][0]["state"], "pending");
    let next_after = first_poll["next_after"].as_str().unwrap().to_owned();
    assert_eq!(next_after, created_at);

    fx.clock.advance(Duration::seconds(30));
    fx.resolve_decision(&attempt_id, "dec-1", "allow_once")
        .await;
    let (status, second_poll) = fx.poll(&attempt_id, fencing_token, &next_after).await;
    assert_eq!(status, StatusCode::OK, "{second_poll}");
    assert_eq!(second_poll["decisions"][0]["state"], "resolved");
    assert_eq!(
        second_poll["decisions"][0]["answer"]["option_id"],
        "allow_once"
    );
    assert_ne!(second_poll["next_after"].as_str().unwrap(), next_after);
}

// ---------------------------------------------------------------------
// 8. Artifact manifest is accepted and names a PUT upload target.
// ---------------------------------------------------------------------

#[tokio::test]
async fn artifact_manifest_is_accepted_with_put_upload_target() {
    let fx = Fixture::new().await;
    let (_, attempt_id, fencing_token) = fx.ready_running("manifest-key", "claim-manifest").await;
    let sha256 = "f".repeat(64);
    let (status, artifacts) = fx
        .artifact(&attempt_id, fencing_token, "art-1", &sha256)
        .await;
    assert_eq!(status, StatusCode::OK, "{artifacts}");
    assert_eq!(artifacts["artifacts"][0]["state"], "manifest_accepted");
    assert_eq!(artifacts["artifacts"][0]["upload"]["method"], "PUT");
}

// ---------------------------------------------------------------------
// 9. Completion is idempotent (exact replay returns the original commit)
//     and restores the runner's available capacity exactly once.
// ---------------------------------------------------------------------

#[tokio::test]
async fn completion_is_idempotent_and_restores_capacity_once() {
    let fx = Fixture::new().await;
    let (request_id, attempt_id, fencing_token) = fx
        .enqueue_and_claim("completion-key", "claim-completion")
        .await;
    let (status, completed) = fx
        .complete_default(&attempt_id, fencing_token, "complete-1")
        .await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    assert_eq!(completed["state"], "succeeded");
    assert_eq!(completed["replayed"], false);

    let (status, replayed) = fx
        .complete_default(&attempt_id, fencing_token, "complete-1")
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replayed["replayed"], true);
    assert_eq!(replayed["committed_at"], completed["committed_at"]);

    assert_eq!(fx.request_state(&request_id).await, "succeeded");
    assert_eq!(
        fx.available_capacity().await,
        2,
        "capacity is restored exactly once"
    );
}

// ---------------------------------------------------------------------
// 10. Recovery observation: a proven pre-spawn-stopped observation safely
//     requeues the request, and replays idempotently.
// ---------------------------------------------------------------------

#[tokio::test]
async fn recovery_observation_requeues_and_replays_idempotently() {
    let fx = Fixture::new().await;
    let (request_id, attempt_id, fencing_token) =
        fx.enqueue_and_claim("recovery-key", "claim-recovery").await;

    let (status, applied) = fx
        .recovery_observation(
            &attempt_id,
            fencing_token,
            "process_stopped",
            "prepared",
            false,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{applied}");
    assert_eq!(applied["disposition"], "safe_pre_spawn_requeue");
    assert_eq!(applied["replayed"], false);
    assert_eq!(fx.attempt_state(&attempt_id).await, "lost");
    assert_eq!(fx.request_state(&request_id).await, "queued");

    let (status, replayed) = fx
        .recovery_observation(
            &attempt_id,
            fencing_token,
            "process_stopped",
            "prepared",
            false,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replayed["replayed"], true);
    assert_eq!(replayed["committed_at"], applied["committed_at"]);
}

// ---------------------------------------------------------------------
// 11. `EventApplyResult` splits two causes that used to collapse into one
//     `ReplayConflict`: a benign, out-of-order resync (mismatched
//     `previous_checkpoint`) is the retryable `conflict` code
//     (`StableErrorCode::retryable`, `crates/tack-orch/src/execution/types.rs`);
//     reusing the same `(attempt_id, checkpoint)` key with genuinely
//     different event content is the non-retryable `idempotency_conflict`.
//     Both write nothing.
// ---------------------------------------------------------------------

struct EventConflictCase {
    key: &'static str,
    cp: &'static str,
    prev: Value,
    event_id: &'static str,
    payload: Value,
    code: &'static str,
}

#[tokio::test]
async fn event_batch_conflict_vs_idempotency_conflict_retryable() {
    let fx = Fixture::new().await;
    let cases = [
        EventConflictCase {
            key: "conflict-retryable-key",
            cp: "cp-2",
            prev: json!("stale-checkpoint"),
            event_id: "evt-2",
            payload: json!({}),
            code: "conflict",
        },
        EventConflictCase {
            key: "event-idempotency-key",
            cp: "cp-1",
            prev: Value::Null,
            event_id: "evt-1",
            payload: json!({"note": "CHANGED"}),
            code: "idempotency_conflict",
        },
    ];

    for case in cases {
        let claim_id = format!("claim-{}", case.key);
        let (_, attempt_id, fencing_token) = fx.enqueue_and_claim(case.key, &claim_id).await;
        let first = one_event("evt-1", 1, fx.clock.now(), &json!({"note": "original"}));
        let (status, body) = fx
            .events(&attempt_id, fencing_token, "cp-1", Value::Null, first)
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");

        let retry = one_event(case.event_id, 2, fx.clock.now(), &case.payload);
        let (status, body) = fx
            .events(&attempt_id, fencing_token, case.cp, case.prev, retry)
            .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["error"]["code"], case.code);
        assert_eq!(
            body["error"]["retryable"],
            case.code == "conflict",
            "{}",
            case.code
        );
        assert_eq!(
            fx.count("execution_events", &attempt_id).await,
            1,
            "no new events"
        );
        assert_eq!(fx.checkpoint(&attempt_id).await.as_deref(), Some("cp-1"));
    }
}

// ---------------------------------------------------------------------
// 12. The completion analogue of test 5: reusing the same
//     idempotency-scoped `(attempt_id, completion_id)` key with genuinely
//     different terminal content is `idempotency_conflict`
//     (`retryable: false`), distinct from the benign, retryable `conflict`
//     a distinct completion_id racing a concurrent terminal write would
//     produce.
// ---------------------------------------------------------------------

#[tokio::test]
async fn completion_replay_changed_content_is_idempotency_conflict() {
    let fx = Fixture::new().await;
    let (_, attempt_id, fencing_token) = fx
        .enqueue_and_claim("completion-idempotency-key", "claim-completion-idem")
        .await;

    let (status, committed) = fx
        .complete_default(&attempt_id, fencing_token, "completion-idem")
        .await;
    assert_eq!(status, StatusCode::OK, "{committed}");

    // Same `completion_id`, but a changed `terminal_state`.
    let changed = fx
        .default_completion_body(&attempt_id, fencing_token, "completion-idem")
        .replace("succeeded", "failed");
    let (status, body) = fx
        .post(&format!("/attempts/{attempt_id}/completion"), changed)
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "idempotency_conflict");
    assert_eq!(
        body["error"]["retryable"], false,
        "must be non-retryable: {body}"
    );
    assert_eq!(
        fx.count("execution_completion_replays", &attempt_id).await,
        1,
        "no 2nd replay row"
    );
    assert_eq!(
        fx.attempt_state(&attempt_id).await,
        "succeeded",
        "outcome unchanged"
    );
}

// ---------------------------------------------------------------------
// 13. State-gate alignment, table-driven over the two non-terminal
//     "stop trusting this runner" states: `submit_artifacts` must reject
//     `lost` and `needs_operator` attempts exactly like `create_decision`
//     already does. In both cases the lease itself has not expired (the
//     fake clock never advances), so a rejection can only come from the
//     state gate, not `stale_lease`.
// ---------------------------------------------------------------------

struct BlockedStateCase {
    key: &'static str,
    observation: &'static str,
    journal_state: &'static str,
    process_observed: bool,
    disposition: &'static str,
    state: &'static str,
}

#[tokio::test]
async fn submit_artifacts_rejects_lost_and_needs_operator_states() {
    let fx = Fixture::new().await;
    let cases = [
        BlockedStateCase {
            key: "lost-key",
            observation: "process_stopped",
            journal_state: "prepared",
            process_observed: false,
            disposition: "safe_pre_spawn_requeue",
            state: "lost",
        },
        BlockedStateCase {
            key: "needs-operator-key",
            observation: "process_running",
            journal_state: "process_observed_running",
            process_observed: true,
            disposition: "needs_operator",
            state: "needs_operator",
        },
    ];

    for case in cases {
        let claim_id = format!("claim-{}", case.key);
        let (_, attempt_id, fencing_token) = fx.enqueue_and_claim(case.key, &claim_id).await;
        let (obs, journal, observed) =
            (case.observation, case.journal_state, case.process_observed);
        let (status, recovered) = fx
            .recovery_observation(&attempt_id, fencing_token, obs, journal, observed)
            .await;
        assert_eq!(status, StatusCode::OK, "{recovered}");
        assert_eq!(recovered["disposition"], case.disposition);
        assert_eq!(fx.attempt_state(&attempt_id).await, case.state);

        let (status, body) = fx
            .artifact(&attempt_id, fencing_token, "art-blocked", &"c".repeat(64))
            .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["error"]["code"], "conflict", "{body}");
        assert_eq!(
            fx.count("execution_artifacts", &attempt_id).await,
            0,
            "{}",
            case.state
        );
    }
}
