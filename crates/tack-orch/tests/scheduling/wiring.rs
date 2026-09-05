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
use tack_db::repo::execution::{NewExecutionRequest, NewRunner};
use tack_orch::scheduler::wiring::choose_request_for_runner;
use tack_orch::scheduler::{SchedulingPolicy, choose_request_for_runner as choose};

use crate::support::{FixedClock, codex_capability_snapshot, setup_repo};

async fn register_active_runner(
    repo: &Repository,
    runner_id: &str,
    capacity: i64,
    capability_snapshot: &str,
    now: chrono::DateTime<Utc>,
) {
    repo.register_runner(
        NewRunner {
            id: runner_id,
            name: runner_id,
            credential_hash: "test-hash",
            labels: "{}",
            total_capacity: capacity,
            available_capacity: capacity,
            capability_snapshot,
            protocol_version: 1,
        },
        &FixedClock(now),
    )
    .await
    .expect("register runner");
    // `register_runner` inserts with the schema default `state='active'`
    // already (migration 040), so no extra activation step is needed here
    // — this mirrors real enrollment's end state, not the pending step.
    sqlx::query("UPDATE agent_runners SET last_heartbeat_at = ? WHERE id = ?")
        .bind(now.to_rfc3339())
        .bind(runner_id)
        .execute(repo.pool())
        .await
        .expect("set heartbeat");
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
    model_provider: Option<&str>,
    model_id: Option<&str>,
    metadata: &str,
    created_at: chrono::DateTime<Utc>,
) {
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
        "requested_model_provider": model_provider,
        "requested_model_id": model_id,
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
            requested_model_provider: model_provider,
            requested_model_id: model_id,
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
    .expect("enqueue");
}

#[tokio::test]
async fn healthy_runner_with_a_matching_declared_combination_is_chosen() {
    let (repo, item_id) = setup_repo().await;
    let now = Utc::now();
    register_active_runner(&repo, "runner-a", 1, &codex_capability_snapshot(now), now).await;
    enqueue(
        &repo,
        &item_id,
        "req-a",
        "key-a",
        "exact_runner",
        "runner-a",
        "codex",
        Some("openai"),
        Some("opaque/model-alpha"),
        "{}",
        now,
    )
    .await;

    let chosen = choose_request_for_runner(&repo, "runner-a", now, &SchedulingPolicy::default())
        .await
        .expect("no db error");
    assert_eq!(chosen.as_deref(), Some("req-a"));

    // Re-exported entry point (`tack_orch::scheduler::choose_request_for_runner`)
    // must be the exact same function, not a second, drifting copy.
    let chosen_via_reexport = choose(&repo, "runner-a", now, &SchedulingPolicy::default())
        .await
        .expect("no db error");
    assert_eq!(chosen_via_reexport.as_deref(), Some("req-a"));
}

#[tokio::test]
async fn a_declared_but_mismatched_model_is_never_chosen() {
    let (repo, item_id) = setup_repo().await;
    let now = Utc::now();
    register_active_runner(&repo, "runner-a", 1, &codex_capability_snapshot(now), now).await;
    enqueue(
        &repo,
        &item_id,
        "req-bad-model",
        "key-bad",
        "exact_runner",
        "runner-a",
        "codex",
        Some("openai"),
        Some("opaque/model-that-does-not-exist"),
        "{}",
        now,
    )
    .await;

    let chosen = choose_request_for_runner(&repo, "runner-a", now, &SchedulingPolicy::default())
        .await
        .expect("no db error");
    assert_eq!(
        chosen, None,
        "an undeclared model combination must never be handed to the claim transaction"
    );
}

#[tokio::test]
async fn a_runner_with_no_declared_harnesses_never_claims_anything() {
    let (repo, item_id) = setup_repo().await;
    let now = Utc::now();
    // `{}` is the schema default for a runner that never enrolled/refreshed
    // — deliberately does not parse as `EmbeddedCapabilitySnapshot`.
    register_active_runner(&repo, "runner-bare", 1, "{}", now).await;
    enqueue(
        &repo,
        &item_id,
        "req-bare",
        "key-bare",
        "exact_runner",
        "runner-bare",
        "codex",
        Some("openai"),
        Some("opaque/model-alpha"),
        "{}",
        now,
    )
    .await;

    let chosen = choose_request_for_runner(&repo, "runner-bare", now, &SchedulingPolicy::default())
        .await
        .expect("no db error");
    assert_eq!(
        chosen, None,
        "no declared harness means no eligible pick, not a crash"
    );
}

#[tokio::test]
async fn per_runner_capacity_saturation_leaves_the_request_unchosen() {
    let (repo, item_id) = setup_repo().await;
    let now = Utc::now();
    // Zero available capacity from the start (total=1, available=0) —
    // simulates a runner that already has its one slot in use.
    repo.register_runner(
        NewRunner {
            id: "runner-full",
            name: "runner-full",
            credential_hash: "test-hash",
            labels: "{}",
            total_capacity: 1,
            available_capacity: 0,
            capability_snapshot: &codex_capability_snapshot(now),
            protocol_version: 1,
        },
        &FixedClock(now),
    )
    .await
    .expect("register runner");
    sqlx::query("UPDATE agent_runners SET last_heartbeat_at = ? WHERE id = ?")
        .bind(now.to_rfc3339())
        .bind("runner-full")
        .execute(repo.pool())
        .await
        .expect("heartbeat");
    enqueue(
        &repo,
        &item_id,
        "req-saturated",
        "key-sat",
        "exact_runner",
        "runner-full",
        "codex",
        Some("openai"),
        Some("opaque/model-alpha"),
        "{}",
        now,
    )
    .await;

    let chosen = choose_request_for_runner(&repo, "runner-full", now, &SchedulingPolicy::default())
        .await
        .expect("no db error");
    assert_eq!(
        chosen, None,
        "a runner with zero available capacity has no eligible pick"
    );
}

#[tokio::test]
async fn high_priority_metadata_wins_over_an_older_normal_priority_request() {
    let (repo, item_id) = setup_repo().await;
    let now = Utc::now();
    register_active_runner(&repo, "runner-a", 1, &codex_capability_snapshot(now), now).await;
    let earlier = now - chrono::Duration::seconds(60);
    enqueue(
        &repo,
        &item_id,
        "req-old-normal",
        "key-old",
        "exact_runner",
        "runner-a",
        "codex",
        Some("openai"),
        Some("opaque/model-alpha"),
        "{}",
        earlier,
    )
    .await;
    enqueue(
        &repo,
        &item_id,
        "req-new-high",
        "key-new",
        "exact_runner",
        "runner-a",
        "codex",
        Some("openai"),
        Some("opaque/model-alpha"),
        r#"{"priority":"high"}"#,
        now,
    )
    .await;

    let chosen = choose_request_for_runner(&repo, "runner-a", now, &SchedulingPolicy::default())
        .await
        .expect("no db error");
    assert_eq!(
        chosen.as_deref(),
        Some("req-new-high"),
        "a request whose metadata declares priority:high must be chosen over an older \
         normal-priority peer"
    );
}

#[tokio::test]
async fn fifo_within_the_same_priority_picks_the_older_request() {
    let (repo, item_id) = setup_repo().await;
    let now = Utc::now();
    register_active_runner(&repo, "runner-a", 1, &codex_capability_snapshot(now), now).await;
    let earlier = now - chrono::Duration::seconds(60);
    enqueue(
        &repo,
        &item_id,
        "req-older",
        "key-older",
        "exact_runner",
        "runner-a",
        "codex",
        Some("openai"),
        Some("opaque/model-alpha"),
        "{}",
        earlier,
    )
    .await;
    enqueue(
        &repo,
        &item_id,
        "req-newer",
        "key-newer",
        "exact_runner",
        "runner-a",
        "codex",
        Some("openai"),
        Some("opaque/model-alpha"),
        "{}",
        now,
    )
    .await;

    let chosen = choose_request_for_runner(&repo, "runner-a", now, &SchedulingPolicy::default())
        .await
        .expect("no db error");
    assert_eq!(chosen.as_deref(), Some("req-older"));
}

#[tokio::test]
async fn a_saturated_fleet_concurrency_limit_blocks_a_fleet_selector_request() {
    let (repo, item_id) = setup_repo().await;
    let now = Utc::now();
    // Fleet capped at 1 concurrent execution; already "in use" by a second
    // member runner whose capacity is fully reserved.
    let fleet_id = "fleet-capped";
    sqlx::query(
        "INSERT INTO agent_fleets (id, name, concurrency_limit, default_policy, created_at, updated_at) \
         VALUES (?, 'Capped Fleet', 1, '{}', ?, ?)",
    )
    .bind(fleet_id)
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .execute(repo.pool())
    .await
    .expect("insert fleet");

    register_active_runner(
        &repo,
        "runner-member",
        1,
        &codex_capability_snapshot(now),
        now,
    )
    .await;
    // A second fleet member whose one slot is already fully consumed —
    // this is what makes the fleet's aggregate in-use capacity hit its cap.
    repo.register_runner(
        NewRunner {
            id: "runner-other-member",
            name: "runner-other-member",
            credential_hash: "test-hash",
            labels: "{}",
            total_capacity: 1,
            available_capacity: 0,
            capability_snapshot: &codex_capability_snapshot(now),
            protocol_version: 1,
        },
        &FixedClock(now),
    )
    .await
    .expect("register second runner");
    for runner_id in ["runner-member", "runner-other-member"] {
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

    enqueue(
        &repo,
        &item_id,
        "req-fleet-capped",
        "key-fleet",
        "fleet",
        fleet_id,
        "codex",
        Some("openai"),
        Some("opaque/model-alpha"),
        "{}",
        now,
    )
    .await;

    let chosen =
        choose_request_for_runner(&repo, "runner-member", now, &SchedulingPolicy::default())
            .await
            .expect("no db error");
    assert_eq!(
        chosen, None,
        "a fleet already at its concurrency_limit must reject every fleet-selector request, \
         even though the polling runner itself has a free slot"
    );
}

#[tokio::test]
async fn an_unsaturated_fleet_still_allows_a_member_to_claim() {
    let (repo, item_id) = setup_repo().await;
    let now = Utc::now();
    let fleet_id = "fleet-open";
    sqlx::query(
        "INSERT INTO agent_fleets (id, name, concurrency_limit, default_policy, created_at, updated_at) \
         VALUES (?, 'Open Fleet', 5, '{}', ?, ?)",
    )
    .bind(fleet_id)
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .execute(repo.pool())
    .await
    .expect("insert fleet");
    register_active_runner(
        &repo,
        "runner-member",
        1,
        &codex_capability_snapshot(now),
        now,
    )
    .await;
    sqlx::query(
        "INSERT INTO agent_fleet_members (fleet_id, runner_id, created_at) VALUES (?, ?, ?)",
    )
    .bind(fleet_id)
    .bind("runner-member")
    .bind(now.to_rfc3339())
    .execute(repo.pool())
    .await
    .expect("fleet membership");

    enqueue(
        &repo,
        &item_id,
        "req-fleet-open",
        "key-fleet-open",
        "fleet",
        fleet_id,
        "codex",
        Some("openai"),
        Some("opaque/model-alpha"),
        "{}",
        now,
    )
    .await;

    let chosen =
        choose_request_for_runner(&repo, "runner-member", now, &SchedulingPolicy::default())
            .await
            .expect("no db error");
    assert_eq!(chosen.as_deref(), Some("req-fleet-open"));
}

#[tokio::test]
async fn a_stale_heartbeat_disqualifies_a_runner_that_would_otherwise_match() {
    let (repo, item_id) = setup_repo().await;
    let now = Utc::now();
    register_active_runner(
        &repo,
        "runner-stale",
        1,
        &codex_capability_snapshot(now),
        now,
    )
    .await;
    // Overwrite the heartbeat this test's own helper just set to something
    // far outside `SchedulingPolicy::default()`'s 60-second window.
    sqlx::query("UPDATE agent_runners SET last_heartbeat_at = ? WHERE id = ?")
        .bind((now - chrono::Duration::seconds(600)).to_rfc3339())
        .bind("runner-stale")
        .execute(repo.pool())
        .await
        .expect("stale heartbeat");
    enqueue(
        &repo,
        &item_id,
        "req-stale",
        "key-stale",
        "exact_runner",
        "runner-stale",
        "codex",
        Some("openai"),
        Some("opaque/model-alpha"),
        "{}",
        now,
    )
    .await;

    let chosen =
        choose_request_for_runner(&repo, "runner-stale", now, &SchedulingPolicy::default())
            .await
            .expect("no db error");
    assert_eq!(chosen, None);
}

#[tokio::test]
async fn no_queued_work_at_all_is_a_clean_none_not_an_error() {
    let (repo, _item_id) = setup_repo().await;
    let now = Utc::now();
    register_active_runner(
        &repo,
        "runner-idle",
        1,
        &codex_capability_snapshot(now),
        now,
    )
    .await;

    let chosen = choose_request_for_runner(&repo, "runner-idle", now, &SchedulingPolicy::default())
        .await
        .expect("no db error");
    assert_eq!(chosen, None);
}

#[tokio::test]
async fn an_unknown_runner_id_is_a_clean_none_not_an_error() {
    let (repo, _item_id) = setup_repo().await;
    let now = Utc::now();
    let chosen =
        choose_request_for_runner(&repo, "does-not-exist", now, &SchedulingPolicy::default())
            .await
            .expect("no db error");
    assert_eq!(chosen, None);
}

/// A freshly enrolled runner polling for its very first claim has never
/// called `/heartbeat` (that endpoint only reports *active attempt lease*
/// renewals — `agent_runners.last_heartbeat_at` stays `NULL` until a runner
/// has already been granted at least one lease). Without the capability
/// snapshot's own `reported_at` fallback in `wiring.rs`, this would be a
/// deadlock: no runner could ever get its first piece of work. This is the
/// load-bearing proof that the fallback closes that gap.
#[tokio::test]
async fn a_freshly_enrolled_runner_with_no_heartbeat_yet_can_still_claim_its_first_request() {
    let (repo, item_id) = setup_repo().await;
    let now = Utc::now();
    repo.register_runner(
        NewRunner {
            id: "runner-fresh",
            name: "runner-fresh",
            credential_hash: "test-hash",
            labels: "{}",
            total_capacity: 1,
            available_capacity: 1,
            capability_snapshot: &codex_capability_snapshot(now),
            protocol_version: 1,
        },
        &FixedClock(now),
    )
    .await
    .expect("register runner");
    // Deliberately never sets `last_heartbeat_at` — this is the exact state
    // `redeem_enrollment_token`/the `/refresh` handler leave a runner in.
    let heartbeat: Option<String> =
        sqlx::query_scalar("SELECT last_heartbeat_at FROM agent_runners WHERE id = 'runner-fresh'")
            .fetch_one(repo.pool())
            .await
            .expect("read heartbeat column");
    assert_eq!(
        heartbeat, None,
        "fixture must start with no heartbeat, matching real enrollment"
    );

    enqueue(
        &repo,
        &item_id,
        "req-first-claim",
        "key-first",
        "exact_runner",
        "runner-fresh",
        "codex",
        Some("openai"),
        Some("opaque/model-alpha"),
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

/// The fallback in the test above is not "no heartbeat ever means eligible"
/// — a runner that enrolled/refreshed long ago and never heartbeated since
/// still goes stale once its *capability report* itself ages past the
/// policy window.
#[tokio::test]
async fn a_never_heartbeated_runner_with_a_stale_capability_report_is_still_rejected() {
    let (repo, item_id) = setup_repo().await;
    let now = Utc::now();
    let stale_report_time = now - chrono::Duration::seconds(600);
    repo.register_runner(
        NewRunner {
            id: "runner-stale-report",
            name: "runner-stale-report",
            credential_hash: "test-hash",
            labels: "{}",
            total_capacity: 1,
            available_capacity: 1,
            capability_snapshot: &codex_capability_snapshot(stale_report_time),
            protocol_version: 1,
        },
        &FixedClock(now),
    )
    .await
    .expect("register runner");

    enqueue(
        &repo,
        &item_id,
        "req-stale-report",
        "key-stale-report",
        "exact_runner",
        "runner-stale-report",
        "codex",
        Some("openai"),
        Some("opaque/model-alpha"),
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
