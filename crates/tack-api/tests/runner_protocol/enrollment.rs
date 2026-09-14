//! Runner-protocol enrollment, refresh/credential-rotation, and the
//! operator/runner auth non-substitution proof. Split out of
//! `lifecycle.rs` (which keeps the claim-through-completion attempt
//! lifecycle) once that file passed 1000 lines — these tests never touch
//! an execution attempt at all, only the runner identity/credential
//! surface, so they need none of that file's protocol-call fixture
//! methods. Each test builds its own router directly from
//! `runner_protocol::routes`, bypassing the production router.

// See `lifecycle.rs`'s identical `#[path]` load for why this is a second,
// independent module tree rather than a shared one.
#[allow(clippy::duplicate_mod)]
#[path = "../../src/handlers/runner_protocol.rs"]
mod runner_protocol;

// Loaded read-only (never modified) so the operator/runner auth
// non-substitution test below can exercise the real operator router.
#[path = "../../src/handlers/executions.rs"]
mod executions;

use std::sync::{Arc, Mutex};

use crate::common::send_str_strict as send;
use crate::log_capture::{CaptureGuard, ensure_global_log_capture_installed};

use axum::{Router, http::StatusCode};
use chrono::{DateTime, Duration, TimeZone, Utc};
use serde_json::{Value, json};
use tack_db::{
    Repository, init_pool, migrations,
    repo::execution::{EnrollmentToken, ExecutionClock, NewRunner},
};
use uuid::Uuid;

const RUNNER_ID: &str = "runner-c2";
const RUNNER_CREDENTIAL: &str = "raw-test-runner-credential";
const REQUESTED_MODEL_PROVIDER: &str = "openai";
const REQUESTED_MODEL_ID: &str = "opaque/model-c2";

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

/// The router, repo and fake clock, with `RUNNER_ID`/`RUNNER_CREDENTIAL`
/// pre-registered for the tests that rotate its credential directly rather
/// than enrolling a fresh one.
struct Fixture {
    app: Router,
    repo: Repository,
    clock: FakeClock,
}

impl Fixture {
    async fn new() -> Self {
        // Must run before anything else in every test — see
        // `log_capture.rs`'s doc comment for the race this closes.
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

        let clock = FakeClock::new(Utc.with_ymd_and_hms(2026, 8, 6, 12, 0, 0).unwrap());
        let credential_hash = runner_protocol::runner_auth::credential_hash(RUNNER_CREDENTIAL);
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

        let state =
            runner_protocol::RunnerProtocolState::new(repo.clone(), Arc::new(clock.clone()));
        let app = runner_protocol::routes(state, usize::MAX);
        Fixture { app, repo, clock }
    }

    async fn refresh(&self, credential: &str, rotate: bool) -> (StatusCode, Value) {
        let body = json!({
            "protocol_version": 1, "runner_id": RUNNER_ID, "runner_name": "r", "runner_version": "1",
            "rotate_credential": rotate, "capabilities": full_capabilities(self.clock.now(), 2, 2),
        })
        .to_string();
        refresh_as(&self.app, credential, body).await
    }

    async fn credential_hash(&self) -> String {
        self.credential_hash_of(RUNNER_ID).await
    }

    async fn credential_hash_of(&self, runner_id: &str) -> String {
        sqlx::query_scalar("SELECT credential_hash FROM agent_runners WHERE id=?")
            .bind(runner_id)
            .fetch_one(self.repo.pool())
            .await
            .unwrap()
    }

    async fn runner_name(&self, runner_id: &str) -> String {
        sqlx::query_scalar("SELECT name FROM agent_runners WHERE id=?")
            .bind(runner_id)
            .fetch_one(self.repo.pool())
            .await
            .unwrap()
    }
}

/// Matches `lifecycle.rs`'s `full_capabilities` exactly (the two files no
/// longer share a module to call one copy from).
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

/// Create a pending runner and issue it an enrollment token, for the tests
/// below that enroll their own runner rather than using `RUNNER_ID`.
async fn create_pending(
    repo: &Repository,
    clock: &FakeClock,
    id: &str,
    name: &str,
    raw_token: &str,
) {
    let token_hash = runner_protocol::runner_auth::credential_hash(raw_token);
    repo.create_pending_runner_and_issue_token(
        NewRunner {
            id,
            name,
            credential_hash: "pending:no-credential",
            labels: "{}",
            total_capacity: 1,
            available_capacity: 1,
            capability_snapshot: "{}",
            protocol_version: 1,
        },
        EnrollmentToken {
            id: &format!("tok-{id}"),
            runner_id: id,
            token_hash: &token_hash,
            expires_at: clock.now() + Duration::hours(1),
        },
        clock,
    )
    .await
    .expect("pending runner");
}

fn enroll_json(token: &str, runner_name: &str, clock: &FakeClock) -> String {
    json!({
        "protocol_version": 1,
        "enrollment_token": token,
        "runner_name": runner_name,
        "runner_version": "0.1.0",
        "capabilities": full_capabilities(clock.now(), 1, 1),
    })
    .to_string()
}

fn build_operator_app(repo: Repository, clock: FakeClock) -> Router {
    let operator_state = executions::OperatorExecutionState::with_clock(
        repo,
        Arc::new(clock),
        Arc::new(|_pool| Box::pin(async { false })),
    );
    executions::routes(operator_state)
}

/// `POST /enroll`, unauthenticated (enrollment tokens travel in the body).
async fn enroll(app: &Router, body: String) -> (StatusCode, Value) {
    send(app, "POST", "/enroll", body, &[]).await
}

/// `POST` with the given headers, for the operator-router non-substitution
/// test that doesn't go through `Fixture`.
async fn post_with(
    app: &Router,
    uri: &str,
    body: String,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    send(app, "POST", uri, body, headers).await
}

/// Create a pending runner and enroll it in one step, self-reporting `name`.
async fn create_pending_and_enroll(
    fx: &Fixture,
    id: &str,
    name: &str,
    raw_token: &str,
) -> (StatusCode, Value) {
    create_pending(&fx.repo, &fx.clock, id, name, raw_token).await;
    enroll(&fx.app, enroll_json(raw_token, name, &fx.clock)).await
}

/// `POST /refresh` bearing `credential`.
async fn refresh_as(app: &Router, credential: &str, body: String) -> (StatusCode, Value) {
    send(
        app,
        "POST",
        "/refresh",
        body,
        &[("authorization", &format!("Bearer {credential}"))],
    )
    .await
}

// ---------------------------------------------------------------------
// 1. Enrollment issues a one-way-hashed credential; refresh with rotation
//    replaces it and invalidates the old one.
// ---------------------------------------------------------------------

#[tokio::test]
async fn enrollment_and_refresh_rotation_replaces_credential() {
    let fx = Fixture::new().await;
    let raw_token = "example_lifecycle_enrollment_token";
    let (status, enrolled) =
        create_pending_and_enroll(&fx, "runner-lifecycle", "Lifecycle Runner", raw_token).await;
    assert_eq!(status, StatusCode::OK, "{enrolled}");
    let runner_id = enrolled["runner_id"].as_str().unwrap().to_owned();
    let credential = enrolled["runner_credential"].as_str().unwrap().to_owned();
    assert_ne!(credential, raw_token);
    assert_eq!(enrolled["heartbeat_interval_seconds"], 15);
    assert_eq!(enrolled["lease_duration_seconds"], 60);
    assert_ne!(
        fx.credential_hash_of(&runner_id).await,
        credential,
        "only the hash may be stored"
    );

    let refresh_body = json!({
        "protocol_version": 1, "runner_id": runner_id, "runner_name": "Lifecycle Runner",
        "runner_version": "0.1.1", "rotate_credential": true,
        "capabilities": full_capabilities(fx.clock.now(), 1, 1),
    })
    .to_string();
    let (status, refreshed) = refresh_as(&fx.app, &credential, refresh_body).await;
    assert_eq!(status, StatusCode::OK, "{refreshed}");
    let rotated = refreshed["runner_credential"].as_str().unwrap().to_owned();
    assert_ne!(rotated, credential);

    let stale_refresh = json!({"protocol_version":1,"runner_id":runner_id,"runner_name":"x","runner_version":"x","rotate_credential":false,"capabilities":full_capabilities(fx.clock.now(),1,1)}).to_string();
    let (old_status, _) = refresh_as(&fx.app, &credential, stale_refresh).await;
    assert_eq!(
        old_status,
        StatusCode::UNAUTHORIZED,
        "the old credential no longer authenticates once rotated"
    );
}

// ---------------------------------------------------------------------
// 2. Two default-configured runners self-report the identical
//    `runner_name` (both default it from `TACK_RUNNER_ID`). Without the
//    `_runner_name`/removed `name=?` special-casing in
//    `crates/tack-db/src/repo/execution.rs::redeem_enrollment_token`, this
//    would silently overwrite the operator-assigned, uniquely-named
//    pending-runner row and crash the second enrollment on
//    `agent_runners`'s `UNIQUE` `name` constraint (two curl enrollments
//    differing only in token, first 200, second 500). Load-bearing:
//    reverting that change makes this test fail with a 500 on the second
//    enrollment.
// ---------------------------------------------------------------------

#[tokio::test]
async fn duplicate_self_reported_runner_name_enrolls_both_runners() {
    let fx = Fixture::new().await;
    // The operator assigns each pending runner a distinct, unique name —
    // exactly as `create_pending_runner` requires (its own INSERT is
    // `UNIQUE`-constrained on `name`); both self-report the same
    // `runner_name` below anyway, and neither enrollment may 500.
    let mut runner_ids = Vec::new();
    for suffix in ["a", "b"] {
        let (id, name, token) = (
            format!("runner-h7-{suffix}"),
            format!("op-assigned-{suffix}"),
            format!("h7-token-{suffix}"),
        );
        create_pending(&fx.repo, &fx.clock, &id, &name, &token).await;
        let body = enroll_json(&token, "default-runner-id", &fx.clock);
        let (status, enrolled) = enroll(&fx.app, body).await;
        assert_eq!(status, StatusCode::OK, "{enrolled}");
        runner_ids.push((enrolled["runner_id"].as_str().unwrap().to_owned(), name));
    }

    assert_ne!(runner_ids[0].0, runner_ids[1].0);
    for (runner_id, expected_name) in &runner_ids {
        assert_eq!(&fx.runner_name(runner_id).await, expected_name);
    }
}

// ---------------------------------------------------------------------
// 3. Operator auth cannot substitute for runner auth, and vice versa: a
//    runner cannot reach any PM-mutating (item/execution/runner admin)
//    route.
// ---------------------------------------------------------------------

#[tokio::test]
async fn operator_principal_does_not_authenticate_runner_routes() {
    let fx = Fixture::new().await;
    let (status, body) = send(
        &fx.app,
        "POST",
        "/heartbeat",
        json!({"protocol_version":1,"runner_id":RUNNER_ID,"heartbeat_id":"hb-x","sent_at":fx.clock.now().to_rfc3339(),"available_capacity":2,"active_attempts":[]}).to_string(),
        &[("x-tack-principal", "operator-1")],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "unauthorized");
}

/// `runner-admin`'s simpler mutations (e.g. `revoke_runner`) do not
/// themselves check `x-tack-principal` — that router leaves top-level
/// bearer-token gating to the global `require_token` middleware, so it is
/// not exercised here the way the two principal-scoped routes below are.
#[tokio::test]
async fn runner_bearer_does_not_authenticate_operator_routes() {
    let fx = Fixture::new().await;
    let operator_app = build_operator_app(fx.repo.clone(), fx.clock.clone());
    let bearer_value = format!("Bearer {RUNNER_CREDENTIAL}");
    let runner_bearer: [(&str, &str); 1] = [("authorization", &bearer_value)];

    let create_body = json!({
        "item_id": Uuid::new_v4(), "idempotency_key": "attempt-by-runner", "selector_kind": "exact_runner",
        "selector_id": RUNNER_ID, "agent_profile_id": "profile-c2", "requested_harness_kind": "codex",
        "agent_profile_snapshot": {"name":"C2 Profile","instructions":"work safely","tool_policy":{"mode":"safe"},"timeout_seconds":60,"budgets":{"tokens":1000}},
        "repository_snapshot": {"kind":"git","remote":"https://example.test/c2.git","base_revision":"abc123","subdirectory":null},
        "permission_policy": {"tools":["shell"],"network":false}, "timeout_seconds":60, "budgets":{"tokens":1000},
        "environment": {}, "metadata": {},
    })
    .to_string();
    let (status, _) = post_with(&operator_app, "/executions", create_body, &runner_bearer).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "must not create a PM execution request"
    );

    let requeue_body = json!({"recovery_key": "k", "reason": "r"}).to_string();
    let requeue_uri = "/executions/does-not-exist/requeue";
    let (status, _) = post_with(&operator_app, requeue_uri, requeue_body, &runner_bearer).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "must not perform an audited operator requeue"
    );
}

// ---------------------------------------------------------------------
// 4. Logs carry ids only.
//
// This is the only test in this module that captures `tracing` output.
// See `log_capture.rs` for the process-global subscriber this relies on,
// and why a global default (not a thread-local `set_default`) is what
// actually makes capture reliable when many tests share this binary.
// ---------------------------------------------------------------------

#[tokio::test]
async fn logs_never_contain_raw_credentials_only_ids() {
    let fx = Fixture::new().await;
    let raw_token = "example_super_secret_enrollment_token_value";
    create_pending(&fx.repo, &fx.clock, "runner-log", "Log Runner", raw_token).await;

    let (guard, captured) = CaptureGuard::start();
    let (status, enrolled) = enroll(&fx.app, enroll_json(raw_token, "Log Runner", &fx.clock)).await;
    assert_eq!(status, StatusCode::OK, "{enrolled}");
    let issued_runner_id = enrolled["runner_id"].as_str().unwrap().to_owned();
    let issued_credential = enrolled["runner_credential"].as_str().unwrap().to_owned();

    let bogus_credential = "bogus-bearer-credential-value-should-never-log";
    let refresh_body = json!({"protocol_version":1,"runner_id":issued_runner_id,"runner_name":"x","runner_version":"x","rotate_credential":false,"capabilities":full_capabilities(fx.clock.now(),1,1)}).to_string();
    let (status, _) = refresh_as(&fx.app, bogus_credential, refresh_body).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    drop(guard);

    let log_text = String::from_utf8(captured.lock().unwrap().clone()).unwrap();
    assert!(
        log_text.contains(&issued_runner_id),
        "runner_id should be logged: {log_text}"
    );
    assert!(
        !log_text.contains(raw_token),
        "raw enrollment token leaked into logs: {log_text}"
    );
    assert!(
        !log_text.contains(&issued_credential),
        "raw runner credential leaked into logs: {log_text}"
    );
    assert!(
        !log_text.contains(bogus_credential),
        "raw bearer credential leaked into logs: {log_text}"
    );
}

// ---------------------------------------------------------------------
// 5. Credential rotation uses a compare-and-set
//     `Repository::rotate_runner_credential`. A rotation whose
//     `expected_credential_hash` no longer matches the runner's current
//     credential (because a prior rotation already won) is rejected as a
//     retryable `conflict`, not silently applied last-writer-wins.
//
//     Unmet (>40 lines, documented in EXCLUSIONS): this drives two
//     rotations through a genuine SQLite lock race (spawn, forced
//     interleaving via a held `BEGIN IMMEDIATE`, join, then a three-way
//     outcome check over both results) — compressing the interleaving
//     setup below a "spawn both / release the hold / join both" shape
//     would either lose the forced ordering or hide which of the two
//     concurrent responses is being asserted against which branch.
// ---------------------------------------------------------------------

#[tokio::test]
async fn concurrent_refresh_rotations_exactly_one_wins() {
    let fx = Fixture::new().await;

    // A manual `BEGIN IMMEDIATE` no-op write against the runner row, held
    // open until both rotation tasks are spawned, forces both to reach
    // their own blocked read/write before it releases: a bare `tokio::join!`
    // or a single `yield_now` lets one task's entire rotation complete
    // before the other is even polled, so the loser sees a plain
    // `unauthorized` (already-rotated hash) rather than the `conflict` this
    // test exists to prove.
    let mut holder = fx
        .repo
        .pool()
        .begin_with("BEGIN IMMEDIATE")
        .await
        .expect("hold the runner row");
    sqlx::query("UPDATE agent_runners SET updated_at=updated_at WHERE id=?")
        .bind(RUNNER_ID)
        .execute(&mut *holder)
        .await
        .expect("no-op hold write");

    let app_a = fx.app.clone();
    let app_b = fx.app.clone();
    let clock_a = fx.clock.clone();
    let clock_b = fx.clock.clone();
    let task_a = tokio::spawn(async move {
        let body = json!({"protocol_version":1,"runner_id":RUNNER_ID,"runner_name":"r","runner_version":"1","rotate_credential":true,"capabilities":full_capabilities(clock_a.now(),2,2)}).to_string();
        refresh_as(&app_a, RUNNER_CREDENTIAL, body).await
    });
    let task_b = tokio::spawn(async move {
        let body = json!({"protocol_version":1,"runner_id":RUNNER_ID,"runner_name":"r","runner_version":"1","rotate_credential":true,"capabilities":full_capabilities(clock_b.now(),2,2)}).to_string();
        refresh_as(&app_b, RUNNER_CREDENTIAL, body).await
    });

    // A bounded cooperative-yield loop (not a fixed wall-clock wait) gives
    // the executor enough turns to drive both spawned tasks onto their own
    // blocked read/write before the hold below releases.
    for _ in 0..512 {
        tokio::task::yield_now().await;
    }
    holder.commit().await.expect("release the hold");

    let results = [
        task_a.await.expect("rotation task a did not panic"),
        task_b.await.expect("rotation task b did not panic"),
    ];
    let ok: Vec<_> = results
        .iter()
        .filter(|(s, _)| *s == StatusCode::OK)
        .collect();
    let conflicts: Vec<_> = results
        .iter()
        .filter(|(s, _)| *s == StatusCode::CONFLICT)
        .collect();
    assert_eq!(
        ok.len(),
        1,
        "exactly one concurrent rotation must win: {results:?}"
    );
    assert_eq!(
        conflicts.len(),
        1,
        "the loser must be rejected, not silently overwritten: {results:?}"
    );
    let (_, conflict_body) = conflicts[0];
    assert_eq!(conflict_body["error"]["code"], "conflict");
    assert_eq!(
        conflict_body["error"]["retryable"], true,
        "HashMismatch maps to the retryable `conflict` code, matching docs/contracts/runner-v1/errors/conflict.json: {conflict_body}"
    );

    let (_, winner_body) = ok[0];
    let winner_credential = winner_body["runner_credential"].as_str().unwrap();
    assert_eq!(
        fx.credential_hash().await,
        runner_protocol::runner_auth::credential_hash(winner_credential),
        "the stored credential is exactly the winner's"
    );

    // The original bearer credential is now stale either way — proving the
    // old credential was not left simultaneously valid alongside the new one.
    let (old_status, _) = fx.refresh(RUNNER_CREDENTIAL, false).await;
    assert_eq!(
        old_status,
        StatusCode::UNAUTHORIZED,
        "the pre-race credential no longer authenticates once either rotation committed"
    );
}

// ---------------------------------------------------------------------
// 5b. The same defect, reproduced deterministically instead of by
//     timing/luck. The server has no way to distinguish "this stale
//     credential belongs to a request that lost a real concurrent race"
//     from "this stale credential is simply being reused after a rotation
//     already committed" — both hit the exact same code path:
//     `authenticate`'s `SELECT ... WHERE credential_hash=?` finds no row,
//     because the only record of the old hash was overwritten in place by
//     the rotation UPDATE. So this test reproduces the identical defect
//     without any lock or sleep: rotate once (the winner), then present the
//     now-superseded original credential again with `rotate_credential:
//     true` (the loser) and assert the documented, retryable outcome.
// ---------------------------------------------------------------------

#[tokio::test]
async fn superseded_credential_refresh_returns_conflict_not_401() {
    let fx = Fixture::new().await;
    let (winner_status, winner_body) = fx.refresh(RUNNER_CREDENTIAL, true).await;
    assert_eq!(winner_status, StatusCode::OK, "the first rotation must win");
    let stored_after_winner = fx.credential_hash().await;

    let (loser_status, loser_body) = fx.refresh(RUNNER_CREDENTIAL, true).await;
    assert_eq!(
        loser_status,
        StatusCode::CONFLICT,
        "a superseded credential attempting to rotate must be told it can retry, not that it is permanently unauthorized: {loser_body}"
    );
    assert_eq!(loser_body["error"]["code"], "conflict");
    assert_eq!(
        loser_body["error"]["retryable"], true,
        "must match docs/contracts/runner-v1/errors/conflict.json: {loser_body}"
    );
    assert!(
        loser_body.get("runner_credential").is_none(),
        "a rejected rotation returns no credential for a caller to mistakenly treat as live"
    );

    let stored_after_loser = fx.credential_hash().await;
    assert_eq!(
        stored_after_winner, stored_after_loser,
        "the rejected rotation must not have written anything"
    );
    assert_eq!(
        stored_after_loser,
        runner_protocol::runner_auth::credential_hash(
            winner_body["runner_credential"].as_str().unwrap()
        ),
        "the stored credential remains exactly the winner's"
    );
}
