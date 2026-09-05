//! Proves the artifact/event retention sweep and the overdue-decision
//! expiry sweep are wired into the real production `ExecutionRuntime`:
//! `sweep_artifacts`/`sweep_events` (`handlers/runner_protocol/retention.rs`)
//! and `expire_overdue_decisions` (`handlers/decisions.rs`) are exercised
//! through the runtime's own start/stop lifecycle, not called directly.
//!
//! Every test here drives the real `ExecutionRuntime::start`/`stop`
//! lifecycle (`src/execution_runtime.rs`) — never a hand-rolled loop calling
//! the sweep functions directly. That distinction is the whole point: a
//! unit test can prove the sweep *functions* work in isolation; only this
//! file proves the running server actually calls them. Reachable normally
//! via `tack_api::execution_runtime` and `tack_api::handlers::*` (both `pub
//! mod`, fully integrated production code — unlike `decisions.rs`/
//! `artifact_events.rs` (modules of the `runner_protocol` binary), this
//! file needs no `#[path]` loading).

use std::time::Duration as StdDuration;

use chrono::{Duration, Utc};
use tack_api::execution_runtime::{ExecutionRuntime, ExecutionRuntimeConfig};
use tack_api::handlers::runner_protocol::artifact_storage::ArtifactStorage;
use tack_core::models::{CreateItem, CreateProject, ProjectType};
use tack_db::{
    Repository, init_pool, migrations,
    repo::execution::{
        NewAgentProfile, NewArtifact, NewDecision, NewExecutionRequest, NewRunner,
        RequestSelection, SystemExecutionClock,
    },
};
use uuid::Uuid;

const RUNNER_ID: &str = "runner-f6d";
const PROFILE_ID: &str = "profile-f6d";

async fn setup() -> (Repository, String) {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_all(&pool).await.expect("migrations");
    let repo = Repository::new(pool);
    let workspace = Uuid::new_v4();
    sqlx::query("INSERT INTO workspaces (id,name,default_vocabulary) VALUES (?, 'F6D', '{}')")
        .bind(workspace.to_string())
        .execute(repo.pool())
        .await
        .expect("workspace");
    let project = repo
        .create_project(
            workspace,
            CreateProject {
                name: "F6D".into(),
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
    repo.register_runner(
        NewRunner {
            id: RUNNER_ID,
            name: "F6D Runner",
            credential_hash: "hash-only",
            labels: "{}",
            total_capacity: 2,
            available_capacity: 2,
            capability_snapshot: "{}",
            protocol_version: 1,
        },
        &SystemExecutionClock,
    )
    .await
    .expect("runner");
    repo.create_agent_profile(
        NewAgentProfile {
            id: PROFILE_ID,
            name: "F6D Profile",
            instructions: "work",
            tool_policy: r#"{"mode":"safe"}"#,
            limits: "{}",
        },
        &SystemExecutionClock,
    )
    .await
    .expect("profile");
    (repo, item.id.to_string())
}

/// A single fixed instant, used both as the snapshot's own declared
/// `created_at` field below and as the clock `enqueue_execution` is called
/// with in `running_attempt` — `enqueue_execution` rejects a snapshot whose
/// `created_at` does not match the clock's `now()` *exactly*
/// (`snapshot_created_at_matches_now`), so this must not be two separate
/// `Utc::now()` reads (those would differ by however many microseconds pass
/// between them and fail that check). Mirrors `handlers/executions.rs`'s own
/// `FixedExecutionClock` precedent for the identical reason.
struct FixedExecutionClock(chrono::DateTime<Utc>);

impl tack_db::repo::execution::ExecutionClock for FixedExecutionClock {
    fn now(&self) -> chrono::DateTime<Utc> {
        self.0
    }
}

fn new_request<'a>(
    id: &'a str,
    item_id: &'a str,
    key: &'a str,
    now: chrono::DateTime<Utc>,
) -> NewExecutionRequest<'a> {
    let request_snapshot: &'static str = Box::leak(
        format!(
            r#"{{"request_id":"{id}","item_id":"{item_id}","idempotency_key":"{key}","created_by":{{"source":"test","subject_id":"f6d-test"}},"created_at":"{now}","selector":{{"kind":"exact_runner","runner_id":"{RUNNER_ID}"}},"agent_profile_id":"{PROFILE_ID}","resolved_agent_profile":{{"name":"P","instructions":"work","tool_policy":{{"mode":"safe"}},"timeout_seconds":60,"budgets":{{}}}},"requested_harness_kind":"codex","requested_model_provider":null,"requested_model_id":null,"repository":{{"kind":"git","remote":"https://example.test/f6d.git","base_revision":"abc123","subdirectory":null}},"permission_policy":{{"tools":[],"network":false}},"timeout_seconds":60,"budgets":{{}},"status_map_policy_id":null,"environment":{{}},"metadata":{{}}}}"#,
            now = now.to_rfc3339(),
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
        selector_id: RUNNER_ID,
        agent_profile_id: Some(PROFILE_ID),
        agent_profile_snapshot: r#"{"name":"P","instructions":"work","tool_policy":{"mode":"safe"},"timeout_seconds":60,"budgets":{}}"#,
        requested_harness_kind: Some("codex"),
        requested_model_provider: None,
        requested_model_id: None,
        repository_snapshot: r#"{"kind":"git","remote":"https://example.test/f6d.git","base_revision":"abc123","subdirectory":null}"#,
        permission_policy: r#"{"tools":[],"network":false}"#,
        timeout_seconds: Some(60),
        budgets: "{}",
        status_map_policy_id: None,
        environment: "{}",
        metadata: "{}",
        request_snapshot,
    }
}

/// Claims a fresh attempt against a real, long-lived lease (10 minutes —
/// comfortably longer than any sweep-wait loop below) and bumps it straight
/// to `running`, mirroring `decisions.rs::claim_running_attempt`'s own
/// shortcut (`decisions.rs` is a module of the `runner_protocol` binary;
/// the resolve/expiry paths under test here don't gate on how an
/// attempt got to `running`, only on its current state/lease).
async fn running_attempt(repo: &Repository, item_id: &str, tag: &str) -> (String, i64) {
    let request_id = format!("req-{tag}");
    let now = Utc::now();
    repo.enqueue_execution(
        new_request(&request_id, item_id, &request_id, now),
        &FixedExecutionClock(now),
    )
    .await
    .expect("enqueue");
    let attempt_id = format!("att-{tag}");
    let claim = repo
        .claim_execution_idempotent_with_snapshot(
            RUNNER_ID,
            &attempt_id,
            &attempt_id,
            Duration::minutes(10),
            &SystemExecutionClock,
            RequestSelection::Naive,
        )
        .await
        .expect("claim")
        .expect("work available");
    sqlx::query("UPDATE execution_attempts SET state='running' WHERE id=?")
        .bind(&attempt_id)
        .execute(repo.pool())
        .await
        .expect("bump to running");
    (claim.lease.attempt_id, claim.lease.fencing_token)
}

/// Artifact storage root for one sweep-wiring test.
///
/// The `TempDir` removes the directory and everything under it when it drops,
/// so a failing assertion leaves nothing behind either.
fn temp_storage_root(label: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(label)
        .tempdir()
        .expect("temporary directory")
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

/// Writes a real blob via the same `ArtifactStorage::store_streaming` path
/// production uses, and records a matching `execution_artifacts` manifest
/// row pointing at it — so "the blob is gone from disk" is a meaningful,
/// non-vacuous assertion later (a bare string `content_reference` with no
/// real file behind it would make `remove_blob`'s `remove_file` a silent
/// no-op either way).
#[allow(clippy::too_many_arguments)]
async fn seed_real_artifact(
    repo: &Repository,
    storage: &ArtifactStorage,
    artifact_root: &std::path::Path,
    attempt_id: &str,
    fence: i64,
    row_id: &str,
    artifact_id: &str,
    content: &[u8],
    backdate_days: Option<i64>,
) -> std::path::PathBuf {
    let digest = sha256_hex(content);
    let stored = storage
        .store_streaming(
            attempt_id,
            artifact_id,
            content.len() as u64,
            &digest,
            futures::stream::iter(std::iter::once(Ok::<_, std::io::Error>(
                axum::body::Bytes::copy_from_slice(content),
            ))),
        )
        .await
        .expect("store real blob");

    repo.record_execution_artifact(
        RUNNER_ID,
        attempt_id,
        fence,
        NewArtifact {
            id: row_id,
            artifact_id,
            kind: "patch",
            name: "content.patch",
            media_type: Some("text/plain"),
            size_bytes: content.len() as i64,
            sha256: &digest,
            content_disposition: Some("inline_upload"),
            content_reference: Some(&stored.content_reference),
            metadata: "{}",
        },
        &SystemExecutionClock,
    )
    .await
    .expect("record artifact");

    if let Some(days) = backdate_days {
        sqlx::query("UPDATE execution_artifacts SET created_at = ? WHERE id = ?")
            .bind((Utc::now() - Duration::days(days)).to_rfc3339())
            .bind(row_id)
            .execute(repo.pool())
            .await
            .expect("backdate artifact");
    }

    artifact_root.join(&stored.content_reference)
}

async fn artifact_row_exists(repo: &Repository, row_id: &str) -> bool {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM execution_artifacts WHERE id=?")
        .bind(row_id)
        .fetch_one(repo.pool())
        .await
        .unwrap();
    count > 0
}

/// Bounded, deterministic poll for an async condition to become true — never
/// a blind sleep standing in for the actual assertion. Mirrors
/// `tack-orch/src/execution_retention.rs`'s own test helper of the same
/// shape/rationale, adapted to an async condition since these tests check
/// real DB/filesystem state.
async fn wait_for<F, Fut>(mut condition: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    for _ in 0..100 {
        if condition().await {
            return true;
        }
        tokio::time::sleep(StdDuration::from_millis(50)).await;
    }
    false
}

// ---------------------------------------------------------------------
// Gap 1: sweep_artifacts/sweep_events via the real ExecutionRuntime.
// ---------------------------------------------------------------------

#[tokio::test]
async fn retention_enabled_purges_expired_artifact_row_and_blob_but_spares_a_fresh_one() {
    let (repo, item_id) = setup().await;
    let (attempt_id, fence) = running_attempt(&repo, &item_id, "artifacts-enabled").await;
    let storage_root_dir = temp_storage_root("enabled");
    let storage_root = storage_root_dir.path();
    let storage_dir = storage_root.to_string_lossy().into_owned();
    let artifact_root = std::path::PathBuf::from(format!("{storage_dir}/execution-artifacts"));
    let artifact_storage = ArtifactStorage::new(artifact_root.clone());

    let old_blob_path = seed_real_artifact(
        &repo,
        &artifact_storage,
        &artifact_root,
        &attempt_id,
        fence,
        "art-row-old",
        "art-old",
        b"old artifact content that must be purged",
        Some(5), // 5 days old
    )
    .await;
    let fresh_blob_path = seed_real_artifact(
        &repo,
        &artifact_storage,
        &artifact_root,
        &attempt_id,
        fence,
        "art-row-fresh",
        "art-fresh",
        b"fresh artifact content that must survive",
        None, // created just now
    )
    .await;

    assert!(
        tokio::fs::try_exists(&old_blob_path).await.unwrap(),
        "precondition: old blob must actually exist on disk before the sweep runs"
    );
    assert!(
        tokio::fs::try_exists(&fresh_blob_path).await.unwrap(),
        "precondition: fresh blob must actually exist on disk before the sweep runs"
    );

    let runtime = ExecutionRuntime::new();
    runtime
        .start(
            repo.clone(),
            ExecutionRuntimeConfig {
                retention_enable: true,
                retention_days: 1,
                retention_interval_secs: 1,
                health_enable: false,
                health_interval_secs: 3600,
                storage_dir: storage_dir.clone(),
            },
        )
        .await;

    let purged = wait_for(|| async { !artifact_row_exists(&repo, "art-row-old").await }).await;
    assert!(
        purged,
        "the old artifact's row must be purged by the real production sweep"
    );
    assert!(
        !tokio::fs::try_exists(&old_blob_path).await.unwrap(),
        "the old artifact's on-disk blob must be removed too, not just its DB row"
    );

    assert!(
        artifact_row_exists(&repo, "art-row-fresh").await,
        "a fresh, under-age artifact's row must never be swept"
    );
    assert!(
        tokio::fs::try_exists(&fresh_blob_path).await.unwrap(),
        "a fresh, under-age artifact's blob must never be removed"
    );

    runtime.stop().await;
    let _ = tokio::fs::remove_dir_all(&storage_root).await;
}

#[tokio::test]
async fn retention_disabled_by_default_leaves_the_same_expired_artifact_row_and_blob_untouched() {
    let (repo, item_id) = setup().await;
    let (attempt_id, fence) = running_attempt(&repo, &item_id, "artifacts-disabled").await;
    let storage_root_dir = temp_storage_root("disabled");
    let storage_root = storage_root_dir.path();
    let storage_dir = storage_root.to_string_lossy().into_owned();
    let artifact_root = std::path::PathBuf::from(format!("{storage_dir}/execution-artifacts"));
    let artifact_storage = ArtifactStorage::new(artifact_root.clone());

    let old_blob_path = seed_real_artifact(
        &repo,
        &artifact_storage,
        &artifact_root,
        &attempt_id,
        fence,
        "art-row-old-disabled",
        "art-old-disabled",
        b"content that would be purged if retention were enabled",
        Some(5),
    )
    .await;
    assert!(tokio::fs::try_exists(&old_blob_path).await.unwrap());

    let runtime = ExecutionRuntime::new();
    runtime
        .start(
            repo.clone(),
            ExecutionRuntimeConfig {
                retention_enable: false, // the production default
                retention_days: 1,
                retention_interval_secs: 1,
                health_enable: false,
                health_interval_secs: 3600,
                storage_dir: storage_dir.clone(),
            },
        )
        .await;

    // No task is spawned at all when disabled (mirrors
    // `execution_retention.rs`'s own `disabled_sweep_spawns_nothing...`
    // proof) — wait out several would-be sweep intervals and confirm
    // nothing changed, rather than asserting immediately after `start()`
    // returns.
    tokio::time::sleep(StdDuration::from_millis(500)).await;
    runtime.stop().await;

    assert!(
        artifact_row_exists(&repo, "art-row-old-disabled").await,
        "the row must survive when retention is disabled (the default)"
    );
    assert!(
        tokio::fs::try_exists(&old_blob_path).await.unwrap(),
        "the blob must survive when retention is disabled (the default)"
    );

    let _ = tokio::fs::remove_dir_all(&storage_root).await;
}

/// The immutability guard (`set_execution_artifact_content_reference`'s own
/// `WHERE content_reference IS NULL`) means a manifested-but-never-uploaded
/// artifact is exactly the shape the race guard below cares about. This test
/// does not attempt to win that race against the real background sweep
/// (inherently timing-dependent — the deterministic proof lives in
/// `crates/tack-db/tests/repository/event_artifact_retention.rs`); it only
/// confirms the production path purges a manifest-only row (no upload ever
/// happened) via the guarded delete, end to end.
#[tokio::test]
async fn retention_enabled_purges_an_old_manifest_row_with_no_upload_ever_completed() {
    let (repo, item_id) = setup().await;
    let (attempt_id, fence) = running_attempt(&repo, &item_id, "manifest-only").await;
    let storage_root_dir = temp_storage_root("manifest-only");
    let storage_root = storage_root_dir.path();
    let storage_dir = storage_root.to_string_lossy().into_owned();

    repo.record_execution_artifact(
        RUNNER_ID,
        &attempt_id,
        fence,
        NewArtifact {
            id: "art-row-manifest-only",
            artifact_id: "art-manifest-only",
            kind: "patch",
            name: "never-uploaded.patch",
            media_type: Some("text/plain"),
            size_bytes: 4,
            sha256: &"7".repeat(64),
            content_disposition: Some("inline_upload"),
            content_reference: None,
            metadata: "{}",
        },
        &SystemExecutionClock,
    )
    .await
    .expect("record manifest-only artifact");
    sqlx::query("UPDATE execution_artifacts SET created_at = ? WHERE id = 'art-row-manifest-only'")
        .bind((Utc::now() - Duration::days(5)).to_rfc3339())
        .execute(repo.pool())
        .await
        .unwrap();

    let runtime = ExecutionRuntime::new();
    runtime
        .start(
            repo.clone(),
            ExecutionRuntimeConfig {
                retention_enable: true,
                retention_days: 1,
                retention_interval_secs: 1,
                health_enable: false,
                health_interval_secs: 3600,
                storage_dir: storage_dir.clone(),
            },
        )
        .await;

    let purged =
        wait_for(|| async { !artifact_row_exists(&repo, "art-row-manifest-only").await }).await;
    assert!(
        purged,
        "a manifest-only row (content_reference: None) must still be purged when genuinely stale"
    );

    runtime.stop().await;
    let _ = tokio::fs::remove_dir_all(&storage_root).await;
}

// ---------------------------------------------------------------------
// Gap 2: expire_overdue_decisions via the real ExecutionRuntime.
// ---------------------------------------------------------------------

#[tokio::test]
async fn overdue_decision_expires_via_the_periodic_sweep_while_a_future_one_stays_pending() {
    let (repo, item_id) = setup().await;
    let (attempt_id, fence) = running_attempt(&repo, &item_id, "decisions").await;

    let overdue_row = format!("row-overdue-{}", Uuid::new_v4());
    let written = repo
        .create_execution_decision(
            RUNNER_ID,
            &attempt_id,
            fence,
            NewDecision {
                id: &overdue_row,
                decision_id: "dec-overdue",
                kind: "tool_permission",
                prompt: "Allow the harness to run a command?",
                options: "[]",
                metadata: "{}",
                expires_at: Some(Utc::now() - Duration::seconds(5)),
            },
            &SystemExecutionClock,
        )
        .await
        .expect("create overdue decision");
    assert!(written);

    let future_row = format!("row-future-{}", Uuid::new_v4());
    let written = repo
        .create_execution_decision(
            RUNNER_ID,
            &attempt_id,
            fence,
            NewDecision {
                id: &future_row,
                decision_id: "dec-future",
                kind: "tool_permission",
                prompt: "Allow the harness to run a command?",
                options: "[]",
                metadata: "{}",
                expires_at: Some(Utc::now() + Duration::minutes(30)),
            },
            &SystemExecutionClock,
        )
        .await
        .expect("create future decision");
    assert!(written);

    let storage_root_dir = temp_storage_root("decisions");
    let storage_root = storage_root_dir.path();
    let runtime = ExecutionRuntime::new();
    runtime
        .start(
            repo.clone(),
            ExecutionRuntimeConfig {
                retention_enable: true,
                retention_days: 90,
                retention_interval_secs: 1,
                health_enable: false,
                health_interval_secs: 3600,
                storage_dir: storage_root.to_string_lossy().into_owned(),
            },
        )
        .await;

    let expired = wait_for(|| async {
        let state: String = sqlx::query_scalar(
            "SELECT state FROM execution_decisions WHERE decision_id='dec-overdue'",
        )
        .fetch_one(repo.pool())
        .await
        .unwrap();
        state == "expired"
    })
    .await;
    assert!(
        expired,
        "an overdue, unresolved decision must transition to expired via the periodic sweep"
    );

    let future_state: String =
        sqlx::query_scalar("SELECT state FROM execution_decisions WHERE decision_id='dec-future'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(
        future_state, "pending",
        "a not-yet-overdue decision must remain pending"
    );

    runtime.stop().await;
    let _ = tokio::fs::remove_dir_all(&storage_root).await;
}

/// Retention disabled (the default) must not silently also disable decision
/// expiry's periodic caller — the two currently share a gate. This confirms
/// the *current* wiring's actual
/// behavior (both riding `retention_enable`), so a future change to that
/// design shows up here as a deliberate, reviewed test change rather than a
/// silent behavior drift.
#[tokio::test]
async fn overdue_decision_stays_pending_while_retention_is_disabled() {
    let (repo, item_id) = setup().await;
    let (attempt_id, fence) = running_attempt(&repo, &item_id, "decisions-disabled").await;

    let overdue_row = format!("row-overdue-disabled-{}", Uuid::new_v4());
    let written = repo
        .create_execution_decision(
            RUNNER_ID,
            &attempt_id,
            fence,
            NewDecision {
                id: &overdue_row,
                decision_id: "dec-overdue-disabled",
                kind: "tool_permission",
                prompt: "Allow the harness to run a command?",
                options: "[]",
                metadata: "{}",
                expires_at: Some(Utc::now() - Duration::seconds(5)),
            },
            &SystemExecutionClock,
        )
        .await
        .expect("create overdue decision");
    assert!(written);

    let storage_root_dir = temp_storage_root("decisions-disabled");
    let storage_root = storage_root_dir.path();
    let runtime = ExecutionRuntime::new();
    runtime
        .start(
            repo.clone(),
            ExecutionRuntimeConfig {
                retention_enable: false,
                retention_days: 90,
                retention_interval_secs: 1,
                health_enable: false,
                health_interval_secs: 3600,
                storage_dir: storage_root.to_string_lossy().into_owned(),
            },
        )
        .await;

    tokio::time::sleep(StdDuration::from_millis(500)).await;
    runtime.stop().await;

    let state: String = sqlx::query_scalar(
        "SELECT state FROM execution_decisions WHERE decision_id='dec-overdue-disabled'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(
        state, "pending",
        "with the periodic sweep disabled, an overdue decision is left pending \
         (still safely fail-closed against resolution — resolve_decision_row's \
         own lazy expiry check still rejects it — just not yet observably \
         'expired' in bookkeeping/dashboards)"
    );

    let _ = tokio::fs::remove_dir_all(&storage_root).await;
}
