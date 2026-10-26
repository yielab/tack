use crate::{Repository, StoreError};

/// The token limit a run takes when neither the request nor its profile sets one: the project's
/// provisional limit, `None` when it is turned off.
pub async fn provisional_tokens(
    repo: &Repository,
    project_id: &str,
) -> Result<Option<i64>, StoreError> {
    Ok(crate::repo::budgets::provisional_tokens(repo.pool(), project_id).await?)
}
