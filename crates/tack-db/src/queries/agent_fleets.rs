use crate::{Repository, StoreError};

/// One runner fleet as `agent_fleets` stores it. `default_policy` stays the raw
/// JSON text: the caller owns parsing it.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct FleetRow {
    pub id: String,
    pub name: String,
    pub concurrency_limit: Option<i64>,
    pub default_policy: String,
}

/// Every runner fleet, by name.
pub async fn fleets_by_name(repo: &Repository) -> Result<Vec<FleetRow>, StoreError> {
    Ok(sqlx::query_as::<_, FleetRow>(
        "SELECT id,name,concurrency_limit,default_policy FROM agent_fleets ORDER BY name",
    )
    .fetch_all(repo.pool())
    .await?)
}

/// Whether a fleet with this id exists.
pub async fn fleet_exists(repo: &Repository, fleet_id: &str) -> Result<bool, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM agent_fleets WHERE id = ?)")
            .bind(fleet_id)
            .fetch_one(repo.pool())
            .await?,
    )
}

/// Whether the runner is on the fleet's roster.
pub async fn fleet_has_member(
    repo: &Repository,
    fleet_id: &str,
    runner_id: &str,
) -> Result<bool, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM agent_fleet_members WHERE fleet_id = ? AND runner_id = ?)",
    )
    .bind(fleet_id)
    .bind(runner_id)
    .fetch_one(repo.pool())
    .await?)
}

/// How many runners are on fleet rosters, across every fleet.
pub async fn fleet_member_count(repo: &Repository) -> Result<i64, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM agent_fleet_members")
            .fetch_one(repo.pool())
            .await?,
    )
}
