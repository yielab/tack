use crate::{Repository, StoreError};

/// Whether prill already took the `kind` record (`mrp`, `merged`, `closed`, `reverted`,
/// `red_ci`) of an attempt.
pub async fn is_recorded(
    repo: &Repository,
    attempt_id: &str,
    kind: &str,
) -> Result<bool, StoreError> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM prill_records WHERE attempt_id = ? AND kind = ?")
            .bind(attempt_id)
            .bind(kind)
            .fetch_one(repo.pool())
            .await?;
    Ok(count > 0)
}
