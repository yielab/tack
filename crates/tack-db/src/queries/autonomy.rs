use crate::repo::autonomy::{
    AttemptCeilingInputs, AttemptContext, MergeRow, PendingDemotion, Settings,
};
use crate::{Repository, StoreError};

/// The Automation settings of a project, `None` when the project does not exist.
pub async fn project_settings(
    repo: &Repository,
    project_id: &str,
) -> Result<Option<Settings>, StoreError> {
    Ok(crate::repo::autonomy::get_settings(repo.pool(), project_id).await?)
}

/// The run an attempt belongs to, as the merge reads it; `None` for an unknown attempt.
pub async fn attempt_context(
    repo: &Repository,
    attempt_id: &str,
) -> Result<Option<AttemptContext>, StoreError> {
    Ok(crate::repo::autonomy::attempt_context(repo.pool(), attempt_id).await?)
}

/// The merge ledger's row of an attempt, `None` when the merge was never tried.
pub async fn merge_of_attempt(
    repo: &Repository,
    attempt_id: &str,
) -> Result<Option<MergeRow>, StoreError> {
    Ok(crate::repo::autonomy::get_merge(repo.pool(), attempt_id).await?)
}

/// What the team's ceiling is read from for one attempt.
pub async fn attempt_ceiling_inputs(
    repo: &Repository,
    attempt_id: &str,
) -> Result<Option<AttemptCeilingInputs>, StoreError> {
    Ok(crate::repo::autonomy::attempt_ceiling_inputs(repo.pool(), attempt_id).await?)
}

/// The automatic merges whose pull request was reverted or went red, not yet charged to their
/// domain.
pub async fn pending_demotions(repo: &Repository) -> Result<Vec<PendingDemotion>, StoreError> {
    Ok(crate::repo::autonomy::pending_demotions(repo.pool()).await?)
}

/// The attempts a move to Done tried to merge and left held, for the tick to retry.
pub async fn held_done_candidates(repo: &Repository) -> Result<Vec<String>, StoreError> {
    Ok(crate::repo::autonomy::held_done_candidates(repo.pool()).await?)
}

/// The attempts that may merge by themselves now.
pub async fn auto_candidates(repo: &Repository) -> Result<Vec<String>, StoreError> {
    Ok(crate::repo::autonomy::auto_candidates(repo.pool()).await?)
}

/// The newest uploaded artifact of the attempt whose kind is `kind`: its id and the reference
/// to its stored content.
pub async fn newest_artifact_of_kind(
    repo: &Repository,
    attempt_id: &str,
    kind: &str,
) -> Result<Option<(String, String)>, StoreError> {
    Ok(crate::repo::autonomy::newest_artifact_of_kind(repo.pool(), attempt_id, kind).await?)
}

/// The newest uploaded artifact of the attempt with this media type: its id and the reference
/// to its stored content.
pub async fn newest_artifact_of_media_type(
    repo: &Repository,
    attempt_id: &str,
    media_type: &str,
) -> Result<Option<(String, String)>, StoreError> {
    Ok(
        crate::repo::autonomy::newest_artifact_of_media_type(repo.pool(), attempt_id, media_type)
            .await?,
    )
}

/// How many merge ledger rows came from `trigger` (`button`, `done` or `auto`).
pub async fn merges_by_trigger(repo: &Repository, trigger: &str) -> Result<i64, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM tack_merges WHERE trigger = ?")
            .bind(trigger)
            .fetch_one(repo.pool())
            .await?,
    )
}
