use crate::{Repository, StoreError};

/// The token limit a run takes when neither the request nor its profile sets one: the project's
/// provisional limit, `None` when it is turned off.
pub async fn provisional_tokens(
    repo: &Repository,
    project_id: &str,
) -> Result<Option<i64>, StoreError> {
    Ok(crate::repo::budgets::provisional_tokens(repo.pool(), project_id).await?)
}

/// The project's token room for today, `None` when it has no limit per day.
pub async fn day_room(
    repo: &Repository,
    project_id: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Option<crate::repo::budgets::DayRoom>, StoreError> {
    Ok(crate::repo::budgets::day_room(repo.pool(), project_id, now).await?)
}

/// The merges Tack made on its own for the project since 00:00 UTC.
pub async fn auto_merges_today(
    repo: &Repository,
    project_id: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<i64, StoreError> {
    Ok(crate::repo::budgets::auto_merges_today(repo.pool(), project_id, now).await?)
}
