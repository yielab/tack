//! What an operator does to fleets, agent profiles and enrollment tokens: each
//! command is one transaction, and a name that is already taken or a row that
//! is not there comes back as the store's own `Conflict` / `NotFound`.

use chrono::{DateTime, Utc};
use sqlx::Row;
use tracing::instrument;

use super::claim::NewAgentProfile;
use crate::{Repository, StoreError};

/// Raises a unique-constraint violation as `Conflict(reason)`; every other
/// database fault stays a `Sql` error.
pub(super) fn conflict_on_unique(err: sqlx::Error, reason: &str) -> StoreError {
    if err
        .as_database_error()
        .is_some_and(|db_error| db_error.is_unique_violation())
    {
        StoreError::Conflict(reason.to_owned())
    } else {
        StoreError::Sql(err)
    }
}

/// The columns of an agent profile an update may change. `None` leaves a column
/// alone; for `fleet_id` and `delivers`, `Some(None)` clears it.
#[derive(Debug, Clone, Default)]
pub struct AgentProfilePatch<'a> {
    pub name: Option<&'a str>,
    pub instructions: Option<&'a str>,
    pub tool_policy: Option<&'a str>,
    pub limits: Option<&'a str>,
    pub summary: Option<&'a str>,
    pub fleet_id: Option<Option<&'a str>>,
    pub delivers: Option<Option<&'a str>>,
}

impl Repository {
    /// Adds a runner fleet. A taken name is `Conflict("fleet_name_taken")`.
    pub async fn create_agent_fleet(
        &self,
        id: &str,
        name: &str,
        concurrency_limit: Option<i64>,
        default_policy: &str,
        now: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        let now = now.to_rfc3339();
        sqlx::query("INSERT INTO agent_fleets (id,name,concurrency_limit,default_policy,created_at,updated_at) VALUES (?,?,?,?,?,?)")
            .bind(id).bind(name).bind(concurrency_limit).bind(default_policy).bind(&now).bind(&now)
            .execute(self.pool())
            .await
            .map_err(|err| conflict_on_unique(err, "fleet_name_taken"))?;
        Ok(())
    }

    /// Revokes a runner's enrollment token that has not been redeemed. A token
    /// that was redeemed is `Conflict("already_consumed")`; one that never
    /// existed is `NotFound`. Revoking a revoked token again succeeds.
    #[instrument(skip(self))]
    pub async fn revoke_runner_enrollment_token(
        &self,
        runner_id: &str,
        token_id: &str,
        now: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let revoked = sqlx::query("UPDATE agent_enrollment_tokens SET revoked_at=COALESCE(revoked_at,?) WHERE runner_id=? AND id=? AND consumed_at IS NULL")
            .bind(now.to_rfc3339())
            .bind(runner_id)
            .bind(token_id)
            .execute(&mut *tx)
            .await?;
        if revoked.rows_affected() == 1 {
            tx.commit().await?;
            return Ok(());
        }
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM agent_enrollment_tokens WHERE runner_id = ? AND id = ?)",
        )
        .bind(runner_id)
        .bind(token_id)
        .fetch_one(&mut *tx)
        .await?;
        Err(if exists {
            StoreError::Conflict("already_consumed".into())
        } else {
            StoreError::not_found("enrollment token")
        })
    }

    /// Adds an agent profile, placed in `fleet_id` and given a `delivers` line
    /// when either is named, in one transaction. A taken name is
    /// `Conflict("profile_name_taken")`.
    #[instrument(skip(self, input))]
    pub async fn create_agent_profile_placed(
        &self,
        input: NewAgentProfile<'_>,
        fleet_id: Option<&str>,
        delivers: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        let now = now.to_rfc3339();
        let mut tx = self.pool().begin().await?;
        sqlx::query(
            "INSERT INTO agent_profiles (id, name, instructions, tool_policy, limits, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(input.id).bind(input.name).bind(input.instructions)
        .bind(input.tool_policy).bind(input.limits).bind(&now).bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(|err| conflict_on_unique(err, "profile_name_taken"))?;
        if fleet_id.is_some() || delivers.is_some() {
            sqlx::query("UPDATE agent_profiles SET fleet_id = ?, delivers = ? WHERE id = ?")
                .bind(fleet_id)
                .bind(delivers)
                .bind(input.id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Changes the columns of agent profile `id` that `patch` names. No such
    /// profile is `NotFound`; a taken name is `Conflict("profile_name_taken")`.
    #[instrument(skip(self, patch))]
    pub async fn update_agent_profile(
        &self,
        id: &str,
        patch: AgentProfilePatch<'_>,
        now: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        let (set_fleet, fleet_id) = match patch.fleet_id {
            Some(value) => (1, value),
            None => (0, None),
        };
        let (set_delivers, delivers) = match patch.delivers {
            Some(value) => (1, value),
            None => (0, None),
        };
        let done = sqlx::query(
            "UPDATE agent_profiles SET name = COALESCE(?, name), instructions = COALESCE(?, instructions), \
             tool_policy = COALESCE(?, tool_policy), limits = COALESCE(?, limits), \
             summary = COALESCE(?, summary), \
             fleet_id = CASE WHEN ? = 1 THEN ? ELSE fleet_id END, \
             delivers = CASE WHEN ? = 1 THEN ? ELSE delivers END, updated_at = ? WHERE id = ?",
        )
        .bind(patch.name)
        .bind(patch.instructions)
        .bind(patch.tool_policy)
        .bind(patch.limits)
        .bind(patch.summary)
        .bind(set_fleet)
        .bind(fleet_id)
        .bind(set_delivers)
        .bind(delivers)
        .bind(now.to_rfc3339())
        .bind(id)
        .execute(self.pool())
        .await
        .map_err(|err| conflict_on_unique(err, "profile_name_taken"))?;
        if done.rows_affected() == 0 {
            return Err(StoreError::not_found("agent profile"));
        }
        Ok(())
    }

    /// Deletes agent profile `id`. No such profile is `NotFound`; a built-in
    /// one is `Conflict("builtin_profile")` and stays.
    #[instrument(skip(self))]
    pub async fn delete_agent_profile(&self, id: &str) -> Result<(), StoreError> {
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let row = sqlx::query("SELECT builtin FROM agent_profiles WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
        let Some(row) = row else {
            return Err(StoreError::not_found("agent profile"));
        };
        if row.get::<i64, _>("builtin") != 0 {
            return Err(StoreError::Conflict("builtin_profile".into()));
        }
        sqlx::query("DELETE FROM agent_profiles WHERE id = ? AND builtin = 0")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}
