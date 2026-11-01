//! Agent credentials (migration 099): a name, the hash of a secret shown once, scopes, and when
//! it was made, expires, was last used and was revoked. Nothing here decides who may do what;
//! `tack_core::principal` does.

use chrono::{DateTime, Utc};
use tack_core::principal::{AgentCredential, Scope};
use tracing::instrument;
use uuid::Uuid;

use super::Repository;
use crate::{StoreError, decode};

/// The SQL that turns a principal label stored in `$column` (`operator:...`, `agent:<id>`) into
/// the words a screen shows: `operator`, or the credential's name, else `NULL`.
macro_rules! writer_name {
    ($column:literal) => {
        concat!(
            "(CASE WHEN ",
            $column,
            " LIKE 'agent:%' THEN COALESCE((SELECT c.name FROM agent_credentials c WHERE 'agent:' || c.id = ",
            $column,
            "), 'agent') WHEN ",
            $column,
            " LIKE 'operator:%' THEN 'operator' END)"
        )
    };
}
pub(crate) use writer_name;

const COLUMNS: &str = "id, name, scopes, created_at, expires_at, last_used_at, revoked_at";

#[derive(sqlx::FromRow)]
struct CredentialRow {
    id: String,
    name: String,
    scopes: String,
    created_at: String,
    expires_at: String,
    last_used_at: Option<String>,
    revoked_at: Option<String>,
}

impl CredentialRow {
    fn into_credential(self) -> Result<AgentCredential, StoreError> {
        const TABLE: &str = "agent_credentials";
        Ok(AgentCredential {
            id: decode::uuid(TABLE, "id", &self.id)?,
            name: self.name,
            scopes: decode::json::<Vec<Scope>>(TABLE, "scopes", &self.scopes)?,
            created_at: decode::instant(TABLE, "created_at", &self.created_at)?,
            expires_at: decode::instant(TABLE, "expires_at", &self.expires_at)?,
            last_used_at: decode::optional_instant(
                TABLE,
                "last_used_at",
                self.last_used_at.as_deref(),
            )?,
            revoked_at: decode::optional_instant(TABLE, "revoked_at", self.revoked_at.as_deref())?,
        })
    }
}

impl Repository {
    /// Stores a new credential under the hash of its secret. A name another credential not
    /// revoked holds is `StoreError::Conflict`.
    #[instrument(skip(self, secret_hash, scopes))]
    pub async fn create_agent_credential(
        &self,
        name: &str,
        secret_hash: &str,
        scopes: &[Scope],
        now: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Result<AgentCredential, StoreError> {
        let id = Uuid::new_v4();
        let scopes_json = decode::to_json(scopes)?;
        sqlx::query(
            "INSERT INTO agent_credentials (id, name, secret_hash, scopes, created_at, expires_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(name)
        .bind(secret_hash)
        .bind(&scopes_json)
        .bind(now.to_rfc3339())
        .bind(expires_at.to_rfc3339())
        .execute(self.pool())
        .await
        .map_err(name_conflict)?;
        self.get_agent_credential(id)
            .await?
            .ok_or_else(|| StoreError::not_found("credential"))
    }

    pub async fn get_agent_credential(
        &self,
        id: Uuid,
    ) -> Result<Option<AgentCredential>, StoreError> {
        sqlx::query_as::<_, CredentialRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {COLUMNS} FROM agent_credentials WHERE id = ?"
        )))
        .bind(id.to_string())
        .fetch_optional(self.pool())
        .await?
        .map(CredentialRow::into_credential)
        .transpose()
    }

    /// Every credential, the newest first, the revoked ones included.
    pub async fn list_agent_credentials(&self) -> Result<Vec<AgentCredential>, StoreError> {
        sqlx::query_as::<_, CredentialRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {COLUMNS} FROM agent_credentials ORDER BY created_at DESC, rowid DESC"
        )))
        .fetch_all(self.pool())
        .await?
        .into_iter()
        .map(CredentialRow::into_credential)
        .collect()
    }

    /// The credential whose secret has this hash, whatever its state.
    pub async fn find_agent_credential_by_hash(
        &self,
        secret_hash: &str,
    ) -> Result<Option<AgentCredential>, StoreError> {
        sqlx::query_as::<_, CredentialRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {COLUMNS} FROM agent_credentials WHERE secret_hash = ?"
        )))
        .bind(secret_hash)
        .fetch_optional(self.pool())
        .await?
        .map(CredentialRow::into_credential)
        .transpose()
    }

    /// Gives a credential not revoked a new secret hash and a new expiry; the old secret stops
    /// working at once. `None` when there is no such credential or it was revoked.
    #[instrument(skip(self, secret_hash))]
    pub async fn rotate_agent_credential(
        &self,
        id: Uuid,
        secret_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<Option<AgentCredential>, StoreError> {
        let changed = sqlx::query(
            "UPDATE agent_credentials SET secret_hash = ?, expires_at = ?, last_used_at = NULL
             WHERE id = ? AND revoked_at IS NULL",
        )
        .bind(secret_hash)
        .bind(expires_at.to_rfc3339())
        .bind(id.to_string())
        .execute(self.pool())
        .await?
        .rows_affected();
        if changed == 0 {
            return Ok(None);
        }
        self.get_agent_credential(id).await
    }

    /// Revokes a credential; revoking a revoked one keeps the first time. `None` when there is
    /// no such credential.
    #[instrument(skip(self))]
    pub async fn revoke_agent_credential(
        &self,
        id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Option<AgentCredential>, StoreError> {
        sqlx::query(
            "UPDATE agent_credentials SET revoked_at = COALESCE(revoked_at, ?) WHERE id = ?",
        )
        .bind(now.to_rfc3339())
        .bind(id.to_string())
        .execute(self.pool())
        .await?;
        self.get_agent_credential(id).await
    }

    /// Notes that the credential was used at `now`.
    pub async fn touch_agent_credential(
        &self,
        id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE agent_credentials SET last_used_at = ? WHERE id = ?")
            .bind(now.to_rfc3339())
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        Ok(())
    }
}

fn name_conflict(error: sqlx::Error) -> StoreError {
    match &error {
        sqlx::Error::Database(db) if db.is_unique_violation() => StoreError::Conflict(
            "another credential that is not revoked already has that name".into(),
        ),
        _ => StoreError::Sql(error),
    }
}
