use crate::repo::execution::state;
use crate::repo::{Parent, require_row};
use crate::{StoreError, decode};
use chrono::Utc;
use sqlx::{AssertSqlSafe, QueryBuilder, Sqlite, SqliteConnection};
use tracing::{debug, instrument, warn};
use uuid::Uuid;

use tack_core::CoreError;
use tack_core::models::{
    CreateItem, DependencyType, Item, ItemFilter, ItemSource, ItemType, UpdateItem,
};
use tack_core::outline::{PlannedParent, ResolvedOutline};
use tack_core::workflow::{StatusCategory, WorkflowConfig};

use super::Repository;
use super::dependencies::insert_dependency;

/// The `needs_review` column of an `items` row: the latest terminal attempt of
/// the item's latest execution request left a non-empty patch or a kept
/// workspace, and nobody has reviewed it yet.
pub(crate) fn needs_review_sql() -> String {
    state::expand(
        "EXISTS (
    SELECT 1 FROM execution_attempts a
    LEFT JOIN attempt_reviews rv ON rv.attempt_id = a.id
    WHERE a.id = (
        SELECT a2.id FROM execution_attempts a2
        WHERE a2.request_id = (
            SELECT r.id FROM execution_requests r
            WHERE r.item_id = items.id ORDER BY r.created_at DESC, r.rowid DESC LIMIT 1
        ) AND a2.state IN ({terminal})
        ORDER BY a2.attempt_number DESC LIMIT 1
    )
    AND rv.attempt_id IS NULL
    AND (
        json_extract(a.terminal_reason, '$.workspace_kept_at') IS NOT NULL
        OR EXISTS (
            SELECT 1 FROM json_each(json_extract(a.terminal_reason, '$.artifacts')) art
            WHERE json_extract(art.value, '$.kind') = 'patch'
              AND json_extract(art.value, '$.size_bytes') > 0
        )
    )
)",
    )
}

/// The `merge_note` of an `items` row: why the item's latest merge was held,
/// while it has not merged.
pub(crate) const MERGE_NOTE_SQL: &str = "(SELECT CASE WHEN t.merged_at IS NULL THEN t.held_reason END FROM tack_merges t JOIN execution_attempts a ON a.id = t.attempt_id JOIN execution_requests r ON r.id = a.request_id WHERE r.item_id = items.id ORDER BY t.checked_at DESC LIMIT 1)";

/// The `started_by` of an `items` row: `metadata.started_by` of the newest request.
pub(crate) const STARTED_BY_SQL: &str = "(SELECT json_extract(r.metadata, '$.started_by') FROM execution_requests r WHERE r.item_id = items.id ORDER BY r.created_at DESC, r.rowid DESC LIMIT 1)";

/// The `team` of an `items` row: the docket team in the newest request's resolved profile.
pub(crate) const TEAM_SQL: &str = "(SELECT json_extract(r.request_snapshot, '$.resolved_agent_profile.tool_policy.docket.team') FROM execution_requests r WHERE r.item_id = items.id ORDER BY r.created_at DESC, r.rowid DESC LIMIT 1)";

/// The `merged_by` of an `items` row: the trigger (`button`, `done` or `auto`) of the newest
/// attempt whose merge happened.
pub(crate) const MERGED_BY_SQL: &str = "(SELECT t.trigger FROM tack_merges t JOIN execution_attempts a ON a.id = t.attempt_id JOIN execution_requests r ON r.id = a.request_id WHERE r.item_id = items.id AND t.merged_at IS NOT NULL ORDER BY a.created_at DESC, a.rowid DESC LIMIT 1)";

/// Result of one complete item PATCH.  Unlike the older helpers this owns the
/// WIP decision, field update, timestamps, and version increment in one
/// transaction, so callers cannot accidentally compose partial mutations.
#[derive(Debug)]
pub enum AtomicItemUpdateOutcome {
    Updated {
        item: Box<Item>,
        version: i64,
        old_status: String,
    },
    NotFound,
    PreconditionFailed,
    Rejected(CoreError),
}

/// A read snapshot whose item and version were observed in the same SQLite
/// transaction.  This is the only safe source for an item response plus ETag.
#[derive(Debug)]
pub struct ItemSnapshot {
    pub item: Item,
    pub version: i64,
}

impl Repository {
    /// Create an item via the ordinary path — always `ItemSource::Manual`
    /// (the operator's own words, typed or spoken by them: the UI, `tack
    /// add` and the MCP tool all go through this).
    /// External-data import paths must call
    /// [`create_item_with_source`](Self::create_item_with_source) instead so
    /// the item's provenance is recorded truthfully
    /// and `tack_core::models::ItemSource`.
    #[instrument(skip(self))]
    pub async fn create_item(
        &self,
        project_id: Uuid,
        initial_status: &str,
        input: CreateItem,
    ) -> Result<Item, StoreError> {
        self.create_item_with_source(project_id, initial_status, input, ItemSource::Manual)
            .await
    }

    /// Create an item recording an explicit provenance `source`. Every
    /// import path (GitHub, Linear, JSON/YAML project import, CSV import)
    /// calls this directly with its own [`ItemSource`] variant instead of
    /// [`create_item`](Self::create_item), which is hardcoded to `Manual`.
    /// `source` is written once, here, and nowhere else — `update_item` has
    /// no code path that touches this column, which is what makes the trust
    /// marker sticky for the lifetime of the item.
    #[instrument(skip(self))]
    pub async fn create_item_with_source(
        &self,
        project_id: Uuid,
        initial_status: &str,
        input: CreateItem,
        source: ItemSource,
    ) -> Result<Item, StoreError> {
        let mut conn = self.pool().acquire().await?;
        insert_item(&mut conn, project_id, initial_status, input, source).await
    }

    /// Write a resolved outline's items, parents first, and its dependencies
    /// in one transaction: either the whole outline lands or none of it does.
    /// Returns the items in written order and how many dependencies were
    /// recorded. `source` is the outline's provenance, as for
    /// [`create_item_with_source`](Self::create_item_with_source).
    #[instrument(skip(self, outline), fields(items = outline.items.len()))]
    pub async fn create_outline_items(
        &self,
        project_id: Uuid,
        outline: ResolvedOutline,
        source: ItemSource,
    ) -> Result<(Vec<Item>, usize), StoreError> {
        let mut tx = self.pool().begin().await?;
        let mut created: Vec<Item> = Vec::with_capacity(outline.items.len());
        for planned in outline.items {
            let mut input = planned.input;
            input.parent_id = planned.parent.map(|parent| match parent {
                PlannedParent::Planned(i) => created[i].id,
                PlannedParent::Existing(id) => id,
            });
            let item =
                insert_item(&mut tx, project_id, &planned.status, input, source.clone()).await?;
            created.push(item);
        }
        for &(blocker, blocked) in &outline.blocks {
            insert_dependency(
                &mut tx,
                created[blocker].id,
                created[blocked].id,
                &DependencyType::Blocks,
            )
            .await?;
        }
        tx.commit().await?;
        Ok((created, outline.blocks.len()))
    }

    #[instrument(skip(self))]
    pub async fn get_item(&self, id: Uuid) -> Result<Option<Item>, StoreError> {
        let needs_review = needs_review_sql();
        let row = sqlx::query_as::<_, ItemRow>(AssertSqlSafe(format!(
            "SELECT id, project_id, parent_id, title, description, item_type, status, priority, estimate, estimate_unit, tags, sort_order, sprint_id, assignee, due_date, recurrence, recurrence_of, source, source_artifact_id, run_settings, started_at, completed_at, created_at, updated_at, {needs_review} AS needs_review, {MERGE_NOTE_SQL} AS merge_note, {STARTED_BY_SQL} AS started_by, {TEAM_SQL} AS team, {MERGED_BY_SQL} AS merged_by
             FROM items WHERE id = ?"
        )))
        .bind(id.to_string())
        .fetch_optional(self.pool())
        .await?;

        row.map(ItemRow::into_item).transpose()
    }

    /// The optimistic-concurrency counter an `ETag` is derived from
    /// (migration 034). A
    /// dedicated read rather than folding `version` into [`get_item`](Self::get_item)'s
    /// `Item`-shaped query: `tack_core::models::Item` has no `version` field,
    /// so a caller that needs the counter asks for it separately rather than
    /// widening the domain model for one internal reader.
    #[instrument(skip(self))]
    pub async fn get_item_version(&self, id: Uuid) -> Result<Option<i64>, StoreError> {
        sqlx::query_scalar("SELECT version FROM items WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(self.pool())
            .await
            .map_err(StoreError::from)
    }

    /// Read the body and ETag counter from one database snapshot.  Two plain
    /// reads can otherwise observe different writers and send a body whose
    /// ETag describes a later version.
    #[instrument(skip(self))]
    pub async fn get_item_snapshot(&self, id: Uuid) -> Result<Option<ItemSnapshot>, StoreError> {
        let mut tx = self.pool().begin().await?;
        let needs_review = needs_review_sql();
        let row = sqlx::query_as::<_, ItemRow>(AssertSqlSafe(format!(
            "SELECT id, project_id, parent_id, title, description, item_type, status, priority, estimate, estimate_unit, tags, sort_order, sprint_id, assignee, due_date, recurrence, recurrence_of, source, source_artifact_id, run_settings, started_at, completed_at, created_at, updated_at, {needs_review} AS needs_review, {MERGE_NOTE_SQL} AS merge_note, {STARTED_BY_SQL} AS started_by, {TEAM_SQL} AS team, {MERGED_BY_SQL} AS merged_by FROM items WHERE id = ?"
        )))
        .bind(id.to_string())
        .fetch_optional(&mut *tx)
        .await?;
        let version = sqlx::query_scalar("SELECT version FROM items WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&mut *tx)
            .await?;
        tx.commit().await?;
        row.zip(version)
            .map(|(row, version)| {
                Ok(ItemSnapshot {
                    item: row.into_item()?,
                    version,
                })
            })
            .transpose()
    }

    #[instrument(skip(self))]
    pub async fn list_items(
        &self,
        project_id: Uuid,
        filter: &ItemFilter,
    ) -> Result<Vec<Item>, StoreError> {
        let (where_clause, binds) = item_filter_clause(project_id, filter);
        let needs_review = needs_review_sql();
        let mut query = format!(
            "SELECT id, project_id, parent_id, title, description, item_type, status, priority, estimate, estimate_unit, tags, sort_order, sprint_id, assignee, due_date, recurrence, recurrence_of, source, source_artifact_id, run_settings, started_at, completed_at, created_at, updated_at, {needs_review} AS needs_review, {MERGE_NOTE_SQL} AS merge_note, {STARTED_BY_SQL} AS started_by, {TEAM_SQL} AS team, {MERGED_BY_SQL} AS merged_by
             FROM items{where_clause} ORDER BY sort_order ASC"
        );

        let per_page = filter.effective_per_page() as i64;
        let page = filter.effective_page() as i64;
        let offset = (page - 1) * per_page;
        query.push_str(&format!(" LIMIT {per_page} OFFSET {offset}"));

        // Build the query dynamically
        let mut q = sqlx::query_as::<_, ItemRow>(AssertSqlSafe(query));
        for bind in &binds {
            q = q.bind(bind);
        }

        let rows = q.fetch_all(self.pool()).await?;
        rows.into_iter().map(ItemRow::into_item).collect()
    }

    /// Total number of items matching `filter` (ignoring pagination). Used to
    /// build the `{ data, total, page, per_page }` list envelope so clients can
    /// page through everything instead of silently truncating at one page.
    #[instrument(skip(self))]
    pub async fn count_items(
        &self,
        project_id: Uuid,
        filter: &ItemFilter,
    ) -> Result<i64, StoreError> {
        let (where_clause, binds) = item_filter_clause(project_id, filter);
        let query = format!("SELECT COUNT(*) FROM items{where_clause}");
        let mut q = sqlx::query_scalar::<_, i64>(AssertSqlSafe(query));
        for bind in &binds {
            q = q.bind(bind);
        }
        q.fetch_one(self.pool()).await.map_err(StoreError::from)
    }

    /// Return all incomplete items whose `due_date` falls in `[from, to)`.
    /// Used by the background webhook task to fire `item.due_soon` events.
    pub async fn list_items_due_soon(
        &self,
        from: chrono::DateTime<chrono::Utc>,
        to: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<Item>, crate::StoreError> {
        let rows = sqlx::query_as::<_, ItemRow>(
            "SELECT id, project_id, parent_id, title, description, item_type, status, priority, estimate, estimate_unit, tags, sort_order, sprint_id, assignee, due_date, recurrence, recurrence_of, source, source_artifact_id, run_settings, started_at, completed_at, created_at, updated_at
             FROM items
             WHERE due_date IS NOT NULL
               AND due_date >= ?
               AND due_date < ?
               AND completed_at IS NULL
             ORDER BY due_date ASC",
        )
        .bind(from.to_rfc3339())
        .bind(to.to_rfc3339())
        .fetch_all(self.pool())
        .await?;
        rows.into_iter().map(ItemRow::into_item).collect()
    }

    /// Every branch below bumps `version`, not just the
    /// obvious one — a counter that only moves on the common path is worse
    /// than none, because a caller would trust a stale `ETag` computed from
    /// a version this method silently left behind. See
    /// `crates/tack-db/tests/repository/version_concurrency.rs`'s
    /// `version_increments_on_every_item_update`.
    #[instrument(skip(self))]
    pub async fn update_item(
        &self,
        id: Uuid,
        input: UpdateItem,
    ) -> Result<Option<Item>, StoreError> {
        let now = Utc::now().to_rfc3339();

        if let Some(Some(sprint_id)) = input.sprint_id {
            self.require_sprint(sprint_id).await?;
        }
        if let Some(ref title) = input.title {
            sqlx::query(
                "UPDATE items SET title = ?, updated_at = ?, version = version + 1 WHERE id = ?",
            )
            .bind(title)
            .bind(&now)
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        }
        if let Some(description) = input.description {
            sqlx::query(
                "UPDATE items SET description = ?, updated_at = ?, version = version + 1 WHERE id = ?",
            )
            .bind(description)
            .bind(&now)
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        }
        if let Some(ref status) = input.status {
            sqlx::query(
                "UPDATE items SET status = ?, updated_at = ?, version = version + 1 WHERE id = ?",
            )
            .bind(status)
            .bind(&now)
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        }
        if let Some(ref priority) = input.priority {
            sqlx::query(
                "UPDATE items SET priority = ?, updated_at = ?, version = version + 1 WHERE id = ?",
            )
            .bind(priority.to_string())
            .bind(&now)
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        }
        if let Some(ref item_type) = input.item_type {
            sqlx::query(
                "UPDATE items SET item_type = ?, updated_at = ?, version = version + 1 WHERE id = ?",
            )
            .bind(item_type.to_string())
            .bind(&now)
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        }
        if let Some(estimate) = input.estimate {
            sqlx::query(
                "UPDATE items SET estimate = ?, updated_at = ?, version = version + 1 WHERE id = ?",
            )
            .bind(estimate)
            .bind(&now)
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        }
        if let Some(ref tags) = input.tags {
            let tags_json = decode::to_json(tags)?;
            sqlx::query(
                "UPDATE items SET tags = ?, updated_at = ?, version = version + 1 WHERE id = ?",
            )
            .bind(&tags_json)
            .bind(&now)
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        }
        if let Some(sort_order) = input.sort_order {
            sqlx::query(
                "UPDATE items SET sort_order = ?, updated_at = ?, version = version + 1 WHERE id = ?",
            )
            .bind(sort_order)
            .bind(&now)
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        }
        if let Some(assignee) = input.assignee {
            sqlx::query(
                "UPDATE items SET assignee = ?, updated_at = ?, version = version + 1 WHERE id = ?",
            )
            .bind(assignee)
            .bind(&now)
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        }
        // Double-`Option` fields: outer `Some` means "the client addressed this
        // field", inner `Option` carries the new value (`None` ⇒ clear to NULL).
        if let Some(sprint_id) = input.sprint_id {
            sqlx::query(
                "UPDATE items SET sprint_id = ?, updated_at = ?, version = version + 1 WHERE id = ?",
            )
            .bind(sprint_id.map(|s| s.to_string()))
            .bind(&now)
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        }
        if let Some(due_date) = input.due_date {
            sqlx::query(
                "UPDATE items SET due_date = ?, updated_at = ?, version = version + 1 WHERE id = ?",
            )
            .bind(due_date.map(|d| d.to_rfc3339()))
            .bind(&now)
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        }
        if let Some(recurrence) = input.recurrence {
            sqlx::query(
                "UPDATE items SET recurrence = ?, updated_at = ?, version = version + 1 WHERE id = ?",
            )
            .bind(recurrence.map(|r| r.to_string()))
            .bind(&now)
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        }
        if let Some(estimate_unit) = input.estimate_unit {
            // `estimate_unit` is stored as a JSON string; `None` clears it to SQL NULL.
            let value = estimate_unit.map(|u| decode::to_json(&u)).transpose()?;
            sqlx::query(
                "UPDATE items SET estimate_unit = ?, updated_at = ?, version = version + 1 WHERE id = ?",
            )
            .bind(value)
            .bind(&now)
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        }

        if let Some(run_settings) = input.run_settings {
            sqlx::query(
                "UPDATE items SET run_settings = ?, updated_at = ?, version = version + 1 WHERE id = ?",
            )
            .bind(run_settings.and_then(|v| serde_json::to_string(&v).ok()))
            .bind(&now)
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        }

        // Maintain started_at / completed_at from the status category the handler
        // resolved for the target status. Only runs when the status is changing.
        if let Some(category) = input.status_category {
            match category {
                // Entering in-progress: stamp the first start, and (if we came
                // back out of Done) clear the completion timestamp.
                StatusCategory::InProgress => {
                    sqlx::query(
                        "UPDATE items SET started_at = COALESCE(started_at, ?), completed_at = NULL, updated_at = ?, version = version + 1 WHERE id = ?",
                    )
                    .bind(&now)
                    .bind(&now)
                    .bind(id.to_string())
                    .execute(self.pool())
                    .await?;
                }
                // Entering Done: record completion (keep an earlier value if set).
                StatusCategory::Done => {
                    sqlx::query(
                        "UPDATE items SET completed_at = COALESCE(completed_at, ?), updated_at = ?, version = version + 1 WHERE id = ?",
                    )
                    .bind(&now)
                    .bind(&now)
                    .bind(id.to_string())
                    .execute(self.pool())
                    .await?;
                }
                // Leaving Done back to a Todo column: drop the completion stamp.
                StatusCategory::Todo => {
                    sqlx::query(
                        "UPDATE items SET completed_at = NULL, updated_at = ?, version = version + 1 WHERE id = ?",
                    )
                    .bind(&now)
                    .bind(id.to_string())
                    .execute(self.pool())
                    .await?;
                }
            }
        }

        self.get_item(id).await
    }

    /// Apply a browser/API PATCH as one all-or-nothing mutation.  The
    /// `BEGIN IMMEDIATE` lock covers the WIP count and the conditional update;
    /// the generated SQL changes every addressed column plus timestamps and
    /// version exactly once.
    #[instrument(skip(self, input, workflow))]
    pub async fn update_item_atomically(
        &self,
        id: Uuid,
        input: UpdateItem,
        workflow: &WorkflowConfig,
        expected_version: Option<i64>,
    ) -> Result<AtomicItemUpdateOutcome, StoreError> {
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let outcome = update_item_in(&mut tx, id, input, workflow, expected_version).await?;
        if matches!(outcome, AtomicItemUpdateOutcome::Updated { .. }) {
            tx.commit().await?;
        } else {
            tx.rollback().await?;
        }
        Ok(outcome)
    }

    #[instrument(skip(self))]
    pub async fn delete_item(&self, id: Uuid) -> Result<bool, StoreError> {
        let result = sqlx::query("DELETE FROM items WHERE id = ?")
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        Ok(result.rows_affected() > 0)
    }

    #[instrument(skip(self))]
    pub async fn count_items_by_status(
        &self,
        project_id: Uuid,
        status: &str,
    ) -> Result<i64, StoreError> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM items WHERE project_id = ? AND status = ?")
                .bind(project_id.to_string())
                .bind(status)
                .fetch_one(self.pool())
                .await?;
        Ok(count)
    }

    #[instrument(skip(self))]
    pub async fn search_items(
        &self,
        project_id: Uuid,
        query: &str,
    ) -> Result<Vec<Item>, StoreError> {
        let rows = sqlx::query_as::<_, ItemRow>(
            "SELECT i.id, i.project_id, i.parent_id, i.title, i.description, i.item_type, i.status, i.priority, i.estimate, i.estimate_unit, i.tags, i.sort_order, i.sprint_id, i.assignee, i.due_date, i.recurrence, i.recurrence_of, i.source, i.source_artifact_id, i.run_settings, i.started_at, i.completed_at, i.created_at, i.updated_at
             FROM items i
             JOIN items_fts fts ON i.rowid = fts.rowid
             WHERE i.project_id = ? AND items_fts MATCH ?
             ORDER BY rank
             LIMIT 50"
        )
        .bind(project_id.to_string())
        .bind(query)
        .fetch_all(self.pool())
        .await?;

        rows.into_iter().map(ItemRow::into_item).collect()
    }

    #[instrument(skip(self))]
    pub async fn search_items_global(
        &self,
        workspace_id: Uuid,
        query: &str,
    ) -> Result<Vec<Item>, StoreError> {
        let rows = sqlx::query_as::<_, ItemRow>(
            "SELECT i.id, i.project_id, i.parent_id, i.title, i.description, i.item_type, i.status, i.priority, i.estimate, i.estimate_unit, i.tags, i.sort_order, i.sprint_id, i.assignee, i.due_date, i.recurrence, i.recurrence_of, i.source, i.source_artifact_id, i.run_settings, i.started_at, i.completed_at, i.created_at, i.updated_at
             FROM items i
             JOIN items_fts fts ON i.rowid = fts.rowid
             JOIN projects p ON i.project_id = p.id
             WHERE p.workspace_id = ? AND items_fts MATCH ?
             ORDER BY rank
             LIMIT 100"
        )
        .bind(workspace_id.to_string())
        .bind(query)
        .fetch_all(self.pool())
        .await?;

        rows.into_iter().map(ItemRow::into_item).collect()
    }

    #[instrument(skip(self))]
    pub async fn get_item_tree(&self, project_id: Uuid) -> Result<Vec<Item>, StoreError> {
        let rows = sqlx::query_as::<_, ItemRow>(
            "SELECT id, project_id, parent_id, title, description, item_type, status, priority, estimate, estimate_unit, tags, sort_order, sprint_id, assignee, due_date, recurrence, recurrence_of, source, source_artifact_id, run_settings, started_at, completed_at, created_at, updated_at
             FROM items WHERE project_id = ? ORDER BY parent_id NULLS FIRST, sort_order ASC"
        )
        .bind(project_id.to_string())
        .fetch_all(self.pool())
        .await?;

        rows.into_iter().map(ItemRow::into_item).collect()
    }

    /// Returns true if every direct child of `parent_id` has exactly `done_status` as its status.
    /// Returns false when the parent has no children (nothing to complete).
    #[instrument(skip(self))]
    pub async fn siblings_all_done(
        &self,
        parent_id: Uuid,
        done_status: &str,
    ) -> Result<bool, StoreError> {
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM items WHERE parent_id = ?")
            .bind(parent_id.to_string())
            .fetch_one(self.pool())
            .await?;

        if total == 0 {
            return Ok(false);
        }

        let not_done: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM items WHERE parent_id = ? AND status != ?")
                .bind(parent_id.to_string())
                .bind(done_status)
                .fetch_one(self.pool())
                .await?;

        Ok(not_done == 0)
    }

    /// Check if all children of a parent item are completed, and update parent status
    #[instrument(skip(self))]
    pub async fn check_and_update_parent_status(
        &self,
        parent_id: Uuid,
        completed_status: &str,
    ) -> Result<bool, StoreError> {
        // Get all children of this parent
        let children = sqlx::query_as::<_, ItemRow>(
            "SELECT id, project_id, parent_id, title, description, item_type, status, priority, estimate, estimate_unit, tags, sort_order, sprint_id, assignee, due_date, recurrence, recurrence_of, source, source_artifact_id, run_settings, started_at, completed_at, created_at, updated_at
             FROM items WHERE parent_id = ?"
        )
        .bind(parent_id.to_string())
        .fetch_all(self.pool())
        .await?;

        if children.is_empty() {
            return Ok(false);
        }

        // Check if all children are completed
        let all_completed = children
            .iter()
            .all(|child| child.status == completed_status);

        if all_completed {
            // Update parent status to completed
            let now = Utc::now().to_rfc3339();
            sqlx::query(
                "UPDATE items SET status = ?, completed_at = ?, updated_at = ?, version = version + 1 WHERE id = ?",
            )
            .bind(completed_status)
            .bind(&now)
            .bind(&now)
            .bind(parent_id.to_string())
            .execute(self.pool())
            .await?;

            debug!(parent_id = %parent_id, "Parent item auto-completed");
            return Ok(true);
        }

        Ok(false)
    }
}

// ─── Row mapping ─────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct ItemRow {
    id: String,
    project_id: String,
    parent_id: Option<String>,
    title: String,
    description: Option<String>,
    item_type: String,
    status: String,
    priority: String,
    estimate: Option<f64>,
    estimate_unit: String,
    tags: String,
    sort_order: i32,
    sprint_id: Option<String>,
    assignee: Option<String>,
    due_date: Option<String>,
    recurrence: Option<String>,
    recurrence_of: Option<String>,
    source: String,
    source_artifact_id: Option<String>,
    run_settings: Option<String>,
    #[sqlx(default)]
    needs_review: bool,
    #[sqlx(default)]
    merge_note: Option<String>,
    #[sqlx(default)]
    started_by: Option<String>,
    #[sqlx(default)]
    team: Option<String>,
    #[sqlx(default)]
    merged_by: Option<String>,
    started_at: Option<String>,
    completed_at: Option<String>,
    created_at: String,
    updated_at: String,
}

impl ItemRow {
    fn into_item(self) -> Result<Item, StoreError> {
        const TABLE: &str = "items";
        Ok(Item {
            id: decode::uuid(TABLE, "id", &self.id)?,
            project_id: decode::uuid(TABLE, "project_id", &self.project_id)?,
            parent_id: decode::optional_uuid(TABLE, "parent_id", self.parent_id.as_deref())?,
            title: self.title,
            description: self.description,
            item_type: parse_item_type(&self.item_type),
            status: self.status,
            priority: decode::variant(TABLE, "priority", &self.priority)?,
            estimate: self.estimate,
            // The column's default is the bare variant name, which is not JSON.
            estimate_unit: decode::json(TABLE, "estimate_unit", &self.estimate_unit)
                .or_else(|_| decode::variant(TABLE, "estimate_unit", &self.estimate_unit))?,
            tags: decode::json(TABLE, "tags", &self.tags)?,
            sort_order: self.sort_order,
            sprint_id: decode::optional_uuid(TABLE, "sprint_id", self.sprint_id.as_deref())?,
            assignee: self.assignee,
            due_date: decode::optional_instant(TABLE, "due_date", self.due_date.as_deref())?,
            recurrence: self
                .recurrence
                .map(|text| {
                    text.parse()
                        .map_err(|()| StoreError::corrupt(TABLE, "recurrence", "not a recurrence"))
                })
                .transpose()?,
            recurrence_of: decode::optional_uuid(
                TABLE,
                "recurrence_of",
                self.recurrence_of.as_deref(),
            )?,
            // `FromStr` for `ItemSource` is infallible (unrecognised text
            // degrades to `Unknown`, i.e. untrusted) — see its own doc
            // comment for why that's the safe direction.
            source: self.source.parse().unwrap_or_default(),
            source_artifact_id: self.source_artifact_id,
            run_settings: self.run_settings.and_then(|s| {
                serde_json::from_str(&s)
                    .map_err(|error| {
                        // The item id and where the serde error is, never its message:
                        // serde quotes the offending value ("invalid type: string
                        // \"ten\""), and the stored value stays out of the log.
                        warn!(
                            item_id = %self.id,
                            kind = ?error.classify(),
                            line = error.line(),
                            column = error.column(),
                            "stored run_settings no longer fit; reading as none"
                        );
                    })
                    .ok()
            }),
            needs_review: self.needs_review,
            merge_note: self.merge_note,
            started_by: self.started_by,
            team: self.team,
            merged_by: self.merged_by,
            started_at: decode::optional_instant(TABLE, "started_at", self.started_at.as_deref())?,
            completed_at: decode::optional_instant(
                TABLE,
                "completed_at",
                self.completed_at.as_deref(),
            )?,
            created_at: decode::instant(TABLE, "created_at", &self.created_at)?,
            updated_at: decode::instant(TABLE, "updated_at", &self.updated_at)?,
        })
    }
}

fn parse_item_type(s: &str) -> ItemType {
    match s {
        "epic" => ItemType::Epic,
        "feature" => ItemType::Feature,
        "task" => ItemType::Task,
        "subtask" => ItemType::Subtask,
        "bug" => ItemType::Bug,
        "requirement" => ItemType::Requirement,
        other => ItemType::Custom(other.to_string()),
    }
}

/// Build the shared `WHERE` clause (and its ordered bind values) for the item
/// list/count queries so both stay in lockstep. Returns a clause beginning with
/// a leading space, e.g. ` WHERE project_id = ? AND status = ?`.
fn item_filter_clause(project_id: Uuid, filter: &ItemFilter) -> (String, Vec<String>) {
    let mut clause = String::from(" WHERE project_id = ?");
    let mut binds: Vec<String> = vec![project_id.to_string()];

    if let Some(ref status) = filter.status {
        clause.push_str(" AND status = ?");
        binds.push(status.clone());
    }
    if let Some(ref item_type) = filter.item_type {
        clause.push_str(" AND item_type = ?");
        binds.push(item_type.to_string());
    }
    if let Some(ref priority) = filter.priority {
        clause.push_str(" AND priority = ?");
        binds.push(priority.to_string());
    }
    if let Some(ref sprint_id) = filter.sprint_id {
        clause.push_str(" AND sprint_id = ?");
        binds.push(sprint_id.to_string());
    }
    if let Some(ref parent_id) = filter.parent_id {
        clause.push_str(" AND parent_id = ?");
        binds.push(parent_id.to_string());
    }
    if let Some(ref assignee) = filter.assignee {
        clause.push_str(" AND assignee = ?");
        binds.push(assignee.clone());
    }

    (clause, binds)
}

/// The one `INSERT` every item-creating path shares, on whatever connection
/// the caller holds — a pooled one, or a transaction's.
async fn insert_item(
    conn: &mut SqliteConnection,
    project_id: Uuid,
    initial_status: &str,
    input: CreateItem,
    source: ItemSource,
) -> Result<Item, StoreError> {
    require_row(conn, Parent::Project, project_id).await?;
    if let Some(parent_id) = input.parent_id {
        require_row(conn, Parent::Item, parent_id).await?;
    }
    if let Some(sprint_id) = input.sprint_id {
        require_row(conn, Parent::Sprint, sprint_id).await?;
    }
    let id = Uuid::new_v4();
    let now = Utc::now();
    let now_str = now.to_rfc3339();
    let item_type = input.item_type.unwrap_or(ItemType::Task);
    let priority = input.priority.unwrap_or_default();
    let estimate_unit = input.estimate_unit.unwrap_or_default();
    let tags = input.tags.unwrap_or_default();
    let tags_json = decode::to_json(&tags)?;
    let item_type_str = item_type.to_string();
    let priority_str = priority.to_string();
    let estimate_unit_str = decode::to_json(&estimate_unit)?;

    // Get next sort order
    let max_sort: Option<i32> =
        sqlx::query_scalar("SELECT MAX(sort_order) FROM items WHERE project_id = ? AND status = ?")
            .bind(project_id.to_string())
            .bind(initial_status)
            .fetch_one(&mut *conn)
            .await?;
    let sort_order = max_sort.unwrap_or(0) + 1;

    let source_str = source.to_string();

    sqlx::query(
            "INSERT INTO items (id, project_id, parent_id, title, description, item_type, status, priority, estimate, estimate_unit, tags, sort_order, sprint_id, assignee, due_date, recurrence, recurrence_of, source, source_artifact_id, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(id.to_string())
        .bind(project_id.to_string())
        .bind(input.parent_id.map(|p| p.to_string()))
        .bind(&input.title)
        .bind(&input.description)
        .bind(&item_type_str)
        .bind(initial_status)
        .bind(&priority_str)
        .bind(input.estimate)
        .bind(&estimate_unit_str)
        .bind(&tags_json)
        .bind(sort_order)
        .bind(input.sprint_id.map(|s| s.to_string()))
        .bind(&input.assignee)
        .bind(input.due_date.map(|d| d.to_rfc3339()))
        .bind(input.recurrence.map(|r| r.to_string()))
        .bind(input.recurrence_of.map(|r| r.to_string()))
        .bind(&source_str)
        .bind(&input.source_artifact_id)
        .bind(&now_str)
        .bind(&now_str)
        .execute(&mut *conn)
        .await?;

    debug!(item_id = %id, title = %input.title, source = %source_str, "Item created");

    Ok(Item {
        id,
        project_id,
        parent_id: input.parent_id,
        title: input.title,
        description: input.description,
        item_type,
        status: initial_status.to_string(),
        priority,
        estimate: input.estimate,
        estimate_unit,
        tags,
        sort_order,
        sprint_id: input.sprint_id,
        assignee: input.assignee,
        due_date: input.due_date,
        recurrence: input.recurrence,
        recurrence_of: input.recurrence_of,
        started_at: None,
        completed_at: None,
        source,
        source_artifact_id: input.source_artifact_id,
        run_settings: None,
        needs_review: false,
        merge_note: None,
        started_by: None,
        team: None,
        merged_by: None,
        created_at: now,
        updated_at: now,
    })
}

impl Repository {
    /// Point `item` at `parent` as its parent, touching nothing else of the row.
    pub async fn set_item_parent(&self, item: Uuid, parent: Uuid) -> Result<(), crate::StoreError> {
        sqlx::query("UPDATE items SET parent_id = ? WHERE id = ?")
            .bind(parent.to_string())
            .bind(item.to_string())
            .execute(self.pool())
            .await?;
        Ok(())
    }
}

/// The body of [`Repository::update_item_atomically`] over a transaction the caller opened with
/// `BEGIN IMMEDIATE` and ends: it writes only when it answers `Updated`, so any other answer
/// leaves nothing to undo, and a caller that has more to write in the same transaction (a
/// completion moving the item) calls this and goes on.
pub(crate) async fn update_item_in(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    id: Uuid,
    input: UpdateItem,
    workflow: &WorkflowConfig,
    expected_version: Option<i64>,
) -> Result<AtomicItemUpdateOutcome, StoreError> {
    let now = Utc::now().to_rfc3339();
    let current = sqlx::query_as::<_, ItemRow>(
        "SELECT id, project_id, parent_id, title, description, item_type, status, priority, estimate, estimate_unit, tags, sort_order, sprint_id, assignee, due_date, recurrence, recurrence_of, source, source_artifact_id, run_settings, started_at, completed_at, created_at, updated_at FROM items WHERE id = ?",
    )
    .bind(id.to_string())
    .fetch_optional(&mut **tx)
    .await?;
    let Some(current) = current else {
        return Ok(AtomicItemUpdateOutcome::NotFound);
    };
    let current_version: i64 = sqlx::query_scalar("SELECT version FROM items WHERE id = ?")
        .bind(id.to_string())
        .fetch_one(&mut **tx)
        .await?;
    if expected_version.is_some_and(|expected| expected != current_version) {
        return Ok(AtomicItemUpdateOutcome::PreconditionFailed);
    }

    let old_status = current.status.clone();
    if let Some(Some(sprint_id)) = input.sprint_id {
        require_row(&mut **tx, Parent::Sprint, sprint_id).await?;
    }
    let addresses_a_field = input.title.is_some()
        || input.description.is_some()
        || input.item_type.is_some()
        || input.status.is_some()
        || input.priority.is_some()
        || input.estimate.is_some()
        || input.estimate_unit.is_some()
        || input.tags.is_some()
        || input.due_date.is_some()
        || input.sprint_id.is_some()
        || input.sort_order.is_some()
        || input.assignee.is_some()
        || input.recurrence.is_some()
        || input.run_settings.is_some();
    // Match the pre-A2 no-op PATCH behavior: without an addressed field
    // there is no logical mutation and therefore no version to consume.
    if !addresses_a_field {
        let item = current.into_item()?;
        return Ok(AtomicItemUpdateOutcome::Updated {
            item: Box::new(item),
            version: current_version,
            old_status,
        });
    }
    let status_category = if let Some(target) = input.status.as_deref() {
        if let Err(error) = workflow.validate_transition(&old_status, target) {
            return Ok(AtomicItemUpdateOutcome::Rejected(error));
        }
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM items WHERE project_id = ? AND status = ?")
                .bind(&current.project_id)
                .bind(target)
                .fetch_one(&mut **tx)
                .await?;
        if let Err(error) = workflow.check_wip_limit(target, count as usize) {
            return Ok(AtomicItemUpdateOutcome::Rejected(error));
        }
        workflow
            .statuses
            .iter()
            .find(|status| status.name == target)
            .map(|status| status.category.clone())
    } else {
        None
    };

    let mut query = QueryBuilder::<Sqlite>::new("UPDATE items SET ");
    {
        let mut fields = query.separated(", ");
        if let Some(title) = input.title.as_deref() {
            fields.push("title = ").push_bind_unseparated(title);
        }
        if let Some(description) = input.description.as_ref() {
            fields
                .push("description = ")
                .push_bind_unseparated(description.as_deref());
        }
        if let Some(item_type) = input.item_type.as_ref() {
            fields
                .push("item_type = ")
                .push_bind_unseparated(item_type.to_string());
        }
        if let Some(status) = input.status.as_deref() {
            fields.push("status = ").push_bind_unseparated(status);
        }
        if let Some(priority) = input.priority.as_ref() {
            fields
                .push("priority = ")
                .push_bind_unseparated(priority.to_string());
        }
        if let Some(estimate) = input.estimate {
            fields.push("estimate = ").push_bind_unseparated(estimate);
        }
        if let Some(estimate_unit) = input.estimate_unit.as_ref() {
            let value = estimate_unit.as_ref().map(decode::to_json).transpose()?;
            fields.push("estimate_unit = ").push_bind_unseparated(value);
        }
        if let Some(tags) = input.tags.as_ref() {
            let value = decode::to_json(tags)?;
            fields.push("tags = ").push_bind_unseparated(value);
        }
        if let Some(due_date) = input.due_date.as_ref() {
            fields
                .push("due_date = ")
                .push_bind_unseparated(due_date.as_ref().map(|date| date.to_rfc3339()));
        }
        if let Some(sprint_id) = input.sprint_id.as_ref() {
            fields
                .push("sprint_id = ")
                .push_bind_unseparated(sprint_id.map(|sprint_id| sprint_id.to_string()));
        }
        if let Some(sort_order) = input.sort_order {
            fields
                .push("sort_order = ")
                .push_bind_unseparated(sort_order);
        }
        if let Some(assignee) = input.assignee.as_ref() {
            fields
                .push("assignee = ")
                .push_bind_unseparated(assignee.as_deref());
        }
        if let Some(recurrence) = input.recurrence.as_ref() {
            fields
                .push("recurrence = ")
                .push_bind_unseparated(recurrence.map(|r| r.to_string()));
        }
        if let Some(run_settings) = input.run_settings.as_ref() {
            fields.push("run_settings = ").push_bind_unseparated(
                run_settings
                    .as_ref()
                    .and_then(|v| serde_json::to_string(v).ok()),
            );
        }
        match status_category {
            Some(StatusCategory::InProgress) => {
                fields
                    .push("started_at = COALESCE(started_at, ")
                    .push_bind_unseparated(&now)
                    .push_unseparated(")");
                fields.push("completed_at = NULL");
            }
            Some(StatusCategory::Done) => {
                fields
                    .push("completed_at = COALESCE(completed_at, ")
                    .push_bind_unseparated(&now)
                    .push_unseparated(")");
            }
            Some(StatusCategory::Todo) => {
                fields.push("completed_at = NULL");
            }
            None => {}
        }
        fields.push("updated_at = ").push_bind_unseparated(&now);
        fields.push("version = version + 1");
    }
    query.push(" WHERE id = ").push_bind(id.to_string());
    if let Some(expected_version) = expected_version {
        query.push(" AND version = ").push_bind(expected_version);
    }
    let result = query.build().execute(&mut **tx).await?;
    if result.rows_affected() != 1 {
        return Ok(if expected_version.is_some() {
            AtomicItemUpdateOutcome::PreconditionFailed
        } else {
            AtomicItemUpdateOutcome::NotFound
        });
    }

    let needs_review = needs_review_sql();
    let item = sqlx::query_as::<_, ItemRow>(AssertSqlSafe(format!(
        "SELECT id, project_id, parent_id, title, description, item_type, status, priority, estimate, estimate_unit, tags, sort_order, sprint_id, assignee, due_date, recurrence, recurrence_of, source, source_artifact_id, run_settings, started_at, completed_at, created_at, updated_at, {needs_review} AS needs_review, {MERGE_NOTE_SQL} AS merge_note, {STARTED_BY_SQL} AS started_by, {TEAM_SQL} AS team, {MERGED_BY_SQL} AS merged_by FROM items WHERE id = ?"
    )))
    .bind(id.to_string())
    .fetch_one(&mut **tx)
    .await?
    .into_item()?;
    Ok(AtomicItemUpdateOutcome::Updated {
        item: Box::new(item),
        version: current_version + 1,
        old_status,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use tracing::field::{Field, Visit};
    use tracing::span::{Attributes, Id, Record};
    use tracing::{Event, Level, Metadata, Subscriber};

    use super::*;

    /// Records every event as `(level, rendered fields)`; just enough of a
    /// subscriber to see what `into_item` logs, without a new dependency.
    struct Capture(Arc<Mutex<Vec<(Level, String)>>>);

    struct Render(String);

    impl Visit for Render {
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            self.0.push_str(&format!("{}={:?} ", field.name(), value));
        }
    }

    impl Subscriber for Capture {
        fn enabled(&self, _: &Metadata<'_>) -> bool {
            true
        }
        fn new_span(&self, _: &Attributes<'_>) -> Id {
            Id::from_u64(1)
        }
        fn record(&self, _: &Id, _: &Record<'_>) {}
        fn record_follows_from(&self, _: &Id, _: &Id) {}
        fn event(&self, event: &Event<'_>) {
            let mut render = Render(String::new());
            event.record(&mut render);
            self.0
                .lock()
                .unwrap()
                .push((*event.metadata().level(), render.0));
        }
        fn enter(&self, _: &Id) {}
        fn exit(&self, _: &Id) {}
    }

    fn row(run_settings: Option<&str>) -> ItemRow {
        ItemRow {
            id: Uuid::new_v4().to_string(),
            project_id: Uuid::new_v4().to_string(),
            parent_id: None,
            title: "t".to_owned(),
            description: None,
            item_type: "task".to_owned(),
            status: "todo".to_owned(),
            priority: "medium".to_owned(),
            estimate: None,
            estimate_unit: "\"story_points\"".to_owned(),
            tags: "[]".to_owned(),
            sort_order: 0,
            sprint_id: None,
            assignee: None,
            due_date: None,
            recurrence: None,
            recurrence_of: None,
            source: "human".to_owned(),
            source_artifact_id: None,
            run_settings: run_settings.map(str::to_owned),
            needs_review: false,
            merge_note: None,
            started_by: None,
            team: None,
            merged_by: None,
            started_at: None,
            completed_at: None,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        }
    }

    #[test]
    fn a_stored_run_setting_that_no_longer_fits_reads_none_and_warns_with_the_item_id() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let row = row(Some(r#"{"timeout_seconds": "ten"}"#));
        let id = row.id.clone();

        let item =
            tracing::subscriber::with_default(Capture(events.clone()), || row.into_item().unwrap());

        assert!(item.run_settings.is_none());
        let events = events.lock().unwrap();
        let warns: Vec<_> = events.iter().filter(|(l, _)| *l == Level::WARN).collect();
        assert_eq!(warns.len(), 1, "exactly one warn: {events:?}");
        assert!(warns[0].1.contains(&id), "names the item: {}", warns[0].1);
        assert!(
            !warns[0].1.contains("ten"),
            "never the stored value: {}",
            warns[0].1
        );
    }
}
