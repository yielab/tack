//! What a test asks of the store that the program never does: counts, single columns and rows
//! read back to check what a use case or a route wrote. Each answers as the store does, with
//! `Result<_, StoreError>`.

use sqlx::Row;
use tack_db::repo::execution::state;
use tack_db::{Repository, StoreError};
use tack_protocol::execution::ExecutionState;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrollmentTokenMetadata {
    pub id: String,
    pub runner_id: String,
    pub expires_at: String,
    pub consumed_at: Option<String>,
    pub revoked_at: Option<String>,
}

/// The stored hash of the newest enrollment token issued for `runner_id`, or `None` when
/// none was.
pub async fn latest_token_hash_of_runner(
    repo: &Repository,
    runner_id: &str,
) -> Result<Option<String>, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT token_hash FROM agent_enrollment_tokens WHERE runner_id=? ORDER BY created_at DESC LIMIT 1",
    )
    .bind(runner_id)
    .fetch_optional(repo.pool())
    .await?)
}

/// How many enrollment tokens are stored under this hash.
pub async fn count_with_hash(repo: &Repository, token_hash: &str) -> Result<i64, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM agent_enrollment_tokens WHERE token_hash = ?")
            .bind(token_hash)
            .fetch_one(repo.pool())
            .await?,
    )
}

/// Whether the runner is on the fleet's roster.
pub async fn fleet_has_member(
    repo: &Repository,
    fleet_id: &str,
    runner_id: &str,
) -> Result<bool, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM agent_fleet_members WHERE fleet_id = ? AND runner_id = ?)",
    )
    .bind(fleet_id)
    .bind(runner_id)
    .fetch_one(repo.pool())
    .await?)
}

/// How many runners are on fleet rosters, across every fleet.
pub async fn fleet_member_count(repo: &Repository) -> Result<i64, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM agent_fleet_members")
            .fetch_one(repo.pool())
            .await?,
    )
}

/// A runner's name and stored credential hash, as a test checks them after a
/// route changed them.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct RunnerIdentityRow {
    pub name: String,
    pub credential_hash: String,
}

/// The name and credential hash of runner `runner_id`, or `None` when there is none.
pub async fn runner_identity(
    repo: &Repository,
    runner_id: &str,
) -> Result<Option<RunnerIdentityRow>, StoreError> {
    Ok(sqlx::query_as::<_, RunnerIdentityRow>(
        "SELECT name, credential_hash FROM agent_runners WHERE id = ?",
    )
    .bind(runner_id)
    .fetch_optional(repo.pool())
    .await?)
}

/// One runner's lifecycle columns.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct RunnerLifecycle {
    pub state: String,
    pub revoked_at: Option<String>,
}

/// The runner's state and when it was revoked, or `None` when no runner has this id.
pub async fn runner_lifecycle(
    repo: &Repository,
    runner_id: &str,
) -> Result<Option<RunnerLifecycle>, StoreError> {
    Ok(
        sqlx::query_as("SELECT state, revoked_at FROM agent_runners WHERE id = ?")
            .bind(runner_id)
            .fetch_optional(repo.pool())
            .await?,
    )
}

/// When the runner last sent a heartbeat, or `None` when it never has. An id no runner has is
/// `StoreError::NotFound`.
pub async fn runner_last_heartbeat_at(
    repo: &Repository,
    runner_id: &str,
) -> Result<Option<String>, StoreError> {
    sqlx::query_scalar("SELECT last_heartbeat_at FROM agent_runners WHERE id = ?")
        .bind(runner_id)
        .fetch_optional(repo.pool())
        .await?
        .ok_or(StoreError::not_found("runner"))
}

/// How many manifest rows an attempt has.
pub async fn artifact_count_of_attempt(
    repo: &Repository,
    attempt_id: &str,
) -> Result<i64, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_artifacts WHERE attempt_id = ?")
            .bind(attempt_id)
            .fetch_one(repo.pool())
            .await?,
    )
}

/// The `artifact_id` of every manifest row of an attempt, in order.
pub async fn artifact_ids_of_attempt(
    repo: &Repository,
    attempt_id: &str,
) -> Result<Vec<String>, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT artifact_id FROM execution_artifacts WHERE attempt_id = ? ORDER BY artifact_id",
    )
    .bind(attempt_id)
    .fetch_all(repo.pool())
    .await?)
}

/// Whether an artifact manifest row with this row id exists.
pub async fn artifact_row_exists(repo: &Repository, row_id: &str) -> Result<bool, StoreError> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM execution_artifacts WHERE id=?")
        .bind(row_id)
        .fetch_one(repo.pool())
        .await?;
    Ok(count > 0)
}

/// How many `operator_requeue` audits an attempt has.
pub async fn operator_requeues_of_attempt(
    repo: &Repository,
    attempt_id: &str,
) -> Result<i64, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_recovery_audits WHERE attempt_id = ? AND classification = 'operator_requeue'",
    )
    .bind(attempt_id)
    .fetch_one(repo.pool())
    .await?)
}

/// The request an attempt belongs to. An id no attempt has is `StoreError::NotFound`.
pub async fn request_id_of_attempt(
    repo: &Repository,
    attempt_id: &str,
) -> Result<String, StoreError> {
    sqlx::query_scalar("SELECT request_id FROM execution_attempts WHERE id = ?")
        .bind(attempt_id)
        .fetch_optional(repo.pool())
        .await?
        .ok_or(StoreError::not_found("attempt"))
}

/// How many attempts a request has.
pub async fn attempt_count_of_request(
    repo: &Repository,
    request_id: &str,
) -> Result<i64, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_attempts WHERE request_id = ?")
            .bind(request_id)
            .fetch_one(repo.pool())
            .await?,
    )
}

/// How many different fencing tokens the attempts of a request have been issued.
pub async fn fence_count_of_request(
    repo: &Repository,
    request_id: &str,
) -> Result<i64, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT COUNT(DISTINCT fencing_token) FROM execution_attempts WHERE request_id = ?",
    )
    .bind(request_id)
    .fetch_one(repo.pool())
    .await?)
}

/// The completion id that ended an attempt, or `None` while it has none. An id no attempt has is
/// `StoreError::NotFound`.
pub async fn completion_id_of_attempt(
    repo: &Repository,
    attempt_id: &str,
) -> Result<Option<String>, StoreError> {
    sqlx::query_scalar("SELECT completion_id FROM execution_attempts WHERE id = ?")
        .bind(attempt_id)
        .fetch_optional(repo.pool())
        .await?
        .ok_or(StoreError::not_found("attempt"))
}

/// What a Reviewer run said about `attempt_id`, or `None` when none did.
pub async fn reviewer_decision_of_attempt(
    repo: &Repository,
    attempt_id: &str,
) -> Result<Option<String>, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT decision FROM reviewer_verdicts WHERE attempt_id = ?")
            .bind(attempt_id)
            .fetch_optional(repo.pool())
            .await?,
    )
}

/// How many attempts were ended by this completion id and sit in `state`.
pub async fn count_completed_with(
    repo: &Repository,
    completion_id: &str,
    state: ExecutionState,
) -> Result<i64, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_attempts WHERE completion_id = ? AND state = ?",
    )
    .bind(completion_id)
    .bind(state::text(state))
    .fetch_one(repo.pool())
    .await?)
}

/// The runner an attempt was leased to, or `None` when no attempt has this id.
pub async fn runner_of_attempt(
    repo: &Repository,
    attempt_id: &str,
) -> Result<Option<String>, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT runner_id FROM execution_attempts WHERE id = ?")
            .bind(attempt_id)
            .fetch_optional(repo.pool())
            .await?,
    )
}

/// When an attempt started and when it ended, as stored; the outer `None` means no attempt has
/// this id, an inner one that the moment was not recorded.
pub async fn started_and_ended_at(
    repo: &Repository,
    attempt_id: &str,
) -> Result<Option<(Option<String>, Option<String>)>, StoreError> {
    Ok(
        sqlx::query_as("SELECT started_at, ended_at FROM execution_attempts WHERE id = ?")
            .bind(attempt_id)
            .fetch_optional(repo.pool())
            .await?,
    )
}

/// How many claims are remembered for replay, across every runner.
pub async fn claim_replay_count(repo: &Repository) -> Result<i64, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_claim_replays")
            .fetch_one(repo.pool())
            .await?,
    )
}

/// How many heartbeat answers are remembered for replay, across every runner.
pub async fn heartbeat_replay_count(repo: &Repository) -> Result<i64, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_heartbeat_replays")
            .fetch_one(repo.pool())
            .await?,
    )
}

/// How many completions of an attempt are remembered for replay.
pub async fn replay_count_of_attempt(
    repo: &Repository,
    attempt_id: &str,
) -> Result<i64, StoreError> {
    Ok(
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM execution_completion_replays WHERE attempt_id = ?",
        )
        .bind(attempt_id)
        .fetch_one(repo.pool())
        .await?,
    )
}

/// How many decisions an attempt has raised.
pub async fn decision_count_of_attempt(
    repo: &Repository,
    attempt_id: &str,
) -> Result<i64, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_decisions WHERE attempt_id = ?")
            .bind(attempt_id)
            .fetch_one(repo.pool())
            .await?,
    )
}

/// How many events an attempt has stored.
pub async fn event_count_of_attempt(
    repo: &Repository,
    attempt_id: &str,
) -> Result<i64, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_events WHERE attempt_id = ?")
            .bind(attempt_id)
            .fetch_one(repo.pool())
            .await?,
    )
}

/// How many stored events of an attempt carry this `event_id`: 0 or 1.
pub async fn event_count_with_id(
    repo: &Repository,
    attempt_id: &str,
    event_id: &str,
) -> Result<i64, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_events WHERE attempt_id = ? AND event_id = ?",
    )
    .bind(attempt_id)
    .bind(event_id)
    .fetch_one(repo.pool())
    .await?)
}

/// The columns of an `execution_requests` row a caller reads back to check what
/// a route stored. `metadata` and `request_snapshot` stay raw JSON text.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ExecutionRequestRow {
    pub id: String,
    pub idempotency_scope: String,
    pub idempotency_key: String,
    pub state: String,
    pub selector_id: Option<String>,
    pub waiting_reason: Option<String>,
    pub metadata: String,
    pub request_snapshot: String,
    pub created_at: String,
}

/// The execution request with this id, or `None` when there is none.
pub async fn execution_request_row(
    repo: &Repository,
    request_id: &str,
) -> Result<Option<ExecutionRequestRow>, StoreError> {
    Ok(sqlx::query_as::<_, ExecutionRequestRow>(
        "SELECT id, idempotency_scope, idempotency_key, state, selector_id, waiting_reason, \
         metadata, request_snapshot, created_at FROM execution_requests WHERE id = ?",
    )
    .bind(request_id)
    .fetch_optional(repo.pool())
    .await?)
}

/// Every execution request raised against one item.
pub async fn execution_requests_of_item(
    repo: &Repository,
    item_id: &str,
) -> Result<Vec<ExecutionRequestRow>, StoreError> {
    Ok(sqlx::query_as::<_, ExecutionRequestRow>(
        "SELECT id, idempotency_scope, idempotency_key, state, selector_id, waiting_reason, \
         metadata, request_snapshot, created_at FROM execution_requests WHERE item_id = ?",
    )
    .bind(item_id)
    .fetch_all(repo.pool())
    .await?)
}

/// How many execution requests the install holds.
pub async fn execution_request_count(repo: &Repository) -> Result<i64, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_requests")
            .fetch_one(repo.pool())
            .await?,
    )
}

/// The requests that rework `request_id`, oldest first.
pub async fn follow_ups_of_request(
    repo: &Repository,
    request_id: &str,
) -> Result<Vec<FollowUp>, StoreError> {
    Ok(sqlx::query_as(
        "SELECT id, metadata, request_snapshot FROM execution_requests \
         WHERE json_extract(metadata, '$.rework_of') = ? ORDER BY created_at, rowid",
    )
    .bind(request_id)
    .fetch_all(repo.pool())
    .await?)
}

/// The agent profile a request ran, `None` when it names none or the request is gone.
pub async fn agent_profile_of_request(
    repo: &Repository,
    request_id: &str,
) -> Result<Option<String>, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT agent_profile_id FROM execution_requests WHERE id = ?")
            .bind(request_id)
            .fetch_optional(repo.pool())
            .await?
            .flatten(),
    )
}

/// The selector, repository, model, permissions, profile and timeout of one request, or `None`
/// when no request has this id.
pub async fn request_inputs(
    repo: &Repository,
    request_id: &str,
) -> Result<Option<RequestInputsRow>, StoreError> {
    Ok(sqlx::query_as(
        "SELECT selector_id, repository_snapshot, requested_model, permission_policy,
                agent_profile_id, timeout_seconds FROM execution_requests WHERE id = ?",
    )
    .bind(request_id)
    .fetch_optional(repo.pool())
    .await?)
}

/// The waiting columns of a request. An id no request has is `StoreError::NotFound`.
pub async fn waiting_of_request(
    repo: &Repository,
    request_id: &str,
) -> Result<WaitingRow, StoreError> {
    sqlx::query_as(
        "SELECT state, waiting_code, waiting_reason, waiting_since FROM execution_requests WHERE id = ?",
    )
    .bind(request_id)
    .fetch_optional(repo.pool())
    .await?
    .ok_or(StoreError::not_found("request"))
}

/// A request a failed verification queued after another.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FollowUp {
    pub id: String,
    pub metadata: String,
    pub request_snapshot: String,
}

/// What a request was queued with, as its columns hold it.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct RequestInputsRow {
    pub selector_id: String,
    pub repository_snapshot: String,
    pub requested_model: Option<String>,
    pub permission_policy: String,
    pub agent_profile_id: String,
    pub timeout_seconds: i64,
}

/// What a poll left on a request that waits: its state, the code and sentence of the reason, and
/// when the wait began.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct WaitingRow {
    pub state: String,
    pub waiting_code: Option<String>,
    pub waiting_reason: Option<String>,
    pub waiting_since: Option<String>,
}

pub async fn enrollment_token_metadata(
    repo: &Repository,
    runner_id: &str,
    token_id: &str,
) -> Result<Option<EnrollmentTokenMetadata>, StoreError> {
    sqlx::query("SELECT id,runner_id,expires_at,consumed_at,revoked_at FROM agent_enrollment_tokens WHERE runner_id=? AND id=?").bind(runner_id).bind(token_id).fetch_optional(repo.pool()).await?.map(|r|Ok(EnrollmentTokenMetadata{id:r.get("id"),runner_id:r.get("runner_id"),expires_at:r.get("expires_at"),consumed_at:r.get("consumed_at"),revoked_at:r.get("revoked_at")})).transpose()
}

/// The optimistic-concurrency counter an `ETag` is derived from
/// (migration 034). A
/// dedicated read rather than folding `version` into `Repository::get_item`'s
/// `Item`-shaped query: `tack_core::models::Item` has no `version` field,
/// so a caller that needs the counter asks for it separately rather than
/// widening the domain model for one internal reader.
pub async fn get_item_version(repo: &Repository, id: Uuid) -> Result<Option<i64>, StoreError> {
    sqlx::query_scalar("SELECT version FROM items WHERE id = ?")
        .bind(id.to_string())
        .fetch_optional(repo.pool())
        .await
        .map_err(StoreError::from)
}

pub async fn count_items_by_status(
    repo: &Repository,
    project_id: Uuid,
    status: &str,
) -> Result<i64, StoreError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM items WHERE project_id = ? AND status = ? AND hidden = 0",
    )
    .bind(project_id.to_string())
    .bind(status)
    .fetch_one(repo.pool())
    .await?;
    Ok(count)
}
