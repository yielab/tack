//! The inbox (migration 105) and the deliveries of its rows to channels. Nothing here decides what
//! notifies or when a delivery is due again; `tack_core::notification` and `tack-app` do. The one
//! rule kept here is the one the index keeps: a situation (a [`Key`]) is open at most once.

use chrono::{DateTime, Utc};
use tack_core::notification::{Key, Notice, Notification};
use tracing::instrument;
use uuid::Uuid;

use super::Repository;
use crate::{StoreError, decode};

const TABLE: &str = "notifications";

const COLUMNS: &str = "id, project_id, kind, level, key, item_id, request_id, title, cause, \
     created_at, read_at, resolved_at";

/// What counts as open: a needs-you row until the fact that answers it, an info row until read.
const OPEN: &str = "resolved_at IS NULL AND (level = 'needs_you' OR read_at IS NULL)";

#[derive(sqlx::FromRow)]
pub(crate) struct NotificationRow {
    id: String,
    project_id: String,
    kind: String,
    level: String,
    key: String,
    item_id: Option<String>,
    request_id: Option<String>,
    title: String,
    cause: String,
    created_at: String,
    read_at: Option<String>,
    resolved_at: Option<String>,
}

impl NotificationRow {
    fn into_notification(self) -> Result<Notification, StoreError> {
        Ok(Notification {
            id: decode::uuid(TABLE, "id", &self.id)?,
            project_id: decode::uuid(TABLE, "project_id", &self.project_id)?,
            kind: decode::variant(TABLE, "kind", &self.kind)?,
            level: decode::variant(TABLE, "level", &self.level)?,
            key: Key::from_stored(self.key),
            item_id: decode::optional_uuid(TABLE, "item_id", self.item_id.as_deref())?,
            request_id: self.request_id,
            title: self.title,
            cause: self.cause,
            created_at: decode::instant(TABLE, "created_at", &self.created_at)?,
            read_at: decode::optional_instant(TABLE, "read_at", self.read_at.as_deref())?,
            resolved_at: decode::optional_instant(
                TABLE,
                "resolved_at",
                self.resolved_at.as_deref(),
            )?,
        })
    }
}

/// Which rows a listing wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Listing {
    /// What still asks something of the person.
    Open,
    /// Everything kept, newest first.
    All,
}

/// A delivery that is due: the notification and how often the channel has tried it.
#[derive(Debug, Clone)]
pub struct DueDelivery {
    pub notification: Notification,
    pub channel: String,
    pub attempts: u32,
}

#[derive(sqlx::FromRow)]
struct DueRow {
    #[sqlx(flatten)]
    notification: NotificationRow,
    channel: String,
    attempts: i64,
}

impl Repository {
    /// Opens `notice` unless its situation is open already, and queues it for each of `channels`,
    /// in one transaction. `None` when the key was open: nothing was written.
    #[instrument(skip(self, notice, channels), fields(key = %notice.key))]
    pub async fn insert_open_notification(
        &self,
        notice: &Notice,
        channels: &[String],
        now: DateTime<Utc>,
    ) -> Result<Option<Notification>, StoreError> {
        let mut tx = self.pool().begin().await?;
        let row = sqlx::query_as::<_, NotificationRow>(sqlx::AssertSqlSafe(format!(
            "INSERT INTO notifications (id, project_id, kind, level, key, item_id, request_id, title, cause, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(key) WHERE resolved_at IS NULL DO NOTHING
             RETURNING {COLUMNS}"
        )))
        .bind(Uuid::new_v4().to_string())
        .bind(notice.project_id.to_string())
        .bind(notice.kind.as_str())
        .bind(notice.level.as_str())
        .bind(notice.key.as_str())
        .bind(notice.item_id.map(|id| id.to_string()))
        .bind(notice.request_id.as_deref())
        .bind(&notice.title)
        .bind(&notice.cause)
        .bind(now.to_rfc3339())
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        for channel in channels {
            sqlx::query(
                "INSERT INTO notification_deliveries (notification_id, channel, next_at) VALUES (?, ?, ?)",
            )
            .bind(&row.id)
            .bind(channel)
            .bind(now.to_rfc3339())
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        row.into_notification().map(Some)
    }

    /// Closes the open notification of `key`; its id, or `None` when none was open.
    #[instrument(skip(self))]
    pub async fn close_notification_by_key(
        &self,
        key: &Key,
        now: DateTime<Utc>,
    ) -> Result<Option<Uuid>, StoreError> {
        let id: Option<String> = sqlx::query_scalar(
            "UPDATE notifications SET resolved_at = ? WHERE key = ? AND resolved_at IS NULL RETURNING id",
        )
        .bind(now.to_rfc3339())
        .bind(key.as_str())
        .fetch_optional(self.pool())
        .await?;
        id.map(|id| decode::uuid(TABLE, "id", &id)).transpose()
    }

    /// Marks one notification read. `false` when it is not there or was read already.
    #[instrument(skip(self))]
    pub async fn mark_notification_read(
        &self,
        id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<bool, StoreError> {
        let done =
            sqlx::query("UPDATE notifications SET read_at = ? WHERE id = ? AND read_at IS NULL")
                .bind(now.to_rfc3339())
                .bind(id.to_string())
                .execute(self.pool())
                .await?
                .rows_affected();
        Ok(done > 0)
    }

    /// Marks every unread notification read; the ids it marked.
    #[instrument(skip(self))]
    pub async fn mark_all_notifications_read(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<Uuid>, StoreError> {
        let ids: Vec<String> = sqlx::query_scalar(
            "UPDATE notifications SET read_at = ? WHERE read_at IS NULL RETURNING id",
        )
        .bind(now.to_rfc3339())
        .fetch_all(self.pool())
        .await?;
        ids.iter().map(|id| decode::uuid(TABLE, "id", id)).collect()
    }

    /// The notifications of `listing`, newest first, at most `limit`.
    #[instrument(skip(self))]
    pub async fn list_notifications(
        &self,
        listing: Listing,
        limit: u32,
    ) -> Result<Vec<Notification>, StoreError> {
        let filter = match listing {
            Listing::Open => OPEN,
            Listing::All => "1 = 1",
        };
        let rows = sqlx::query_as::<_, NotificationRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {COLUMNS} FROM notifications WHERE {filter} ORDER BY created_at DESC, id LIMIT ?"
        )))
        .bind(i64::from(limit))
        .fetch_all(self.pool())
        .await?;
        rows.into_iter()
            .map(NotificationRow::into_notification)
            .collect()
    }

    /// How many notifications are open.
    #[instrument(skip(self))]
    pub async fn count_open_notifications(&self) -> Result<u64, StoreError> {
        let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT COUNT(*) FROM notifications WHERE {OPEN}"
        )))
        .fetch_one(self.pool())
        .await?;
        Ok(u64::try_from(count).unwrap_or(0))
    }

    /// Deliveries whose time has come, oldest first, at most `limit`.
    #[instrument(skip(self))]
    pub async fn deliveries_due(
        &self,
        now: DateTime<Utc>,
        limit: u32,
    ) -> Result<Vec<DueDelivery>, StoreError> {
        let columns = COLUMNS
            .split(", ")
            .map(|column| format!("n.{column}"))
            .collect::<Vec<_>>()
            .join(", ");
        let rows = sqlx::query_as::<_, DueRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {columns}, d.channel, d.attempts
             FROM notification_deliveries d JOIN notifications n ON n.id = d.notification_id
             WHERE d.delivered_at IS NULL AND d.next_at IS NOT NULL AND d.next_at <= ?
             ORDER BY d.next_at, n.id LIMIT ?"
        )))
        .bind(now.to_rfc3339())
        .bind(i64::from(limit))
        .fetch_all(self.pool())
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(DueDelivery {
                    notification: row.notification.into_notification()?,
                    channel: row.channel,
                    attempts: u32::try_from(row.attempts).map_err(|e| {
                        StoreError::corrupt("notification_deliveries", "attempts", e)
                    })?,
                })
            })
            .collect()
    }

    /// The channel took the notification.
    #[instrument(skip(self))]
    pub async fn delivery_done(
        &self,
        notification: Uuid,
        channel: &str,
        now: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "UPDATE notification_deliveries
             SET attempts = attempts + 1, delivered_at = ?, next_at = NULL, last_error = NULL
             WHERE notification_id = ? AND channel = ?",
        )
        .bind(now.to_rfc3339())
        .bind(notification.to_string())
        .bind(channel)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// A try failed: `next_at` is the time of the next one, or `None` when there will be none.
    #[instrument(skip(self, error))]
    pub async fn delivery_failed(
        &self,
        notification: Uuid,
        channel: &str,
        next_at: Option<DateTime<Utc>>,
        error: &str,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "UPDATE notification_deliveries
             SET attempts = attempts + 1, next_at = ?, last_error = ?
             WHERE notification_id = ? AND channel = ?",
        )
        .bind(next_at.map(|at| at.to_rfc3339()))
        .bind(error)
        .bind(notification.to_string())
        .bind(channel)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Forgets what was settled before `cutoff`: a notification resolved by then, or an info one
    /// read by then, with its deliveries. A needs-you one that still waits stays, read or not.
    #[instrument(skip(self))]
    pub async fn purge_settled_notifications(
        &self,
        cutoff: DateTime<Utc>,
    ) -> Result<u64, StoreError> {
        Ok(sqlx::query(
            "DELETE FROM notifications
             WHERE COALESCE(resolved_at, CASE WHEN level = 'info' THEN read_at END) < ?",
        )
        .bind(cutoff.to_rfc3339())
        .execute(self.pool())
        .await?
        .rows_affected())
    }
}
