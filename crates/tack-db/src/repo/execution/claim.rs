//! Claiming: runners and their enrollment, the agent profiles and fleets they belong to, what the scheduler may hand out, and the lease a claim
//! issues and its heartbeats extend.

use super::state::{self, Move};
use chrono::{DateTime, Duration, Utc};
use sqlx::Row;
use tack_protocol::execution::{
    AttemptId, Checkpoint, ClaimRequestId, ExecutionRequestId, ExecutionState, FencingToken,
    RecoveryJournalState, RunnerId,
};
use tracing::instrument;

use super::admin::conflict_on_unique;
use super::enqueue::parse_execution_request_snapshot;
use super::shared::{token_column, token_from_column};
use crate::{Repository, StoreError};

#[derive(Debug, Clone)]
pub struct NewRunner<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub credential_hash: &'a str,
    pub labels: &'a str,
    pub total_capacity: i64,
    pub available_capacity: i64,
    pub capability_snapshot: &'a str,
    pub protocol_version: i64,
}

#[derive(Debug, Clone)]
pub struct NewAgentProfile<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub instructions: &'a str,
    pub tool_policy: &'a str,
    pub limits: &'a str,
}

/// The four built-in profiles: (kind, name, summary, claude-code tools, instructions).
const BUILTIN_PROFILES: [(&str, &str, &str, &[&str], &str); 4] = [
    (
        "implementer",
        "Implementer",
        "Reads, edits and runs commands to make the change",
        &["Read", "Edit", "Write", "Bash", "Agent", "Grep", "Glob"],
        "Complete the requested change. Keep the diff focused. Run the project's tests when they exist. Summarize what changed and what you could not do.",
    ),
    (
        "reviewer",
        "Reviewer",
        "Reads and reports; changes nothing",
        &["Read", "Grep", "Glob", "WebFetch"],
        "Review the task against its acceptance criteria. Do not modify files. Report findings, each with file and line, most severe first.",
    ),
    (
        "researcher",
        "Researcher",
        "Reads and searches; writes a report",
        &["Read", "Grep", "Glob", "WebFetch", "WebSearch"],
        "Investigate the question. Do not modify project files. Write your findings as the result, with sources.",
    ),
    (
        "planner",
        "Planner",
        "Plans and designs; proposes subtasks you can accept",
        &["Read", "Grep", "Glob", "WebFetch"],
        "Plan the work. Do not modify project files. Write `tack-plan.json` at the workspace root: `{\"v\":\"1\",\"subtasks\":[{\"title\",\"description\",\"acceptance\":[\"…\"],\"depends_on\":[index…],\"estimate\":\"S|M|L\"}]}`. Explain the plan as the result.",
    ),
];

/// Inserts each built-in agent profile whose `kind` is absent (profiles are
/// global, so once per database). A `builtin = 1` row that already exists is
/// only ever refreshed in `summary`; edits to the rest are never overwritten.
/// A custom profile already holding a built-in's name makes that insert a
/// no-op (`name` is unique) rather than failing startup.
pub async fn seed_builtin_profiles(
    repo: &crate::Repository,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    let pool = repo.pool();
    let now = now.to_rfc3339();
    for (kind, name, summary, tools, instructions) in BUILTIN_PROFILES {
        let tool_policy = serde_json::json!({"tools": {
            "claude-code": tools, "codex": ["*"], "opencode": ["*"], "docket": ["*"],
        }});
        sqlx::query(
            "INSERT OR IGNORE INTO agent_profiles \
             (id, name, instructions, tool_policy, limits, kind, builtin, summary, created_at, updated_at) \
             SELECT ?, ?, ?, ?, '{}', ?, 1, ?, ?, ? \
             WHERE NOT EXISTS (SELECT 1 FROM agent_profiles WHERE kind = ?)",
        )
        .bind(format!("ap_{}", uuid::Uuid::new_v4()))
        .bind(name)
        .bind(instructions)
        .bind(tool_policy.to_string())
        .bind(kind)
        .bind(summary)
        .bind(&now)
        .bind(&now)
        .bind(kind)
        .execute(pool)
        .await?;
        sqlx::query("UPDATE agent_profiles SET summary = ? WHERE kind = ? AND builtin = 1 AND summary IS NOT ?")
            .bind(summary)
            .bind(kind)
            .bind(summary)
            .execute(pool)
            .await?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lease {
    pub attempt_id: AttemptId,
    pub request_id: ExecutionRequestId,
    pub attempt_number: i64,
    pub runner_id: RunnerId,
    pub fencing_token: FencingToken,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct EnrollmentToken<'a> {
    pub id: &'a str,
    pub runner_id: &'a str,
    pub token_hash: &'a str,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrollmentTokenMetadata {
    pub id: String,
    pub runner_id: String,
    pub expires_at: String,
    pub consumed_at: Option<String>,
    pub revoked_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedeemEnrollmentResult {
    Redeemed(String),
    InvalidOrExpired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunnerCapabilityUpdate {
    /// The runner is not active: nothing was written.
    Revoked,
    /// Written. `credential_expires_at` is the stored expiry, or `None` when the runner has
    /// none. An expiry that is not a timestamp is a database error.
    Updated {
        credential_expires_at: Option<DateTime<Utc>>,
    },
}

/// A unique-constraint violation is a `Conflict` carrying the database's message; every other
/// failure stays a database error.
fn unique_as_conflict(error: sqlx::Error) -> StoreError {
    match error.as_database_error() {
        Some(db_error) if db_error.is_unique_violation() => {
            StoreError::Conflict(db_error.message().to_owned())
        }
        _ => StoreError::Sql(error),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RotatedCredential {
    pub runner_id: String,
    pub credential_expires_at: String,
    pub rotated_at: String,
}

/// Outcome of a compare-and-set credential rotation. `HashMismatch` covers
/// both "another rotation already won the race" and "the runner is no
/// longer active/is revoked" — either way the caller must not treat the
/// newly generated credential as live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialRotationResult {
    Rotated(RotatedCredential),
    HashMismatch,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClaimedExecution {
    pub lease: Lease,
    pub request_snapshot: serde_json::Value,
}

/// How `claim_execution_idempotent_with_snapshot` picks *which* queued
/// request to attempt to claim, once its own capacity check has passed.
///
/// `tack-db` cannot depend on `tack-orch` (the dependency arrow points the
/// other way — see `crates/tack-orch/Cargo.toml`'s own header comment), so
/// the pure `tack_orch::scheduler` decision cannot be called from inside
/// this module or its transaction. Instead the caller (today, only
/// `tack_app::scheduler_wiring::choose_request_for_runner`, invoked from
/// `crates/tack-api/src/handlers/runner_protocol.rs`'s `claim` handler)
/// resolves the decision *first*, against a read-only snapshot fetched via
/// [`Repository::fetch_runner_scheduling_snapshot`]/
/// [`Repository::list_eligible_queued_requests`], and hands the resulting
/// choice back in here so the actual fenced write stays exactly as strict
/// as it always was — pure
/// selection never grants the authoritative lease.
#[derive(Debug, Clone, Copy)]
pub enum RequestSelection<'a> {
    /// The pre-Wave-4 behavior: `ORDER BY created_at LIMIT 1` over every
    /// selector-eligible queued request, ignoring harness/model/label/
    /// heartbeat eligibility entirely. Kept only so every pre-existing test
    /// call site (none of which set up runner capability data) keeps its
    /// exact original behavior unmodified. The production runner-v1 claim
    /// handler never passes this variant.
    Naive,
    /// The caller already ran the pure scheduler against live candidate
    /// data and decided which single request, if any, this runner should
    /// attempt to claim. `Some(id)` names it — this method still re-checks
    /// that id is genuinely `queued` and selector-eligible for `runner_id`
    /// before leasing it (defense in depth against a stale decision).
    /// `None` means the scheduler considered every selector-eligible
    /// request and found none this runner is eligible for right now
    /// (including "there were no queued requests at all") — this call
    /// reports `no work` without falling back to naive selection, which
    /// would silently undo the scheduler's rejection.
    Scheduled(Option<&'a ExecutionRequestId>),
}

/// One row of `GET /api/runners` — every column
/// `agent_runners` carries, minus `credential_hash`/`credential_expires_at`/
/// `credential_rotated_at` (never read back to an operator; enrollment
/// tokens/credentials are one-time-reveal by design), plus this runner's
/// current fleet roster from `agent_fleet_members`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerListingRow {
    pub id: String,
    pub name: String,
    pub state: String,
    pub labels: String,
    pub total_capacity: i64,
    pub available_capacity: i64,
    pub capability_snapshot: String,
    pub protocol_version: i64,
    pub runner_version: Option<String>,
    pub last_heartbeat_at: Option<String>,
    pub revoked_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub fleet_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct HeartbeatLease<'a> {
    pub attempt_id: &'a AttemptId,
    pub fencing_token: FencingToken,
    pub state: ExecutionState,
    pub journal_state: RecoveryJournalState,
    pub last_event_checkpoint: Option<&'a Checkpoint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeartbeatLeaseResponse {
    pub attempt_id: AttemptId,
    pub fencing_token: FencingToken,
    pub state: ExecutionState,
    pub journal_state: RecoveryJournalState,
    pub last_event_checkpoint: Option<Checkpoint>,
    pub lease_expires_at: DateTime<Utc>,
    pub cancellation_requested: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeartbeatBatchResponse {
    pub heartbeat_id: String,
    pub accepted_at: DateTime<Utc>,
    pub leases: Vec<HeartbeatLeaseResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeartbeatBatchResult {
    Accepted(HeartbeatBatchResponse),
    Replayed(HeartbeatBatchResponse),
    Conflict,
    /// A reported lease is no longer the runner's; names its attempt.
    StaleLease(AttemptId),
    /// The runner is revoked or was never enrolled, so it holds nothing.
    RunnerNotActive,
}

fn heartbeat_fingerprint(
    runner_id: &RunnerId,
    heartbeat_id: &str,
    sent_at: DateTime<Utc>,
    available_capacity: i64,
    leases: &[HeartbeatLease<'_>],
    lease_duration: Duration,
) -> Result<String, StoreError> {
    let lease_duration_nanoseconds = lease_duration
        .num_nanoseconds()
        .ok_or_else(|| StoreError::malformed("heartbeat lease duration is out of range"))?;
    let mut leases = leases
        .iter()
        .map(|lease| {
            (
                lease.attempt_id.as_str(),
                lease.fencing_token.get(),
                state::text(lease.state),
                lease.journal_state.as_str(),
                lease.last_event_checkpoint.map(Checkpoint::as_str),
            )
        })
        .collect::<Vec<_>>();
    leases.sort_unstable();
    serde_json::to_string(&serde_json::json!({
        "runner_id": runner_id.as_str(),
        "heartbeat_id": heartbeat_id,
        "sent_at": sent_at.to_rfc3339(),
        "available_capacity": available_capacity,
        "lease_duration_nanoseconds": lease_duration_nanoseconds,
        "leases": leases
            .into_iter()
            .map(|(attempt_id, fencing_token, state, journal_state, last_event_checkpoint)| serde_json::json!({
                "attempt_id": attempt_id,
                "fencing_token": fencing_token,
                "state": state,
                "journal_state": journal_state,
                "last_event_checkpoint": last_event_checkpoint,
            }))
            .collect::<Vec<_>>(),
    }))
    .map_err(|error| StoreError::malformed(error.to_string()))
}

fn heartbeat_response(serialized: &str) -> Result<HeartbeatBatchResponse, StoreError> {
    let value: serde_json::Value = serde_json::from_str(serialized)
        .map_err(|error| StoreError::malformed(error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| StoreError::malformed("invalid heartbeat replay response"))?;
    let required_string = |object: &serde_json::Map<String, serde_json::Value>, field: &str| {
        object
            .get(field)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                StoreError::malformed(format!("invalid heartbeat replay response field: {field}"))
            })
    };
    let accepted_at = DateTime::parse_from_rfc3339(&required_string(object, "accepted_at")?)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| StoreError::malformed(error.to_string()))?;
    let leases = object
        .get("leases")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| StoreError::malformed("invalid heartbeat replay response field: leases"))?
        .iter()
        .map(|value| {
            let value = value
                .as_object()
                .ok_or_else(|| StoreError::malformed("invalid heartbeat replay response lease"))?;
            let lease_expires_at =
                DateTime::parse_from_rfc3339(&required_string(value, "lease_expires_at")?)
                    .map(|value| value.with_timezone(&Utc))
                    .map_err(|error| StoreError::malformed(error.to_string()))?;
            Ok(HeartbeatLeaseResponse {
                attempt_id: AttemptId::new(required_string(value, "attempt_id")?),
                fencing_token: value
                    .get("fencing_token")
                    .and_then(serde_json::Value::as_u64)
                    .map(FencingToken::new)
                    .ok_or_else(|| {
                        StoreError::malformed(
                            "invalid heartbeat replay response field: fencing_token",
                        )
                    })?,
                state: state::parse(&required_string(value, "state")?)?,
                journal_state: serde_json::from_value(
                    value
                        .get("journal_state")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null),
                )
                .map_err(|_| {
                    StoreError::malformed("invalid heartbeat replay response field: journal_state")
                })?,
                last_event_checkpoint: match value.get("last_event_checkpoint") {
                    Some(serde_json::Value::Null) => None,
                    Some(serde_json::Value::String(value)) => Some(Checkpoint::new(value.clone())),
                    _ => {
                        return Err(StoreError::malformed(
                            "invalid heartbeat replay response field: last_event_checkpoint",
                        ));
                    }
                },
                lease_expires_at,
                cancellation_requested: value
                    .get("cancellation_requested")
                    .and_then(serde_json::Value::as_bool)
                    .ok_or_else(|| {
                        StoreError::malformed(
                            "invalid heartbeat replay response field: cancellation_requested",
                        )
                    })?,
            })
        })
        .collect::<Result<Vec<_>, StoreError>>()?;
    Ok(HeartbeatBatchResponse {
        heartbeat_id: required_string(object, "heartbeat_id")?,
        accepted_at,
        leases,
    })
}

fn lease_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Lease, StoreError> {
    let issued: String = row.get("lease_issued_at");
    let expires: String = row.get("lease_expires_at");
    let parse = |value: String| {
        DateTime::parse_from_rfc3339(&value)
            .map(|value| value.with_timezone(&Utc))
            .map_err(|error| StoreError::malformed(error.to_string()))
    };
    Ok(Lease {
        attempt_id: AttemptId::new(row.get::<String, _>("id")),
        request_id: ExecutionRequestId::new(row.get::<String, _>("request_id")),
        attempt_number: row.get("attempt_number"),
        runner_id: RunnerId::new(row.get::<String, _>("runner_id")),
        fencing_token: token_from_column(row.get("fencing_token"))?,
        issued_at: parse(issued)?,
        expires_at: parse(expires)?,
    })
}

fn snapshot(row: &sqlx::sqlite::SqliteRow) -> Result<serde_json::Value, StoreError> {
    parse_execution_request_snapshot(&row.get::<String, _>("request_snapshot"))
}

/// Outcome of [`Repository::add_fleet_member`] — distinguishes an idempotent
/// no-op (`AlreadyMember`) from either side of the pair not existing, so the
/// handler can report a precise 404 instead of collapsing every failure into
/// one generic error. `agent_fleet_members` (migration 041) has
/// existed as a scheduling *read* input
/// (`fetch_runner_scheduling_snapshot`, `fetch_fleet_concurrency`, the
/// claimable-request query below) but had no write path — an operator could
/// never actually populate a fleet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddFleetMemberOutcome {
    Added,
    AlreadyMember,
    FleetNotFound,
    RunnerNotFound,
}

/// Whether a pending runner and its enrollment token can be stored together: the
/// token belongs to the runner, expires after `now`, and the capacities are
/// non-negative with the available one within the total.
pub fn pending_runner_input_is_valid(
    runner: &NewRunner<'_>,
    token: &EnrollmentToken<'_>,
    now: DateTime<Utc>,
) -> bool {
    token.runner_id == runner.id
        && token.expires_at > now
        && runner.total_capacity >= 0
        && runner.available_capacity >= 0
        && runner.available_capacity <= runner.total_capacity
}

impl Repository {
    pub async fn create_pending_runner_and_issue_token(
        &self,
        runner: NewRunner<'_>,
        token: EnrollmentToken<'_>,
        now: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        if !pending_runner_input_is_valid(&runner, &token, now) {
            return Err(StoreError::malformed("invalid pending runner/token input"));
        }
        let now = now.to_rfc3339();
        let mut tx = self.pool().begin().await?;
        sqlx::query("INSERT INTO agent_runners(id,name,credential_hash,state,labels,total_capacity,available_capacity,capability_snapshot,protocol_version,created_at,updated_at) VALUES(?,?,?,'pending_enrollment',?,?,?,?,?,?,?)").bind(runner.id).bind(runner.name).bind("pending:no-credential").bind(runner.labels).bind(runner.total_capacity).bind(runner.available_capacity).bind(runner.capability_snapshot).bind(runner.protocol_version).bind(&now).bind(&now).execute(&mut *tx).await.map_err(|err| conflict_on_unique(err, "runner_name_taken"))?;
        sqlx::query("INSERT INTO agent_enrollment_tokens(id,runner_id,token_hash,expires_at,created_at) VALUES(?,?,?,?,?)").bind(token.id).bind(token.runner_id).bind(token.token_hash).bind(token.expires_at.to_rfc3339()).bind(&now).execute(&mut *tx).await.map_err(|err| conflict_on_unique(err, "runner_name_taken"))?;
        tx.commit().await?;
        Ok(())
    }

    /// Renames a runner whose name is still one the board generated for a local
    /// runner (`local-` and a UUID) and nothing else: a name a person chose is
    /// never touched. Whether a row changed is the answer; a taken `name` is
    /// the usual unique violation.
    pub async fn rename_generated_local_runner(
        &self,
        runner_id: &str,
        name: &str,
        now: DateTime<Utc>,
    ) -> Result<bool, StoreError> {
        let result = sqlx::query(
            "UPDATE agent_runners SET name = ?, updated_at = ? WHERE id = ? \
             AND name GLOB 'local-????????-????-????-????-????????????'",
        )
        .bind(name)
        .bind(now.to_rfc3339())
        .bind(runner_id)
        .execute(self.pool())
        .await
        .map_err(|err| conflict_on_unique(err, "runner_name_taken"))?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn enrollment_token_metadata(
        &self,
        runner_id: &str,
        token_id: &str,
    ) -> Result<Option<EnrollmentTokenMetadata>, StoreError> {
        sqlx::query("SELECT id,runner_id,expires_at,consumed_at,revoked_at FROM agent_enrollment_tokens WHERE runner_id=? AND id=?").bind(runner_id).bind(token_id).fetch_optional(self.pool()).await?.map(|r|Ok(EnrollmentTokenMetadata{id:r.get("id"),runner_id:r.get("runner_id"),expires_at:r.get("expires_at"),consumed_at:r.get("consumed_at"),revoked_at:r.get("revoked_at")})).transpose()
    }

    pub async fn revoke_runner(
        &self,
        runner_id: &str,
        now: DateTime<Utc>,
    ) -> Result<bool, StoreError> {
        let now = now.to_rfc3339();
        let mut tx = self.pool().begin().await?;
        let r=sqlx::query("UPDATE agent_runners SET state='revoked',revoked_at=COALESCE(revoked_at,?),updated_at=? WHERE id=?").bind(&now).bind(&now).bind(runner_id).execute(&mut *tx).await?;
        if r.rows_affected() == 1 {
            sqlx::query("UPDATE agent_enrollment_tokens SET revoked_at=COALESCE(revoked_at,?) WHERE runner_id=? AND consumed_at IS NULL").bind(&now).bind(runner_id).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(r.rows_affected() == 1)
    }

    /// Adds `runner_id` to `fleet_id`'s roster (`agent_fleet_members`).
    /// Idempotent: adding a runner that is already a member is a successful
    /// no-op (`AlreadyMember`), not a conflict — re-populating a fleet with
    /// the same membership twice is the expected operator workflow, not an
    /// error case. Existence of both sides is checked explicitly first (two
    /// reads outside a transaction — this table has no other writer racing
    /// against it, unlike credential rotation) so the caller gets a precise
    /// `FleetNotFound`/`RunnerNotFound` distinction rather than a generic
    /// foreign-key failure from the insert.
    pub async fn add_fleet_member(
        &self,
        fleet_id: &str,
        runner_id: &str,
        now: DateTime<Utc>,
    ) -> Result<AddFleetMemberOutcome, StoreError> {
        let fleet_exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM agent_fleets WHERE id = ?)")
                .bind(fleet_id)
                .fetch_one(self.pool())
                .await?;
        if !fleet_exists {
            return Ok(AddFleetMemberOutcome::FleetNotFound);
        }
        let runner_exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM agent_runners WHERE id = ?)")
                .bind(runner_id)
                .fetch_one(self.pool())
                .await?;
        if !runner_exists {
            return Ok(AddFleetMemberOutcome::RunnerNotFound);
        }
        let now = now.to_rfc3339();
        let result = sqlx::query(
            "INSERT OR IGNORE INTO agent_fleet_members (fleet_id,runner_id,created_at) VALUES (?,?,?)",
        )
        .bind(fleet_id)
        .bind(runner_id)
        .bind(&now)
        .execute(self.pool())
        .await?;
        Ok(if result.rows_affected() == 1 {
            AddFleetMemberOutcome::Added
        } else {
            AddFleetMemberOutcome::AlreadyMember
        })
    }

    /// Removes `runner_id` from `fleet_id`'s roster. Returns `false` if the
    /// pair was never a member (including either id not existing at all) —
    /// the handler maps that to `not_found` rather than treating a no-op
    /// delete as success, matching `revoke_runner`'s own
    /// exists-vs-no-op-true convention just above.
    pub async fn remove_fleet_member(
        &self,
        fleet_id: &str,
        runner_id: &str,
    ) -> Result<bool, StoreError> {
        let result =
            sqlx::query("DELETE FROM agent_fleet_members WHERE fleet_id = ? AND runner_id = ?")
                .bind(fleet_id)
                .bind(runner_id)
                .execute(self.pool())
                .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Compare-and-set credential rotation: the write only applies if
    /// `expected_credential_hash` still matches the runner's currently
    /// stored hash at commit time. Two concurrent or retried rotations from
    /// the same runner authenticate against the same still-valid old hash;
    /// without this compare-and-set the second writer silently overwrites
    /// the first, discarding a credential the runner may already have
    /// cached and persisted (unrecoverable without a fresh operator-issued
    /// enrollment token). Every other mutating path in this protocol has a
    /// compare-and-set or a replay-dedup table — this closes the one gap.
    pub async fn rotate_runner_credential(
        &self,
        runner_id: &str,
        expected_credential_hash: &str,
        new_credential_hash: &str,
        credential_expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<CredentialRotationResult, StoreError> {
        let now = now.to_rfc3339();
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let updated = sqlx::query(
            "UPDATE agent_runners SET credential_hash=?,credential_expires_at=?,credential_rotated_at=?,updated_at=? \
             WHERE id=? AND state='active' AND revoked_at IS NULL AND credential_hash=?",
        )
        .bind(new_credential_hash)
        .bind(credential_expires_at.to_rfc3339())
        .bind(&now)
        .bind(&now)
        .bind(runner_id)
        .bind(expected_credential_hash)
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() != 1 {
            tx.rollback().await?;
            return Ok(CredentialRotationResult::HashMismatch);
        }
        tx.commit().await?;
        Ok(CredentialRotationResult::Rotated(RotatedCredential {
            runner_id: runner_id.into(),
            credential_expires_at: credential_expires_at.to_rfc3339(),
            rotated_at: now,
        }))
    }

    #[allow(clippy::too_many_arguments)] // protocol batch fields must fingerprint together
    pub async fn heartbeat_batch(
        &self,
        runner_id: &RunnerId,
        heartbeat_id: &str,
        sent_at: DateTime<Utc>,
        available_capacity: i64,
        leases: &[HeartbeatLease<'_>],
        lease_duration: Duration,
        now: DateTime<Utc>,
    ) -> Result<HeartbeatBatchResult, StoreError> {
        let now_s = now.to_rfc3339();
        let expires = now + lease_duration;
        let fingerprint = heartbeat_fingerprint(
            runner_id,
            heartbeat_id,
            sent_at,
            available_capacity,
            leases,
            lease_duration,
        )?;
        // Serialize duplicate heartbeat batches for the same runner (e.g. a
        // runner retrying an unacknowledged heartbeat POST) before either
        // transaction reads the runner's capacity row. Two deferred readers
        // can otherwise deadlock while both try to upgrade to writers.
        // Mirrors the redeem_enrollment_token and
        // claim_execution_idempotent_with_snapshot fixes above.
        let runner = runner_id.as_str();
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let capacity: Option<i64> = sqlx::query_scalar(
            "SELECT total_capacity FROM agent_runners WHERE id=? AND state='active' AND revoked_at IS NULL",
        )
        .bind(runner)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(capacity) = capacity else {
            tx.commit().await?;
            return Ok(HeartbeatBatchResult::RunnerNotActive);
        };
        if let Some(row) = sqlx::query(
            "SELECT fingerprint,response FROM execution_heartbeat_replays WHERE runner_id=? AND heartbeat_id=?",
        )
        .bind(runner)
        .bind(heartbeat_id)
        .fetch_optional(&mut *tx)
        .await?
        {
            let stored_fingerprint: String = row.get("fingerprint");
            if stored_fingerprint != fingerprint {
                tx.commit().await?;
                return Ok(HeartbeatBatchResult::Conflict);
            }
            let response: String = row.get("response");
            let response = heartbeat_response(&response)?;
            tx.commit().await?;
            return Ok(HeartbeatBatchResult::Replayed(response));
        }
        let active_reservations: i64 = sqlx::query_scalar(state::sql(
            "SELECT COUNT(*) FROM execution_attempts WHERE runner_id=? AND state IN ({held})",
        ))
        .bind(runner)
        .fetch_one(&mut *tx)
        .await?;
        let expected_available_capacity = capacity - active_reservations;
        if expected_available_capacity < 0 || available_capacity != expected_available_capacity {
            tx.rollback().await?;
            return Ok(HeartbeatBatchResult::Conflict);
        }
        let mut result = Vec::with_capacity(leases.len());
        for lease in leases {
            let (attempt, fencing_token) = (
                lease.attempt_id.as_str(),
                token_column(lease.fencing_token)?,
            );
            let row=sqlx::query(state::sql("SELECT r.cancellation_requested_at FROM execution_attempts a JOIN execution_requests r ON r.id=a.request_id WHERE a.id=? AND a.runner_id=? AND a.fencing_token=? AND a.state IN ({held}) AND a.lease_expires_at>?")).bind(attempt).bind(runner).bind(fencing_token).bind(&now_s).fetch_optional(&mut *tx).await?;
            let Some(row) = row else {
                tx.rollback().await?;
                return Ok(HeartbeatBatchResult::StaleLease(lease.attempt_id.clone()));
            };
            let cancellation: Option<String> = row.get("cancellation_requested_at");
            sqlx::query("UPDATE execution_attempts SET last_heartbeat_at=?,lease_expires_at=?,updated_at=? WHERE id=? AND runner_id=? AND fencing_token=?").bind(&now_s).bind(expires.to_rfc3339()).bind(&now_s).bind(attempt).bind(runner).bind(fencing_token).execute(&mut *tx).await?;
            result.push(HeartbeatLeaseResponse {
                attempt_id: lease.attempt_id.clone(),
                fencing_token: lease.fencing_token,
                state: lease.state,
                journal_state: lease.journal_state,
                last_event_checkpoint: lease.last_event_checkpoint.cloned(),
                lease_expires_at: expires,
                cancellation_requested: cancellation.is_some(),
            });
        }
        sqlx::query("UPDATE agent_runners SET available_capacity=?,last_heartbeat_at=?,updated_at=? WHERE id=?").bind(available_capacity).bind(&now_s).bind(&now_s).bind(runner).execute(&mut *tx).await?;
        let response = HeartbeatBatchResponse {
            heartbeat_id: heartbeat_id.into(),
            accepted_at: now,
            leases: result,
        };
        let stored = serde_json::json!({
            "heartbeat_id": response.heartbeat_id,
            "accepted_at": response.accepted_at.to_rfc3339(),
            "leases": response.leases.iter().map(|lease| serde_json::json!({
                "attempt_id": lease.attempt_id.as_str(),
                "fencing_token": lease.fencing_token.get(),
                "state": state::text(lease.state),
                "journal_state": lease.journal_state.as_str(),
                "last_event_checkpoint": lease.last_event_checkpoint.as_ref().map(Checkpoint::as_str),
                "lease_expires_at": lease.lease_expires_at.to_rfc3339(),
                "cancellation_requested": lease.cancellation_requested,
            })).collect::<Vec<_>>(),
        })
        .to_string();
        sqlx::query("INSERT INTO execution_heartbeat_replays(runner_id,heartbeat_id,fingerprint,response,created_at) VALUES(?,?,?,?,?)").bind(runner).bind(heartbeat_id).bind(fingerprint).bind(&stored).bind(&now_s).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(HeartbeatBatchResult::Accepted(response))
    }

    /// Store only a token hash. Tokens are tied to a pre-created runner so a
    /// redemption can atomically consume the token and activate that identity.
    pub async fn issue_enrollment_token(
        &self,
        token: EnrollmentToken<'_>,
        now: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        sqlx::query("INSERT INTO agent_enrollment_tokens (id, runner_id, token_hash, expires_at, created_at) VALUES (?, ?, ?, ?, ?)")
            .bind(token.id).bind(token.runner_id).bind(token.token_hash).bind(token.expires_at.to_rfc3339()).bind(now.to_rfc3339())
            .execute(self.pool()).await?;
        Ok(())
    }

    pub async fn revoke_enrollment_token(
        &self,
        token_hash: &str,
        now: DateTime<Utc>,
    ) -> Result<bool, StoreError> {
        let result = sqlx::query("UPDATE agent_enrollment_tokens SET revoked_at = COALESCE(revoked_at, ?) WHERE token_hash = ? AND consumed_at IS NULL")
            .bind(now.to_rfc3339()).bind(token_hash).execute(self.pool()).await?;
        Ok(result.rows_affected() == 1)
    }

    // `runner_name` is accepted (the runner-v1 protocol requires the
    // field and validates its presence upstream in
    // `crates/tack-api/src/handlers/runner_protocol.rs::enroll`) but
    // deliberately NOT written to `agent_runners.name` here. `name` carries a
    // `UNIQUE` constraint (migration 040); the operator already assigned it,
    // uniquely, when the pending-runner row was created
    // (`create_pending_runner_and_issue_token`). Two default-configured
    // runners on one host self-report the same `runner_name` (both default it
    // from `TACK_RUNNER_ID`), so letting the self-reported value overwrite
    // the operator-assigned one made the *second* runner's enrollment race
    // the first's for that string and fail a `UNIQUE` constraint the caller
    // never intended to touch — surfaced as an unhandled 500.
    // The operator-assigned name is authoritative for identity;
    // the self-report is accepted for protocol-shape validation only.
    #[allow(clippy::too_many_arguments)] // protocol exchange fields must commit together
    pub async fn redeem_enrollment_token(
        &self,
        token_hash: &str,
        credential_hash: &str,
        credential_expires_at: DateTime<Utc>,
        runner_version: &str,
        _runner_name: &str,
        labels: &str,
        total_capacity: i64,
        available_capacity: i64,
        capability_snapshot: &str,
        protocol_version: i64,
        now: DateTime<Utc>,
    ) -> Result<RedeemEnrollmentResult, StoreError> {
        let now = now.to_rfc3339();
        // Serialize token consumers before either transaction reads the
        // single-use row. Two deferred readers can otherwise deadlock while
        // both try to upgrade to writers, surfacing a database error instead
        // of one authoritative winner and one invalid/expired result.
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let token = sqlx::query("SELECT runner_id FROM agent_enrollment_tokens WHERE token_hash = ? AND consumed_at IS NULL AND revoked_at IS NULL AND expires_at > ?")
            .bind(token_hash).bind(&now).fetch_optional(&mut *tx).await?;
        let Some(token) = token else {
            tx.commit().await?;
            return Ok(RedeemEnrollmentResult::InvalidOrExpired);
        };
        let runner_id: String = token.get("runner_id");
        let used = sqlx::query("UPDATE agent_enrollment_tokens SET consumed_at = ? WHERE token_hash = ? AND consumed_at IS NULL AND revoked_at IS NULL AND expires_at > ?")
            .bind(&now).bind(token_hash).bind(&now).execute(&mut *tx).await.map_err(unique_as_conflict)?;
        if used.rows_affected() != 1 {
            tx.rollback().await?;
            return Ok(RedeemEnrollmentResult::InvalidOrExpired);
        }
        let runner = sqlx::query("UPDATE agent_runners SET credential_hash=?, credential_expires_at=?, credential_rotated_at=?, runner_version=?, labels=?, total_capacity=?, available_capacity=?, capability_snapshot=?, protocol_version=?, state='active', updated_at=? WHERE id=? AND state='pending_enrollment' AND revoked_at IS NULL")
            .bind(credential_hash).bind(credential_expires_at.to_rfc3339()).bind(&now).bind(runner_version).bind(labels).bind(total_capacity).bind(available_capacity).bind(capability_snapshot).bind(protocol_version).bind(&now).bind(&runner_id).execute(&mut *tx).await.map_err(unique_as_conflict)?;
        if runner.rows_affected() != 1 {
            tx.rollback().await?;
            return Ok(RedeemEnrollmentResult::InvalidOrExpired);
        }
        tx.commit().await?;
        Ok(RedeemEnrollmentResult::Redeemed(runner_id))
    }

    /// Writes a runner's self-reported version, labels and capability snapshot, and says what
    /// credential expiry it holds. The read of the expiry and the write are one transaction, so
    /// a revoke cannot land between them. A runner that is no longer active (revoked, or never
    /// enrolled) is `RunnerCapabilityUpdate::Revoked` and nothing is written. A value another
    /// runner already holds under a unique column is `StoreError::Conflict` carrying the
    /// database's message.
    pub async fn update_runner_capabilities(
        &self,
        runner_id: &str,
        runner_version: &str,
        labels: &str,
        capability_snapshot: &str,
        now: DateTime<Utc>,
    ) -> Result<RunnerCapabilityUpdate, StoreError> {
        let now = now.to_rfc3339();
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let row = sqlx::query(
            "SELECT credential_expires_at FROM agent_runners WHERE id=? AND state='active' AND revoked_at IS NULL",
        )
        .bind(runner_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(RunnerCapabilityUpdate::Revoked);
        };
        let stored: Option<String> = row.get("credential_expires_at");
        let credential_expires_at = stored
            .map(|raw| {
                DateTime::parse_from_rfc3339(&raw)
                    .map(|value| value.with_timezone(&Utc))
                    .map_err(|error| sqlx::Error::Decode(Box::new(error)))
            })
            .transpose()?;
        let updated = sqlx::query(
            "UPDATE agent_runners SET runner_version=?, labels=?, capability_snapshot=?, updated_at=? \
             WHERE id=? AND state='active' AND revoked_at IS NULL",
        )
        .bind(runner_version)
        .bind(labels)
        .bind(capability_snapshot)
        .bind(&now)
        .bind(runner_id)
        .execute(&mut *tx)
        .await
        .map_err(unique_as_conflict)?;
        if updated.rows_affected() != 1 {
            tx.rollback().await?;
            return Ok(RunnerCapabilityUpdate::Revoked);
        }
        tx.commit().await?;
        Ok(RunnerCapabilityUpdate::Updated {
            credential_expires_at,
        })
    }

    /// Strong claim: replay lookup, capacity reservation, attempt and replay
    /// record are committed together, so a crash cannot lose the replay key.
    ///
    /// `selection` decides which queued request (if any) this call attempts
    /// to lease once the capacity check above has passed — see
    /// [`RequestSelection`]'s doc comment for why that decision is made by
    /// the caller rather than this module.
    pub async fn claim_execution_idempotent_with_snapshot(
        &self,
        runner_id: &RunnerId,
        claim_request_id: &ClaimRequestId,
        attempt_id: &AttemptId,
        lease_duration: Duration,
        now: DateTime<Utc>,
        selection: RequestSelection<'_>,
    ) -> Result<Option<ClaimedExecution>, StoreError> {
        let now_s = now.to_rfc3339();
        let expires = now + lease_duration;
        let (runner, claim_request, attempt) = (
            runner_id.as_str(),
            claim_request_id.as_str(),
            attempt_id.as_str(),
        );
        // Serialize claimants before either transaction reads the runner's
        // shared capacity row. Two deferred readers can otherwise deadlock
        // while both try to upgrade to writers, surfacing a database error
        // instead of one authoritative lease and one well-typed no-work
        // result. Mirrors the redeem_enrollment_token fix above.
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let replay=sqlx::query("SELECT a.id,a.request_id,a.attempt_number,a.runner_id,a.fencing_token,a.lease_issued_at,a.lease_expires_at,r.request_snapshot FROM execution_claim_replays c JOIN execution_attempts a ON a.id=c.attempt_id JOIN execution_requests r ON r.id=a.request_id WHERE c.runner_id=? AND c.claim_request_id=?").bind(runner).bind(claim_request).fetch_optional(&mut *tx).await?;
        if let Some(row) = replay {
            let lease = lease_from_row(&row)?;
            let snapshot = snapshot(&row)?;
            tx.commit().await?;
            return Ok(Some(ClaimedExecution {
                lease,
                request_snapshot: snapshot,
            }));
        }
        if sqlx::query("UPDATE agent_runners SET available_capacity=available_capacity-1,updated_at=? WHERE id=? AND state='active' AND revoked_at IS NULL AND available_capacity>0").bind(&now_s).bind(runner).execute(&mut *tx).await?.rows_affected()!=1 { tx.commit().await?; return Ok(None); }
        // `Scheduled(None)` means the scheduler already looked at every
        // selector-eligible request and found none this runner is eligible
        // for — reported as "no work" (after undoing the capacity
        // reservation above) without ever issuing the naive query below,
        // which would silently override that rejection.
        if matches!(selection, RequestSelection::Scheduled(None)) {
            tx.rollback().await?;
            return Ok(None);
        }
        let request = match selection {
            RequestSelection::Naive => {
                sqlx::query(state::sql("SELECT id,request_snapshot FROM execution_requests WHERE state={queued} AND ((selector_kind='exact_runner' AND selector_id=?) OR (selector_kind='fleet' AND EXISTS(SELECT 1 FROM agent_fleet_members m WHERE m.fleet_id=selector_id AND m.runner_id=?))) ORDER BY created_at LIMIT 1"))
                    .bind(runner).bind(runner).fetch_optional(&mut *tx).await?
            }
            RequestSelection::Scheduled(Some(chosen_id)) => {
                // Defense in depth: re-verify the scheduler's chosen id is
                // still `queued` and still selector-eligible for this
                // runner, exactly as the naive path always has, rather than
                // trusting the earlier read-only snapshot blindly.
                sqlx::query(state::sql("SELECT id,request_snapshot FROM execution_requests WHERE id=? AND state={queued} AND ((selector_kind='exact_runner' AND selector_id=?) OR (selector_kind='fleet' AND EXISTS(SELECT 1 FROM agent_fleet_members m WHERE m.fleet_id=selector_id AND m.runner_id=?)))"))
                    .bind(chosen_id.as_str()).bind(runner).bind(runner).fetch_optional(&mut *tx).await?
            }
            RequestSelection::Scheduled(None) => unreachable!("handled above"),
        };
        let Some(request) = request else {
            tx.rollback().await?;
            return Ok(None);
        };
        let request_id = ExecutionRequestId::new(request.get::<String, _>("id"));
        let (set, guard) = (Move::Lease.set(), Move::Lease.guard());
        if sqlx::query(sqlx::AssertSqlSafe(format!("UPDATE execution_requests SET {set},waiting_code=NULL,waiting_reason=NULL,waiting_since=NULL,updated_at=? WHERE id=? AND {guard}"))).bind(&now_s).bind(request_id.as_str()).execute(&mut *tx).await?.rows_affected()!=1 { tx.rollback().await?; return Ok(None); }
        let n: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(attempt_number),0)+1 FROM execution_attempts WHERE request_id=?",
        )
        .bind(request_id.as_str())
        .fetch_one(&mut *tx)
        .await?;
        let fencing_token: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(fencing_token),0)+1 FROM execution_attempts WHERE request_id=?",
        )
        .bind(request_id.as_str())
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO execution_attempts(id,request_id,attempt_number,runner_id,fencing_token,lease_issued_at,lease_expires_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(attempt).bind(request_id.as_str()).bind(n).bind(runner).bind(fencing_token).bind(&now_s).bind(expires.to_rfc3339()).bind(&now_s).bind(&now_s).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO execution_claim_replays(runner_id,claim_request_id,attempt_id,created_at) VALUES(?,?,?,?)").bind(runner).bind(claim_request).bind(attempt).bind(&now_s).execute(&mut *tx).await?;
        let lease = Lease {
            attempt_id: attempt_id.clone(),
            request_id,
            attempt_number: n,
            runner_id: runner_id.clone(),
            fencing_token: token_from_column(fencing_token)?,
            issued_at: now,
            expires_at: expires,
        };
        let snapshot = snapshot(&request)?;
        tx.commit().await?;
        Ok(Some(ClaimedExecution {
            lease,
            request_snapshot: snapshot,
        }))
    }

    /// Every enrolled runner, newest-created last, with its current fleet
    /// roster. Backs `GET /api/runners` — `agent_runners`
    /// itself has held this data since migration 040; nothing read it back
    /// to an operator before this.
    pub async fn list_runners(&self) -> Result<Vec<RunnerListingRow>, StoreError> {
        let rows = sqlx::query(
            "SELECT id, name, state, labels, total_capacity, available_capacity, \
             capability_snapshot, protocol_version, runner_version, last_heartbeat_at, \
             revoked_at, created_at, updated_at FROM agent_runners ORDER BY created_at",
        )
        .fetch_all(self.pool())
        .await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let id: String = row.get("id");
            let fleet_ids: Vec<String> =
                sqlx::query_scalar("SELECT fleet_id FROM agent_fleet_members WHERE runner_id = ?")
                    .bind(&id)
                    .fetch_all(self.pool())
                    .await?;
            out.push(RunnerListingRow {
                id,
                name: row.get("name"),
                state: row.get("state"),
                labels: row.get("labels"),
                total_capacity: row.get("total_capacity"),
                available_capacity: row.get("available_capacity"),
                capability_snapshot: row.get("capability_snapshot"),
                protocol_version: row.get("protocol_version"),
                runner_version: row.get("runner_version"),
                last_heartbeat_at: row.get("last_heartbeat_at"),
                revoked_at: row.get("revoked_at"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                fleet_ids,
            });
        }
        Ok(out)
    }

    #[instrument(skip(self, input))]
    pub async fn register_runner(
        &self,
        input: NewRunner<'_>,
        now: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        let now = now.to_rfc3339();
        sqlx::query(
            "INSERT INTO agent_runners (id, name, credential_hash, labels, total_capacity, \
             available_capacity, capability_snapshot, protocol_version, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(input.id)
        .bind(input.name)
        .bind(input.credential_hash)
        .bind(input.labels)
        .bind(input.total_capacity)
        .bind(input.available_capacity)
        .bind(input.capability_snapshot)
        .bind(input.protocol_version)
        .bind(&now)
        .bind(&now)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    #[instrument(skip(self, input))]
    pub async fn create_agent_profile(
        &self,
        input: NewAgentProfile<'_>,
        now: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        let now = now.to_rfc3339();
        sqlx::query(
            "INSERT INTO agent_profiles (id, name, instructions, tool_policy, limits, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(input.id).bind(input.name).bind(input.instructions)
        .bind(input.tool_policy).bind(input.limits).bind(&now).bind(&now)
        .execute(self.pool()).await?;
        Ok(())
    }

    /// Every agent profile, by name.
    #[instrument(skip(self))]
    pub async fn list_agent_profile_rows(
        &self,
    ) -> Result<Vec<tack_core::outline::bundle::ProfileRow>, StoreError> {
        use sqlx::Row;
        let rows = sqlx::query(
            "SELECT id, name, kind, instructions, tool_policy, limits, builtin, fleet_id, delivers \
             FROM agent_profiles ORDER BY name",
        )
        .fetch_all(self.pool())
        .await?;
        let json =
            |text: String| serde_json::from_str(&text).unwrap_or_else(|_| serde_json::json!({}));
        Ok(rows
            .iter()
            .map(|r| tack_core::outline::bundle::ProfileRow {
                id: r.get("id"),
                name: r.get("name"),
                kind: r.get("kind"),
                instructions: r.get("instructions"),
                tool_policy: json(r.get("tool_policy")),
                limits: json(r.get("limits")),
                fleet_id: r.get("fleet_id"),
                delivers: r.get("delivers"),
                builtin: r.get::<i64, _>("builtin") != 0,
            })
            .collect())
    }

    /// Every fleet as `(id, name)`, by name.
    #[instrument(skip(self))]
    pub async fn fleet_names(&self) -> Result<Vec<(String, String)>, StoreError> {
        sqlx::query_as("SELECT id, name FROM agent_fleets ORDER BY name")
            .fetch_all(self.pool())
            .await
            .map_err(StoreError::from)
    }

    /// Insert the profile, or change the row with its id; `builtin` is never written.
    #[instrument(skip(self, row))]
    pub async fn write_agent_profile(
        &self,
        row: &tack_core::outline::bundle::ProfileRow,
        now: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        let now = now.to_rfc3339();
        sqlx::query(
            "INSERT INTO agent_profiles \
             (id, name, instructions, tool_policy, limits, kind, fleet_id, delivers, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, \
             instructions = excluded.instructions, tool_policy = excluded.tool_policy, \
             limits = excluded.limits, kind = excluded.kind, fleet_id = excluded.fleet_id, \
             delivers = excluded.delivers, \
             updated_at = excluded.updated_at",
        )
        .bind(&row.id)
        .bind(&row.name)
        .bind(&row.instructions)
        .bind(row.tool_policy.to_string())
        .bind(row.limits.to_string())
        .bind(&row.kind)
        .bind(&row.fleet_id)
        .bind(&row.delivers)
        .bind(&now)
        .bind(&now)
        .execute(self.pool())
        .await?;
        Ok(())
    }
}
