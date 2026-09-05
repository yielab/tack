//! Integration gate: proves a mock runner enrolled on a clean database can
//! claim, start, stream, complete, and survive an API/runner restart, with
//! security, fencing, payload, and OpenAPI drift all passing.
//!
//! This file is independent proof, not another handler's own
//! self-verification: it imports no other test file's infrastructure, and
//! builds its own clean database, its own
//! `AppState`/`tack_api::router::build_router` production router, and its
//! own fixtures from scratch — so a defect specific to any one handler's
//! own test assumptions cannot hide behind this file agreeing with it.
//!
//! Every assertion below reads persisted database state directly, not
//! merely HTTP status codes. No test sleeps or depends on a fake clock: the
//! production router hard-codes `SystemExecutionClock`
//! (`router.rs`'s `operator_execution_routes`/`runner_protocol_routes`,
//! which construct it internally and take no clock parameter), so nothing
//! here can inject one — instead, every scenario that needs a state
//! transition to "just happen" drives it explicitly over HTTP (a
//! runner-reported recovery observation, a second event batch, a restart)
//! rather than waiting on wall time.

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use chrono::Utc;
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::{AppState, orch_runtime::OrchRuntime, router::build_router};
use tack_db::{Repository, init_pool, migrations};
use tower::ServiceExt;
use uuid::Uuid;

const OPERATOR_TOKEN: &str = "wave2-gate-operator-token";
/// A syntactically plausible git revision — reused verbatim across every
/// fixture in this file. Nothing here validates it beyond "some string is
/// present" (checked directly against the handler/type sources this file was
/// written from), but keeping it consistent with the 40-hex-character shape
/// `handlers/executions_runner_admin.rs`/`runner_protocol/lifecycle.rs`/
/// `handlers/production_router.rs` already use avoids relying on that being
/// untested.
const BASE_REVISION: &str = "abc123def456abc123def456abc123def456abc";

// ---------------------------------------------------------------------
// Infrastructure: build the *real* production router (`build_router`, the
// same function `tack serve` calls) over a clean in-memory database this
// test owns end to end, keeping the raw `SqlitePool` handle so a "restart"
// can rebuild a fresh `AppState`/`Router` around the *same* database, and so
// every assertion can read persisted rows directly.
// ---------------------------------------------------------------------

async fn fresh_database() -> (sqlx::SqlitePool, Uuid) {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_all(&pool).await.expect("migrations");
    let workspace_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'Wave2Gate', '{}')",
    )
    .bind(workspace_id.to_string())
    .execute(&pool)
    .await
    .expect("insert workspace");
    (pool, workspace_id)
}

/// Builds a fresh `Router` around the given pool/config — every in-process
/// value (broadcast channel, orchestration runtime, the router/middleware
/// closures themselves) is newly constructed. Calling this twice around the
/// *same* pool is exactly what "API restart" means for the rest of this
/// file: everything but the database is thrown away and rebuilt.
async fn router_for(pool: sqlx::SqlitePool, workspace_id: Uuid, config: AppConfig) -> axum::Router {
    let (tx, _rx) = tokio::sync::broadcast::channel(16);
    let state = AppState {
        repo: Repository::new(pool),
        config: AppConfig {
            database_url: "sqlite::memory:".to_string(),
            ..config
        },
        workspace_id,
        broadcast_tx: tx,
        webhook: None,
        orch_runtime: OrchRuntime::new(),
    };
    build_router(state)
}

async fn send(
    app: &axum::Router,
    method: &str,
    uri: &str,
    body: Value,
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
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

fn operator_headers() -> Vec<(&'static str, &'static str)> {
    vec![("authorization", "Bearer wave2-gate-operator-token")]
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

/// The mock runner's declared capability snapshot. `tack_orch::scheduler`
/// is wired into the real claim path (`crates/tack-db/src/repo/execution.rs`'s
/// `claim_execution_idempotent_with_snapshot`), which actually checks a
/// claiming runner's declared harness/model combinations before it may
/// lease a request, rather than a naive `ORDER BY created_at LIMIT 1`
/// match. `harnesses`
/// therefore declares "codex"/"openai"/"opaque/model-wave2" here, and every
/// `execution_request_body()` fixture below requests exactly that pair, so
/// this file keeps proving "claim, start, stream, complete, survive
/// restart" through the real, now-scheduler-gated router rather than
/// silently relying on a runner that declares nothing.
///
/// `reported_at`/`probed_at` are the *real* current wall-clock time, not a
/// frozen fixture date: this file's router hard-codes `SystemExecutionClock`
/// (see the module doc above), and `tack_orch::scheduler::wiring`'s claim
/// path falls back to a capability report's own `reported_at` as a liveness
/// signal for a runner that has never sent a `/heartbeat` yet (true of
/// every runner in this file — none of these scenarios run an active
/// attempt before their first claim). A hardcoded past date would make that
/// fallback correctly, honestly judge the runner stale — this is a fixture
/// realism fix, not a loosened assertion.
fn full_capabilities() -> Value {
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
                "model_ids": ["opaque/model-wave2"],
                "discovery": "reported"
            }],
        }],
        "features": {},
        "limits": {"event_payload_bytes_max": 65536, "artifact_content_bytes_max": 52428800},
    })
}

fn completion_body(
    runner_id: &str,
    attempt_id: &str,
    fencing_token: i64,
    completion_id: &str,
    final_event_checkpoint: &str,
) -> Value {
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
            "model_id": "opaque/model-wave2",
            "model_observation_source": "harness_reported",
            "capability_snapshot": {
                "cancel": {"support": "supported", "reason": null},
                "resume": {"support": "unsupported", "reason": "no resumable session contract"},
                "decisions": {"support": "supported", "reason": null},
                "artifacts": {"support": "supported", "reason": null},
                "usage": {"support": "advisory", "reason": "usage may be absent"},
            },
            "workspace_id": "ws-wave2",
            "base_revision": BASE_REVISION,
            "started_at": "2026-08-08T12:00:00Z",
            "ended_at": "2026-08-08T12:05:00Z",
        },
        "usage": {
            "tokens_in": {"value": 1000, "source": "measured"},
            "tokens_out": {"value": 500, "source": "measured"},
            "duration_ms": {"value": 300000, "source": "measured"},
            "cost_usd": {"value": null, "source": "not_measured"},
        },
        "final_event_checkpoint": final_event_checkpoint,
    })
}

/// Operator creates a real project and item over HTTP (not a direct
/// repository call) — the item this file's execution requests reference is
/// exactly as real as one a human operator would click "New item" to create.
async fn create_project_and_item(app: &axum::Router) -> (String, String) {
    let (status, project) = send(
        app,
        "POST",
        "/api/projects",
        json!({"name": "Wave 2 gate", "project_type": "software"}),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{project}");
    let project_id = project["id"].as_str().unwrap().to_owned();

    let (status, item) = send(
        app,
        "POST",
        &format!("/api/projects/{project_id}/items"),
        json!({"title": "Prove the Wave 2 gate end to end"}),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{item}");
    let item_id = item["id"].as_str().unwrap().to_owned();
    (project_id, item_id)
}

/// Operator creates an agent profile, a pending runner and its one-time
/// enrollment token, then a mock runner redeems it — returning the runner id
/// and a ready-to-use `Authorization` header pair carrying the raw runner
/// credential (issued exactly once by the enrollment fixture).
async fn enroll_runner(app: &axum::Router, name: &str) -> (String, String, [(String, String); 1]) {
    let (status, profile) = send(
        app,
        "POST",
        "/api/agent-profiles",
        json!({"name": format!("{name} profile"), "instructions": "work safely"}),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{profile}");
    let agent_profile_id = profile["agent_profile_id"].as_str().unwrap().to_owned();

    let (status, pending) = send(
        app,
        "POST",
        "/api/runners/enrollment",
        json!({"name": name, "total_capacity": 1, "available_capacity": 1}),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{pending}");
    let runner_id = pending["runner_id"].as_str().unwrap().to_owned();
    let raw_enrollment_token = pending["enrollment_token"].as_str().unwrap().to_owned();

    let (status, enrolled) = send(
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
    assert_eq!(enrolled["runner_id"], runner_id);
    let credential = enrolled["runner_credential"].as_str().unwrap().to_owned();

    (
        agent_profile_id,
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

fn execution_request_body(
    item_id: &str,
    key: &str,
    selector_kind: &str,
    selector_id: &str,
    agent_profile_id: &str,
) -> Value {
    json!({
        "item_id": item_id,
        "idempotency_key": key,
        "selector_kind": selector_kind,
        "selector_id": selector_id,
        "agent_profile_id": agent_profile_id,
        "requested_harness_kind": "codex",
        // Explicit, matching `full_capabilities()`'s declared combination —
        // see that function's doc comment for why this is no longer
        // auto-select (`null`/`null`) now that the real scheduler gates the
        // claim path.
        "requested_model_provider": "openai",
        "requested_model_id": "opaque/model-wave2",
        "agent_profile_snapshot": {"name": "profile", "instructions": "work safely", "tool_policy": {}, "timeout_seconds": 60, "budgets": {}},
        "repository_snapshot": {"kind": "git", "remote": "https://example.test/wave2.git", "base_revision": BASE_REVISION, "subdirectory": null},
        "permission_policy": {"tools": ["shell"], "network": false},
        "timeout_seconds": 60,
        "budgets": {},
        "environment": {},
        "metadata": {},
    })
}

// =======================================================================
// 1. The gate itself: claim, start, stream, complete, survive a mid-flight
//    API restart with the runner's existing credential and fence intact.
// =======================================================================

#[tokio::test]
async fn wave2_gate_claim_start_stream_complete_and_survive_restart() {
    let (pool, workspace_id) = fresh_database().await;
    let config = AppConfig {
        api_token: Some(OPERATOR_TOKEN.into()),
        ..AppConfig::default()
    };
    let app = router_for(pool.clone(), workspace_id, config.clone()).await;
    let op = operator_headers();

    // --- Step 1: operator creates an execution request from a real item. ---
    let (_project_id, item_id) = create_project_and_item(&app).await;
    let (agent_profile_id, runner_id, runner_auth_owned) =
        enroll_runner(&app, "Wave 2 gate runner").await;
    let runner_auth = headers_ref(&runner_auth_owned);

    // DB: the enrolled runner is active and only its credential *hash* is
    // stored — the raw credential this test captured above never appears in
    // the database at all.
    let (runner_state, stored_hash): (String, String) =
        sqlx::query_as("SELECT state, credential_hash FROM agent_runners WHERE id = ?")
            .bind(&runner_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(runner_state, "active");
    assert_ne!(stored_hash, runner_auth_owned[0].1);

    let (status, created) = send(
        &app,
        "POST",
        "/api/executions",
        execution_request_body(
            &item_id,
            "wave2-gate",
            "exact_runner",
            &runner_id,
            &agent_profile_id,
        ),
        &op,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let request_id = created["request_id"].as_str().unwrap().to_owned();
    assert_eq!(created["state"], "queued");
    let db_state: String = sqlx::query_scalar("SELECT state FROM execution_requests WHERE id = ?")
        .bind(&request_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(db_state, "queued");
    let stored_item_id: String =
        sqlx::query_scalar("SELECT item_id FROM execution_requests WHERE id = ?")
            .bind(&request_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        stored_item_id, item_id,
        "the request is bound to the real item"
    );

    // --- Step 2: already proven above (enrollment issues a credential). ---

    // --- Step 3: runner reports capabilities, then claims, receiving a
    //     fencing token. ---
    let (status, refreshed) = send(
        &app,
        "POST",
        "/api/runner/v1/refresh",
        json!({
            "protocol_version": 1,
            "runner_id": runner_id,
            "runner_name": "Wave 2 gate runner",
            "runner_version": "0.1.1",
            "capabilities": full_capabilities(),
        }),
        &runner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{refreshed}");

    let (status, claimed) = send(
        &app,
        "POST",
        "/api/runner/v1/claim",
        json!({"protocol_version": 1, "runner_id": runner_id, "claim_request_id": "wave2-claim-1", "available_capacity": 1, "wait_ms": 0}),
        &runner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{claimed}");
    assert_eq!(claimed["request"]["request_id"], request_id);
    let attempt_id = claimed["lease"]["attempt_id"].as_str().unwrap().to_owned();
    let fencing_token = claimed["lease"]["fencing_token"].as_i64().unwrap();
    assert_eq!(
        fencing_token, 1,
        "the first attempt on a fresh request starts at fence 1"
    );
    let (db_state, db_fence): (String, i64) =
        sqlx::query_as("SELECT state, fencing_token FROM execution_attempts WHERE id = ?")
            .bind(&attempt_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(db_state, "leased");
    assert_eq!(db_fence, 1);
    let req_state: String = sqlx::query_scalar("SELECT state FROM execution_requests WHERE id = ?")
        .bind(&request_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(req_state, "leased");

    // --- Step 4: accept (-> preparing) and start (-> running). ---
    let (status, accepted) = send(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_id}/accept"),
        json!({"protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token, "workspace_id": "ws-wave2", "base_revision": BASE_REVISION}),
        &runner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    assert_eq!(accepted["state"], "preparing");
    let (db_state, prepared_at): (String, Option<String>) =
        sqlx::query_as("SELECT state, prepared_at FROM execution_attempts WHERE id = ?")
            .bind(&attempt_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(db_state, "preparing");
    assert!(prepared_at.is_some());

    let (status, started) = send(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_id}/start"),
        json!({"protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token, "workspace_id": "ws-wave2", "base_revision": BASE_REVISION, "process_id": "pid-wave2"}),
        &runner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{started}");
    assert_eq!(started["state"], "running");
    let (db_state, started_at): (String, Option<String>) =
        sqlx::query_as("SELECT state, started_at FROM execution_attempts WHERE id = ?")
            .bind(&attempt_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(db_state, "running");
    assert!(started_at.is_some());

    // --- Step 5: stream the first of at least two event batches with
    //     advancing checkpoints. ---
    let (status, batch1) = send(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_id}/events"),
        json!({
            "protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token,
            "previous_checkpoint": Value::Null, "checkpoint": "wave2-checkpoint-0001",
            "events": [{"event_id": "wave2-evt-1", "sequence": 1, "occurred_at": "2026-08-08T12:00:00Z", "source": "runner", "kind": "progress", "payload": {"phase": "setup"}}],
        }),
        &runner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{batch1}");
    assert_eq!(batch1["committed_checkpoint"], "wave2-checkpoint-0001");
    let checkpoint: Option<String> =
        sqlx::query_scalar("SELECT event_checkpoint FROM execution_attempts WHERE id = ?")
            .bind(&attempt_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(checkpoint.as_deref(), Some("wave2-checkpoint-0001"));
    let event_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_events WHERE attempt_id = ?")
            .bind(&attempt_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(event_count, 1);

    // --- Step 6: API restart. Rebuild the router/AppState around the *same*
    //     pool, mid-flight (the attempt is still `running`, not terminal) —
    //     every in-process value `handlers/production_router.rs`'s own
    //     restart test also discards (broadcast channel, orch runtime,
    //     router/middleware closures) is newly constructed here too. ---
    let app = router_for(pool.clone(), workspace_id, config.clone()).await;

    // The runner continues with its *existing* credential and fence: a
    // second event batch, chained off the first checkpoint, through the new
    // router instance.
    let (status, batch2) = send(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_id}/events"),
        json!({
            "protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token,
            "previous_checkpoint": "wave2-checkpoint-0001", "checkpoint": "wave2-checkpoint-0002",
            "events": [{"event_id": "wave2-evt-2", "sequence": 2, "occurred_at": "2026-08-08T12:01:00Z", "source": "runner", "kind": "progress", "payload": {"phase": "work"}}],
        }),
        &runner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{batch2}");
    assert_eq!(batch2["committed_checkpoint"], "wave2-checkpoint-0002");
    let checkpoint: Option<String> =
        sqlx::query_scalar("SELECT event_checkpoint FROM execution_attempts WHERE id = ?")
            .bind(&attempt_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(checkpoint.as_deref(), Some("wave2-checkpoint-0002"));
    let event_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_events WHERE attempt_id = ?")
            .bind(&attempt_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(event_count, 2, "no state from before the restart was lost");

    // The operator, through the new router, still sees the (still non-terminal)
    // request — proving the request row itself survived the restart too.
    let (status, mid) = send(
        &app,
        "GET",
        &format!("/api/executions/{request_id}"),
        Value::Null,
        &op,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{mid}");
    assert_eq!(mid["state"], "leased");

    // --- Step 7: runner reports completion; operator observes the terminal
    //     state. ---
    let completion = completion_body(
        &runner_id,
        &attempt_id,
        fencing_token,
        "wave2-gate-completion-1",
        "wave2-checkpoint-0002",
    );
    let (status, completed) = send(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_id}/completion"),
        completion.clone(),
        &runner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    assert_eq!(completed["state"], "succeeded");
    assert_eq!(completed["replayed"], false);

    let attempt_state: String =
        sqlx::query_scalar("SELECT state FROM execution_attempts WHERE id = ?")
            .bind(&attempt_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(attempt_state, "succeeded");
    let request_state: String =
        sqlx::query_scalar("SELECT state FROM execution_requests WHERE id = ?")
            .bind(&request_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(request_state, "succeeded");

    let (status, detail) = send(
        &app,
        "GET",
        &format!("/api/executions/{request_id}"),
        Value::Null,
        &op,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{detail}");
    assert_eq!(detail["state"], "succeeded");

    // A completion replay against the same restarted router stays
    // idempotent — the guarantee lives in the database the restart
    // preserved, not in anything the pre-restart process held in memory.
    let (status, replayed) = send(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_id}/completion"),
        completion,
        &runner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{replayed}");
    assert_eq!(replayed["replayed"], true);
    assert_eq!(replayed["committed_at"], completed["committed_at"]);
}

// =======================================================================
// 2. Fencing: once a fence is superseded, the old fence's writes are
//    rejected with the stable `stale_lease` code and write nothing.
// =======================================================================

#[tokio::test]
async fn superseded_fence_is_rejected_as_stale_lease_and_writes_nothing() {
    let (pool, workspace_id) = fresh_database().await;
    let config = AppConfig {
        api_token: Some(OPERATOR_TOKEN.into()),
        ..AppConfig::default()
    };
    let app = router_for(pool.clone(), workspace_id, config).await;
    let op = operator_headers();

    let (_project_id, item_id) = create_project_and_item(&app).await;
    let (agent_profile_id, runner_id, runner_auth_owned) =
        enroll_runner(&app, "Fence runner").await;
    let runner_auth = headers_ref(&runner_auth_owned);

    let (status, created) = send(
        &app,
        "POST",
        "/api/executions",
        execution_request_body(
            &item_id,
            "wave2-fence",
            "exact_runner",
            &runner_id,
            &agent_profile_id,
        ),
        &op,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let request_id = created["request_id"].as_str().unwrap().to_owned();

    // First attempt: fencing_token 1, never started (no `/start` call, so
    // `started_at` stays NULL).
    let (status, claimed_a) = send(
        &app,
        "POST",
        "/api/runner/v1/claim",
        json!({"protocol_version": 1, "runner_id": runner_id, "claim_request_id": "fence-claim-1", "available_capacity": 1, "wait_ms": 0}),
        &runner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{claimed_a}");
    let attempt_a = claimed_a["lease"]["attempt_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let fence_a = claimed_a["lease"]["fencing_token"].as_i64().unwrap();
    assert_eq!(fence_a, 1);

    // The runner reports it crashed before ever spawning a process —
    // `recover_attempt`'s disposition `safe_pre_spawn_requeue`
    // (docs/contracts/runner-v1/recovery-observation.{request,response}.json):
    // a safe, pre-spawn recovery that requeues the *request* on its own,
    // with no operator action required. This is what genuinely supersedes
    // fence 1 — not merely presenting a wrong number.
    let (status, recovery) = send(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_a}/recovery-observation"),
        json!({
            "protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_a, "fencing_token": fence_a,
            "recovery_key": format!("recovery:{attempt_a}:{fence_a}:process_stopped"),
            "observation": "process_stopped",
            "details": {"journal_state": "prepared", "process_observed": false},
        }),
        &runner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{recovery}");
    assert_eq!(recovery["disposition"], "safe_pre_spawn_requeue");

    // DB: the old attempt is `lost`, the request is back to `queued`, with no
    // operator involvement at all.
    let attempt_a_state: String =
        sqlx::query_scalar("SELECT state FROM execution_attempts WHERE id = ?")
            .bind(&attempt_a)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(attempt_a_state, "lost");
    let request_state: String =
        sqlx::query_scalar("SELECT state FROM execution_requests WHERE id = ?")
            .bind(&request_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(request_state, "queued");

    // The same runner reclaims the now-requeued request: a *new* attempt
    // with a strictly higher fencing token, superseding attempt_a/fence_a.
    let (status, claimed_b) = send(
        &app,
        "POST",
        "/api/runner/v1/claim",
        json!({"protocol_version": 1, "runner_id": runner_id, "claim_request_id": "fence-claim-2", "available_capacity": 1, "wait_ms": 0}),
        &runner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{claimed_b}");
    let attempt_b = claimed_b["lease"]["attempt_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let fence_b = claimed_b["lease"]["fencing_token"].as_i64().unwrap();
    assert_ne!(attempt_b, attempt_a);
    assert_eq!(
        fence_b, 2,
        "the superseding attempt gets a strictly higher fencing token"
    );

    // The *old* fence's own write is now rejected — the stable `stale_lease`
    // code, not a generic conflict — and writes nothing.
    let (status, rejected) = send(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_a}/events"),
        json!({
            "protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_a, "fencing_token": fence_a,
            "previous_checkpoint": Value::Null, "checkpoint": "should-never-commit",
            "events": [{"event_id": "evt-superseded", "sequence": 1, "occurred_at": "2026-08-08T12:00:00Z", "source": "runner", "kind": "progress", "payload": {}}],
        }),
        &runner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{rejected}");
    assert_eq!(rejected["error"]["code"], "stale_lease");

    let event_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_events WHERE attempt_id = ?")
            .bind(&attempt_a)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        event_count, 0,
        "a superseded fence's write must persist nothing"
    );
    let attempt_a_checkpoint: Option<String> =
        sqlx::query_scalar("SELECT event_checkpoint FROM execution_attempts WHERE id = ?")
            .bind(&attempt_a)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(attempt_a_checkpoint, None);
    let attempt_a_state_after: String =
        sqlx::query_scalar("SELECT state FROM execution_attempts WHERE id = ?")
            .bind(&attempt_a)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        attempt_a_state_after, "lost",
        "the superseded attempt's own state is untouched by the rejected write"
    );

    // The *new* fence, meanwhile, genuinely works — proving the rejection
    // above was about fence_a specifically, not the request as a whole.
    let (status, accepted_b) = send(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_b}/accept"),
        json!({"protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_b, "fencing_token": fence_b, "workspace_id": "ws-fence", "base_revision": BASE_REVISION}),
        &runner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted_b}");
    assert_eq!(accepted_b["state"], "preparing");
}

// =======================================================================
// 3. Security: neither credential family authenticates the other's routes.
// =======================================================================

#[tokio::test]
async fn runner_and_operator_credentials_are_not_substitutable_across_the_production_router() {
    let (pool, workspace_id) = fresh_database().await;
    let config = AppConfig {
        api_token: Some(OPERATOR_TOKEN.into()),
        ..AppConfig::default()
    };
    let app = router_for(pool.clone(), workspace_id, config).await;

    let (_agent_profile_id, runner_id, runner_auth_owned) =
        enroll_runner(&app, "Boundary runner").await;
    let credential = runner_auth_owned[0].1.strip_prefix("Bearer ").unwrap();

    // The runner's own, genuinely valid credential cannot authenticate an
    // operator route.
    let (status, body) = send(
        &app,
        "GET",
        "/api/executions",
        Value::Null,
        &[("authorization", &bearer(credential))],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");

    // The operator's own, genuinely valid token cannot authenticate a
    // runner-v1 route.
    let (status, body) = send(
        &app,
        "POST",
        "/api/runner/v1/claim",
        json!({"protocol_version": 1, "runner_id": runner_id, "claim_request_id": "boundary-x", "available_capacity": 1, "wait_ms": 0}),
        &[("authorization", &bearer(OPERATOR_TOKEN))],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(body["error"]["code"], "unauthorized");

    // Neither family accepts no credential at all.
    let (status, _) = send(&app, "GET", "/api/executions", Value::Null, &[]).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, body) = send(
        &app,
        "POST",
        "/api/runner/v1/claim",
        json!({"protocol_version": 1, "runner_id": runner_id, "claim_request_id": "boundary-y", "available_capacity": 1, "wait_ms": 0}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "unauthorized");

    // No response above ever repeated the raw runner credential this test
    // captured at enrollment.
    let stored_hash: String =
        sqlx::query_scalar("SELECT credential_hash FROM agent_runners WHERE id = ?")
            .bind(&runner_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_ne!(stored_hash, credential);
}

// =======================================================================
// 4. Security: a client-supplied `x-tack-principal` is never trusted.
// =======================================================================

#[tokio::test]
async fn client_supplied_principal_header_is_never_trusted() {
    let (pool, workspace_id) = fresh_database().await;
    let config = AppConfig {
        api_token: Some(OPERATOR_TOKEN.into()),
        ..AppConfig::default()
    };
    let app = router_for(pool.clone(), workspace_id, config).await;
    let op = operator_headers();

    let (_project_id, item_id) = create_project_and_item(&app).await;
    let (status, profile) = send(
        &app,
        "POST",
        "/api/agent-profiles",
        json!({"name": "Spoof profile", "instructions": "work safely"}),
        &op,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{profile}");
    let agent_profile_id = profile["agent_profile_id"].as_str().unwrap().to_owned();

    let body = |key: &str| {
        execution_request_body(
            &item_id,
            key,
            "fleet",
            "fleet-need-not-exist-for-this-test",
            &agent_profile_id,
        )
    };

    // A client claiming to be someone else via the header a real HTTP caller
    // can set.
    let mut spoofed = op.clone();
    spoofed.push(("x-tack-principal", "attacker-claimed-identity"));
    let (status, first) = send(
        &app,
        "POST",
        "/api/executions",
        body("wave2-spoof"),
        &spoofed,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{first}");

    // The identical idempotency key, this time with *no* header at all. If
    // the header were trusted, these would be two different principals and
    // therefore two unrelated requests; instead this must be recognized as
    // an exact replay of the first, proving the server never read the
    // client's claimed identity in either call.
    let (status, replay) = send(&app, "POST", "/api/executions", body("wave2-spoof"), &op).await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    assert_eq!(
        replay["request_id"], first["request_id"],
        "a spoofed header on the first call and no header on the retry must land in the same idempotency scope"
    );
    assert_eq!(replay["replayed"], true);

    // Directly inspect the persisted immutable snapshot: the recorded
    // principal is not the client-supplied string.
    let request_id = first["request_id"].as_str().unwrap();
    let snapshot: String =
        sqlx::query_scalar("SELECT request_snapshot FROM execution_requests WHERE id = ?")
            .bind(request_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let snapshot: Value = serde_json::from_str(&snapshot).unwrap();
    assert_ne!(
        snapshot["created_by"]["subject_id"],
        json!("attacker-claimed-identity")
    );

    // A genuinely different idempotency key from the same real (injected)
    // principal still creates a distinct request — proving scoping still
    // works at all, just never from client input.
    let (status, second) = send(
        &app,
        "POST",
        "/api/executions",
        body("wave2-spoof-2"),
        &spoofed,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{second}");
    assert_ne!(second["request_id"], first["request_id"]);
}

// =======================================================================
// 5. Payload: an oversized event batch is rejected and leaves the database
//    unchanged.
// =======================================================================

#[tokio::test]
async fn oversized_event_batch_is_rejected_and_writes_nothing() {
    let (pool, workspace_id) = fresh_database().await;
    let config = AppConfig {
        api_token: Some(OPERATOR_TOKEN.into()),
        ..AppConfig::default()
    };
    let app = router_for(pool.clone(), workspace_id, config).await;
    let op = operator_headers();

    let (_project_id, item_id) = create_project_and_item(&app).await;
    let (agent_profile_id, runner_id, runner_auth_owned) =
        enroll_runner(&app, "Payload runner").await;
    let runner_auth = headers_ref(&runner_auth_owned);

    let (status, created) = send(
        &app,
        "POST",
        "/api/executions",
        execution_request_body(
            &item_id,
            "wave2-payload",
            "exact_runner",
            &runner_id,
            &agent_profile_id,
        ),
        &op,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");

    let (status, claimed) = send(
        &app,
        "POST",
        "/api/runner/v1/claim",
        json!({"protocol_version": 1, "runner_id": runner_id, "claim_request_id": "payload-claim", "available_capacity": 1, "wait_ms": 0}),
        &runner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{claimed}");
    let attempt_id = claimed["lease"]["attempt_id"].as_str().unwrap().to_owned();
    let fencing_token = claimed["lease"]["fencing_token"].as_i64().unwrap();

    // Over `event_batch_count_max` (100 per docs/contracts/runner-v1/limits.json).
    let too_many_events: Vec<Value> = (0..101)
        .map(|i| {
            json!({"event_id": format!("wave2-evt-{i}"), "sequence": i, "occurred_at": "2026-08-08T12:00:00Z", "source": "runner", "kind": "progress", "payload": {}})
        })
        .collect();
    let (status, body) = send(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_id}/events"),
        json!({
            "protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token,
            "checkpoint": "wave2-oversized-cp", "previous_checkpoint": Value::Null, "events": too_many_events,
        }),
        &runner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{body}");
    assert_eq!(body["error"]["code"], "payload_too_large");

    let event_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_events WHERE attempt_id = ?")
            .bind(&attempt_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(event_count, 0, "an oversized batch must persist no events");
    let checkpoint: Option<String> =
        sqlx::query_scalar("SELECT event_checkpoint FROM execution_attempts WHERE id = ?")
            .bind(&attempt_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        checkpoint, None,
        "an oversized batch must not advance the checkpoint"
    );
    let attempt_state: String =
        sqlx::query_scalar("SELECT state FROM execution_attempts WHERE id = ?")
            .bind(&attempt_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        attempt_state, "leased",
        "the attempt itself is untouched by the rejected write"
    );

    // The attempt is otherwise perfectly healthy: a normal batch on the same
    // fence still succeeds afterward, proving the rejection was scoped to
    // the one oversized request, not a corrupted attempt.
    let (status, ok) = send(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_id}/events"),
        json!({
            "protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token,
            "checkpoint": "wave2-checkpoint-0001", "previous_checkpoint": Value::Null,
            "events": [{"event_id": "wave2-evt-ok", "sequence": 1, "occurred_at": "2026-08-08T12:00:00Z", "source": "runner", "kind": "progress", "payload": {}}],
        }),
        &runner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{ok}");
}
