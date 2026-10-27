//! Recovery: a lost or silent attempt is recovered or handed to the operator, leases that nobody holds expire, and the fleet's health is counted.

use crate::StoreError;
use chrono::{DateTime, Duration, Utc};
use sqlx::Row;
use tack_protocol::Clock;
use tracing::instrument;

use super::decisions::end_attempt_decisions;
use super::shared::{canonical_json, stamp, terminal};
use super::state::{self, Move};
use crate::Repository;
use tack_protocol::execution::ExecutionState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperatorRequeueResult {
    Requeued,
    Replayed,
    Conflict,
    InvalidTransition,
    NotFound,
}

/// A fleet-wide, id-free snapshot of execution runtime health, computed
/// fresh on every call from live tables (nothing cached). Every count is a
/// fixed-cardinality aggregate — keyed only by the small, closed
/// vocabularies of `agent_runners.state` (3 values, migration 040) and
/// `execution_requests.state` (the 10-value `ExecutionState` vocabulary)
/// — never by attempt/request/runner id, so nothing here can grow
/// an unbounded metric label set. See `crates/tack-app/src/observability.rs`
/// for the background task that logs alerts from this snapshot and the pure
/// `evaluate_alerts` function that decides what counts as "stuck"/"ambiguous."
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExecutionFleetSnapshotRow {
    /// `agent_runners.state` -> count.
    pub runner_state_counts: std::collections::BTreeMap<String, i64>,
    /// `execution_requests.state` -> count.
    pub request_state_counts: std::collections::BTreeMap<String, i64>,
    /// Attempts whose lease has expired (`lease_expires_at < now`) but whose
    /// state is still one of `leased`/`preparing`/`running`/`waiting_decision`
    /// — a lease the recovery service has not yet resolved to `lost` or
    /// `needs_operator`. This is what makes a stale lease *observable*
    /// independent of whatever (if anything) later resolves it.
    pub stale_lease_count: i64,
    /// Age in seconds of the single oldest stale lease found above (by
    /// `lease_expires_at`) — `None` iff `stale_lease_count == 0`, never `0`
    /// standing in for "none found."
    pub oldest_stale_lease_age_secs: Option<i64>,
    /// `execution_requests` currently in `needs_operator` — an ambiguous
    /// state that is never automatically retried.
    pub needs_operator_count: i64,
    /// Age in seconds since the oldest `needs_operator` request's `updated_at`
    /// (i.e. since it became ambiguous) — `None` iff `needs_operator_count == 0`.
    pub oldest_needs_operator_age_secs: Option<i64>,
    /// Count of `execution_events` rows with `occurred_at` inside the
    /// caller-chosen trailing window ending at `now`. A coarse ingestion-rate
    /// signal — a single bounded total, never broken down by attempt/event id.
    pub events_ingested_in_window: i64,
}

fn parse_rfc3339_checked(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryObservation {
    ProcessStopped,
    ProcessRunning,
    Ambiguous,
}

#[derive(Debug, Clone)]
pub struct RecoveryObservationInput<'a> {
    pub runner_id: &'a str,
    pub attempt_id: &'a str,
    pub fencing_token: i64,
    pub recovery_key: &'a str,
    pub observation: RecoveryObservation,
    pub details: &'a str,
}

pub use tack_protocol::execution::RecoveryDisposition;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryResponse {
    pub attempt_id: String,
    pub recovery_key: String,
    pub disposition: RecoveryDisposition,
    pub committed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryObservationResult {
    Applied(RecoveryResponse),
    Replayed(RecoveryResponse),
    Conflict,
    Stale,
}

struct RecoveryDetails {
    journal_state: String,
    process_observed: bool,
}

fn recovery_details(details: &str) -> Result<RecoveryDetails, StoreError> {
    let value: serde_json::Value =
        serde_json::from_str(details).map_err(|error| StoreError::malformed(error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| StoreError::malformed("invalid recovery details"))?;
    let journal_state = object
        .get("journal_state")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| StoreError::malformed("invalid recovery details journal_state"))?;
    let process_observed = object
        .get("process_observed")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| StoreError::malformed("invalid recovery details process_observed"))?;
    Ok(RecoveryDetails {
        journal_state,
        process_observed,
    })
}

fn recovery_observation_name(observation: RecoveryObservation) -> &'static str {
    match observation {
        RecoveryObservation::ProcessStopped => "process_stopped",
        RecoveryObservation::ProcessRunning => "process_running",
        RecoveryObservation::Ambiguous => "ambiguous",
    }
}

fn recovery_fingerprint(input: &RecoveryObservationInput<'_>) -> Result<String, StoreError> {
    let details: serde_json::Value = serde_json::from_str(input.details)
        .map_err(|error| StoreError::malformed(error.to_string()))?;
    serde_json::to_string(&serde_json::json!({
        "runner_id": input.runner_id,
        "attempt_id": input.attempt_id,
        "fencing_token": input.fencing_token,
        "recovery_key": input.recovery_key,
        "observation": recovery_observation_name(input.observation),
        "details": canonical_json(details),
    }))
    .map_err(|error| StoreError::malformed(error.to_string()))
}

/// Everything `recover_attempt` does once it holds the open transaction and
/// the attempt's row: replay or conflict on the audit, the terminal case, the
/// disposition, the three updates and the audit row. The runner's own report
/// reaches it through a row read with the runner's authentication; the claim
/// poll's expired-lease rule reaches it with a row read without one, and
/// passes `waiting` — the code and sentence the request keeps — which is
/// written in this same transaction only when the attempt moved.
async fn apply_recovery(
    mut tx: sqlx::Transaction<'static, sqlx::Sqlite>,
    row: &sqlx::sqlite::SqliteRow,
    input: &RecoveryObservationInput<'_>,
    details: &RecoveryDetails,
    fingerprint: &str,
    now: &str,
    waiting: Option<(&str, &str)>,
) -> Result<RecoveryObservationResult, StoreError> {
    if let Some(row) = sqlx::query("SELECT fingerprint,response FROM execution_recovery_audits WHERE attempt_id=? AND recovery_key=?")
        .bind(input.attempt_id).bind(input.recovery_key).fetch_optional(&mut *tx).await? {
        let stored_fingerprint: String = row.get("fingerprint");
        if stored_fingerprint != fingerprint {
            tx.commit().await?;
            return Ok(RecoveryObservationResult::Conflict);
        }
        let response: String = row.get("response");
        let response = recovery_response(&response)?;
        tx.commit().await?;
        return Ok(RecoveryObservationResult::Replayed(response));
    }
    let request: String = row.get("request_id");
    let runner: String = row.get("runner_id");
    let state: String = row.get("state");
    if terminal(&state) {
        let response = RecoveryResponse {
            attempt_id: input.attempt_id.into(),
            recovery_key: input.recovery_key.into(),
            disposition: RecoveryDisposition::AlreadyTerminal,
            committed_at: now.to_owned(),
        };
        let serialized_response = serde_json::json!({
            "attempt_id": response.attempt_id,
            "recovery_key": response.recovery_key,
            "disposition": "already_terminal",
            "committed_at": response.committed_at,
        })
        .to_string();
        sqlx::query("INSERT INTO execution_recovery_audits(attempt_id,recovery_key,classification,details,fingerprint,response,created_at) VALUES(?,?, 'already_terminal',?,?,?,?)")
            .bind(input.attempt_id).bind(input.recovery_key).bind(input.details).bind(fingerprint).bind(serialized_response).bind(now).execute(&mut *tx).await?;
        tx.commit().await?;
        return Ok(RecoveryObservationResult::Applied(response));
    }
    if !state::is_live(&state) {
        tx.commit().await?;
        return Ok(RecoveryObservationResult::Stale);
    }
    let started: Option<String> = row.get("started_at");
    let disposition = match input.observation {
        RecoveryObservation::ProcessStopped
            if started.is_none()
                && details.journal_state == "prepared"
                && !details.process_observed =>
        {
            RecoveryDisposition::SafePreSpawnRequeue
        }
        RecoveryObservation::ProcessStopped
        | RecoveryObservation::ProcessRunning
        | RecoveryObservation::Ambiguous => RecoveryDisposition::NeedsOperator,
    };
    let (attempt_state, request_state) = match disposition {
        RecoveryDisposition::SafePreSpawnRequeue => (ExecutionState::Lost, ExecutionState::Queued),
        RecoveryDisposition::NeedsOperator => {
            (ExecutionState::NeedsOperator, ExecutionState::NeedsOperator)
        }
        RecoveryDisposition::AlreadyTerminal => unreachable!("terminal states return above"),
    };
    let classification = disposition.as_str();
    let attempt_state_value = attempt_state;
    let (attempt_state, request_state) = (state::text(attempt_state), state::text(request_state));
    let guard = Move::Abandon(attempt_state_value).guard();
    let updated = sqlx::query(sqlx::AssertSqlSafe(format!("UPDATE execution_attempts SET state=?,updated_at=? WHERE id=? AND runner_id=? AND fencing_token=? AND {guard}")))
        .bind(attempt_state)
        .bind(now)
        .bind(input.attempt_id)
        .bind(input.runner_id)
        .bind(input.fencing_token)
        .execute(&mut *tx)
        .await?;
    if updated.rows_affected() != 1 {
        tx.rollback().await?;
        return Ok(RecoveryObservationResult::Stale);
    }
    sqlx::query("UPDATE execution_requests SET state=?,cancellation_requested_at=CASE WHEN ?=? THEN NULL ELSE cancellation_requested_at END,updated_at=? WHERE id=?")
        .bind(request_state)
        .bind(request_state)
        .bind(state::text(ExecutionState::Queued))
        .bind(now)
        .bind(&request)
        .execute(&mut *tx)
        .await?;
    end_attempt_decisions(&mut tx, input.attempt_id, now).await?;
    if let Some((code, sentence)) = waiting {
        sqlx::query("UPDATE execution_requests SET waiting_code=?,waiting_reason=? WHERE id=?")
            .bind(code)
            .bind(sentence)
            .bind(&request)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("UPDATE agent_runners SET available_capacity=MIN(total_capacity,available_capacity+1),updated_at=? WHERE id=?").bind(now).bind(&runner).execute(&mut *tx).await?;
    let response = RecoveryResponse {
        attempt_id: input.attempt_id.into(),
        recovery_key: input.recovery_key.into(),
        disposition,
        committed_at: now.to_owned(),
    };
    let serialized_response = serde_json::json!({
        "attempt_id": response.attempt_id,
        "recovery_key": response.recovery_key,
        "disposition": response.disposition.as_str(),
        "committed_at": response.committed_at,
    })
    .to_string();
    sqlx::query("INSERT INTO execution_recovery_audits(attempt_id,recovery_key,classification,details,fingerprint,response,created_at) VALUES(?,?,?,?,?,?,?)")
        .bind(input.attempt_id).bind(input.recovery_key).bind(classification).bind(input.details).bind(fingerprint).bind(serialized_response).bind(now).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(RecoveryObservationResult::Applied(response))
}

fn recovery_response(serialized: &str) -> Result<RecoveryResponse, StoreError> {
    let value: serde_json::Value = serde_json::from_str(serialized)
        .map_err(|error| StoreError::malformed(error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| StoreError::malformed("invalid recovery replay response"))?;
    let required = |field: &str| {
        object
            .get(field)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                StoreError::malformed(format!("invalid recovery replay response field: {field}"))
            })
    };
    let disposition = required("disposition")?
        .parse::<RecoveryDisposition>()
        .map_err(|_| StoreError::malformed("invalid recovery replay response disposition"))?;
    Ok(RecoveryResponse {
        attempt_id: required("attempt_id")?,
        recovery_key: required("recovery_key")?,
        disposition,
        committed_at: required("committed_at")?,
    })
}

impl Repository {
    pub async fn operator_requeue_needs_operator(
        &self,
        request_id: &str,
        recovery_key: &str,
        actor: &str,
        reason_fingerprint: &str,
        clock: &dyn Clock,
    ) -> Result<OperatorRequeueResult, StoreError> {
        let now = stamp(clock);
        // Serialize duplicate operator requeues for the same request (e.g. a
        // double-clicked retry) before either transaction reads the
        // needs_operator attempt row. Two deferred readers can otherwise
        // deadlock while both try to upgrade to writers. Mirrors the
        // redeem_enrollment_token and claim_execution_idempotent_with_snapshot
        // fixes above.
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let attempt:Option<String>=sqlx::query_scalar(state::sql("SELECT id FROM execution_attempts WHERE request_id=? AND state={needs_operator} ORDER BY attempt_number DESC LIMIT 1")).bind(request_id).fetch_optional(&mut *tx).await?;
        let Some(attempt) = attempt else {
            tx.commit().await?;
            return Ok(OperatorRequeueResult::InvalidTransition);
        };
        let details = format!("actor={actor};reason_fingerprint={reason_fingerprint}");
        if let Some(old) = sqlx::query_scalar::<_, String>(
            "SELECT details FROM execution_recovery_audits WHERE attempt_id=? AND recovery_key=?",
        )
        .bind(&attempt)
        .bind(recovery_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            tx.commit().await?;
            return Ok(if old == details {
                OperatorRequeueResult::Replayed
            } else {
                OperatorRequeueResult::Conflict
            });
        };
        let authoritative_recovery: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM execution_recovery_audits WHERE attempt_id=? AND classification=? AND fingerprint <> '' AND response <> '')",
        )
        .bind(&attempt)
        .bind(RecoveryDisposition::NeedsOperator.as_str())
        .fetch_one(&mut *tx)
        .await?;
        if !authoritative_recovery {
            tx.commit().await?;
            return Ok(OperatorRequeueResult::InvalidTransition);
        }
        let (set, guard) = (Move::Requeue.set(), Move::Requeue.guard());
        if sqlx::query(sqlx::AssertSqlSafe(format!("UPDATE execution_requests SET {set},cancellation_requested_at=NULL,updated_at=? WHERE id=? AND {guard}"))).bind(&now).bind(request_id).execute(&mut *tx).await?.rows_affected()!=1{tx.rollback().await?;return Ok(OperatorRequeueResult::InvalidTransition)};
        sqlx::query("INSERT INTO execution_recovery_audits(attempt_id,recovery_key,classification,details,created_at) VALUES(?,?, 'operator_requeue', ?,?)").bind(&attempt).bind(recovery_key).bind(details).bind(&now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(OperatorRequeueResult::Requeued)
    }

    pub async fn recover_attempt(
        &self,
        input: RecoveryObservationInput<'_>,
        clock: &dyn Clock,
    ) -> Result<RecoveryObservationResult, StoreError> {
        let now = stamp(clock);
        let details = recovery_details(input.details)?;
        let fingerprint = recovery_fingerprint(&input)?;
        // Serialize duplicate recovery observations for the same attempt
        // (e.g. a runner or reconciler retrying an unacknowledged recovery
        // report) before either transaction reads this attempt row. Two
        // deferred readers can otherwise deadlock while both try to upgrade
        // to writers. Mirrors the redeem_enrollment_token and
        // claim_execution_idempotent_with_snapshot fixes above.
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let row = sqlx::query("SELECT a.request_id,a.runner_id,a.state,a.started_at FROM execution_attempts a JOIN agent_runners r ON r.id=a.runner_id WHERE a.id=? AND a.runner_id=? AND a.fencing_token=? AND r.state='active' AND r.revoked_at IS NULL")
            .bind(input.attempt_id).bind(input.runner_id).bind(input.fencing_token).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(RecoveryObservationResult::Stale);
        };
        apply_recovery(tx, &row, &input, &details, &fingerprint, &now, None).await
    }

    /// The claim poll's closing of a run nobody holds: every attempt in
    /// `leased`, `preparing`, `running` or `waiting_decision` whose lease
    /// expired before `expired_before` moves, with its request, to
    /// `needs_operator` through the same steps [`Self::recover_attempt`] takes,
    /// whatever state its runner is in (the runner's authentication guards
    /// only the runner's own report), exactly as a recovery that cannot prove
    /// the agent absent does (a lease expiry alone
    /// never starts another process, ADR 0002). The request keeps `code` and
    /// `sentence` for a person to read. Returns how many it closed; a second
    /// call finds nothing.
    pub async fn expire_unheld_leases(
        &self,
        expired_before: DateTime<Utc>,
        code: &str,
        sentence: &str,
        clock: &dyn Clock,
    ) -> Result<u64, StoreError> {
        let candidates = sqlx::query_scalar::<_, String>(state::sql(
            "SELECT id FROM execution_attempts WHERE state IN ({held}) AND lease_expires_at < ?",
        ))
        .bind(expired_before.to_rfc3339())
        .fetch_all(self.pool())
        .await?;
        let details =
            serde_json::json!({"journal_state": code, "process_observed": false}).to_string();
        let mut closed = 0;
        for attempt in candidates {
            let now = stamp(clock);
            // The holder's state does not matter here: `needs_operator` never
            // starts a process, so a revoked or stopped runner's attempts are
            // asked about like any other. The row is read inside the
            // transaction, so an attempt that moved since the list is skipped.
            let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
            let row = sqlx::query(state::sql("SELECT request_id,runner_id,fencing_token,state,started_at FROM execution_attempts WHERE id=? AND state IN ({held}) AND lease_expires_at < ?"))
                .bind(&attempt).bind(expired_before.to_rfc3339()).fetch_optional(&mut *tx).await?;
            let Some(row) = row else {
                tx.commit().await?;
                continue;
            };
            let runner: String = row.get("runner_id");
            let input = RecoveryObservationInput {
                runner_id: &runner,
                attempt_id: &attempt,
                fencing_token: row.get("fencing_token"),
                recovery_key: code,
                observation: RecoveryObservation::Ambiguous,
                details: &details,
            };
            let recovery = recovery_details(&details)?;
            let fingerprint = recovery_fingerprint(&input)?;
            let waiting = Some((code, sentence));
            if matches!(
                apply_recovery(tx, &row, &input, &recovery, &fingerprint, &now, waiting).await?,
                RecoveryObservationResult::Applied(_)
            ) {
                closed += 1;
            }
        }
        Ok(closed)
    }

    /// See [`ExecutionFleetSnapshotRow`]'s own doc comment for the shape and
    /// the cardinality guarantee. Independent read-only queries — nothing
    /// here writes, so no transaction is needed; a snapshot that is
    /// eventually-consistent across its own several reads by a few
    /// milliseconds is an acceptable trade for an observability signal that
    /// is only ever logged/alerted on, never used to gate a safety decision.
    #[instrument(skip(self))]
    pub async fn execution_fleet_snapshot(
        &self,
        now: DateTime<Utc>,
        event_window: Duration,
    ) -> Result<ExecutionFleetSnapshotRow, StoreError> {
        let now_s = now.to_rfc3339();

        let runner_rows: Vec<(String, i64)> =
            sqlx::query_as("SELECT state, COUNT(*) FROM agent_runners GROUP BY state")
                .fetch_all(self.pool())
                .await?;
        let runner_state_counts = runner_rows.into_iter().collect();

        let request_rows: Vec<(String, i64)> =
            sqlx::query_as("SELECT state, COUNT(*) FROM execution_requests GROUP BY state")
                .fetch_all(self.pool())
                .await?;
        let request_state_counts = request_rows.into_iter().collect();

        let stale_lease_count: i64 = sqlx::query_scalar(state::sql(
            "SELECT COUNT(*) FROM execution_attempts WHERE lease_expires_at < ? \
             AND state IN ({held})",
        ))
        .bind(&now_s)
        .fetch_one(self.pool())
        .await?;
        let oldest_stale_lease_expires_at: Option<String> = sqlx::query_scalar(state::sql(
            "SELECT lease_expires_at FROM execution_attempts WHERE lease_expires_at < ? \
             AND state IN ({held}) \
             ORDER BY lease_expires_at ASC LIMIT 1",
        ))
        .bind(&now_s)
        .fetch_optional(self.pool())
        .await?;
        let oldest_stale_lease_age_secs = oldest_stale_lease_expires_at
            .and_then(|s| parse_rfc3339_checked(&s))
            .map(|expired_at| (now - expired_at).num_seconds().max(0));

        let needs_operator_count: i64 = sqlx::query_scalar(state::sql(
            "SELECT COUNT(*) FROM execution_requests WHERE state = {needs_operator}",
        ))
        .fetch_one(self.pool())
        .await?;
        let oldest_needs_operator_updated_at: Option<String> = sqlx::query_scalar(state::sql(
            "SELECT updated_at FROM execution_requests WHERE state = {needs_operator} \
             ORDER BY updated_at ASC LIMIT 1",
        ))
        .fetch_optional(self.pool())
        .await?;
        let oldest_needs_operator_age_secs = oldest_needs_operator_updated_at
            .and_then(|s| parse_rfc3339_checked(&s))
            .map(|since| (now - since).num_seconds().max(0));

        let window_start = (now - event_window).to_rfc3339();
        let events_ingested_in_window: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM execution_events WHERE occurred_at >= ? AND occurred_at <= ?",
        )
        .bind(&window_start)
        .bind(&now_s)
        .fetch_one(self.pool())
        .await?;

        Ok(ExecutionFleetSnapshotRow {
            runner_state_counts,
            request_state_counts,
            stale_lease_count,
            oldest_stale_lease_age_secs,
            needs_operator_count,
            oldest_needs_operator_age_secs,
            events_ingested_in_window,
        })
    }
}
