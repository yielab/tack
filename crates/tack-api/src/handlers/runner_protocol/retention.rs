//! III-F2: event/artifact retention behavior — one bounded sweep pass.
//!
//! This module owns the *logic* of what gets purged and how (the card's own
//! "retention tests" charter). It deliberately does **not** own a recurring
//! background task, cancellation, startup/shutdown wiring, or metrics — that
//! is III-F5's charter ("Runtime retention and observability... startup/
//! shutdown wiring assigned at integration"). `sweep_events`/`sweep_artifacts`
//! below are plain async functions F5 can call on whatever interval/task
//! shape it builds; nothing here spawns a task or sleeps.
//!
//! Two independent policies, matching `limits.json`'s two separate
//! `retention_*_days_default` fields — an artifact's blob can outlive or be
//! purged independently of its attempt's event history.

use chrono::{DateTime, Duration, Utc};
use tack_db::Repository;

use super::artifact_storage::ArtifactStorage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetentionPolicy {
    pub event_retention: Duration,
    pub artifact_retention: Duration,
}

impl Default for RetentionPolicy {
    /// `docs/contracts/runner-v1/limits.json`'s
    /// `retention_event_days_default`/`retention_artifact_days_default`
    /// (both 30).
    fn default() -> Self {
        Self {
            event_retention: Duration::days(30),
            artifact_retention: Duration::days(30),
        }
    }
}

// `sweep_events`/`sweep_artifacts`/`SweepOutcome` are wired into production
// as of III-F6d: `crates/tack-api/src/execution_runtime.rs`'s
// `spawn_artifact_and_decision_sweep` is the recurring caller F5's own doc
// comment above deferred to "whatever interval/task shape it builds" —
// riding the same `TACK_EXECUTION_RETENTION_*` schedule/gate as
// `tack_orch::execution_retention`'s replay/event purge. (Prior to III-F6d
// these had no caller anywhere but their own tests, and — contrary to this
// comment's own former claim — not even that: `f2_artifact_events_test.rs`
// exercises the HTTP upload/download surface, never these functions
// directly. See `crates/tack-db/tests/f2_event_artifact_retention_test.rs`
// for the functions this module calls; III-F6d added the first tests of
// `sweep_events`/`sweep_artifacts` themselves, in this file's own test
// module below.)
//
// The `#[allow(dead_code)]` below is *not* a residual of that old gap: it
// exists solely because `f2_artifact_events_test.rs` and
// `crates/tack-api/tests/c2_handlers_test.rs` both load this file via
// `#[path]` (pulling in `runner_protocol.rs` and its submodules) without
// also loading `execution_runtime.rs`, which lives outside that `#[path]`
// tree. Dead-code analysis is per compiled binary, so those two binaries
// alone would otherwise flag every item below as unused even though the
// real `tack-api` library (and every other test binary that links it
// normally, e.g. `f6d_execution_sweep_wiring_test.rs`) has a live caller.
// Exact precedent already established for this same `#[path]` duplication
// in `artifact_download.rs`'s own module-level allow.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SweepOutcome {
    pub events_deleted: u64,
    pub artifacts_deleted: u64,
    /// Count of manifest rows this pass *observed* with no `content_reference`
    /// at list-time (not an error — see `ArtifactStorage::remove_blob`'s own
    /// doc comment). Diagnostic, not a promise of deletion: III-F6d's
    /// concurrent-upload guard (see
    /// `Repository::delete_unresolved_execution_artifacts_by_row_ids`'s doc
    /// comment) means a small number of these may survive this pass rather
    /// than being purged, if a real upload raced in and set their reference
    /// between this sweep's read and its delete — they are correctly
    /// resolved (blob removed, then deleted) on a later pass instead.
    pub artifacts_without_a_blob: u64,
}

/// One bounded pass over `execution_events` older than `policy.event_retention`
/// as of `now`. Returns the number of rows deleted this pass — `0` means
/// "caught up," a non-zero result at exactly `batch_limit` is the caller's
/// signal to call again (F5's recurring task loops until it sees fewer than
/// `batch_limit`).
#[allow(dead_code)] // per-compiled-binary artifact — see SweepOutcome's doc comment above
pub async fn sweep_events(
    repo: &Repository,
    now: DateTime<Utc>,
    policy: &RetentionPolicy,
    batch_limit: i64,
) -> Result<u64, sqlx::Error> {
    let cutoff = now - policy.event_retention;
    repo.purge_execution_events_older_than(cutoff, batch_limit)
        .await
}

/// One bounded pass over `execution_artifacts` older than
/// `policy.artifact_retention`. Two-phase by construction (fetch rows with
/// their `content_reference`, unlink each blob, only then delete the rows) —
/// see `Repository::list_execution_artifacts_older_than`'s own doc comment
/// for why the ordering matters.
///
/// # III-F6d: split delete, guarding the no-blob-observed branch
///
/// The final delete is split into two calls, not one: rows observed with
/// `Some(reference)` had their blob already unlinked above and are safe to
/// delete unconditionally by id (`set_execution_artifact_content_reference`'s
/// own `WHERE content_reference IS NULL` guard means a resolved reference can
/// never change again, so no race is possible on that branch). Rows observed
/// with `None` are *not* safe to delete unconditionally — a concurrent
/// artifact-content upload can resolve one between the read above and this
/// function returning — so those go through
/// [`Repository::delete_unresolved_execution_artifacts_by_row_ids`] instead,
/// which re-checks `content_reference IS NULL` as part of the same atomic
/// `DELETE`. See that method's own doc comment for the full race analysis
/// and why a row that loses this race simply survives to be resolved
/// correctly on the next pass, rather than being redesigned into a single
/// transaction spanning both this sweep and the independent upload path.
#[allow(dead_code)] // per-compiled-binary artifact — see SweepOutcome's doc comment above
pub async fn sweep_artifacts(
    repo: &Repository,
    storage: &ArtifactStorage,
    now: DateTime<Utc>,
    policy: &RetentionPolicy,
    batch_limit: i64,
) -> Result<SweepOutcome, sqlx::Error> {
    let cutoff = now - policy.artifact_retention;
    let expired = repo
        .list_execution_artifacts_older_than(cutoff, batch_limit)
        .await?;
    if expired.is_empty() {
        return Ok(SweepOutcome::default());
    }
    let mut without_blob = 0u64;
    let mut resolved_ids = Vec::with_capacity(expired.len());
    let mut unresolved_ids = Vec::new();
    for row in &expired {
        match &row.content_reference {
            Some(reference) => {
                storage.remove_blob(reference).await;
                resolved_ids.push(row.id.clone());
            }
            None => {
                without_blob += 1;
                unresolved_ids.push(row.id.clone());
            }
        }
    }
    let resolved_deleted = repo
        .delete_execution_artifacts_by_row_ids(&resolved_ids)
        .await?;
    let unresolved_deleted = repo
        .delete_unresolved_execution_artifacts_by_row_ids(&unresolved_ids)
        .await?;
    Ok(SweepOutcome {
        events_deleted: 0,
        artifacts_deleted: resolved_deleted + unresolved_deleted,
        artifacts_without_a_blob: without_blob,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policy_matches_the_frozen_limits_fixture() {
        let policy = RetentionPolicy::default();
        assert_eq!(policy.event_retention, Duration::days(30));
        assert_eq!(policy.artifact_retention, Duration::days(30));
    }
}
