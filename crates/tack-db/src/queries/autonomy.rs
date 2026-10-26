use crate::repo::autonomy::Settings;
use crate::{Repository, StoreError};

/// The Automation settings of a project, `None` when the project does not exist.
pub async fn project_settings(
    repo: &Repository,
    project_id: &str,
) -> Result<Option<Settings>, StoreError> {
    Ok(crate::repo::autonomy::get_settings(repo.pool(), project_id).await?)
}

/// The ledger row of an attempt's merge, `None` when the attempt was never checked for merging.
pub async fn merge_of_attempt(
    repo: &Repository,
    attempt_id: &str,
) -> Result<Option<crate::repo::autonomy::MergeRow>, StoreError> {
    Ok(crate::repo::autonomy::get_merge(repo.pool(), attempt_id).await?)
}
