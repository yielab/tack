//! Artifacts an attempt produced, their stored content, and the operator's review of a Merge-Readiness Pack.

use super::state;
use crate::StoreError;
use chrono::{DateTime, Utc};
use sqlx::{AssertSqlSafe, Row};
use tack_protocol::Clock;
use tracing::instrument;

use super::shared::stamp;
use crate::Repository;

#[derive(Debug, Clone)]
pub struct NewArtifact<'a> {
    pub id: &'a str,
    pub artifact_id: &'a str,
    pub kind: &'a str,
    pub name: &'a str,
    pub media_type: Option<&'a str>,
    pub size_bytes: i64,
    pub sha256: &'a str,
    pub content_disposition: Option<&'a str>,
    pub content_reference: Option<&'a str>,
    pub metadata: &'a str,
}

/// Every column `execution_artifacts` carries for one manifest row.
/// `content_reference` is `None` until [`Repository::set_execution_artifact_content_reference`]
/// commits a verified upload — its presence, not a separate status column
/// (none exists: that would just duplicate this field), is
/// the manifest-vs-content-verified distinction this crate exposes.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ExecutionArtifactRow {
    pub id: String,
    pub attempt_id: String,
    pub artifact_id: String,
    pub kind: String,
    pub name: String,
    pub media_type: Option<String>,
    pub size_bytes: i64,
    pub sha256: String,
    pub content_disposition: Option<String>,
    pub content_reference: Option<String>,
    pub metadata: String,
    pub created_at: String,
}

/// Outcome of [`Repository::set_execution_artifact_content_reference`].
/// `AlreadySet` is distinct from `Committed` (rule 7: no structural
/// stand-in) — a second attempt to record content for the same
/// `(attempt_id, artifact_id)` is reported honestly rather than silently
/// treated as success or as the same thing as a fencing failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactContentCommitResult {
    Committed,
    AlreadySet,
    Stale,
}

// ─── Runtime retention and observability ───────────────────────────────────

/// One `mrp_reviews` row: the human verdict on an attempt's Merge-Readiness
/// Pack. `verdict` is `None` until the pack has been reviewed; a row with only
/// `viewed_at` set records that the operator opened it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MrpReviewRow {
    pub attempt_id: String,
    pub artifact_id: String,
    pub verdict: Option<String>,
    pub reason: Option<String>,
    pub viewed_at: Option<String>,
    pub reviewed_at: Option<String>,
    pub reviewed_by: Option<String>,
}

const MRP_REVIEW_COLUMNS: &str =
    "attempt_id, artifact_id, verdict, reason, viewed_at, reviewed_at, reviewed_by";

fn mrp_review_row(row: &sqlx::sqlite::SqliteRow) -> MrpReviewRow {
    MrpReviewRow {
        attempt_id: row.get("attempt_id"),
        artifact_id: row.get("artifact_id"),
        verdict: row.get("verdict"),
        reason: row.get("reason"),
        viewed_at: row.get("viewed_at"),
        reviewed_at: row.get("reviewed_at"),
        reviewed_by: row.get("reviewed_by"),
    }
}

impl Repository {
    #[instrument(skip(self, artifact, clock))]
    pub async fn record_execution_artifact(
        &self,
        runner_id: &str,
        attempt_id: &str,
        fence: i64,
        artifact: NewArtifact<'_>,
        clock: &dyn Clock,
    ) -> Result<bool, StoreError> {
        let now = stamp(clock);
        // Serialize the eligibility check and the insert against a concurrent
        // completion, cancellation, or recovery transition for the same
        // attempt (all of which already open BEGIN IMMEDIATE). Without this,
        // the SELECT and INSERT were two separate un-transacted statements: a
        // concurrent terminal transition could commit between them, letting
        // an artifact land against an attempt that has already gone
        // terminal, lost, or needs_operator.
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let valid: bool = sqlx::query_scalar(state::sql("SELECT EXISTS(SELECT 1 FROM execution_attempts WHERE id = ? AND runner_id = ? AND fencing_token = ? AND state NOT IN ({terminal}) AND lease_expires_at > ?)"))
            .bind(attempt_id).bind(runner_id).bind(fence).bind(&now).fetch_one(&mut *tx).await?;
        if !valid {
            tx.commit().await?;
            return Ok(false);
        }
        sqlx::query("INSERT INTO execution_artifacts (id, attempt_id, artifact_id, kind, name, media_type, size_bytes, sha256, content_disposition, content_reference, metadata, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(attempt_id, artifact_id) DO NOTHING")
            .bind(artifact.id).bind(attempt_id).bind(artifact.artifact_id).bind(artifact.kind).bind(artifact.name).bind(artifact.media_type).bind(artifact.size_bytes).bind(artifact.sha256).bind(artifact.content_disposition).bind(artifact.content_reference).bind(artifact.metadata).bind(now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Looks up one manifest row by its natural key. `Ok(None)`
    /// means no such artifact was ever manifested for this attempt —
    /// distinct from a manifested-but-not-yet-content-verified row (which is
    /// `Some` with `content_reference: None`).
    #[instrument(skip(self))]
    pub async fn get_execution_artifact(
        &self,
        attempt_id: &str,
        artifact_id: &str,
    ) -> Result<Option<ExecutionArtifactRow>, StoreError> {
        let row = sqlx::query(
            "SELECT id, attempt_id, artifact_id, kind, name, media_type, size_bytes, sha256, \
             content_disposition, content_reference, metadata, created_at \
             FROM execution_artifacts WHERE attempt_id = ? AND artifact_id = ?",
        )
        .bind(attempt_id)
        .bind(artifact_id)
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(|row| ExecutionArtifactRow {
            id: row.get("id"),
            attempt_id: row.get("attempt_id"),
            artifact_id: row.get("artifact_id"),
            kind: row.get("kind"),
            name: row.get("name"),
            media_type: row.get("media_type"),
            size_bytes: row.get("size_bytes"),
            sha256: row.get("sha256"),
            content_disposition: row.get("content_disposition"),
            content_reference: row.get("content_reference"),
            metadata: row.get("metadata"),
            created_at: row.get("created_at"),
        }))
    }

    /// Resolves an artifact the same way
    /// [`Repository::list_events_for_attempt_number`] resolves events — by
    /// the operator-facing `(request_id, attempt_number)` pair rather than
    /// the internal opaque `attempt_id` — so `GET
    /// /api/executions/{request_id}/attempts/{attempt_number}/artifacts/{artifact_id}/content`
    /// never needs
    /// to expose `attempt_id` to an operator caller. `Ok(None)` collapses
    /// "no such attempt" and "no such artifact on that attempt" into one
    /// not-found outcome, matching every other operator lookup in this file.
    #[instrument(skip(self))]
    pub async fn get_execution_artifact_by_attempt_number(
        &self,
        request_id: &str,
        attempt_number: i64,
        artifact_id: &str,
    ) -> Result<Option<ExecutionArtifactRow>, StoreError> {
        let attempt_id: Option<String> = sqlx::query_scalar(
            "SELECT id FROM execution_attempts WHERE request_id = ? AND attempt_number = ?",
        )
        .bind(request_id)
        .bind(attempt_number)
        .fetch_optional(self.pool())
        .await?;
        let Some(attempt_id) = attempt_id else {
            return Ok(None);
        };
        self.get_execution_artifact(&attempt_id, artifact_id).await
    }

    /// Every artifact manifested for one specific attempt, as
    /// [`crate::queries::execution_artifacts::artifacts_of_attempt`] answers it.
    /// Kept under this name for the callers that already use it.
    #[instrument(skip(self))]
    pub async fn list_execution_artifacts_for_attempt_number(
        &self,
        request_id: &str,
        attempt_number: i64,
    ) -> Result<Option<Vec<ExecutionArtifactRow>>, crate::StoreError> {
        crate::queries::execution_artifacts::artifacts_of_attempt(self, request_id, attempt_number)
            .await
    }

    /// Commits a verified artifact's storage reference. Mirrors
    /// `record_execution_artifact`'s own `BEGIN IMMEDIATE` eligibility
    /// pattern (CLAUDE.md: read-then-write requires `BEGIN IMMEDIATE`) so a
    /// concurrent terminal transition cannot land a content reference
    /// against an attempt that has already gone terminal, lost, or
    /// needs_operator between the caller's own fencing check (at manifest
    /// time) and this call (after the — potentially long — upload finished
    /// streaming). `content_reference IS NULL` in the `UPDATE`'s `WHERE`
    /// makes a committed reference immutable: a second call for the same
    /// `(attempt_id, artifact_id)` — even with byte-identical content —
    /// reports `AlreadySet` rather than silently re-writing, so "checksum
    /// mismatch stages nothing" can never be defeated by racing two uploads
    /// for the same artifact_id.
    #[instrument(skip(self, clock))]
    pub async fn set_execution_artifact_content_reference(
        &self,
        runner_id: &str,
        attempt_id: &str,
        artifact_id: &str,
        fence: i64,
        content_reference: &str,
        clock: &dyn Clock,
    ) -> Result<ArtifactContentCommitResult, StoreError> {
        let now = stamp(clock);
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let valid: bool = sqlx::query_scalar(state::sql("SELECT EXISTS(SELECT 1 FROM execution_attempts WHERE id = ? AND runner_id = ? AND fencing_token = ? AND state NOT IN ({terminal}) AND lease_expires_at > ?)"))
            .bind(attempt_id).bind(runner_id).bind(fence).bind(&now).fetch_one(&mut *tx).await?;
        if !valid {
            tx.commit().await?;
            return Ok(ArtifactContentCommitResult::Stale);
        }
        let changed = sqlx::query(
            "UPDATE execution_artifacts SET content_reference = ? WHERE attempt_id = ? AND artifact_id = ? AND content_reference IS NULL",
        )
        .bind(content_reference)
        .bind(attempt_id)
        .bind(artifact_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        if changed.rows_affected() == 1 {
            Ok(ArtifactContentCommitResult::Committed)
        } else {
            Ok(ArtifactContentCommitResult::AlreadySet)
        }
    }

    /// The read half of the two-phase artifact sweep — the
    /// blob referenced by `content_reference` must be unlinked from disk
    /// (a filesystem concern this crate does not own) *before* the row
    /// naming it disappears, so the caller fetches full rows here, deletes
    /// each blob it can reach, and only then calls
    /// [`Repository::delete_execution_artifacts_by_row_ids`]. A manifest row
    /// with no verified content (`content_reference: None`) is included —
    /// there is no blob to unlink, but the manifest row itself is still
    /// subject to the same age-based retention.
    #[instrument(skip(self))]
    pub async fn list_execution_artifacts_older_than(
        &self,
        cutoff: DateTime<Utc>,
        batch_limit: i64,
    ) -> Result<Vec<ExecutionArtifactRow>, StoreError> {
        let rows = sqlx::query(
            "SELECT id, attempt_id, artifact_id, kind, name, media_type, size_bytes, sha256, \
             content_disposition, content_reference, metadata, created_at \
             FROM execution_artifacts WHERE created_at < ? LIMIT ?",
        )
        .bind(cutoff.to_rfc3339())
        .bind(batch_limit)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| ExecutionArtifactRow {
                id: row.get("id"),
                attempt_id: row.get("attempt_id"),
                artifact_id: row.get("artifact_id"),
                kind: row.get("kind"),
                name: row.get("name"),
                media_type: row.get("media_type"),
                size_bytes: row.get("size_bytes"),
                sha256: row.get("sha256"),
                content_disposition: row.get("content_disposition"),
                content_reference: row.get("content_reference"),
                metadata: row.get("metadata"),
                created_at: row.get("created_at"),
            })
            .collect())
    }

    /// The write half — deletes exactly the rows named by
    /// `ids` (each `execution_artifacts.id`, not `artifact_id`), so a
    /// caller that already unlinked their blobs cannot accidentally sweep a
    /// row it never inspected.
    #[instrument(skip(self, ids))]
    pub async fn delete_execution_artifacts_by_row_ids(
        &self,
        ids: &[String],
    ) -> Result<u64, StoreError> {
        if ids.is_empty() {
            return Ok(0);
        }
        let mut query = String::from("DELETE FROM execution_artifacts WHERE id IN (");
        for (index, _) in ids.iter().enumerate() {
            if index > 0 {
                query.push(',');
            }
            query.push('?');
        }
        query.push(')');
        let mut built = sqlx::query(AssertSqlSafe(query));
        for id in ids {
            built = built.bind(id);
        }
        let result = built.execute(self.pool()).await?;
        Ok(result.rows_affected())
    }

    /// Conditional counterpart to
    /// [`Repository::delete_execution_artifacts_by_row_ids`], scoped to rows
    /// whose `content_reference` is *still* `NULL` — i.e. rows the caller
    /// observed as having no blob to unlink.
    ///
    /// A row observed with `None` can race: a runner's real content upload
    /// (`put_artifact_content` → `store_streaming` then
    /// `set_execution_artifact_content_reference`) can land between the
    /// sweep's read and its delete, writing a real blob and setting
    /// `content_reference` — an unconditional delete would still remove
    /// that row, permanently orphaning the fresh blob. Re-checking
    /// `content_reference IS NULL` inside this same `DELETE` closes that
    /// window to one atomic statement: a row that raced past `NULL`
    /// survives this pass and is picked up correctly on the next. See
    /// `tests/repository/event_artifact_retention.rs` for proof.
    #[instrument(skip(self, ids))]
    pub async fn delete_unresolved_execution_artifacts_by_row_ids(
        &self,
        ids: &[String],
    ) -> Result<u64, StoreError> {
        if ids.is_empty() {
            return Ok(0);
        }
        let mut query = String::from(
            "DELETE FROM execution_artifacts WHERE content_reference IS NULL AND id IN (",
        );
        for (index, _) in ids.iter().enumerate() {
            if index > 0 {
                query.push(',');
            }
            query.push('?');
        }
        query.push(')');
        let mut built = sqlx::query(AssertSqlSafe(query));
        for id in ids {
            built = built.bind(id);
        }
        let result = built.execute(self.pool()).await?;
        Ok(result.rows_affected())
    }

    /// The review record for an attempt's pack, if the operator has viewed or
    /// reviewed it.
    #[instrument(skip(self))]
    pub async fn get_mrp_review(
        &self,
        attempt_id: &str,
    ) -> Result<Option<MrpReviewRow>, StoreError> {
        let row = sqlx::query(AssertSqlSafe(format!(
            "SELECT {MRP_REVIEW_COLUMNS} FROM mrp_reviews WHERE attempt_id = ?"
        )))
        .bind(attempt_id)
        .fetch_optional(self.pool())
        .await?;
        Ok(row.as_ref().map(mrp_review_row))
    }

    /// Stamps the first time the pack was opened. Idempotent: a second call
    /// keeps the first `viewed_at` and changes nothing else.
    #[instrument(skip(self, clock))]
    pub async fn mark_mrp_viewed(
        &self,
        attempt_id: &str,
        artifact_id: &str,
        clock: &dyn Clock,
    ) -> Result<MrpReviewRow, StoreError> {
        let row = sqlx::query(AssertSqlSafe(format!(
            "INSERT INTO mrp_reviews (attempt_id, artifact_id, viewed_at) VALUES (?, ?, ?) \
             ON CONFLICT(attempt_id) DO UPDATE SET viewed_at = COALESCE(mrp_reviews.viewed_at, excluded.viewed_at) \
             RETURNING {MRP_REVIEW_COLUMNS}"
        )))
        .bind(attempt_id)
        .bind(artifact_id)
        .bind(stamp(clock))
        .fetch_one(self.pool())
        .await?;
        Ok(mrp_review_row(&row))
    }

    /// Records the verdict. The first review is the record: `Ok(None)` means
    /// the pack was already reviewed and nothing was written.
    #[instrument(skip(self, clock))]
    pub async fn record_mrp_review(
        &self,
        attempt_id: &str,
        artifact_id: &str,
        verdict: &str,
        reason: &str,
        reviewed_by: &str,
        clock: &dyn Clock,
    ) -> Result<Option<MrpReviewRow>, StoreError> {
        let now = stamp(clock);
        let row = sqlx::query(AssertSqlSafe(format!(
            "INSERT INTO mrp_reviews (attempt_id, artifact_id, verdict, reason, viewed_at, reviewed_at, reviewed_by) \
             VALUES (?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(attempt_id) DO UPDATE SET verdict = excluded.verdict, reason = excluded.reason, \
               viewed_at = COALESCE(mrp_reviews.viewed_at, excluded.viewed_at), \
               reviewed_at = excluded.reviewed_at, reviewed_by = excluded.reviewed_by \
             WHERE mrp_reviews.verdict IS NULL \
             RETURNING {MRP_REVIEW_COLUMNS}"
        )))
        .bind(attempt_id)
        .bind(artifact_id)
        .bind(verdict)
        .bind(reason)
        .bind(&now)
        .bind(&now)
        .bind(reviewed_by)
        .fetch_optional(self.pool())
        .await?;
        Ok(row.as_ref().map(mrp_review_row))
    }
}
