//! What a backup needs from the database: the install identity, the sync generation, a
//! consistent snapshot file, and the one place that strips secrets out of that file.

use std::path::Path;

use sqlx::ConnectOptions;
use sqlx::Connection;
use sqlx::sqlite::SqliteConnectOptions;
use tracing::warn;
use uuid::Uuid;

use super::Repository;
use crate::StoreError;

/// `app_meta` keys that must never leave the machine inside a backup: the S3
/// secret key is stored (JSON-encoded) under `backup_config`, and `install_id`
/// is this install's identity — restoring it elsewhere would clone the identity.
///
/// This list only covers `app_meta`. **This is not the only thing
/// [`scrub_snapshot_secrets`] scrubs** — any other table that grows a
/// secret-bearing column needs its own dedicated block in that function,
/// following the same null-before-VACUUM shape. Read that function's doc
/// comment before adding a new secret column anywhere in the schema.
const SENSITIVE_META_KEYS: &[&str] = &["backup_config", "install_id"];

impl Repository {
    /// This install's stable identifier (`app_meta.install_id`), created on first use.
    pub async fn install_id(&self) -> Result<String, StoreError> {
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let existing: Option<String> =
            sqlx::query_scalar("SELECT value FROM app_meta WHERE key = 'install_id'")
                .fetch_optional(&mut *tx)
                .await?;
        if let Some(id) = existing {
            return Ok(id);
        }
        let id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO app_meta (key, value) VALUES ('install_id', ?)")
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(id)
    }

    /// The monotonic sync generation (`app_meta.generation`); 0 on a brand-new install. The
    /// value is carried inside the database snapshot (it is not scrubbed), so a device that
    /// restores another device's bundle adopts that bundle's generation.
    pub async fn backup_generation(&self) -> Result<u64, StoreError> {
        let existing: Option<String> =
            sqlx::query_scalar("SELECT value FROM app_meta WHERE key = 'generation'")
                .fetch_optional(self.pool())
                .await?;
        Ok(existing.and_then(|s| s.parse::<u64>().ok()).unwrap_or(0))
    }

    /// Persist the sync generation.
    pub async fn set_backup_generation(&self, value: u64) -> Result<(), StoreError> {
        self.set_app_meta("generation", &value.to_string()).await
    }

    /// Keep `value` under `key` in `app_meta`, replacing what was there.
    pub async fn set_app_meta(&self, key: &str, value: &str) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO app_meta (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(key)
        .bind(value)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Write a consistent copy of the live database to `target` (which must not exist): the WAL
    /// is checkpointed into the main file, then `VACUUM INTO` writes the copy. The copy still
    /// holds every secret the live database does; pass it through
    /// [`scrub_snapshot_secrets`] before it goes anywhere.
    pub async fn snapshot_into(&self, target: &Path) -> Result<(), StoreError> {
        sqlx::query("PRAGMA wal_checkpoint(FULL)")
            .execute(self.pool())
            .await?;
        let target = target.to_string_lossy().replace('\'', "''");
        sqlx::query(sqlx::AssertSqlSafe(format!("VACUUM INTO '{target}'")))
            .execute(self.pool())
            .await?;
        Ok(())
    }
}

/// How many migrations the database file at `path` has applied, read-only. Used to refuse a
/// restore of a schema newer than the running binary.
pub async fn migration_count_of_file(path: &Path) -> Result<u32, StoreError> {
    let mut conn = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false)
        .read_only(true)
        .connect()
        .await?;
    let count: Result<i64, StoreError> = sqlx::query_scalar("SELECT COUNT(*) FROM _migrations")
        .fetch_one(&mut conn)
        .await
        .map_err(StoreError::from);
    conn.close().await.ok();
    Ok(count? as u32)
}

/// Strip machine-local secrets/identity from a freshly-created snapshot DB
/// file so they never ship inside a downloadable or uploadable bundle.
///
/// The single chokepoint for scrubbing backup secrets — every table with a
/// secret-bearing column must be handled here: today that is `app_meta`'s
/// [`SENSITIVE_META_KEYS`], deleted outright, and `projects.github_token_ref`
/// (a secret *reference*, never the token value, but still install-local —
/// see `tack_core::models::Project::github_token_ref`), nulled outright.
/// Removing `install_id` means a restore regenerates a fresh one via
/// [`Repository::install_id`] on first use, never adopting the source install's
/// identity.
///
/// **Add new secret columns here, not just to this doc comment**, and
/// before the trailing `VACUUM` so the freed bytes actually drop from the
/// file rather than sitting unreferenced in the freelist.
pub async fn scrub_snapshot_secrets(db_file: &Path) -> Result<(), StoreError> {
    let mut conn = SqliteConnectOptions::new()
        .filename(db_file)
        .create_if_missing(false)
        .connect()
        .await?;

    // The table may be absent on a brand-new DB — create defensively so the
    // DELETE below is always valid.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS app_meta (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL)",
    )
    .execute(&mut conn)
    .await?;

    for key in SENSITIVE_META_KEYS {
        sqlx::query("DELETE FROM app_meta WHERE key = ?")
            .bind(key)
            .execute(&mut conn)
            .await?;
    }

    // Unlike `app_meta` above, `projects` is a core table present in every
    // schema this binary understands, so a defensive `CREATE TABLE IF NOT
    // EXISTS` would be the wrong guard here (it would silently paper over a
    // snapshot missing a table it should always have). Instead this
    // tolerates its own query error — the smaller of the two guards the
    // doc comment above allows — and logs rather than fails the scrub.
    if let Err(error) = sqlx::query("UPDATE projects SET github_token_ref = NULL")
        .execute(&mut conn)
        .await
    {
        warn!(%error, "scrubbing projects.github_token_ref failed (table may be absent)");
    }

    // A plain DELETE/UPDATE leaves the secret bytes in freed/overwritten
    // pages (the SQLite freelist), so a hex-dump of the snapshot would still
    // reveal them. VACUUM rewrites the file and physically drops that
    // content — it must run after every scrub step above, not before.
    sqlx::query("VACUUM").execute(&mut conn).await?;

    conn.close().await?;
    Ok(())
}
