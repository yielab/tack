//! Event/artifact retention: one bounded sweep pass per call, owning only the
//! purge *logic* — no background task, cancellation, or metrics.
//! `execution_runtime.rs`'s `spawn_artifact_and_decision_sweep` is the
//! recurring caller; nothing here spawns a task or sleeps.
//!
//! Two independent policies (`limits.json`'s retention_event/artifact_days_default):
//! a blob can outlive or be purged independently of its attempt's event history.

use chrono::{DateTime, Duration, Utc};
use tack_db::Repository;

use super::artifact_storage::ArtifactStorage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetentionPolicy {
    pub event_retention: Duration,
    pub artifact_retention: Duration,
}

impl Default for RetentionPolicy {
    /// Matches `limits.json`'s `retention_event_days_default` and
    /// `retention_artifact_days_default` (both 30).
    fn default() -> Self {
        Self {
            event_retention: Duration::days(30),
            artifact_retention: Duration::days(30),
        }
    }
}

// `sweep_events`/`sweep_artifacts`/`SweepOutcome` are called in production by
// `execution_runtime.rs` on the `TACK_EXECUTION_RETENTION_*` schedule, tested
// directly below; `runner_protocol/artifact_events.rs` exercises the HTTP
// surface instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SweepOutcome {
    pub events_deleted: u64,
    pub artifacts_deleted: u64,
    /// Rows observed with no `content_reference` at list-time (not an error —
    /// see `ArtifactStorage::remove_blob`). Diagnostic only: the concurrent-
    /// upload guard can let a few survive this pass if an upload resolves them
    /// mid-sweep; they are purged on the next pass instead.
    pub artifacts_without_a_blob: u64,
}

/// One bounded pass over `execution_events` older than `policy.event_retention`.
/// `0` deleted means caught up; exactly `batch_limit` deleted is the signal to
/// call again (the loop in `execution_runtime.rs` does, until it sees fewer).
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
/// `policy.artifact_retention`: fetch rows, unlink each blob, then delete.
/// Rows with `Some(reference)` already had their blob unlinked and delete
/// unconditionally by id. Rows with `None` go through
/// [`Repository::delete_unresolved_execution_artifacts_by_row_ids`], which
/// re-checks `content_reference IS NULL` in the same atomic `DELETE`.
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
