//! Threads (migration 102): a conversation with a profile, its messages, and the turns the
//! messages started. A thread keeps one hidden item, the anchor its turns run against (a run
//! belongs to an item); hidden items appear on no board, list or digest. Nothing here decides
//! whether a turn may start; `tack_core::thread` does, and the use cases in `tack-app` ask it.

use chrono::{DateTime, Utc};
use tack_core::thread::{Message, Ref, Thread, ThreadScope, Turn, TurnState};
use tracing::instrument;
use uuid::Uuid;

use super::Repository;
use crate::queries;
use crate::{StoreError, decode};

const TABLE: &str = "threads";

const COLUMNS: &str = "id, project_id, credential_id, profile, title, status, turns, closed_reason, \
     created_at, last_message_at";

#[derive(sqlx::FromRow)]
struct ThreadRow {
    id: String,
    project_id: String,
    credential_id: Option<String>,
    profile: String,
    title: String,
    status: String,
    turns: i64,
    closed_reason: Option<String>,
    created_at: String,
    last_message_at: String,
}

impl ThreadRow {
    fn into_thread(self) -> Result<Thread, StoreError> {
        let home = decode::uuid(TABLE, "project_id", &self.project_id)?;
        let scope =
            match decode::optional_uuid(TABLE, "credential_id", self.credential_id.as_deref())? {
                Some(credential) => ThreadScope::Credential { credential, home },
                None => ThreadScope::Project { project: home },
            };
        Ok(Thread {
            id: decode::uuid(TABLE, "id", &self.id)?,
            scope,
            profile: self.profile,
            title: self.title,
            status: decode::variant(TABLE, "status", &self.status)?,
            turns: u32::try_from(self.turns).map_err(|e| StoreError::corrupt(TABLE, "turns", e))?,
            closed_reason: self.closed_reason,
            created_at: decode::instant(TABLE, "created_at", &self.created_at)?,
            last_message_at: decode::instant(TABLE, "last_message_at", &self.last_message_at)?,
        })
    }
}

#[derive(sqlx::FromRow)]
struct MessageRow {
    id: String,
    thread_id: String,
    principal: String,
    body: String,
    refs: String,
    created_at: String,
}

impl MessageRow {
    fn into_message(self) -> Result<Message, StoreError> {
        Ok(Message {
            id: decode::uuid("thread_messages", "id", &self.id)?,
            thread: decode::uuid("thread_messages", "thread_id", &self.thread_id)?,
            principal: self.principal,
            body: self.body,
            refs: decode::json::<Vec<Ref>>("thread_messages", "refs", &self.refs)?,
            created_at: decode::instant("thread_messages", "created_at", &self.created_at)?,
        })
    }
}

#[derive(sqlx::FromRow)]
struct TurnRow {
    thread_id: String,
    position: i64,
    message_id: String,
    request_id: String,
    state: String,
    started_at: String,
    settled_at: Option<String>,
}

impl TurnRow {
    fn into_turn(self) -> Result<Turn, StoreError> {
        Ok(Turn {
            thread: decode::uuid("thread_turns", "thread_id", &self.thread_id)?,
            position: u32::try_from(self.position)
                .map_err(|e| StoreError::corrupt("thread_turns", "position", e))?,
            message: decode::uuid("thread_turns", "message_id", &self.message_id)?,
            request_id: self.request_id,
            state: decode::variant("thread_turns", "state", &self.state)?,
            started_at: decode::instant("thread_turns", "started_at", &self.started_at)?,
            settled_at: decode::optional_instant(
                "thread_turns",
                "settled_at",
                self.settled_at.as_deref(),
            )?,
        })
    }
}

/// What a store call needs to open a thread.
pub struct NewThread<'a> {
    pub scope: ThreadScope,
    pub profile: &'a str,
    pub title: &'a str,
    /// The status the thread's hidden item is kept in (the project's first).
    pub anchor_status: &'a str,
    pub now: DateTime<Utc>,
}

impl Repository {
    /// Opens a thread and its hidden anchor item together.
    #[instrument(skip(self, new), fields(project = %new.scope.home()))]
    pub async fn create_thread(&self, new: &NewThread<'_>) -> Result<Thread, StoreError> {
        let (id, anchor) = (Uuid::new_v4(), Uuid::new_v4());
        let now = new.now.to_rfc3339();
        let mut tx = self.pool().begin().await?;
        let item_type = queries::item_types::default_work_key(&mut tx, new.scope.home()).await?;
        sqlx::query(
            "INSERT INTO items (id, project_id, title, item_type, status, source, hidden, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, 'manual', 1, ?, ?)",
        )
        .bind(anchor.to_string())
        .bind(new.scope.home().to_string())
        .bind(new.title)
        .bind(&item_type)
        .bind(new.anchor_status)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO threads (id, project_id, credential_id, profile, title, status, anchor_item_id,
                                  created_at, last_message_at)
             VALUES (?, ?, ?, ?, ?, 'open', ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(new.scope.home().to_string())
        .bind(new.scope.credential().map(|credential| credential.to_string()))
        .bind(new.profile)
        .bind(new.title)
        .bind(anchor.to_string())
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.get_thread(id)
            .await?
            .ok_or(StoreError::not_found("thread"))
    }

    #[instrument(skip(self))]
    pub async fn get_thread(&self, id: Uuid) -> Result<Option<Thread>, StoreError> {
        let row = sqlx::query_as::<_, ThreadRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {COLUMNS} FROM threads WHERE id = ?"
        )))
        .bind(id.to_string())
        .fetch_optional(self.pool())
        .await?;
        row.map(ThreadRow::into_thread).transpose()
    }

    /// The threads whose home project is one of `projects` (every thread when `None`), newest
    /// activity first, and only the open ones when `open_only`.
    #[instrument(skip(self, projects))]
    pub async fn list_threads(
        &self,
        projects: Option<&[Uuid]>,
        open_only: bool,
    ) -> Result<Vec<Thread>, StoreError> {
        let listed = projects.map(|projects| {
            serde_json::json!(
                projects
                    .iter()
                    .map(|project| project.to_string())
                    .collect::<Vec<_>>()
            )
            .to_string()
        });
        let rows = sqlx::query_as::<_, ThreadRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {COLUMNS} FROM threads
             WHERE (?1 IS NULL OR project_id IN (SELECT value FROM json_each(?1)))
               AND (?2 = 0 OR status = 'open')
             ORDER BY last_message_at DESC, id"
        )))
        .bind(listed)
        .bind(i64::from(open_only))
        .fetch_all(self.pool())
        .await?;
        rows.into_iter().map(ThreadRow::into_thread).collect()
    }

    /// The hidden item a thread's turns run against.
    #[instrument(skip(self))]
    pub async fn thread_anchor(&self, id: Uuid) -> Result<Option<Uuid>, StoreError> {
        let anchor: Option<Option<String>> =
            sqlx::query_scalar("SELECT anchor_item_id FROM threads WHERE id = ?")
                .bind(id.to_string())
                .fetch_optional(self.pool())
                .await?;
        decode::optional_uuid(TABLE, "anchor_item_id", anchor.flatten().as_deref())
    }

    /// Rewrites what the anchor item says, which is what the next turn's run is given.
    #[instrument(skip(self, description))]
    pub async fn set_anchor_text(
        &self,
        anchor: Uuid,
        description: &str,
        now: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE items SET description = ?, updated_at = ? WHERE id = ? AND hidden = 1")
            .bind(description)
            .bind(now.to_rfc3339())
            .bind(anchor.to_string())
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Appends a message to its thread, numbered after the last, and moves the thread's
    /// latest-message time to `now`.
    #[instrument(skip(self, body, refs))]
    pub async fn append_thread_message(
        &self,
        thread: Uuid,
        principal: &str,
        body: &str,
        refs: &[Ref],
        now: DateTime<Utc>,
    ) -> Result<Message, StoreError> {
        let id = Uuid::new_v4();
        let mut tx = self.pool().begin().await?;
        let position: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(position), 0) + 1 FROM thread_messages WHERE thread_id = ?",
        )
        .bind(thread.to_string())
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO thread_messages (id, thread_id, position, principal, body, refs, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(thread.to_string())
        .bind(position)
        .bind(principal)
        .bind(body)
        .bind(decode::to_json(refs)?)
        .bind(now.to_rfc3339())
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE threads SET last_message_at = ? WHERE id = ?")
            .bind(now.to_rfc3339())
            .bind(thread.to_string())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Message {
            id,
            thread,
            principal: principal.to_owned(),
            body: body.to_owned(),
            refs: refs.to_vec(),
            created_at: now,
        })
    }

    /// The thread's messages, oldest first.
    #[instrument(skip(self))]
    pub async fn list_thread_messages(&self, thread: Uuid) -> Result<Vec<Message>, StoreError> {
        let rows = sqlx::query_as::<_, MessageRow>(
            "SELECT id, thread_id, principal, body, refs, created_at FROM thread_messages
             WHERE thread_id = ? ORDER BY position",
        )
        .bind(thread.to_string())
        .fetch_all(self.pool())
        .await?;
        rows.into_iter().map(MessageRow::into_message).collect()
    }

    /// Records that `message` started a run, as the thread's next turn.
    #[instrument(skip(self))]
    pub async fn start_thread_turn(
        &self,
        thread: Uuid,
        message: Uuid,
        request_id: &str,
        now: DateTime<Utc>,
    ) -> Result<Turn, StoreError> {
        let mut tx = self.pool().begin().await?;
        let position: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(position), 0) + 1 FROM thread_turns WHERE thread_id = ?",
        )
        .bind(thread.to_string())
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO thread_turns (thread_id, position, message_id, request_id, state, started_at)
             VALUES (?, ?, ?, ?, 'pending', ?)",
        )
        .bind(thread.to_string())
        .bind(position)
        .bind(message.to_string())
        .bind(request_id)
        .bind(now.to_rfc3339())
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE threads SET turns = turns + 1 WHERE id = ?")
            .bind(thread.to_string())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Turn {
            thread,
            position: u32::try_from(position).unwrap_or(u32::MAX),
            message,
            request_id: request_id.to_owned(),
            state: TurnState::Pending,
            started_at: now,
            settled_at: None,
        })
    }

    /// The thread's turns, oldest first.
    #[instrument(skip(self))]
    pub async fn list_thread_turns(&self, thread: Uuid) -> Result<Vec<Turn>, StoreError> {
        let rows = sqlx::query_as::<_, TurnRow>(
            "SELECT thread_id, position, message_id, request_id, state, started_at, settled_at
             FROM thread_turns WHERE thread_id = ? ORDER BY position",
        )
        .bind(thread.to_string())
        .fetch_all(self.pool())
        .await?;
        rows.into_iter().map(TurnRow::into_turn).collect()
    }

    /// Every turn whose run has not been answered for yet, oldest first.
    #[instrument(skip(self))]
    pub async fn running_thread_turns(&self) -> Result<Vec<Turn>, StoreError> {
        let rows = sqlx::query_as::<_, TurnRow>(
            "SELECT thread_id, position, message_id, request_id, state, started_at, settled_at
             FROM thread_turns WHERE state = 'pending' ORDER BY started_at, thread_id, position",
        )
        .fetch_all(self.pool())
        .await?;
        rows.into_iter().map(TurnRow::into_turn).collect()
    }

    /// Ends a running turn, once: `false` when it was not running.
    #[instrument(skip(self))]
    pub async fn settle_thread_turn(
        &self,
        thread: Uuid,
        position: u32,
        state: TurnState,
        now: DateTime<Utc>,
    ) -> Result<bool, StoreError> {
        let done = sqlx::query(
            "UPDATE thread_turns SET state = ?, settled_at = ?
             WHERE thread_id = ? AND position = ? AND state = 'pending'",
        )
        .bind(state.as_str())
        .bind(now.to_rfc3339())
        .bind(thread.to_string())
        .bind(i64::from(position))
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected() == 1)
    }

    /// Closes an open thread with the reason; `false` when it was not open.
    #[instrument(skip(self))]
    pub async fn close_thread(&self, id: Uuid, reason: &str) -> Result<bool, StoreError> {
        let done = sqlx::query(
            "UPDATE threads SET status = 'closed', closed_reason = ? WHERE id = ? AND status = 'open'",
        )
        .bind(reason)
        .bind(id.to_string())
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected() == 1)
    }

    /// Closes every open thread whose latest message is older than `cutoff` and has no turn
    /// running; the ids it closed.
    #[instrument(skip(self))]
    pub async fn close_idle_threads(&self, cutoff: DateTime<Utc>) -> Result<Vec<Uuid>, StoreError> {
        let ids: Vec<String> = sqlx::query_scalar(
            "UPDATE threads SET status = 'closed', closed_reason = 'idle'
             WHERE status = 'open' AND julianday(last_message_at) <= julianday(?)
               AND NOT EXISTS (SELECT 1 FROM thread_turns t WHERE t.thread_id = threads.id AND t.state = 'pending')
             RETURNING id",
        )
        .bind(cutoff.to_rfc3339())
        .fetch_all(self.pool())
        .await?;
        ids.iter().map(|id| decode::uuid(TABLE, "id", id)).collect()
    }

    /// The change sets made with the thread as their origin (its id, or `thread:<id>`) at or
    /// after `since`, oldest first.
    #[instrument(skip(self))]
    pub async fn change_sets_of_thread(
        &self,
        thread: Uuid,
        since: DateTime<Utc>,
    ) -> Result<Vec<Uuid>, StoreError> {
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM change_sets
             WHERE origin = 'thread' AND origin_ref IN (?, ?)
               AND julianday(created_at) >= julianday(?)
             ORDER BY created_at, id",
        )
        .bind(thread.to_string())
        .bind(tack_core::thread::turn_principal(thread))
        .bind(since.to_rfc3339())
        .fetch_all(self.pool())
        .await?;
        ids.iter()
            .map(|id| decode::uuid("change_sets", "id", id))
            .collect()
    }
}
