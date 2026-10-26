//! Decisions: a question an attempt asks and waits on, and the parking and resuming of the attempt around it.

use chrono::{DateTime, Utc};
use sqlx::SqliteConnection;
use tack_protocol::Clock;
use tracing::instrument;

use super::shared::{mirror_request_state, stamp};
use crate::Repository;

#[derive(Debug, Clone)]
pub struct NewDecision<'a> {
    pub id: &'a str,
    pub decision_id: &'a str,
    pub kind: &'a str,
    pub prompt: &'a str,
    pub options: &'a str,
    pub metadata: &'a str,
    pub expires_at: Option<DateTime<Utc>>,
    /// The runner's recommended option as JSON, stored in `recommendation`.
    pub recommendation: Option<&'a str>,
}

/// The `resolved_by` of a decision that expired because its attempt ended.
const ATTEMPT_ENDED: &str = r#"{"kind":"system","subject_id":"attempt_ended"}"#;

/// The attempt ended — `succeeded`, `failed`, `cancelled`, `lost` or
/// `needs_operator` — so nobody can answer a question it was waiting on: every
/// `pending` decision becomes `expired`, in the transaction that ended it. An
/// answered decision is left as it was. Every road that ends an attempt calls
/// this; [`Repository::expire_decisions_of_ended_attempts`] closes what a board
/// from before it left behind.
pub(super) async fn end_attempt_decisions(
    conn: &mut SqliteConnection,
    attempt_id: &str,
    now: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE execution_decisions SET state='expired',answer=NULL,resolved_at=?,resolved_by=?,updated_at=? \
         WHERE attempt_id=? AND state='pending'",
    )
    .bind(now)
    .bind(ATTEMPT_ENDED)
    .bind(now)
    .bind(attempt_id)
    .execute(conn)
    .await?;
    Ok(())
}

/// Parks `attempt_id` in `waiting_decision` while it is `running` with a
/// pending decision, and its request with it. Keyed on a pending row rather
/// than on the insert that preceded it, so a replayed create of an already
/// settled decision leaves a running attempt alone; an attempt already
/// waiting, or no longer live, is untouched.
async fn park_attempt_on_pending_decision(
    conn: &mut SqliteConnection,
    attempt_id: &str,
    now: &str,
) -> Result<(), sqlx::Error> {
    let parked = sqlx::query(
        "UPDATE execution_attempts SET state='waiting_decision',updated_at=? \
         WHERE id=? AND state='running' \
         AND EXISTS(SELECT 1 FROM execution_decisions WHERE attempt_id=? AND state='pending')",
    )
    .bind(now)
    .bind(attempt_id)
    .bind(attempt_id)
    .execute(&mut *conn)
    .await?;
    if parked.rows_affected() == 1 {
        mirror_request_state(conn, attempt_id, "waiting_decision", now).await?;
    }
    Ok(())
}

/// Releases `attempt_id` from `waiting_decision` back to `running` once no
/// pending decision remains on it, moving its request with it. Runs on the
/// caller's connection so a decision's own resolution or expiry commits
/// together with the resume. Returns whether the attempt resumed; one that
/// still has a pending decision, or is not `waiting_decision` at all
/// (terminal, lost, needs an operator), is left as it is.
pub async fn resume_attempt_after_decisions(
    conn: &mut SqliteConnection,
    attempt_id: &str,
    now: DateTime<Utc>,
) -> Result<bool, sqlx::Error> {
    let now = now.to_rfc3339();
    let resumed = sqlx::query(
        "UPDATE execution_attempts SET state='running',updated_at=? \
         WHERE id=? AND state='waiting_decision' \
         AND NOT EXISTS(SELECT 1 FROM execution_decisions WHERE attempt_id=? AND state='pending')",
    )
    .bind(&now)
    .bind(attempt_id)
    .bind(attempt_id)
    .execute(&mut *conn)
    .await?;
    if resumed.rows_affected() != 1 {
        return Ok(false);
    }
    mirror_request_state(conn, attempt_id, "running", &now).await?;
    Ok(true)
}

impl Repository {
    /// Closes, as [`end_attempt_decisions`] does when an attempt ends, the
    /// `pending` decisions of attempts already `succeeded`, `failed`,
    /// `cancelled`, `lost` or `needs_operator` — the rows a board from before
    /// that rule left. One statement; a second call finds nothing. Returns how
    /// many it closed.
    pub async fn expire_decisions_of_ended_attempts(
        &self,
        clock: &dyn Clock,
    ) -> Result<u64, sqlx::Error> {
        let now = stamp(clock);
        let closed = sqlx::query(
            "UPDATE execution_decisions SET state='expired',answer=NULL,resolved_at=?,resolved_by=?,updated_at=? \
             WHERE state='pending' AND attempt_id IN (SELECT id FROM execution_attempts \
             WHERE state IN ('succeeded','failed','cancelled','lost','needs_operator'))",
        )
        .bind(&now)
        .bind(ATTEMPT_ENDED)
        .bind(&now)
        .execute(self.pool())
        .await?;
        Ok(closed.rows_affected())
    }

    /// Records a `pending` decision against a live `running` or
    /// `waiting_decision` attempt and parks the attempt (and its request) in
    /// `waiting_decision` in the same transaction. `false` means the attempt
    /// is not eligible (wrong fence, expired lease, or not in either state)
    /// and nothing was written.
    #[instrument(skip(self, decision, clock))]
    pub async fn create_execution_decision(
        &self,
        runner_id: &str,
        attempt_id: &str,
        fence: i64,
        decision: NewDecision<'_>,
        clock: &dyn Clock,
    ) -> Result<bool, sqlx::Error> {
        let now = stamp(clock);
        // Serialize the eligibility check and the insert against a concurrent
        // completion, cancellation, or recovery transition for the same
        // attempt (all of which already open BEGIN IMMEDIATE). Without this,
        // the SELECT and INSERT were two separate un-transacted statements: a
        // concurrent terminal transition could commit between them, letting
        // a decision land against an attempt that has already gone terminal,
        // lost, or needs_operator.
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM execution_attempts WHERE id = ? AND runner_id = ? AND fencing_token = ? AND state IN ('running','waiting_decision') AND lease_expires_at > ?)")
            .bind(attempt_id).bind(runner_id).bind(fence).bind(&now).fetch_one(&mut *tx).await?;
        if !valid {
            tx.commit().await?;
            return Ok(false);
        }
        sqlx::query("INSERT INTO execution_decisions (id, attempt_id, decision_id, kind, prompt, options, metadata, expires_at, recommendation, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(attempt_id, decision_id) DO NOTHING")
        .bind(decision.id).bind(attempt_id).bind(decision.decision_id).bind(decision.kind).bind(decision.prompt).bind(decision.options).bind(decision.metadata).bind(decision.expires_at.map(|v| v.to_rfc3339())).bind(decision.recommendation).bind(&now).bind(&now).execute(&mut *tx).await?;
        park_attempt_on_pending_decision(&mut tx, attempt_id, &now).await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Stamps `viewed_at` the first time an operator opens a decision; later
    /// calls keep the first timestamp. `Ok(None)` means no such decision for
    /// this attempt; `Ok(Some(ts))` is the stored (first) timestamp.
    #[instrument(skip(self, clock))]
    pub async fn mark_execution_decision_viewed(
        &self,
        attempt_id: &str,
        decision_id: &str,
        clock: &dyn Clock,
    ) -> Result<Option<String>, sqlx::Error> {
        let now = stamp(clock);
        sqlx::query_scalar(
            "UPDATE execution_decisions SET viewed_at = COALESCE(viewed_at, ?) \
             WHERE attempt_id = ? AND decision_id = ? RETURNING viewed_at",
        )
        .bind(&now)
        .bind(attempt_id)
        .bind(decision_id)
        .fetch_optional(self.pool())
        .await
    }

    // ─── Runtime retention and observability ───────────────────────────────
}
