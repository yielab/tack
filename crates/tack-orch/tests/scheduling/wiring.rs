//! Proves `tack_orch::scheduler::wiring::choose_request_for_runner` against a
//! real, file-backed-shape (in-memory) `tack_db::Repository` — not the pure
//! unit tests in `scheduler/wiring.rs` itself, which only cover the small
//! pure helper functions. This file is the actual "wire the scheduler to
//! live data" proof: real
//! `agent_runners`/`agent_fleet_members`/`agent_fleets`/`execution_requests`
//! rows, inserted through the same repository methods the real API handlers
//! use, then handed to `choose_request_for_runner` exactly as
//! `crates/tack-api/src/handlers/runner_protocol.rs`'s `claim` handler does.

use chrono::Utc;
use tack_db::Repository;
use tack_db::repo::execution::{NewExecutionRequest, NewRunner, RequestSelection};
use tack_orch::scheduler::wiring::choose_request_for_runner;
use tack_orch::scheduler::{SchedulingPolicy, choose_request_for_runner as choose};

use crate::support::{FixedClock, codex_capability_snapshot, setup_repo};

const ALPHA_MODEL: Option<&str> = Some("opaque/model-alpha");

/// Registers a runner and sets its heartbeat `heartbeat_offset_secs` away
/// from `now` (negative for stale), or leaves it `NULL` (`None`) to mirror a
/// runner that enrolled/refreshed but has never yet been granted a lease.
async fn setup_runner(
    repo: &Repository,
    id: &str,
    total_capacity: i64,
    available_capacity: i64,
    capability_snapshot: &str,
    heartbeat_offset_secs: Option<i64>,
    now: chrono::DateTime<Utc>,
) {
    repo.register_runner(
        NewRunner {
            id,
            name: id,
            credential_hash: "test-hash",
            labels: "{}",
            total_capacity,
            available_capacity,
            capability_snapshot,
            protocol_version: 1,
        },
        &FixedClock(now),
    )
    .await
    .expect("register runner");
    if let Some(offset) = heartbeat_offset_secs {
        sqlx::query("UPDATE agent_runners SET last_heartbeat_at = ? WHERE id = ?")
            .bind((now + chrono::Duration::seconds(offset)).to_rfc3339())
            .bind(id)
            .execute(repo.pool())
            .await
            .expect("set heartbeat");
    }
}

/// `setup_runner` with the standard `codex` snapshot.
async fn setup_codex_runner(
    repo: &Repository,
    id: &str,
    total_capacity: i64,
    available_capacity: i64,
    heartbeat_offset_secs: Option<i64>,
    now: chrono::DateTime<Utc>,
) {
    setup_runner(
        repo,
        id,
        total_capacity,
        available_capacity,
        &codex_capability_snapshot(now),
        heartbeat_offset_secs,
        now,
    )
    .await;
}

async fn setup_fleet(
    repo: &Repository,
    id: &str,
    concurrency_limit: i64,
    now: chrono::DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO agent_fleets (id, name, concurrency_limit, default_policy, created_at, updated_at) \
         VALUES (?, ?, ?, '{}', ?, ?)",
    )
    .bind(id)
    .bind(id)
    .bind(concurrency_limit)
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .execute(repo.pool())
    .await
    .expect("insert fleet");
}

async fn join_fleet(
    repo: &Repository,
    fleet_id: &str,
    runner_id: &str,
    now: chrono::DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO agent_fleet_members (fleet_id, runner_id, created_at) VALUES (?, ?, ?)",
    )
    .bind(fleet_id)
    .bind(runner_id)
    .bind(now.to_rfc3339())
    .execute(repo.pool())
    .await
    .expect("fleet membership");
}

#[allow(clippy::too_many_arguments)]
async fn enqueue(
    repo: &Repository,
    item_id: &str,
    request_id: &str,
    idempotency_key: &str,
    selector_kind: &str,
    selector_id: &str,
    harness: &str,
    model: Option<&str>,
    metadata: &str,
    created_at: chrono::DateTime<Utc>,
) -> tack_db::repo::execution::EnqueueResult {
    let selector = match selector_kind {
        "exact_runner" => serde_json::json!({"kind":"exact_runner","runner_id":selector_id}),
        "fleet" => serde_json::json!({"kind":"fleet","fleet_id":selector_id}),
        other => panic!("unsupported selector kind in test fixture: {other}"),
    };
    let snapshot = serde_json::json!({
        "request_id": request_id,
        "item_id": item_id,
        "idempotency_key": idempotency_key,
        "created_by": {"source": "test", "subject_id": "wiring-test"},
        "created_at": created_at.to_rfc3339(),
        "selector": selector,
        "agent_profile_id": "profile-1",
        "resolved_agent_profile": {
            "name": "profile", "instructions": "work safely",
            "tool_policy": {}, "budgets": {}, "timeout_seconds": 60
        },
        "requested_harness_kind": harness,
        "requested_model": model,
        "repository": {
            "kind": "git", "remote": "https://example.test/wiring.git",
            "base_revision": "abc123def456abc123def456abc123def456abc", "subdirectory": null
        },
        "permission_policy": {"tools": [], "network": false},
        "timeout_seconds": 60,
        "budgets": {}, "status_map_policy_id": null,
        "environment": {}, "metadata": serde_json::from_str::<serde_json::Value>(metadata).unwrap()
    })
    .to_string();
    repo.enqueue_execution(
        NewExecutionRequest {
            id: request_id,
            item_id,
            idempotency_scope: "wiring-test",
            idempotency_key,
            request_fingerprint: request_id,
            selector_kind,
            selector_id,
            agent_profile_id: Some("profile-1"),
            agent_profile_snapshot: r#"{"name":"profile","instructions":"work safely","tool_policy":{},"budgets":{},"timeout_seconds":60}"#,
            requested_harness_kind: Some(harness),
            requested_model: model,
            repository_snapshot: r#"{"kind":"git","remote":"https://example.test/wiring.git","base_revision":"abc123def456abc123def456abc123def456abc","subdirectory":null}"#,
            permission_policy: r#"{"tools":[],"network":false}"#,
            timeout_seconds: Some(60),
            budgets: "{}",
            status_map_policy_id: None,
            environment: "{}",
            metadata,
            request_snapshot: &snapshot,
        },
        &FixedClock(created_at),
    )
    .await
    .expect("enqueue")
}

/// `enqueue` with the standard `codex` harness, targeting one runner directly.
async fn enqueue_for_runner(
    repo: &Repository,
    item_id: &str,
    request_id: &str,
    runner_id: &str,
    model: Option<&str>,
    metadata: &str,
    created_at: chrono::DateTime<Utc>,
) -> tack_db::repo::execution::EnqueueResult {
    let key = format!("{request_id}-key");
    enqueue(
        repo,
        item_id,
        request_id,
        &key,
        "exact_runner",
        runner_id,
        "codex",
        model,
        metadata,
        created_at,
    )
    .await
}

/// `enqueue` with the standard `codex` harness, targeting a fleet selector.
async fn enqueue_for_fleet(
    repo: &Repository,
    item_id: &str,
    request_id: &str,
    fleet_id: &str,
    model: Option<&str>,
    metadata: &str,
    created_at: chrono::DateTime<Utc>,
) {
    let key = format!("{request_id}-key");
    enqueue(
        repo, item_id, request_id, &key, "fleet", fleet_id, "codex", model, metadata, created_at,
    )
    .await;
}

/// One row of the eligibility table below: registers a runner with the given
/// capacity/capability/heartbeat, enqueues one request for `model_id`, and
/// asserts whether it gets chosen.
#[allow(clippy::too_many_arguments)]
async fn assert_eligibility(
    case: &str,
    total: i64,
    available: i64,
    capability: &str,
    heartbeat_offset_secs: i64,
    model: Option<&str>,
    expect_chosen: bool,
) {
    let (repo, item_id) = setup_repo().await;
    let now = Utc::now();
    setup_runner(
        &repo,
        "runner",
        total,
        available,
        capability,
        Some(heartbeat_offset_secs),
        now,
    )
    .await;
    enqueue_for_runner(&repo, &item_id, "req", "runner", model, "{}", now).await;

    let chosen = choose_request_for_runner(&repo, "runner", now, &SchedulingPolicy::default())
        .await
        .expect("no db error");
    assert_eq!(chosen.is_some(), expect_chosen, "case: {case}");
}

#[tokio::test]
async fn eligibility_needs_capability_capacity_and_fresh_heartbeat() {
    let codex = codex_capability_snapshot(Utc::now());
    assert_eligibility("match", 1, 1, &codex, 0, ALPHA_MODEL, true).await;
    assert_eligibility("no_model_named", 1, 1, &codex, 0, None, false).await;
    assert_eligibility("no_harness", 1, 1, "{}", 0, ALPHA_MODEL, false).await;
    assert_eligibility("zero_capacity", 1, 0, &codex, 0, ALPHA_MODEL, false).await;
    assert_eligibility("stale_heartbeat", 1, 1, &codex, -600, ALPHA_MODEL, false).await;
}

#[tokio::test]
async fn reexported_choose_matches_the_primary_function() {
    let (repo, item_id) = setup_repo().await;
    let now = Utc::now();
    setup_codex_runner(&repo, "runner-a", 1, 1, Some(0), now).await;
    enqueue_for_runner(&repo, &item_id, "req-a", "runner-a", ALPHA_MODEL, "{}", now).await;

    let via_module =
        choose_request_for_runner(&repo, "runner-a", now, &SchedulingPolicy::default())
            .await
            .expect("no db error");
    let via_reexport = choose(&repo, "runner-a", now, &SchedulingPolicy::default())
        .await
        .expect("no db error");
    assert_eq!(
        via_module, via_reexport,
        "tack_orch::scheduler::choose_request_for_runner must not drift from the wiring module's own copy"
    );
}

/// One row of the priority table below: enqueues an older normal-priority
/// request and a newer one carrying `second_metadata`, then checks which id
/// gets chosen.
async fn assert_priority(case: &str, second_metadata: &str, expect_chosen: &str) {
    let (repo, item_id) = setup_repo().await;
    let now = Utc::now();
    let earlier = now - chrono::Duration::seconds(60);
    setup_codex_runner(&repo, "runner-a", 1, 1, Some(0), now).await;
    enqueue_for_runner(
        &repo,
        &item_id,
        "req-first",
        "runner-a",
        ALPHA_MODEL,
        "{}",
        earlier,
    )
    .await;
    enqueue_for_runner(
        &repo,
        &item_id,
        "req-second",
        "runner-a",
        ALPHA_MODEL,
        second_metadata,
        now,
    )
    .await;

    let chosen = choose_request_for_runner(&repo, "runner-a", now, &SchedulingPolicy::default())
        .await
        .expect("no db error");
    assert_eq!(chosen.as_deref(), Some(expect_chosen), "case: {case}");
}

#[tokio::test]
async fn priority_and_recency_break_ties_in_the_expected_order() {
    assert_priority("high_priority_wins", r#"{"priority":"high"}"#, "req-second").await;
    assert_priority("fifo_within_same_priority", "{}", "req-first").await;
}

/// One row of the fleet table below: an already-joined `runner-member` polls
/// while the fleet is at `concurrency_limit`, optionally saturated by a
/// second member, and checks whether it can still claim.
async fn assert_fleet_concurrency(
    case: &str,
    concurrency_limit: i64,
    saturate: bool,
    expect_chosen: bool,
) {
    let (repo, item_id) = setup_repo().await;
    let now = Utc::now();
    let fleet_id = "fleet-under-test";
    setup_fleet(&repo, fleet_id, concurrency_limit, now).await;
    setup_codex_runner(&repo, "runner-member", 1, 1, Some(0), now).await;
    join_fleet(&repo, fleet_id, "runner-member", now).await;
    if saturate {
        setup_codex_runner(&repo, "runner-other-member", 1, 0, Some(0), now).await;
        join_fleet(&repo, fleet_id, "runner-other-member", now).await;
    }
    enqueue_for_fleet(
        &repo,
        &item_id,
        "req-fleet",
        fleet_id,
        ALPHA_MODEL,
        "{}",
        now,
    )
    .await;

    let chosen =
        choose_request_for_runner(&repo, "runner-member", now, &SchedulingPolicy::default())
            .await
            .expect("no db error");
    assert_eq!(chosen.is_some(), expect_chosen, "case: {case}");
}

#[tokio::test]
async fn fleet_concurrency_limit_gates_every_member() {
    assert_fleet_concurrency("at_limit_blocks", 1, true, false).await;
    assert_fleet_concurrency("under_limit_allows", 5, false, true).await;
}

/// One row of the absence table below: with or without an idle runner
/// registered, no queued work ever produces an error — only a clean `None`.
async fn assert_absence(case: &str, register_idle_runner: bool) {
    let (repo, _item_id) = setup_repo().await;
    let now = Utc::now();
    let runner_id = "runner-idle";
    if register_idle_runner {
        setup_codex_runner(&repo, runner_id, 1, 1, Some(0), now).await;
    }
    let chosen = choose_request_for_runner(&repo, runner_id, now, &SchedulingPolicy::default())
        .await
        .expect("no db error");
    assert_eq!(chosen, None, "case: {case}");
}

#[tokio::test]
async fn absence_of_eligible_work_resolves_to_none_not_an_error() {
    assert_absence("no_queued_work", true).await;
    assert_absence("unknown_runner_id", false).await;
}

/// A freshly enrolled runner polling for its very first claim has never
/// called `/heartbeat` (that endpoint only reports *active attempt lease*
/// renewals — `agent_runners.last_heartbeat_at` stays `NULL` until a runner
/// has already been granted at least one lease). Without the capability
/// snapshot's own `reported_at` fallback in `wiring.rs`, this would be a
/// deadlock: no runner could ever get its first piece of work.
#[tokio::test]
async fn fresh_runner_with_no_heartbeat_can_claim_its_first_request() {
    let (repo, item_id) = setup_repo().await;
    let now = Utc::now();
    setup_codex_runner(&repo, "runner-fresh", 1, 1, None, now).await;
    let heartbeat: Option<String> =
        sqlx::query_scalar("SELECT last_heartbeat_at FROM agent_runners WHERE id = 'runner-fresh'")
            .fetch_one(repo.pool())
            .await
            .expect("read heartbeat column");
    assert_eq!(
        heartbeat, None,
        "fixture must start with no heartbeat, matching real enrollment"
    );

    enqueue_for_runner(
        &repo,
        &item_id,
        "req-first-claim",
        "runner-fresh",
        ALPHA_MODEL,
        "{}",
        now,
    )
    .await;

    let chosen =
        choose_request_for_runner(&repo, "runner-fresh", now, &SchedulingPolicy::default())
            .await
            .expect("no db error");
    assert_eq!(
        chosen.as_deref(),
        Some("req-first-claim"),
        "a runner's own attested capability report time must stand in for a heartbeat it has \
         never had the chance to send yet"
    );
}

/// The fallback proven above is not "no heartbeat ever means eligible" — a
/// runner that enrolled/refreshed long ago and never heartbeated since still
/// goes stale once its *capability report* itself ages past the policy window.
#[tokio::test]
async fn stale_capability_report_rejects_despite_no_heartbeat() {
    let (repo, item_id) = setup_repo().await;
    let now = Utc::now();
    let stale_report_time = now - chrono::Duration::seconds(600);
    setup_runner(
        &repo,
        "runner-stale-report",
        1,
        1,
        &codex_capability_snapshot(stale_report_time),
        None,
        now,
    )
    .await;

    enqueue_for_runner(
        &repo,
        &item_id,
        "req-stale-report",
        "runner-stale-report",
        ALPHA_MODEL,
        "{}",
        now,
    )
    .await;

    let chosen = choose_request_for_runner(
        &repo,
        "runner-stale-report",
        now,
        &SchedulingPolicy::default(),
    )
    .await
    .expect("no db error");
    assert_eq!(chosen, None);
}

async fn waiting_row(
    repo: &Repository,
    id: &str,
) -> (String, Option<String>, Option<String>, Option<String>) {
    sqlx::query_as(
        "SELECT state, waiting_code, waiting_reason, waiting_since FROM execution_requests WHERE id = ?",
    )
    .bind(id)
    .fetch_one(repo.pool())
    .await
    .expect("request row")
}

async fn refresh_heartbeat(repo: &Repository, runner_id: &str, at: chrono::DateTime<Utc>) {
    sqlx::query("UPDATE agent_runners SET last_heartbeat_at = ? WHERE id = ?")
        .bind(at.to_rfc3339())
        .bind(runner_id)
        .execute(repo.pool())
        .await
        .expect("refresh heartbeat");
}

#[tokio::test]
async fn a_request_no_runner_can_claim_says_why_and_clears_when_selected() {
    let (repo, item_id) = setup_repo().await;
    let now = Utc::now();
    setup_codex_runner(&repo, "runner-a", 1, 1, Some(0), now).await;
    enqueue_for_runner(&repo, &item_id, "req", "runner-a", None, "{}", now).await;
    assert_eq!(
        waiting_row(&repo, "req").await,
        ("queued".into(), None, None, None),
        "a request no runner polled carries nothing"
    );

    let chosen = choose_request_for_runner(&repo, "runner-a", now, &SchedulingPolicy::default())
        .await
        .expect("no db error");
    assert_eq!(chosen, None);
    let (state, code, reason, since) = waiting_row(&repo, "req").await;
    assert_eq!(state, "queued");
    assert_eq!(code.as_deref(), Some("model_not_named"));
    assert_eq!(reason.as_deref(), Some("codex needs a model named"));
    assert_eq!(since, Some(now.to_rfc3339()));

    // The same request, made claimable, is selected and all three clear.
    sqlx::query("UPDATE execution_requests SET requested_model = ? WHERE id = 'req'")
        .bind("opaque/model-alpha")
        .execute(repo.pool())
        .await
        .expect("make claimable");
    let chosen = choose_request_for_runner(&repo, "runner-a", now, &SchedulingPolicy::default())
        .await
        .expect("no db error");
    assert_eq!(chosen.as_deref(), Some("req"));
    repo.claim_execution_idempotent_with_snapshot(
        "runner-a",
        "claim",
        "attempt",
        chrono::Duration::seconds(60),
        &FixedClock(now),
        RequestSelection::Scheduled(chosen.as_deref()),
    )
    .await
    .expect("no db error")
    .expect("claimed");
    assert_eq!(
        waiting_row(&repo, "req").await,
        ("leased".into(), None, None, None)
    );
}

#[tokio::test]
async fn a_request_unclaimable_past_the_setting_fails_with_no_runner_can_claim() {
    // (case, fail_after minutes, expected state after the late poll)
    for (case, minutes, expected) in [("default", 30, "failed"), ("zero never fails", 0, "queued")]
    {
        let (repo, item_id) = setup_repo().await;
        let now = Utc::now();
        setup_codex_runner(&repo, "runner-a", 1, 1, Some(0), now).await;
        enqueue_for_runner(&repo, &item_id, "req", "runner-a", None, "{}", now).await;
        let policy = SchedulingPolicy {
            unclaimable_fail_after: chrono::Duration::minutes(minutes),
            ..SchedulingPolicy::default()
        };

        choose_request_for_runner(&repo, "runner-a", now, &policy)
            .await
            .expect("no db error");
        let later = now + chrono::Duration::minutes(31);
        refresh_heartbeat(&repo, "runner-a", later).await;
        choose_request_for_runner(&repo, "runner-a", later, &policy)
            .await
            .expect("no db error");

        let (state, code, reason, since) = waiting_row(&repo, "req").await;
        assert_eq!(state, expected, "case: {case}");
        assert!(reason.is_some(), "case: {case}");
        assert!(since.is_some(), "case: {case}");
        if expected == "failed" {
            assert_eq!(code.as_deref(), Some("no_runner_can_claim"), "case: {case}");
        }
    }
}

/// A poll by a runner the request does not name says nothing about whether it
/// can be claimed, and a runner that could take it once it has room restarts
/// the clock: neither may fail a request.
#[tokio::test]
async fn a_poll_that_proves_nothing_neither_starts_nor_keeps_the_clock() {
    // Another runner polls a request that names runner-a: nothing is written,
    // however long it goes on.
    let (repo, item_id) = setup_repo().await;
    let now = Utc::now();
    setup_codex_runner(&repo, "runner-a", 1, 1, Some(0), now).await;
    setup_codex_runner(&repo, "runner-b", 1, 1, Some(0), now).await;
    enqueue_for_runner(&repo, &item_id, "req", "runner-a", ALPHA_MODEL, "{}", now).await;
    for at in [now, now + chrono::Duration::minutes(31)] {
        refresh_heartbeat(&repo, "runner-b", at).await;
        let chosen = choose_request_for_runner(&repo, "runner-b", at, &SchedulingPolicy::default())
            .await
            .expect("no db error");
        assert_eq!(chosen, None);
    }
    assert_eq!(
        waiting_row(&repo, "req").await,
        ("queued".into(), None, None, None),
        "a runner the request does not name proves nothing"
    );

    // The runner it names is busy: the reason is shown, the clock is cleared,
    // and an overdue clock from an earlier blocking poll does not fail it.
    let (repo, item_id) = setup_repo().await;
    setup_codex_runner(&repo, "runner-a", 1, 0, Some(0), now).await;
    enqueue_for_runner(&repo, &item_id, "req", "runner-a", ALPHA_MODEL, "{}", now).await;
    sqlx::query("UPDATE execution_requests SET waiting_code = 'harness_not_declared', waiting_since = ? WHERE id = 'req'")
        .bind((now - chrono::Duration::minutes(60)).to_rfc3339())
        .execute(repo.pool())
        .await
        .expect("an earlier blocking poll");
    let chosen = choose_request_for_runner(&repo, "runner-a", now, &SchedulingPolicy::default())
        .await
        .expect("no db error");
    assert_eq!(chosen, None);
    let (state, code, reason, since) = waiting_row(&repo, "req").await;
    assert_eq!(
        state, "queued",
        "a busy runner that could take it never fails it"
    );
    assert_eq!(code.as_deref(), Some("no_available_capacity"));
    assert!(reason.is_some());
    assert_eq!(
        since, None,
        "a runner that could take it restarts the clock"
    );
}

async fn set_budgets(repo: &Repository, item_id: &str, tokens: Option<i64>, queue: Option<i64>) {
    let project_id: String = sqlx::query_scalar("SELECT project_id FROM items WHERE id = ?")
        .bind(item_id)
        .fetch_one(repo.pool())
        .await
        .expect("project");
    tack_db::repo::budgets::put(
        repo.pool(),
        &project_id,
        &tack_db::repo::budgets::ProjectBudgets {
            tokens_per_day: tokens,
            auto_merges_per_day: None,
            queue_items_max: queue,
            ..Default::default()
        },
    )
    .await
    .expect("put budgets");
}

#[tokio::test]
async fn a_budget_reached_stops_new_work_and_says_why() {
    let (repo, item_id) = setup_repo().await;
    // Noon, so the poll three hours later is still the same day: a day's tokens
    // start over at midnight UTC, and a run that crossed it chose the request.
    let now = Utc::now()
        .date_naive()
        .and_hms_opt(12, 0, 0)
        .expect("noon exists")
        .and_utc();
    setup_codex_runner(&repo, "runner-a", 3, 3, Some(0), now).await;
    // One run already going, with measured usage, and one item waiting for review.
    enqueue_for_runner(
        &repo,
        &item_id,
        "running",
        "runner-a",
        Some("opaque/model-alpha"),
        "{}",
        now,
    )
    .await;
    repo.claim_execution_idempotent_with_snapshot(
        "runner-a",
        "claim",
        "attempt",
        // Held past the three-hour poll below: a lease that ended unrenewed
        // would be handed to a person, which is not what this test is about.
        chrono::Duration::hours(4),
        &FixedClock(now),
        RequestSelection::Scheduled(Some("running")),
    )
    .await
    .expect("no db error")
    .expect("claimed");
    sqlx::query("UPDATE execution_attempts SET usage = ? WHERE id = 'attempt'")
        .bind(r#"{"tokens_in":{"value":60,"source":"measured"},"tokens_out":{"value":50,"source":"measured"}}"#)
        .execute(repo.pool())
        .await
        .expect("usage");
    // A second item whose finished run kept its workspace: it shows Needs your review.
    sqlx::query(
        "INSERT INTO items (id, project_id, title, status) \
         SELECT 'item-review', project_id, 'Review me', 'In Progress' FROM items WHERE id = ?",
    )
    .bind(&item_id)
    .execute(repo.pool())
    .await
    .expect("second item");
    enqueue_for_runner(
        &repo,
        "item-review",
        "review",
        "runner-a",
        Some("opaque/model-alpha"),
        "{}",
        now,
    )
    .await;
    repo.claim_execution_idempotent_with_snapshot(
        "runner-a",
        "claim-review",
        "attempt-review",
        chrono::Duration::seconds(60),
        &FixedClock(now),
        RequestSelection::Scheduled(Some("review")),
    )
    .await
    .expect("no db error")
    .expect("claimed");
    sqlx::query(
        "UPDATE execution_attempts SET state = 'succeeded', \
         terminal_reason = '{\"workspace_kept_at\":\"/tmp/kept\"}' WHERE id = 'attempt-review'",
    )
    .execute(repo.pool())
    .await
    .expect("finished with a kept workspace");
    enqueue_for_runner(
        &repo,
        &item_id,
        "next",
        "runner-a",
        Some("opaque/model-alpha"),
        "{}",
        now,
    )
    .await;
    let policy = SchedulingPolicy::default();

    // (budgets, expected code and sentence; None = the request is chosen)
    let cases = [
        ("no row", None, None, None),
        (
            "tokens",
            Some(100),
            None,
            Some("This project used its tokens for today"),
        ),
        (
            "queue",
            None,
            Some(1),
            Some("This project's review queue is full (1 waiting)"),
        ),
    ];
    for (case, tokens, queue, expect) in cases {
        if case == "no row" {
            // no project_budgets row at all
        } else {
            set_budgets(&repo, &item_id, tokens, queue).await;
        }
        let chosen = choose_request_for_runner(&repo, "runner-a", now, &policy)
            .await
            .expect("no db error");
        let (state, code, reason, since) = waiting_row(&repo, "next").await;
        match expect {
            None => {
                assert_eq!(chosen.as_deref(), Some("next"), "{case}");
                assert_eq!(state, "queued", "{case}");
            }
            Some(sentence) => {
                assert_eq!(chosen, None, "{case}");
                assert_eq!(code.as_deref(), Some("budget_reached"), "{case}");
                assert_eq!(reason.as_deref(), Some(sentence), "{case}");
                assert_eq!(since, Some(now.to_rfc3339()), "{case}");
                // The budget never fails the request, however long it waits.
                let later = now + chrono::Duration::hours(3);
                choose_request_for_runner(&repo, "runner-a", later, &policy)
                    .await
                    .expect("no db error");
                assert_eq!(waiting_row(&repo, "next").await.0, "queued", "{case}");
            }
        }
    }
    // The running attempt is untouched throughout.
    let running: String =
        sqlx::query_scalar("SELECT state FROM execution_attempts WHERE id = 'attempt'")
            .fetch_one(repo.pool())
            .await
            .expect("attempt");
    assert_eq!(running, "leased");
}

// ADR 0083 decision 5: the day's limit is reserved when a run is admitted.
#[tokio::test]
async fn the_days_tokens_are_reserved_by_open_runs() {
    let (repo, item_id) = setup_repo().await;
    let now = Utc::now();
    setup_codex_runner(&repo, "runner-a", 5, 5, Some(0), now).await;
    set_budgets(&repo, &item_id, Some(2_000_000), None).await;
    // Two open runs of 800k each hold 1.6M of the 2M day.
    for n in 1..=2 {
        let id = format!("open-{n}");
        enqueue_for_runner(&repo, &item_id, &id, "runner-a", ALPHA_MODEL, "{}", now).await;
        set_run_limit(&repo, &id, 800_000).await;
        repo.claim_execution_idempotent_with_snapshot(
            "runner-a",
            &format!("claim-{n}"),
            &format!("attempt-{n}"),
            chrono::Duration::hours(4),
            &FixedClock(now),
            RequestSelection::Scheduled(Some(&id)),
        )
        .await
        .expect("no db error")
        .expect("claimed");
    }
    let project_id: String = sqlx::query_scalar("SELECT project_id FROM items WHERE id = ?")
        .bind(&item_id)
        .fetch_one(repo.pool())
        .await
        .expect("project");
    let room = tack_db::repo::budgets::day_room(repo.pool(), &project_id, now)
        .await
        .expect("no db error")
        .expect("a daily limit");
    assert_eq!(
        (room.counted, room.reserved, room.open_runs, room.room),
        (0, 1_600_000, 2, 400_000)
    );
    // An open run that has counted 400k so far (500k in, 150k of them read from the
    // cache, 50k out) reserves 400k less; the day's room is the same, the 400k now counted.
    let usage = serde_json::json!({
        "tokens_in": {"value": 500_000, "source": "measured"},
        "tokens_out": {"value": 50_000, "source": "measured"},
        "cache_read_tokens": 150_000,
    });
    sqlx::query(
        "INSERT INTO execution_events(id,attempt_id,event_id,sequence,source,kind,payload,occurred_at,created_at) \
         VALUES('row-usage','attempt-1','evt-usage',1,'runner','agent.hop_finished',?,?,?)",
    )
    .bind(serde_json::json!({"text": "done", "usage": usage}).to_string())
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .execute(repo.pool())
    .await
    .expect("a progress line with usage");
    let running = tack_db::repo::budgets::day_room(repo.pool(), &project_id, now)
        .await
        .expect("no db error")
        .expect("a daily limit");
    assert_eq!(
        (
            running.counted,
            running.reserved,
            running.open_runs,
            running.room
        ),
        (400_000, 1_200_000, 2, 400_000)
    );
    sqlx::query("DELETE FROM execution_events WHERE id = 'row-usage'")
        .execute(repo.pool())
        .await
        .expect("back to no usage");

    // A third that asks for 500k waits, and says why; one that asks for nothing is
    // admitted with the 400k that is left as its limit.
    enqueue_for_runner(
        &repo,
        &item_id,
        "asks-500k",
        "runner-a",
        ALPHA_MODEL,
        "{}",
        now,
    )
    .await;
    set_run_limit(&repo, "asks-500k", 500_000).await;
    enqueue_for_runner(
        &repo,
        &item_id,
        "asks-none",
        "runner-a",
        ALPHA_MODEL,
        "{}",
        now,
    )
    .await;
    let chosen = choose_request_for_runner(&repo, "runner-a", now, &SchedulingPolicy::default())
        .await
        .expect("no db error");
    assert_eq!(chosen.as_deref(), Some("asks-none"));
    let (state, code, reason, _) = waiting_row(&repo, "asks-500k").await;
    assert_eq!(state, "queued");
    assert_eq!(code.as_deref(), Some("budget_reached"));
    assert_eq!(
        reason.as_deref(),
        Some("Today's token limit is reserved by 2 open runs (1 600 000 of 2 000 000)")
    );
    let (tokens, source): (i64, String) = sqlx::query_as(
        "SELECT json_extract(request_snapshot, '$.budgets.tokens'), \
         json_extract(request_snapshot, '$.metadata.limit_source') \
         FROM execution_requests WHERE id = 'asks-none'",
    )
    .fetch_one(repo.pool())
    .await
    .expect("snapshot");
    assert_eq!((tokens, source.as_str()), (400_000, "day"));
    // A run asking for more than the whole day, with nothing reserved, says so.
    sqlx::query("UPDATE execution_attempts SET state = 'succeeded'")
        .execute(repo.pool())
        .await
        .expect("finish the open runs");
    enqueue_for_runner(
        &repo,
        &item_id,
        "asks-3m",
        "runner-a",
        ALPHA_MODEL,
        "{}",
        now,
    )
    .await;
    set_run_limit(&repo, "asks-3m", 3_000_000).await;
    choose_request_for_runner(&repo, "runner-a", now, &SchedulingPolicy::default())
        .await
        .expect("no db error");
    let (_, _, reason, _) = waiting_row(&repo, "asks-3m").await;
    assert_eq!(
        reason.as_deref(),
        Some(
            "This run's token limit is more than what is left of today's (2 000 000 of 2 000 000)"
        )
    );
    // The same request sent again is the one already there, not a conflict.
    let again = enqueue_for_runner(
        &repo,
        &item_id,
        "asks-none",
        "runner-a",
        ALPHA_MODEL,
        "{}",
        now,
    )
    .await;
    assert_eq!(
        again,
        tack_db::repo::execution::EnqueueResult::Replayed("asks-none".into())
    );
}

// A provisional limit is not the run's own: a day with less room than it admits the
// run with what is left, and a day with more keeps the provisional limit.
#[tokio::test]
async fn a_provisional_limit_gives_way_to_the_days_room() {
    for (day, expected) in [
        (300_000, (300_000, "day")),
        (5_000_000, (2_000_000, "provisional")),
    ] {
        let (repo, item_id) = setup_repo().await;
        let now = Utc::now();
        setup_codex_runner(&repo, "runner-a", 5, 5, Some(0), now).await;
        set_budgets(&repo, &item_id, Some(day), None).await;
        enqueue_for_runner(&repo, &item_id, "prov", "runner-a", ALPHA_MODEL, "{}", now).await;
        set_run_limit(&repo, "prov", 2_000_000).await;
        sqlx::query(
            "UPDATE execution_requests SET request_snapshot = \
             json_set(request_snapshot, '$.metadata.limit_source', 'provisional') WHERE id = 'prov'",
        )
        .execute(repo.pool())
        .await
        .expect("provisional source");
        let chosen =
            choose_request_for_runner(&repo, "runner-a", now, &SchedulingPolicy::default())
                .await
                .expect("no db error");
        assert_eq!(chosen.as_deref(), Some("prov"), "day {day}");
        let (tokens, source): (i64, String) = sqlx::query_as(
            "SELECT json_extract(request_snapshot, '$.budgets.tokens'), \
             json_extract(request_snapshot, '$.metadata.limit_source') \
             FROM execution_requests WHERE id = 'prov'",
        )
        .fetch_one(repo.pool())
        .await
        .expect("snapshot");
        assert_eq!((tokens, source.as_str()), expected, "day {day}");
    }
}

async fn set_run_limit(repo: &Repository, request_id: &str, tokens: i64) {
    sqlx::query(
        "UPDATE execution_requests SET request_snapshot = json_set(request_snapshot, '$.budgets.tokens', ?) WHERE id = ?",
    )
    .bind(tokens)
    .bind(request_id)
    .execute(repo.pool())
    .await
    .expect("run limit");
}
