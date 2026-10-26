use crate::{Repository, StoreError};

/// One agent profile as `agent_profiles` stores it. `tool_policy` and `limits`
/// stay the raw JSON text: the caller owns parsing them.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct AgentProfileRow {
    pub id: String,
    pub name: String,
    pub instructions: String,
    pub tool_policy: String,
    pub limits: String,
    pub kind: String,
    pub builtin: i64,
    pub summary: Option<String>,
    pub fleet_id: Option<String>,
    pub delivers: Option<String>,
}

/// Every agent profile, by name.
pub async fn profiles_by_name(repo: &Repository) -> Result<Vec<AgentProfileRow>, StoreError> {
    Ok(sqlx::query_as::<_, AgentProfileRow>(concat!(
        "SELECT id,name,instructions,tool_policy,limits,kind,builtin,summary,fleet_id,delivers FROM agent_profiles",
        " ORDER BY name"
    ))
    .fetch_all(repo.pool())
    .await?)
}

/// The agent profile with this id, or `None` when there is none.
pub async fn profile_by_id(
    repo: &Repository,
    profile_id: &str,
) -> Result<Option<AgentProfileRow>, StoreError> {
    Ok(sqlx::query_as::<_, AgentProfileRow>(concat!(
        "SELECT id,name,instructions,tool_policy,limits,kind,builtin,summary,fleet_id,delivers FROM agent_profiles",
        " WHERE id = ?"
    ))
    .bind(profile_id)
    .fetch_optional(repo.pool())
    .await?)
}

/// Whether an agent profile with this id exists.
pub async fn profile_exists(repo: &Repository, profile_id: &str) -> Result<bool, StoreError> {
    let found: Option<i64> = sqlx::query_scalar("SELECT 1 FROM agent_profiles WHERE id = ?")
        .bind(profile_id)
        .fetch_optional(repo.pool())
        .await?;
    Ok(found.is_some())
}

/// The counted tokens of a profile's last `limit` successful, measured runs, newest first.
pub async fn successful_run_tokens(
    repo: &Repository,
    profile_id: &str,
    limit: i64,
) -> Result<Vec<i64>, StoreError> {
    Ok(crate::repo::metrics::profile_successful_run_tokens(repo.pool(), profile_id, limit).await?)
}

/// An agent profile as the profile resolver reads it: the columns that decide which profile a
/// run gets and which fleet it names.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ProfileCandidateRow {
    pub id: String,
    pub fleet_id: Option<String>,
    pub tool_policy: String,
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
