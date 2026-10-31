use crate::{Repository, StoreError};

/// Every column `execution_decisions` carries for one row. `options`,
/// `metadata`, `answer` and `resolved_by` stay raw JSON-string columns: the
/// caller owns parsing them into a typed response, this crate does not depend
/// on the wire schema.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct DecisionRow {
    pub id: String,
    pub attempt_id: String,
    pub decision_id: String,
    pub kind: String,
    pub state: String,
    pub prompt: String,
    pub options: String,
    pub metadata: String,
    pub answer: Option<String>,
    pub expires_at: Option<String>,
    pub resolved_at: Option<String>,
    pub resolved_by: Option<String>,
    pub recommendation: Option<String>,
    pub viewed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Every decision raised against one attempt, identified by its request id and
/// 1-based attempt number, oldest first. `None` means no attempt with that
/// number exists for the request, which differs from `Some(vec![])`: an attempt
/// that has raised no decisions yet.
pub async fn decisions_of_attempt(
    repo: &Repository,
    request_id: &str,
    attempt_number: i64,
) -> Result<Option<Vec<DecisionRow>>, StoreError> {
    let attempt_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM execution_attempts WHERE request_id = ? AND attempt_number = ?",
    )
    .bind(request_id)
    .bind(attempt_number)
    .fetch_optional(repo.pool())
    .await?;
    let Some(attempt_id) = attempt_id else {
        return Ok(None);
    };
    let rows = sqlx::query_as::<_, DecisionRow>(
        "SELECT id, attempt_id, decision_id, kind, state, prompt, options, metadata, \
         answer, expires_at, resolved_at, resolved_by, recommendation, viewed_at, created_at, updated_at \
         FROM execution_decisions WHERE attempt_id = ? ORDER BY created_at",
    )
    .bind(&attempt_id)
    .fetch_all(repo.pool())
    .await?;
    Ok(Some(rows))
}

const DECISION_COLUMNS: &str = "id, attempt_id, decision_id, kind, state, prompt, options, metadata, \
     answer, expires_at, resolved_at, resolved_by, recommendation, viewed_at, created_at, updated_at";

/// One decision of an attempt, or `None` when the attempt raised no decision with this id.
pub async fn decision_of_attempt(
    repo: &Repository,
    attempt_id: &str,
    decision_id: &str,
) -> Result<Option<DecisionRow>, StoreError> {
    Ok(sqlx::query_as::<_, DecisionRow>(sqlx::AssertSqlSafe(format!(
        "SELECT {DECISION_COLUMNS} FROM execution_decisions WHERE attempt_id = ? AND decision_id = ?"
    )))
    .bind(attempt_id)
    .bind(decision_id)
    .fetch_optional(repo.pool())
    .await?)
}

/// The decisions of an attempt that changed after `after` (an RFC 3339 timestamp), oldest
/// change first.
pub async fn decisions_changed_since(
    repo: &Repository,
    attempt_id: &str,
    after: &str,
) -> Result<Vec<DecisionRow>, StoreError> {
    Ok(
        sqlx::query_as::<_, DecisionRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {DECISION_COLUMNS} FROM execution_decisions \
         WHERE attempt_id = ? AND updated_at > ? ORDER BY updated_at ASC"
        )))
        .bind(attempt_id)
        .bind(after)
        .fetch_all(repo.pool())
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
