//! III-F2 repository-level tests: event-batch atomicity and event/artifact
//! retention sweep behavior. HTTP-level artifact-content (streaming,
//! checksum, path-safety) tests live in
//! `crates/tack-api/tests/f2_artifact_events_test.rs`; this file only proves
//! what belongs at the `tack-db` layer.

mod common;

use std::sync::Mutex;

use chrono::{DateTime, Duration, Utc};
use tack_db::{
    Repository,
    repo::execution::{
        ArtifactContentCommitResult, AttemptTransitionInput, AttemptTransitionPhase,
        EventApplyResult, EventBatch, ExecutionClock, NewAgentProfile, NewArtifact, NewEvent,
        NewExecutionRequest, NewRunner, RequestSelection,
    },
};

struct FakeClock(Mutex<DateTime<Utc>>);

impl FakeClock {
    fn new() -> Self {
        Self(Mutex::new(
            DateTime::parse_from_rfc3339("2026-08-12T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        ))
    }
}

impl ExecutionClock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap()
    }
}

async fn ready_repo() -> (Repository, String, FakeClock) {
    let repo = common::setup_test_db().await;
    let workspace = common::create_test_workspace(&repo).await;
    let project = common::make_project(&repo, workspace).await;
    let item = common::make_item(&repo, &project).await;
    let clock = FakeClock::new();
    repo.register_runner(
        NewRunner {
            id: "runner-f2",
            name: "Runner F2",
            credential_hash: "hash-only",
            labels: "{}",
            total_capacity: 2,
            available_capacity: 2,
            capability_snapshot: "{}",
            protocol_version: 1,
        },
        &clock,
    )
    .await
    .unwrap();
    repo.create_agent_profile(
        NewAgentProfile {
            id: "profile-f2",
            name: "Profile F2",
            instructions: "test",
            tool_policy: "{}",
            limits: "{}",
        },
        &clock,
    )
    .await
    .unwrap();
    (repo, item.id.to_string(), clock)
}

fn request<'a>(id: &'a str, item_id: &'a str, key: &'a str) -> NewExecutionRequest<'a> {
    let request_snapshot: &'static str = Box::leak(
        format!(
            r#"{{"request_id":"{id}","item_id":"{item_id}","idempotency_key":"{key}","created_by":{{"source":"operator","subject_id":"test"}},"created_at":"2026-08-12T12:00:00Z","selector":{{"kind":"exact_runner","runner_id":"runner-f2"}},"agent_profile_id":"profile-f2","resolved_agent_profile":{{"name":"Profile F2","instructions":"test","tool_policy":{{"mode":"safe"}},"timeout_seconds":60,"budgets":{{"limit":1}}}},"requested_harness_kind":"codex","requested_model_provider":"openai","requested_model_id":"opaque/model","repository":{{"kind":"git","remote":"https://example.test/repo.git","base_revision":"abc123","subdirectory":null}},"permission_policy":{{"tools":["shell"],"network":false}},"timeout_seconds":60,"budgets":{{"limit":1}},"status_map_policy_id":null,"environment":{{}},"metadata":{{}}}}"#
        )
        .into_boxed_str(),
    );
    NewExecutionRequest {
        id,
        item_id,
        idempotency_scope: "item",
        idempotency_key: key,
        request_fingerprint: key,
        selector_kind: "exact_runner",
        selector_id: "runner-f2",
        agent_profile_id: Some("profile-f2"),
        agent_profile_snapshot: r#"{"name":"Profile F2","instructions":"test","tool_policy":{"mode":"safe"},"timeout_seconds":60,"budgets":{"limit":1}}"#,
        requested_harness_kind: Some("codex"),
        requested_model_provider: Some("openai"),
        requested_model_id: Some("opaque/model"),
        repository_snapshot: r#"{"kind":"git","remote":"https://example.test/repo.git","base_revision":"abc123","subdirectory":null}"#,
        permission_policy: r#"{"tools":["shell"],"network":false}"#,
        timeout_seconds: Some(60),
        budgets: r#"{"limit":1}"#,
        status_map_policy_id: None,
        environment: "{}",
        metadata: "{}",
        request_snapshot,
    }
}

/// Enqueues, claims, and drives one attempt all the way to `running` so
/// event/artifact writes are eligible. Returns its fencing token.
async fn ready_running_attempt(
    repo: &Repository,
    item_id: &str,
    clock: &FakeClock,
    request_id: &str,
    attempt_id: &str,
) -> i64 {
    repo.enqueue_execution(request(request_id, item_id, request_id), clock)
        .await
        .unwrap();
    let claim = repo
        .claim_execution_idempotent_with_snapshot(
            "runner-f2",
            attempt_id,
            attempt_id,
            Duration::seconds(300),
            clock,
            RequestSelection::Naive,
        )
        .await
        .unwrap()
        .unwrap();
    let fence = claim.lease.fencing_token;
    repo.transition_attempt_with_facts(
        AttemptTransitionInput {
            runner_id: "runner-f2",
            attempt_id,
            fencing_token: fence,
            phase: AttemptTransitionPhase::Preparing,
            workspace_id: "workspace-1",
            base_revision: "abc123",
            process_id: None,
        },
        clock,
    )
    .await
    .unwrap();
    repo.transition_attempt_with_facts(
        AttemptTransitionInput {
            runner_id: "runner-f2",
            attempt_id,
            fencing_token: fence,
            phase: AttemptTransitionPhase::Running,
            workspace_id: "workspace-1",
            base_revision: "abc123",
            process_id: Some("pid-1"),
        },
        clock,
    )
    .await
    .unwrap();
    fence
}

fn event<'a>(
    id: &'a str,
    event_id: &'a str,
    sequence: i64,
    occurred_at: DateTime<Utc>,
) -> NewEvent<'a> {
    NewEvent {
        id,
        event_id,
        sequence,
        source: "runner",
        kind: "progress",
        payload: r#"{"phase":"test"}"#,
        occurred_at,
    }
}

// ---------------------------------------------------------------------
// Acceptance: "checkpoint never advances after failed insert."
// ---------------------------------------------------------------------

/// Forces the *second* event's INSERT to fail via a `BEFORE INSERT` trigger
/// (the same deterministic technique `completion_replay_insert_failure_rolls_back_terminal_transition`
/// already uses in `execution_repo_test.rs` for an analogous claim), inside a
/// batch whose first event would otherwise succeed. Proves the whole batch —
/// not just the failing row — rolls back: zero event rows land, and the
/// attempt's `event_checkpoint` stays exactly at its last successfully
/// committed value, never advancing to the failed batch's checkpoint and
/// never regressing to `NULL` either.
///
/// Load-bearing proof performed by hand (not left in the tree): temporarily
/// changed `append_execution_events_result`'s per-event INSERT to execute
/// against `self.pool()` instead of `&mut *tx` (i.e. outside the shared
/// transaction). Re-running this test did not merely fail an assertion — the
/// test process **hung indefinitely**: the outer `BEGIN IMMEDIATE`
/// transaction was still holding the pool's one connection/write lock
/// (in-memory SQLite here), and the detached insert's own attempt to check
/// out a connection from the same pool deadlocked against it, exactly the
/// class of self-deadlock CLAUDE.md's "`BEGIN IMMEDIATE` is mandatory for
/// read-then-write transactions" warning describes. Killed the hung process,
/// reverted the change, and confirmed the test passes again (and completes
/// promptly) with the real code. This is a stronger confirmation than a
/// clean assertion failure would have been: the transaction-scoped insert is
/// load-bearing for both atomicity *and* avoiding a real deadlock.
#[tokio::test]
async fn event_batch_insert_failure_leaves_checkpoint_and_rows_untouched() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_running_attempt(
        &repo,
        &item_id,
        &clock,
        "request-event-atomic",
        "attempt-event-atomic",
    )
    .await;

    // First batch: commits cleanly, establishing a known-good checkpoint.
    let first = repo
        .append_execution_events_result(
            EventBatch {
                runner_id: "runner-f2",
                attempt_id: "attempt-event-atomic",
                fencing_token: fence,
                previous_checkpoint: None,
                checkpoint: "checkpoint-0001",
            },
            &[event("evt-row-1", "evt-1", 1, clock.now())],
            &clock,
        )
        .await
        .unwrap();
    assert!(matches!(first, EventApplyResult::Applied(_)));
    let checkpoint_before: Option<String> = sqlx::query_scalar(
        "SELECT event_checkpoint FROM execution_attempts WHERE id = 'attempt-event-atomic'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(checkpoint_before.as_deref(), Some("checkpoint-0001"));

    // Force the *second* event's insert to fail deterministically.
    sqlx::query(
        "CREATE TRIGGER f2_fail_second_event BEFORE INSERT ON execution_events \
         WHEN NEW.sequence = 2 \
         BEGIN SELECT RAISE(ABORT, 'forced second-event insert failure'); END",
    )
    .execute(repo.pool())
    .await
    .unwrap();

    let second = repo
        .append_execution_events_result(
            EventBatch {
                runner_id: "runner-f2",
                attempt_id: "attempt-event-atomic",
                fencing_token: fence,
                previous_checkpoint: Some("checkpoint-0001"),
                checkpoint: "checkpoint-0002",
            },
            &[
                event("evt-row-2", "evt-2", 2, clock.now()),
                event("evt-row-3", "evt-3", 3, clock.now()),
            ],
            &clock,
        )
        .await;
    assert!(
        second.is_err(),
        "the forced trigger must surface as a real error, not be silently absorbed"
    );

    let checkpoint_after: Option<String> = sqlx::query_scalar(
        "SELECT event_checkpoint FROM execution_attempts WHERE id = 'attempt-event-atomic'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(
        checkpoint_after.as_deref(),
        Some("checkpoint-0001"),
        "checkpoint must remain at its last successfully committed value, not advance"
    );

    let event_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_events WHERE attempt_id = 'attempt-event-atomic'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(
        event_count, 1,
        "only the first batch's one row may exist; the second batch's rows (including \
         evt-2, whose own insert did not trigger the abort) must not survive"
    );

    let replay_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_event_batch_replays WHERE attempt_id = 'attempt-event-atomic' AND checkpoint = 'checkpoint-0002'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(
        replay_rows, 0,
        "no replay bookkeeping for the failed batch may be committed either"
    );
}

/// Same forced-failure technique, but asserts the absence in the case the
/// card's acceptance text names literally first: a *fresh* attempt (no prior
/// successful batch) whose very first batch fails must leave
/// `event_checkpoint` at `NULL`, not some other value, and zero rows.
#[tokio::test]
async fn event_batch_insert_failure_on_a_fresh_attempt_leaves_checkpoint_null() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_running_attempt(
        &repo,
        &item_id,
        &clock,
        "request-event-fresh-fail",
        "attempt-event-fresh-fail",
    )
    .await;

    sqlx::query(
        "CREATE TRIGGER f2_fail_all_events BEFORE INSERT ON execution_events \
         BEGIN SELECT RAISE(ABORT, 'forced insert failure'); END",
    )
    .execute(repo.pool())
    .await
    .unwrap();

    let result = repo
        .append_execution_events_result(
            EventBatch {
                runner_id: "runner-f2",
                attempt_id: "attempt-event-fresh-fail",
                fencing_token: fence,
                previous_checkpoint: None,
                checkpoint: "checkpoint-0001",
            },
            &[event("evt-row-x", "evt-x", 1, clock.now())],
            &clock,
        )
        .await;
    assert!(result.is_err());

    let checkpoint: Option<String> = sqlx::query_scalar(
        "SELECT event_checkpoint FROM execution_attempts WHERE id = 'attempt-event-fresh-fail'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(checkpoint, None);

    let event_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_events WHERE attempt_id = 'attempt-event-fresh-fail'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(event_count, 0);
}

// ---------------------------------------------------------------------
// `set_execution_artifact_content_reference`: immutability + fencing.
// ---------------------------------------------------------------------

#[tokio::test]
async fn content_reference_is_committed_once_and_never_overwritten() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_running_attempt(
        &repo,
        &item_id,
        &clock,
        "request-content-ref",
        "attempt-content-ref",
    )
    .await;
    let written = repo
        .record_execution_artifact(
            "runner-f2",
            "attempt-content-ref",
            fence,
            NewArtifact {
                id: "art-row-1",
                artifact_id: "art-1",
                kind: "patch",
                name: "changes.patch",
                media_type: Some("text/x-diff"),
                size_bytes: 4,
                sha256: &"0".repeat(64),
                content_disposition: Some("inline_upload"),
                content_reference: None,
                metadata: "{}",
            },
            &clock,
        )
        .await
        .unwrap();
    assert!(written);

    let first = repo
        .set_execution_artifact_content_reference(
            "runner-f2",
            "attempt-content-ref",
            "art-1",
            fence,
            "attempt-content-ref-hex/blob-1",
            &clock,
        )
        .await
        .unwrap();
    assert_eq!(first, ArtifactContentCommitResult::Committed);

    // A second attempt to set it — even to a different value — is refused,
    // not silently overwritten.
    let second = repo
        .set_execution_artifact_content_reference(
            "runner-f2",
            "attempt-content-ref",
            "art-1",
            fence,
            "attempt-content-ref-hex/blob-DIFFERENT",
            &clock,
        )
        .await
        .unwrap();
    assert_eq!(second, ArtifactContentCommitResult::AlreadySet);

    let stored: Option<String> = sqlx::query_scalar(
        "SELECT content_reference FROM execution_artifacts WHERE attempt_id='attempt-content-ref' AND artifact_id='art-1'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(stored.as_deref(), Some("attempt-content-ref-hex/blob-1"));
}

#[tokio::test]
async fn content_reference_write_with_a_stale_fence_is_rejected() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_running_attempt(
        &repo,
        &item_id,
        &clock,
        "request-content-stale",
        "attempt-content-stale",
    )
    .await;
    repo.record_execution_artifact(
        "runner-f2",
        "attempt-content-stale",
        fence,
        NewArtifact {
            id: "art-row-2",
            artifact_id: "art-2",
            kind: "log",
            name: "run.log",
            media_type: Some("text/plain"),
            size_bytes: 3,
            sha256: &"1".repeat(64),
            content_disposition: Some("inline_upload"),
            content_reference: None,
            metadata: "{}",
        },
        &clock,
    )
    .await
    .unwrap();

    let result = repo
        .set_execution_artifact_content_reference(
            "runner-f2",
            "attempt-content-stale",
            "art-2",
            fence + 1, // wrong fence
            "attempt-content-stale-hex/blob-2",
            &clock,
        )
        .await
        .unwrap();
    assert_eq!(result, ArtifactContentCommitResult::Stale);

    let stored: Option<String> = sqlx::query_scalar(
        "SELECT content_reference FROM execution_artifacts WHERE attempt_id='attempt-content-stale' AND artifact_id='art-2'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(stored, None);
}

// ---------------------------------------------------------------------
// Retention: `purge_execution_events_older_than` /
// `list_execution_artifacts_older_than` / `delete_execution_artifacts_by_row_ids`.
// ---------------------------------------------------------------------

#[tokio::test]
async fn event_retention_purges_only_rows_older_than_cutoff_in_bounded_batches() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_running_attempt(
        &repo,
        &item_id,
        &clock,
        "request-retention-events",
        "attempt-retention-events",
    )
    .await;
    let old_batch = repo
        .append_execution_events_result(
            EventBatch {
                runner_id: "runner-f2",
                attempt_id: "attempt-retention-events",
                fencing_token: fence,
                previous_checkpoint: None,
                checkpoint: "checkpoint-0001",
            },
            &[
                event("evt-row-old-1", "evt-old-1", 1, clock.now()),
                event("evt-row-old-2", "evt-old-2", 2, clock.now()),
                event("evt-row-old-3", "evt-old-3", 3, clock.now()),
            ],
            &clock,
        )
        .await
        .unwrap();
    assert!(matches!(old_batch, EventApplyResult::Applied(_)));
    let fresh_batch = repo
        .append_execution_events_result(
            EventBatch {
                runner_id: "runner-f2",
                attempt_id: "attempt-retention-events",
                fencing_token: fence,
                previous_checkpoint: Some("checkpoint-0001"),
                checkpoint: "checkpoint-0002",
            },
            &[event("evt-row-fresh-1", "evt-fresh-1", 4, clock.now())],
            &clock,
        )
        .await
        .unwrap();
    assert!(matches!(fresh_batch, EventApplyResult::Applied(_)));

    // Backdate the first batch's rows directly rather than advancing the
    // shared clock 40 days (which would also expire the lease this test's
    // *second* insert still needs to succeed through the normal eligibility
    // check — retention age and lease liveness are deliberately independent
    // concerns, and this test only means to exercise the former).
    sqlx::query(
        "UPDATE execution_events SET created_at = ? WHERE attempt_id = 'attempt-retention-events' AND sequence IN (1,2,3)",
    )
    .bind((clock.now() - Duration::days(40)).to_rfc3339())
    .execute(repo.pool())
    .await
    .unwrap();

    // Cutoff: 30 days before "now", so the first batch (backdated to 40 days
    // ago) is expired and the second (created just now) is not.
    let cutoff = clock.now() - Duration::days(30);

    // Bounded batch of 2: first pass purges 2 of the 3 expired rows.
    let first_pass = repo
        .purge_execution_events_older_than(cutoff, 2)
        .await
        .unwrap();
    assert_eq!(first_pass, 2);
    let second_pass = repo
        .purge_execution_events_older_than(cutoff, 2)
        .await
        .unwrap();
    assert_eq!(second_pass, 1);
    let third_pass = repo
        .purge_execution_events_older_than(cutoff, 2)
        .await
        .unwrap();
    assert_eq!(third_pass, 0, "caught up: nothing left older than cutoff");

    let remaining: Vec<String> = sqlx::query_scalar(
        "SELECT event_id FROM execution_events WHERE attempt_id='attempt-retention-events' ORDER BY sequence",
    )
    .fetch_all(repo.pool())
    .await
    .unwrap();
    assert_eq!(remaining, vec!["evt-fresh-1".to_string()]);
}

#[tokio::test]
async fn artifact_retention_lists_expired_rows_and_deletes_exactly_those_ids() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_running_attempt(
        &repo,
        &item_id,
        &clock,
        "request-retention-artifacts",
        "attempt-retention-artifacts",
    )
    .await;
    repo.record_execution_artifact(
        "runner-f2",
        "attempt-retention-artifacts",
        fence,
        NewArtifact {
            id: "art-row-old",
            artifact_id: "art-old",
            kind: "patch",
            name: "old.patch",
            media_type: Some("text/x-diff"),
            size_bytes: 4,
            sha256: &"2".repeat(64),
            content_disposition: Some("inline_upload"),
            content_reference: Some("attempt-retention-artifacts-hex/old.blob"),
            metadata: "{}",
        },
        &clock,
    )
    .await
    .unwrap();

    repo.record_execution_artifact(
        "runner-f2",
        "attempt-retention-artifacts",
        fence,
        NewArtifact {
            id: "art-row-fresh",
            artifact_id: "art-fresh",
            kind: "patch",
            name: "fresh.patch",
            media_type: Some("text/x-diff"),
            size_bytes: 4,
            sha256: &"3".repeat(64),
            content_disposition: Some("inline_upload"),
            content_reference: Some("attempt-retention-artifacts-hex/fresh.blob"),
            metadata: "{}",
        },
        &clock,
    )
    .await
    .unwrap();

    // Backdate only the first artifact's row (see the sibling event-retention
    // test's own comment on why this is a raw `UPDATE` rather than advancing
    // the shared clock, which would also expire the still-needed lease).
    sqlx::query(
        "UPDATE execution_artifacts SET created_at = ? WHERE attempt_id = 'attempt-retention-artifacts' AND artifact_id = 'art-old'",
    )
    .bind((clock.now() - Duration::days(2)).to_rfc3339())
    .execute(repo.pool())
    .await
    .unwrap();
    let cutoff = clock.now() - Duration::days(1);

    let expired = repo
        .list_execution_artifacts_older_than(cutoff, 100)
        .await
        .unwrap();
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].artifact_id, "art-old");

    let ids: Vec<String> = expired.iter().map(|row| row.id.clone()).collect();
    let deleted = repo
        .delete_execution_artifacts_by_row_ids(&ids)
        .await
        .unwrap();
    assert_eq!(deleted, 1);

    let remaining: Vec<String> = sqlx::query_scalar(
        "SELECT artifact_id FROM execution_artifacts WHERE attempt_id='attempt-retention-artifacts' ORDER BY artifact_id",
    )
    .fetch_all(repo.pool())
    .await
    .unwrap();
    assert_eq!(remaining, vec!["art-fresh".to_string()]);
}

#[tokio::test]
async fn delete_execution_artifacts_by_row_ids_is_a_no_op_on_an_empty_list() {
    let repo = common::setup_test_db().await;
    let deleted = repo
        .delete_execution_artifacts_by_row_ids(&[])
        .await
        .unwrap();
    assert_eq!(deleted, 0);
}
