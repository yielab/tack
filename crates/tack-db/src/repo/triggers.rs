//! Triggers (migration 104): the rules that wake a profile, the events each has not read yet,
//! and the runs it has started. A trigger keeps one hidden item, the anchor its runs are made
//! against, as a thread does. Nothing here decides whether a trigger fires;
//! `tack_core::trigger` does, and the use cases in `tack-app` ask it.

use chrono::{DateTime, Utc};
use tack_core::thread::ThreadScope;
use tack_core::trigger::{EventKind, Happened, Source, Trigger};
use tracing::instrument;
use uuid::Uuid;

use super::Repository;
use crate::{StoreError, decode};

const TABLE: &str = "triggers";

const COLUMNS: &str = "id, project_id, credential_id, profile, name, source, quiet_seconds, \
     daily_cap, enabled, created_at, last_fired_at";

#[derive(sqlx::FromRow)]
struct TriggerRow {
    id: String,
    project_id: String,
    credential_id: Option<String>,
    profile: String,
    name: String,
    source: String,
    quiet_seconds: i64,
    daily_cap: i64,
    enabled: i64,
    created_at: String,
    last_fired_at: Option<String>,
}

impl TriggerRow {
    fn into_trigger(self) -> Result<Trigger, StoreError> {
        let home = decode::uuid(TABLE, "project_id", &self.project_id)?;
        let scope =
            match decode::optional_uuid(TABLE, "credential_id", self.credential_id.as_deref())? {
                Some(credential) => ThreadScope::Credential { credential, home },
                None => ThreadScope::Project { project: home },
            };
        Ok(Trigger {
            id: decode::uuid(TABLE, "id", &self.id)?,
            name: self.name,
            scope,
            profile: self.profile,
            source: decode::json::<Source>(TABLE, "source", &self.source)?,
            quiet_seconds: u32::try_from(self.quiet_seconds)
                .map_err(|e| StoreError::corrupt(TABLE, "quiet_seconds", e))?,
            daily_cap: u32::try_from(self.daily_cap)
                .map_err(|e| StoreError::corrupt(TABLE, "daily_cap", e))?,
            enabled: self.enabled != 0,
            created_at: decode::instant(TABLE, "created_at", &self.created_at)?,
            last_fired_at: decode::optional_instant(
                TABLE,
                "last_fired_at",
                self.last_fired_at.as_deref(),
            )?,
        })
    }
}

#[derive(sqlx::FromRow)]
struct EventRow {
    at: String,
    project_id: String,
    cause: String,
    kind: String,
    status: Option<String>,
    subject: String,
    detail: String,
}

impl EventRow {
    fn into_happened(self) -> Result<Happened, StoreError> {
        Ok(Happened {
            kind: decode::variant::<EventKind>("trigger_events", "kind", &self.kind)?,
            project: decode::uuid("trigger_events", "project_id", &self.project_id)?,
            cause: self.cause,
            at: decode::instant("trigger_events", "at", &self.at)?,
            status: self.status,
            subject: self.subject,
            detail: self.detail,
        })
    }
}

/// What a store call needs to make a trigger.
pub struct NewTrigger<'a> {
    pub scope: ThreadScope,
    pub profile: &'a str,
    pub name: &'a str,
    pub source: &'a Source,
    pub quiet_seconds: u32,
    pub daily_cap: u32,
    pub enabled: bool,
    /// The status the trigger's hidden item is kept in (the project's first).
    pub anchor_status: &'a str,
    pub now: DateTime<Utc>,
}

/// What an edit changes; a field left `None` stays.
#[derive(Default)]
pub struct TriggerChanges<'a> {
    pub name: Option<&'a str>,
    pub profile: Option<&'a str>,
    pub source: Option<&'a Source>,
    pub quiet_seconds: Option<u32>,
    pub daily_cap: Option<u32>,
    pub enabled: Option<bool>,
}

impl Repository {
    /// Makes a trigger and its hidden anchor item together.
    #[instrument(skip(self, new), fields(project = %new.scope.home()))]
    pub async fn create_trigger(&self, new: &NewTrigger<'_>) -> Result<Trigger, StoreError> {
        let (id, anchor) = (Uuid::new_v4(), Uuid::new_v4());
        let now = new.now.to_rfc3339();
        let mut tx = self.pool().begin().await?;
        sqlx::query(
            "INSERT INTO items (id, project_id, title, item_type, status, source, hidden, created_at, updated_at)
             VALUES (?, ?, ?, 'task', ?, 'manual', 1, ?, ?)",
        )
        .bind(anchor.to_string())
        .bind(new.scope.home().to_string())
        .bind(new.name)
        .bind(new.anchor_status)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO triggers (id, project_id, credential_id, profile, name, source, quiet_seconds,
                                   daily_cap, enabled, anchor_item_id, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(new.scope.home().to_string())
        .bind(new.scope.credential().map(|credential| credential.to_string()))
        .bind(new.profile)
        .bind(new.name)
        .bind(decode::to_json(new.source)?)
        .bind(i64::from(new.quiet_seconds))
        .bind(i64::from(new.daily_cap))
        .bind(i64::from(new.enabled))
        .bind(anchor.to_string())
        .bind(&now)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.get_trigger(id)
            .await?
            .ok_or(StoreError::not_found("trigger"))
    }

    #[instrument(skip(self))]
    pub async fn get_trigger(&self, id: Uuid) -> Result<Option<Trigger>, StoreError> {
        let row = sqlx::query_as::<_, TriggerRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {COLUMNS} FROM triggers WHERE id = ?"
        )))
        .bind(id.to_string())
        .fetch_optional(self.pool())
        .await?;
        row.map(TriggerRow::into_trigger).transpose()
    }

    /// The triggers whose home is `project` (every trigger when `None`), oldest first.
    #[instrument(skip(self))]
    pub async fn list_triggers(&self, project: Option<Uuid>) -> Result<Vec<Trigger>, StoreError> {
        let rows = sqlx::query_as::<_, TriggerRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {COLUMNS} FROM triggers WHERE (?1 IS NULL OR project_id = ?1)
             ORDER BY created_at, id"
        )))
        .bind(project.map(|project| project.to_string()))
        .fetch_all(self.pool())
        .await?;
        rows.into_iter().map(TriggerRow::into_trigger).collect()
    }

    /// Changes a trigger; `None` when it does not exist.
    #[instrument(skip(self, changes))]
    pub async fn update_trigger(
        &self,
        id: Uuid,
        changes: &TriggerChanges<'_>,
    ) -> Result<Option<Trigger>, StoreError> {
        let source = changes.source.map(decode::to_json).transpose()?;
        let done = sqlx::query(
            "UPDATE triggers SET
                 name = COALESCE(?, name),
                 profile = COALESCE(?, profile),
                 source = COALESCE(?, source),
                 quiet_seconds = COALESCE(?, quiet_seconds),
                 daily_cap = COALESCE(?, daily_cap),
                 enabled = COALESCE(?, enabled)
             WHERE id = ?",
        )
        .bind(changes.name)
        .bind(changes.profile)
        .bind(source)
        .bind(changes.quiet_seconds.map(i64::from))
        .bind(changes.daily_cap.map(i64::from))
        .bind(changes.enabled.map(i64::from))
        .bind(id.to_string())
        .execute(self.pool())
        .await?;
        if done.rows_affected() == 0 {
            return Ok(None);
        }
        self.get_trigger(id).await
    }

    /// Deletes a trigger with its events, its firings and its hidden item; `false` when it did
    /// not exist.
    #[instrument(skip(self))]
    pub async fn delete_trigger(&self, id: Uuid) -> Result<bool, StoreError> {
        let mut tx = self.pool().begin().await?;
        let anchor: Option<Option<String>> =
            sqlx::query_scalar("SELECT anchor_item_id FROM triggers WHERE id = ?")
                .bind(id.to_string())
                .fetch_optional(&mut *tx)
                .await?;
        let Some(anchor) = anchor else {
            return Ok(false);
        };
        for table in ["trigger_events", "trigger_firings"] {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "DELETE FROM {table} WHERE trigger_id = ?"
            )))
            .bind(id.to_string())
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query("DELETE FROM triggers WHERE id = ?")
            .bind(id.to_string())
            .execute(&mut *tx)
            .await?;
        if let Some(anchor) = anchor {
            sqlx::query("DELETE FROM items WHERE id = ? AND hidden = 1")
                .bind(anchor)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(true)
    }

    /// The hidden item a trigger's runs are made against.
    #[instrument(skip(self))]
    pub async fn trigger_anchor(&self, id: Uuid) -> Result<Option<Uuid>, StoreError> {
        let anchor: Option<Option<String>> =
            sqlx::query_scalar("SELECT anchor_item_id FROM triggers WHERE id = ?")
                .bind(id.to_string())
                .fetch_optional(self.pool())
                .await?;
        decode::optional_uuid(TABLE, "anchor_item_id", anchor.flatten().as_deref())
    }

    /// Whether the item is one a thread or a trigger keeps out of sight.
    #[instrument(skip(self))]
    pub async fn item_is_hidden(&self, item: Uuid) -> Result<bool, StoreError> {
        let hidden: Option<i64> = sqlx::query_scalar("SELECT hidden FROM items WHERE id = ?")
            .bind(item.to_string())
            .fetch_optional(self.pool())
            .await?;
        Ok(hidden.unwrap_or(0) != 0)
    }

    /// The ids of the threads whose profile is `profile`.
    #[instrument(skip(self))]
    pub async fn thread_ids_of_profile(&self, profile: &str) -> Result<Vec<Uuid>, StoreError> {
        let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM threads WHERE profile = ?")
            .bind(profile)
            .fetch_all(self.pool())
            .await?;
        ids.iter()
            .map(|id| decode::uuid("threads", "id", id))
            .collect()
    }

    /// The credential each of the profile's triggers works as.
    #[instrument(skip(self))]
    pub async fn credentials_of_profile_triggers(
        &self,
        profile: &str,
    ) -> Result<Vec<Uuid>, StoreError> {
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT credential_id FROM triggers WHERE profile = ? AND credential_id IS NOT NULL",
        )
        .bind(profile)
        .fetch_all(self.pool())
        .await?;
        ids.iter()
            .map(|id| decode::uuid(TABLE, "credential_id", id))
            .collect()
    }

    /// Keeps `happened` for the trigger's next run. `dedupe_key` makes a second event with the
    /// same key a no-op; the answer is whether the event was kept.
    #[instrument(skip(self, happened))]
    pub async fn record_trigger_event(
        &self,
        trigger: Uuid,
        happened: &Happened,
        dedupe_key: Option<&str>,
    ) -> Result<bool, StoreError> {
        let done = sqlx::query(
            "INSERT OR IGNORE INTO trigger_events
                 (trigger_id, at, project_id, cause, kind, status, subject, detail, dedupe_key)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(trigger.to_string())
        .bind(happened.at.to_rfc3339())
        .bind(happened.project.to_string())
        .bind(&happened.cause)
        .bind(happened.kind.as_str())
        .bind(&happened.status)
        .bind(&happened.subject)
        .bind(&happened.detail)
        .bind(dedupe_key)
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected() > 0)
    }

    /// The events the trigger's next run takes, oldest first.
    #[instrument(skip(self))]
    pub async fn pending_trigger_events(&self, trigger: Uuid) -> Result<Vec<Happened>, StoreError> {
        let rows = sqlx::query_as::<_, EventRow>(
            "SELECT at, project_id, cause, kind, status, subject, detail FROM trigger_events
             WHERE trigger_id = ? AND firing_id IS NULL ORDER BY at, id",
        )
        .bind(trigger.to_string())
        .fetch_all(self.pool())
        .await?;
        rows.into_iter().map(EventRow::into_happened).collect()
    }

    /// How many runs the trigger started since `since`.
    #[instrument(skip(self))]
    pub async fn trigger_firings_since(
        &self,
        trigger: Uuid,
        since: DateTime<Utc>,
    ) -> Result<usize, StoreError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM trigger_firings
             WHERE trigger_id = ? AND julianday(fired_at) >= julianday(?)",
        )
        .bind(trigger.to_string())
        .bind(since.to_rfc3339())
        .fetch_one(self.pool())
        .await?;
        Ok(usize::try_from(count).unwrap_or(0))
    }

    /// The run the trigger started last, if any.
    #[instrument(skip(self))]
    pub async fn last_trigger_request(&self, trigger: Uuid) -> Result<Option<String>, StoreError> {
        Ok(sqlx::query_scalar(
            "SELECT request_id FROM trigger_firings WHERE trigger_id = ?
             ORDER BY fired_at DESC, rowid DESC LIMIT 1",
        )
        .bind(trigger.to_string())
        .fetch_optional(self.pool())
        .await?)
    }

    /// Records that `request_id` is the run the trigger started, and that it took every event
    /// the trigger had kept until then.
    #[instrument(skip(self))]
    pub async fn record_trigger_firing(
        &self,
        trigger: Uuid,
        request_id: &str,
        now: DateTime<Utc>,
    ) -> Result<u32, StoreError> {
        let firing = Uuid::new_v4().to_string();
        let mut tx = self.pool().begin().await?;
        let taken = sqlx::query(
            "UPDATE trigger_events SET firing_id = ? WHERE trigger_id = ? AND firing_id IS NULL",
        )
        .bind(&firing)
        .bind(trigger.to_string())
        .execute(&mut *tx)
        .await?
        .rows_affected();
        let taken = u32::try_from(taken).unwrap_or(u32::MAX);
        sqlx::query(
            "INSERT INTO trigger_firings (id, trigger_id, request_id, fired_at, events)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&firing)
        .bind(trigger.to_string())
        .bind(request_id)
        .bind(now.to_rfc3339())
        .bind(i64::from(taken))
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE triggers SET last_fired_at = ? WHERE id = ?")
            .bind(now.to_rfc3339())
            .bind(trigger.to_string())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(taken)
    }

    /// Forgets the events and firings older than `cutoff`.
    #[instrument(skip(self))]
    pub async fn prune_trigger_events(&self, cutoff: DateTime<Utc>) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM trigger_events WHERE julianday(at) < julianday(?)")
            .bind(cutoff.to_rfc3339())
            .execute(self.pool())
            .await?;
        sqlx::query("DELETE FROM trigger_firings WHERE julianday(fired_at) < julianday(?)")
            .bind(cutoff.to_rfc3339())
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// The change sets that have waited for a person since before `cutoff`: id, project and the
    /// time each was proposed.
    #[instrument(skip(self))]
    pub async fn change_sets_waiting_since(
        &self,
        cutoff: DateTime<Utc>,
    ) -> Result<Vec<(Uuid, Uuid, DateTime<Utc>)>, StoreError> {
        let rows: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT id, project_id, created_at FROM change_sets
             WHERE status = 'waiting' AND julianday(created_at) <= julianday(?)
             ORDER BY created_at, id",
        )
        .bind(cutoff.to_rfc3339())
        .fetch_all(self.pool())
        .await?;
        rows.into_iter()
            .map(|(id, project, created)| {
                Ok((
                    decode::uuid("change_sets", "id", &id)?,
                    decode::uuid("change_sets", "project_id", &project)?,
                    decode::instant("change_sets", "created_at", &created)?,
                ))
            })
            .collect()
    }
}
