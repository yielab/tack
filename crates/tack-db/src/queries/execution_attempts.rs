use std::collections::HashMap;

use tack_protocol::execution::{AttemptId, ExecutionState, FencingToken, RunnerId};

use crate::queries::attempt_reviews::AttemptReviewRow;
use crate::repo::autonomy::MergeRow;
use crate::repo::execution::{AttemptListingRow, state, token_column};
use crate::repo::pull_requests::PullRequestRow;
use crate::{Repository, StoreError};

/// One attempt of a request with what became of it: the pull request it opened, the operator's
/// verdict and the merge ledger's row, each `None` when there is none.
#[derive(Debug, Clone)]
pub struct AttemptWithOutcomes {
    pub attempt: AttemptListingRow,
    pub pull_request: Option<PullRequestRow>,
    pub review: Option<AttemptReviewRow>,
    pub merge: Option<MergeRow>,
}

/// Every attempt of `request_id`, oldest first, each with its pull request, review and merge. The
/// outcomes are read for the whole request at once (one query per table, however many attempts
/// the request has), and any failure is returned.
pub async fn attempts_with_outcomes(
    repo: &Repository,
    request_id: &str,
) -> Result<Vec<AttemptWithOutcomes>, StoreError> {
    let attempts = repo.list_attempts_for_request(request_id).await?;
    if attempts.is_empty() {
        return Ok(Vec::new());
    }
    let mut pull_requests: HashMap<String, PullRequestRow> = sqlx::query_as::<_, PullRequestRow>(
        "SELECT * FROM pull_requests WHERE attempt_id IN \
         (SELECT id FROM execution_attempts WHERE request_id = ?)",
    )
    .bind(request_id)
    .fetch_all(repo.pool())
    .await?
    .into_iter()
    .map(|row| (row.attempt_id.clone(), row))
    .collect();
    let mut reviews: HashMap<String, AttemptReviewRow> = sqlx::query_as::<
        _,
        (String, String, Option<String>, String),
    >(
        "SELECT attempt_id, verdict, note, reviewed_at FROM attempt_reviews WHERE attempt_id IN \
             (SELECT id FROM execution_attempts WHERE request_id = ?)",
    )
    .bind(request_id)
    .fetch_all(repo.pool())
    .await?
    .into_iter()
    .map(|(attempt_id, verdict, note, reviewed_at)| {
        (
            attempt_id,
            AttemptReviewRow {
                verdict,
                note,
                reviewed_at,
            },
        )
    })
    .collect();
    let mut merges: HashMap<String, MergeRow> = sqlx::query_as::<_, MergeRow>(
        "SELECT * FROM tack_merges WHERE attempt_id IN \
         (SELECT id FROM execution_attempts WHERE request_id = ?)",
    )
    .bind(request_id)
    .fetch_all(repo.pool())
    .await?
    .into_iter()
    .map(|row| (row.attempt_id.clone(), row))
    .collect();
    Ok(attempts
        .into_iter()
        .map(|attempt| AttemptWithOutcomes {
            pull_request: pull_requests.remove(&attempt.id),
            review: reviews.remove(&attempt.id),
            merge: merges.remove(&attempt.id),
            attempt,
        })
        .collect())
}

/// The state an attempt is in, or `None` when no attempt has this id.
pub async fn attempt_state(
    repo: &Repository,
    attempt_id: &str,
) -> Result<Option<ExecutionState>, StoreError> {
    let text: Option<String> =
        sqlx::query_scalar("SELECT state FROM execution_attempts WHERE id = ?")
            .bind(attempt_id)
            .fetch_optional(repo.pool())
            .await?;
    text.map(|text| state::parse("execution_attempts", "state", &text))
        .transpose()
}

/// When the attempt's runner last sent a heartbeat for it, or `None` when none was recorded.
/// An id no attempt has is `StoreError::NotFound`.
pub async fn last_heartbeat_at(
    repo: &Repository,
    attempt_id: &str,
) -> Result<Option<String>, StoreError> {
    sqlx::query_scalar("SELECT last_heartbeat_at FROM execution_attempts WHERE id = ?")
        .bind(attempt_id)
        .fetch_optional(repo.pool())
        .await?
        .ok_or(StoreError::not_found("attempt"))
}

/// The state of the newest attempt of a request, or `None` when the request has no attempt.
pub async fn latest_state_of_request(
    repo: &Repository,
    request_id: &str,
) -> Result<Option<ExecutionState>, StoreError> {
    let text: Option<String> = sqlx::query_scalar(
        "SELECT state FROM execution_attempts WHERE request_id = ? ORDER BY attempt_number DESC LIMIT 1",
    )
    .bind(request_id)
    .fetch_optional(repo.pool())
    .await?;
    text.map(|text| state::parse("execution_attempts", "state", &text))
        .transpose()
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

/// Whether an attempt with this id exists.
pub async fn attempt_exists(repo: &Repository, attempt_id: &str) -> Result<bool, StoreError> {
    let found: Option<String> =
        sqlx::query_scalar("SELECT id FROM execution_attempts WHERE id = ?")
            .bind(attempt_id)
            .fetch_optional(repo.pool())
            .await?;
    Ok(found.is_some())
}

/// What an attempt's lease says about a runner's right to write: the state it is in and when
/// its lease ends, as the RFC 3339 text the store keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptLease {
    pub state: ExecutionState,
    pub lease_expires_at: String,
}

/// The lease of the attempt `runner_id` holds under `fencing_token`, or `None` when no attempt
/// has all three: a wrong id, a wrong runner and a stale fence look the same.
pub async fn attempt_lease(
    repo: &Repository,
    attempt_id: &AttemptId,
    runner_id: &RunnerId,
    fencing_token: FencingToken,
) -> Result<Option<AttemptLease>, StoreError> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT state, lease_expires_at FROM execution_attempts WHERE id=? AND runner_id=? AND fencing_token=?",
    )
    .bind(attempt_id.as_str())
    .bind(runner_id.as_str())
    .bind(token_column(fencing_token)?)
    .fetch_optional(repo.pool())
    .await?;
    row.map(|(state_text, lease_expires_at)| {
        Ok(AttemptLease {
            state: state::parse("execution_attempts", "state", &state_text)?,
            lease_expires_at,
        })
    })
    .transpose()
}

/// Whether `runner_id` holds the attempt under `fencing_token`, whatever its state or lease.
pub async fn holds_fence(
    repo: &Repository,
    attempt_id: &AttemptId,
    runner_id: &RunnerId,
    fencing_token: FencingToken,
) -> Result<bool, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM execution_attempts WHERE id=? AND runner_id=? AND fencing_token=?)",
    )
    .bind(attempt_id.as_str())
    .bind(runner_id.as_str())
    .bind(token_column(fencing_token)?)
    .fetch_one(repo.pool())
    .await?)
}

/// The 1-based number of an attempt within its request. An id no attempt has is
/// `StoreError::NotFound`.
pub async fn attempt_number(repo: &Repository, attempt_id: &str) -> Result<i64, StoreError> {
    sqlx::query_scalar("SELECT attempt_number FROM execution_attempts WHERE id = ?")
        .bind(attempt_id)
        .fetch_optional(repo.pool())
        .await?
        .ok_or(StoreError::not_found("attempt"))
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

/// The event checkpoint an attempt's stream has reached, or `None` before the first batch. An id
/// no attempt has is `StoreError::NotFound`.
pub async fn event_checkpoint(
    repo: &Repository,
    attempt_id: &str,
) -> Result<Option<String>, StoreError> {
    sqlx::query_scalar("SELECT event_checkpoint FROM execution_attempts WHERE id = ?")
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

/// The id of the newest attempt of a request, or `None` when it has none.
pub async fn latest_attempt_of_request(
    repo: &Repository,
    request_id: &str,
) -> Result<Option<String>, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT id FROM execution_attempts WHERE request_id = ? ORDER BY attempt_number DESC LIMIT 1",
    )
    .bind(request_id)
    .fetch_optional(repo.pool())
    .await?)
}

/// The newest succeeded attempt of the task that pushed a branch, `None` when it has none.
pub async fn newest_pushed_attempt_of_item(
    repo: &Repository,
    item_id: &str,
) -> Result<Option<String>, StoreError> {
    Ok(sqlx::query_scalar(state::sql(
        "SELECT a.id FROM execution_attempts a
         JOIN execution_requests r ON r.id = a.request_id
         WHERE r.item_id = ? AND a.state = {succeeded}
           AND json_extract(a.actual_execution, '$.git.pushed') = 1
           AND json_extract(a.actual_execution, '$.git.head_commit') IS NOT NULL
         ORDER BY a.created_at DESC, a.rowid DESC LIMIT 1",
    ))
    .bind(item_id)
    .fetch_optional(repo.pool())
    .await?)
}

/// The `actual_execution` JSON an attempt reported, or `None` when the attempt does not exist
/// or reported none.
pub async fn actual_execution_of_attempt(
    repo: &Repository,
    attempt_id: &str,
) -> Result<Option<String>, StoreError> {
    let raw: Option<Option<String>> =
        sqlx::query_scalar("SELECT actual_execution FROM execution_attempts WHERE id = ?")
            .bind(attempt_id)
            .fetch_optional(repo.pool())
            .await?;
    Ok(raw.flatten())
}

/// The item an attempt ran for and the attempt's 1-based number within its request.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AttemptItem {
    pub item_id: String,
    pub attempt_number: i64,
}

/// The item and number of an attempt, or `None` when no attempt has this id.
pub async fn item_of_attempt(
    repo: &Repository,
    attempt_id: &str,
) -> Result<Option<AttemptItem>, StoreError> {
    Ok(sqlx::query_as(
        "SELECT r.item_id, a.attempt_number FROM execution_attempts a \
         JOIN execution_requests r ON r.id = a.request_id WHERE a.id = ?",
    )
    .bind(attempt_id)
    .fetch_optional(repo.pool())
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
