//! Artifact retention: one bounded sweep pass per call, owning only the purge *logic* — no
//! background task, cancellation, or metrics. `execution_runtime.rs` is the recurring caller;
//! nothing here spawns a task or sleeps.
//!
//! The policy is `limits.json`'s `retention_artifact_days_default`: a blob can be purged
//! independently of its attempt's event history, which `tack_app::retention` owns.

use chrono::{DateTime, Duration, Utc};
use tack_db::{Repository, StoreError};

use super::artifact_storage::ArtifactStorage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetentionPolicy {
    pub artifact_retention: Duration,
}

impl Default for RetentionPolicy {
    /// Matches `limits.json`'s `retention_artifact_days_default` (30).
    fn default() -> Self {
        Self {
            artifact_retention: Duration::days(30),
        }
    }
}

// `sweep_artifacts`/`SweepOutcome` are called in production by
// `execution_runtime.rs` on the `TACK_EXECUTION_RETENTION_*` schedule;
// `runner_protocol/artifact_events.rs` exercises the HTTP surface instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SweepOutcome {
    pub artifacts_deleted: u64,
    /// Rows observed with no `content_reference` at list-time (not an error —
    /// see `ArtifactStorage::remove_blob`). Diagnostic only: the concurrent-
    /// upload guard can let a few survive this pass if an upload resolves them
    /// mid-sweep; they are purged on the next pass instead.
    pub artifacts_without_a_blob: u64,
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
) -> Result<SweepOutcome, StoreError> {
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
        assert_eq!(policy.artifact_retention, Duration::days(30));
    }
}
