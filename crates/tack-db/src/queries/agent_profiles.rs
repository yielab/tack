use crate::{Repository, StoreError};

/// An agent profile as the profile resolver reads it: the columns that decide which profile a
/// run gets and which fleet it names.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ProfileCandidateRow {
    pub id: String,
    pub fleet_id: Option<String>,
    pub tool_policy: String,
}

/// An agent profile as a run's snapshot records it.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RunProfileRow {
    pub name: String,
    pub instructions: String,
    pub tool_policy: String,
    pub limits: String,
    pub kind: String,
}

/// The id of the implementer profile, when one exists.
pub async fn implementer_profile_id(repo: &Repository) -> Result<Option<String>, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT id FROM agent_profiles WHERE kind = 'implementer'")
            .fetch_optional(repo.pool())
            .await?,
    )
}

/// Every agent profile, with its fleet and tool policy.
pub async fn profile_candidates(repo: &Repository) -> Result<Vec<ProfileCandidateRow>, StoreError> {
    Ok(
        sqlx::query_as("SELECT id, fleet_id, tool_policy FROM agent_profiles")
            .fetch_all(repo.pool())
            .await?,
    )
}

/// The profile a run is created with, or `None` when no profile has this id.
pub async fn profile_for_run(
    repo: &Repository,
    profile_id: &str,
) -> Result<Option<RunProfileRow>, StoreError> {
    Ok(sqlx::query_as(
        "SELECT name, instructions, tool_policy, limits, kind FROM agent_profiles WHERE id = ?",
    )
    .bind(profile_id)
    .fetch_optional(repo.pool())
    .await?)
}
