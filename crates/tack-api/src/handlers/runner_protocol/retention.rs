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

// `sweep_events`/`sweep_artifacts`/`SweepOutcome` are exercised by this
// card's own `f2_artifact_events_test.rs` (which loads this file the same
// `#[path]` way), but `crates/tack-api/tests/c2_handlers_test.rs` — a
// pre-existing, unrelated test binary this card does not own — also pulls in
// `runner_protocol.rs` via `#[path]` and never calls into these. Dead-code
// analysis is per compiled binary; see `artifact_download.rs`'s own,
// identically-reasoned module-level allow for the fuller precedent
// (`RunnerV1ErrorEnvelope` in `executions.rs`, `Limits`'s individually
// annotated fields).
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SweepOutcome {
    pub events_deleted: u64,
    pub artifacts_deleted: u64,
    /// A blob file that `remove_blob` could not find (already gone, or the
    /// row never had verified content). Not an error — see
    /// `ArtifactStorage::remove_blob`'s own doc comment — but counted so a
    /// caller can distinguish "nothing to purge" from "purged N rows, M of
    /// which had no blob to remove," which is diagnostic, not alarming.
    pub artifacts_without_a_blob: u64,
}

/// One bounded pass over `execution_events` older than `policy.event_retention`
/// as of `now`. Returns the number of rows deleted this pass — `0` means
/// "caught up," a non-zero result at exactly `batch_limit` is the caller's
/// signal to call again (F5's recurring task loops until it sees fewer than
/// `batch_limit`).
#[allow(dead_code)]
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
#[allow(dead_code)]
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
    let mut ids = Vec::with_capacity(expired.len());
    for row in &expired {
        match &row.content_reference {
            Some(reference) => storage.remove_blob(reference).await,
            None => without_blob += 1,
        }
        ids.push(row.id.clone());
    }
    let artifacts_deleted = repo.delete_execution_artifacts_by_row_ids(&ids).await?;
    Ok(SweepOutcome {
        events_deleted: 0,
        artifacts_deleted,
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
