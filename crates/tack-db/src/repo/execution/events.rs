//! Events: a runner's batches of events are appended once, read back per attempt, and purged with the replay bookkeeping.

use super::shared::token_column;
use super::state;
use crate::StoreError;
use chrono::{DateTime, Utc};
use sqlx::{AssertSqlSafe, Row};
use tack_protocol::execution::{AttemptId, Checkpoint, FencingToken, RunnerId};
use tracing::instrument;

use crate::Repository;
use crate::canonical_json;

#[derive(Debug, Clone)]
pub struct NewEvent<'a> {
    pub id: &'a str,
    pub event_id: &'a str,
    pub sequence: i64,
    pub source: &'a str,
    pub kind: &'a str,
    /// Whatever the source emitted for `kind`; the store encodes it.
    pub payload: &'a serde_json::Value,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct EventBatch<'a> {
    pub runner_id: &'a RunnerId,
    pub attempt_id: &'a AttemptId,
    pub fencing_token: FencingToken,
    pub previous_checkpoint: Option<&'a Checkpoint>,
    pub checkpoint: &'a Checkpoint,
}

/// One row of `GET /api/executions/{request_id}/attempts/{attempt_number}/events`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventListingRow {
    pub event_id: String,
    pub sequence: i64,
    pub source: String,
    pub kind: String,
    pub payload: String,
    pub occurred_at: String,
    pub created_at: String,
}

/// Outcome of one bounded batch-purge call (which may run several bounded
/// transactions internally — see the doc comments on
/// [`Repository::purge_stale_execution_replays`] /
/// [`Repository::purge_stale_terminal_execution_events`]). The fields are
/// the shape a `tack-orch` trait impl backed by this repo passes straight
/// through. The type is named `PurgeStats`, not `RollupStats`: nothing here
/// aggregates rows into a daily table before deleting them (see each
/// method's own doc comment for why).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PurgeStats {
    /// Total raw rows deleted.
    pub rows_purged: i64,
    /// Number of batch transactions it took (observability/tuning only).
    pub batches_run: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventBatchResult {
    pub accepted_event_ids: Vec<String>,
    pub duplicate_event_ids: Vec<String>,
    pub committed_checkpoint: Checkpoint,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventApplyResult {
    Applied(EventBatchResult),
    /// The stored replay fingerprint for this `(attempt_id, checkpoint)` key
    /// differs from the new request's fingerprint: the same idempotency-scoped
    /// key was reused with different content. This can never succeed by
    /// retrying with the same key — the wire mapping is the non-retryable
    /// `idempotency_conflict` stable error, never `conflict`.
    IdempotencyConflict,
    /// No replay row matched this checkpoint, but the attempt's event stream
    /// position was not where this batch expected it (checkpoint already
    /// advanced past it, `previous_checkpoint` mismatch) or a concurrent
    /// writer won the compare-and-set. A benign out-of-order resync — the
    /// wire mapping is the retryable `conflict` stable error.
    Conflict,
    Stale,
}

fn event_batch_fingerprint(
    batch: &EventBatch<'_>,
    events: &[NewEvent<'_>],
) -> Result<String, StoreError> {
    let values: Result<Vec<serde_json::Value>, StoreError> = events
        .iter()
        .map(|event| {
            Ok(serde_json::json!({
                "event_id": event.event_id,
                "sequence": event.sequence,
                "source": event.source,
                "kind": event.kind,
                "payload": canonical_json(event.payload.clone()),
                "occurred_at": event.occurred_at.to_rfc3339(),
            }))
        })
        .collect();
    serde_json::to_string(&serde_json::json!({
        "runner_id": batch.runner_id.as_str(),
        "fencing_token": batch.fencing_token.get(),
        "previous_checkpoint": batch.previous_checkpoint.map(Checkpoint::as_str),
        "events": values?,
    }))
    .map_err(|error| StoreError::unwritable(error))
}

impl Repository {
    /// Every event recorded for one specific attempt (identified by its
    /// parent request id + 1-based attempt number, matching how
    /// `list_attempts_for_request` already reports attempts), oldest first.
    /// `Ok(None)` means no attempt with that number exists for this
    /// request — distinct from `Ok(Some(vec![]))`, an attempt that simply
    /// has not reported any events yet. Backs `GET
    /// /api/executions/{request_id}/attempts/{attempt_number}/events`.
    pub async fn list_events_for_attempt_number(
        &self,
        request_id: &str,
        attempt_number: i64,
    ) -> Result<Option<Vec<EventListingRow>>, StoreError> {
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
        let rows = sqlx::query(
            "SELECT event_id, sequence, source, kind, payload, occurred_at, created_at \
             FROM execution_events WHERE attempt_id = ? ORDER BY sequence",
        )
        .bind(&attempt_id)
        .fetch_all(self.pool())
        .await?;
        Ok(Some(
            rows.into_iter()
                .map(|row| EventListingRow {
                    event_id: row.get("event_id"),
                    sequence: row.get("sequence"),
                    source: row.get("source"),
                    kind: row.get("kind"),
                    payload: row.get("payload"),
                    occurred_at: row.get("occurred_at"),
                    created_at: row.get("created_at"),
                })
                .collect(),
        ))
    }

    #[instrument(skip(self, events))]
    pub async fn append_execution_events_result(
        &self,
        batch: EventBatch<'_>,
        events: &[NewEvent<'_>],
        now: DateTime<Utc>,
    ) -> Result<EventApplyResult, StoreError> {
        let now = now.to_rfc3339();
        let (runner, attempt, fencing_token) = (
            batch.runner_id.as_str(),
            batch.attempt_id.as_str(),
            token_column(batch.fencing_token)?,
        );
        let (checkpoint, previous) = (
            batch.checkpoint.as_str(),
            batch.previous_checkpoint.map(Checkpoint::as_str),
        );
        let fingerprint = event_batch_fingerprint(&batch, events)?;
        // Serialize duplicate/replayed event batches for the same attempt
        // (e.g. a runner retrying an unacknowledged event-batch POST) before
        // either transaction reads this attempt row. Two deferred readers
        // can otherwise deadlock while both try to upgrade to writers.
        // Mirrors the redeem_enrollment_token and
        // claim_execution_idempotent_with_snapshot fixes above.
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let row = sqlx::query(state::sql("SELECT event_checkpoint FROM execution_attempts WHERE id=? AND runner_id=? AND fencing_token=? AND state IN ({held}) AND lease_expires_at>?"))
            .bind(attempt).bind(runner).bind(fencing_token).bind(&now).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(EventApplyResult::Stale);
        };
        if let Some(row) = sqlx::query(
            "SELECT fingerprint,response FROM execution_event_batch_replays WHERE attempt_id=? AND checkpoint=?",
        )
        .bind(attempt)
        .bind(checkpoint)
        .fetch_optional(&mut *tx)
        .await?
        {
            let stored: String = row.get("fingerprint");
            if stored != fingerprint {
                // Same (attempt_id, checkpoint) idempotency-scoped key,
                // different content: this can never succeed by retrying.
                tx.commit().await?;
                return Ok(EventApplyResult::IdempotencyConflict);
            }
            let response: String = row.get("response");
            let value: serde_json::Value = serde_json::from_str(&response)
                .map_err(|error| StoreError::corrupt("execution_event_batch_replays", "response", error))?;
            let accepted = value["accepted_event_ids"]
                .as_array()
                .ok_or_else(|| StoreError::corrupt("execution_event_batch_replays", "response", "invalid event replay response"))?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .ok_or_else(|| StoreError::corrupt("execution_event_batch_replays", "response", "invalid event replay id"))
                        .map(str::to_owned)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let duplicate = value["duplicate_event_ids"]
                .as_array()
                .ok_or_else(|| StoreError::corrupt("execution_event_batch_replays", "response", "invalid event replay response"))?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .ok_or_else(|| StoreError::corrupt("execution_event_batch_replays", "response", "invalid event replay id"))
                        .map(str::to_owned)
                })
                .collect::<Result<Vec<_>, _>>()?;
            tx.commit().await?;
            return Ok(EventApplyResult::Applied(EventBatchResult {
                accepted_event_ids: accepted,
                duplicate_event_ids: duplicate,
                committed_checkpoint: batch.checkpoint.clone(),
                replayed: true,
            }));
        }
        let current: Option<String> = row.get("event_checkpoint");
        if current.as_deref() == Some(checkpoint) {
            // Legacy pre-054 attempt whose checkpoint already advanced but has
            // no replay row: no fingerprint to compare, so this is not proven
            // to be a reused key with different content. Benign/retryable.
            tx.commit().await?;
            return Ok(EventApplyResult::Conflict);
        }
        if current.as_deref() != previous {
            // The batch's claimed previous_checkpoint no longer matches the
            // attempt's actual stream position: a benign out-of-order resync,
            // not an idempotency-key reuse. Retryable once the caller re-syncs.
            tx.commit().await?;
            return Ok(EventApplyResult::Conflict);
        }
        let mut accepted = Vec::new();
        let mut duplicate = Vec::new();
        for event in events {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM execution_events WHERE attempt_id=? AND event_id=?)",
            )
            .bind(attempt)
            .bind(event.event_id)
            .fetch_one(&mut *tx)
            .await?;
            if exists {
                duplicate.push(event.event_id.into())
            } else {
                let payload = serde_json::to_string(event.payload)
                    .map_err(|error| StoreError::unwritable(error))?;
                sqlx::query("INSERT INTO execution_events(id,attempt_id,event_id,sequence,source,kind,payload,occurred_at,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(event.id).bind(attempt).bind(event.event_id).bind(event.sequence).bind(event.source).bind(event.kind).bind(payload).bind(event.occurred_at.to_rfc3339()).bind(&now).execute(&mut *tx).await?;
                accepted.push(event.event_id.into())
            }
        }
        let changed=sqlx::query("UPDATE execution_attempts SET event_checkpoint=?,updated_at=? WHERE id=? AND runner_id=? AND fencing_token=? AND event_checkpoint IS ?").bind(checkpoint).bind(&now).bind(attempt).bind(runner).bind(fencing_token).bind(previous).execute(&mut *tx).await?;
        if changed.rows_affected() != 1 {
            // Defensive: near-unreachable under the BEGIN IMMEDIATE this
            // function already opens, but a lost compare-and-set is a
            // benign/retryable Conflict, not an idempotency-key reuse.
            tx.rollback().await?;
            return Ok(EventApplyResult::Conflict);
        }
        let result = EventBatchResult {
            accepted_event_ids: accepted,
            duplicate_event_ids: duplicate,
            committed_checkpoint: batch.checkpoint.clone(),
            replayed: false,
        };
        let response=serde_json::json!({"accepted_event_ids":result.accepted_event_ids,"duplicate_event_ids":result.duplicate_event_ids}).to_string();
        sqlx::query("INSERT INTO execution_event_batch_replays(attempt_id,checkpoint,fingerprint,response,created_at) VALUES(?,?,?,?,?)").bind(attempt).bind(checkpoint).bind(fingerprint).bind(response).bind(&now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(EventApplyResult::Applied(result))
    }

    /// Deletes stale rows from the six idempotency/replay bookkeeping tables
    /// (`execution_claim_replays`, `execution_heartbeat_replays`, `execution_cancellation_replays`,
    /// `execution_event_batch_replays`, `execution_completion_replays`, `execution_recovery_audits`),
    /// batched at up to `batch_size` rows per table, looping until nothing older than `cutoff` remains.
    ///
    /// Purge, not roll-up: each row only answers "have I already processed this retried write?" for
    /// a replay window of seconds to minutes, so once `cutoff` passes nothing is worth preserving —
    /// unlike `execution_events` ([`Self::purge_stale_terminal_execution_events`]).
    ///
    /// Uses `BEGIN IMMEDIATE`: these tables take live concurrent writes, and a deferred
    /// SELECT-then-DELETE can `SQLITE_LOCKED` against a concurrent writer instead of queuing
    /// behind it. Proved load-bearing in `tests/repository/execution_retention.rs`.
    #[instrument(skip(self))]
    pub async fn purge_stale_execution_replays(
        &self,
        cutoff: DateTime<Utc>,
        batch_size: i64,
    ) -> Result<PurgeStats, StoreError> {
        const TABLES: [(&str, &str); 6] = [
            ("execution_claim_replays", "created_at"),
            ("execution_heartbeat_replays", "created_at"),
            ("execution_cancellation_replays", "created_at"),
            ("execution_event_batch_replays", "created_at"),
            ("execution_completion_replays", "committed_at"),
            ("execution_recovery_audits", "created_at"),
        ];
        let cutoff_str = cutoff.to_rfc3339();
        let mut stats = PurgeStats::default();

        for (table, ts_col) in TABLES {
            loop {
                let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;

                let select_sql = format!(
                    "SELECT rowid FROM {table} WHERE {ts_col} < ? ORDER BY {ts_col} ASC LIMIT ?"
                );
                let ids: Vec<i64> = sqlx::query_scalar(AssertSqlSafe(select_sql))
                    .bind(&cutoff_str)
                    .bind(batch_size)
                    .fetch_all(&mut *tx)
                    .await?;

                if ids.is_empty() {
                    tx.commit().await?;
                    break;
                }

                let placeholders = vec!["?"; ids.len()].join(",");
                let delete_sql = format!("DELETE FROM {table} WHERE rowid IN ({placeholders})");
                let mut q = sqlx::query(AssertSqlSafe(delete_sql));
                for id in &ids {
                    q = q.bind(id);
                }
                q.execute(&mut *tx).await?;

                tx.commit().await?;

                let batch_len = ids.len() as i64;
                stats.rows_purged += batch_len;
                stats.batches_run += 1;

                if batch_len < batch_size {
                    break;
                }
            }
        }

        Ok(stats)
    }

    /// Deletes `execution_events` rows once both (a) `occurred_at < cutoff`
    /// and (b) the owning attempt has reached a genuinely terminal state
    /// (`succeeded`/`failed`/`cancelled`, matching
    /// `ExecutionState::is_terminal()`). Excludes `lost`/`needs_operator`:
    /// both remain actionable and are themselves observability targets
    /// ([`Self::execution_fleet_snapshot`]), so their event history must
    /// never be swept out from under an operator who might still act on it.
    ///
    /// Purge only, not a roll-up: no `execution_events_daily` aggregate
    /// exists in this schema, so this deletes raw rows outright with no
    /// day/kind/count aggregate surviving.
    ///
    /// Same `BEGIN IMMEDIATE` batching rationale as
    /// [`Self::purge_stale_execution_replays`].
    #[instrument(skip(self))]
    pub async fn purge_stale_terminal_execution_events(
        &self,
        cutoff: DateTime<Utc>,
        batch_size: i64,
    ) -> Result<PurgeStats, StoreError> {
        let cutoff_str = cutoff.to_rfc3339();
        let mut stats = PurgeStats::default();

        loop {
            let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;

            let ids: Vec<String> = sqlx::query_scalar(state::sql(
                "SELECT ee.id FROM execution_events ee \
                 JOIN execution_attempts ea ON ea.id = ee.attempt_id \
                 WHERE ee.occurred_at < ? AND ea.state IN ({terminal}) \
                 ORDER BY ee.occurred_at ASC LIMIT ?",
            ))
            .bind(&cutoff_str)
            .bind(batch_size)
            .fetch_all(&mut *tx)
            .await?;

            if ids.is_empty() {
                tx.commit().await?;
                break;
            }

            let placeholders = vec!["?"; ids.len()].join(",");
            let delete_sql = format!("DELETE FROM execution_events WHERE id IN ({placeholders})");
            let mut q = sqlx::query(AssertSqlSafe(delete_sql));
            for id in &ids {
                q = q.bind(id);
            }
            q.execute(&mut *tx).await?;

            tx.commit().await?;

            let batch_len = ids.len() as i64;
            stats.rows_purged += batch_len;
            stats.batches_run += 1;

            if batch_len < batch_size {
                break;
            }
        }

        Ok(stats)
    }
}
