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
