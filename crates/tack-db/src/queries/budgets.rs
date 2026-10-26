use crate::{Repository, StoreError};

/// The token limit a run takes when neither the request nor its profile sets one: the project's
/// provisional limit, `None` when it is turned off.
pub async fn provisional_tokens(
    repo: &Repository,
    project_id: &str,
) -> Result<Option<i64>, StoreError> {
    Ok(crate::repo::budgets::provisional_tokens(repo.pool(), project_id).await?)
}

/// What each listed request's latest attempt has counted in tokens; a request without an attempt
/// is left out.
pub async fn counted_by_request(
    repo: &Repository,
    request_ids: &[String],
) -> Result<std::collections::HashMap<String, i64>, StoreError> {
    Ok(crate::repo::budgets::counted_by_request(repo.pool(), request_ids).await?)
}

/// The project's token limit for today and what is spoken for; `None` when it has no daily limit.
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
