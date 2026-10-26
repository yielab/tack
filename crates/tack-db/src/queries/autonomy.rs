use crate::repo::autonomy::Settings;
use crate::{Repository, StoreError};

/// The Automation settings of a project, `None` when the project does not exist.
pub async fn project_settings(
    repo: &Repository,
    project_id: &str,
) -> Result<Option<Settings>, StoreError> {
    Ok(crate::repo::autonomy::get_settings(repo.pool(), project_id).await?)
}
