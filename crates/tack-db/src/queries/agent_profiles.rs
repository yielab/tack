use crate::repo::budgets::COUNTED_TOKENS_SQL;
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

/// The counted tokens (`COUNTED_TOKENS_SQL`) of a profile's last `limit` successful, measured
/// runs, newest first. Successful: the attempt succeeded and no verdict refused it, whether a
/// person's on the pack or the attempt, or the verifier's, which shows as the rework it asked for.
pub async fn successful_run_tokens(
    repo: &Repository,
    profile_id: &str,
    limit: i64,
) -> Result<Vec<i64>, StoreError> {
    // The fragment is a constant of this crate; only values are bound.
    Ok(sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT {COUNTED_TOKENS_SQL}
         FROM execution_attempts a
         JOIN execution_requests r ON r.id = a.request_id
         WHERE r.agent_profile_id = ? AND a.state = 'succeeded'
           AND (json_extract(a.usage, '$.tokens_in.value') IS NOT NULL
                OR json_extract(a.usage, '$.tokens_out.value') IS NOT NULL)
           AND NOT EXISTS (SELECT 1 FROM mrp_reviews m WHERE m.attempt_id = a.id AND m.verdict = 'reject')
           AND NOT EXISTS (SELECT 1 FROM attempt_reviews v WHERE v.attempt_id = a.id AND v.verdict = 'rejected')
           AND NOT EXISTS (SELECT 1 FROM execution_requests w WHERE w.idempotency_key = 'rework-' || a.id)
         ORDER BY a.created_at DESC, a.rowid DESC
         LIMIT ?"
    )))
    .bind(profile_id)
    .bind(limit)
    .fetch_all(repo.pool())
    .await?)
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

/// The stored `tool_policy` text of a profile, `None` when it has none or the profile is gone.
pub async fn tool_policy(
    repo: &Repository,
    profile_id: &str,
) -> Result<Option<String>, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT tool_policy FROM agent_profiles WHERE id = ?")
            .bind(profile_id)
            .fetch_optional(repo.pool())
            .await?,
    )
}
