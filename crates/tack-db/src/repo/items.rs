use chrono::Utc;
use tracing::{debug, instrument};
use uuid::Uuid;

use tack_core::CoreError;
use tack_core::models::{CreateItem, Item, ItemFilter, ItemSource, ItemType, Priority, UpdateItem};
use tack_core::workflow::{StatusCategory, WorkflowConfig};

use super::Repository;

/// The result of [`Repository::update_item_status_checked`] — see its doc
/// comment.
#[derive(Debug)]
pub enum StatusUpdateOutcome {
    /// The transition was applied; carries the freshly reloaded item.
    /// Boxed: `Item` otherwise dominates this enum's size even for the
    /// common `Rejected` case (`clippy::large_enum_variant`) — the same fix
    /// card C3 applied to `sprint_dispatch::ItemResult::Outcome` for the
    /// same reason.
    Applied(Box<Item>),
    /// The target column's WIP limit is at (or over) capacity — nothing was
    /// written, the item was left exactly as it was. Carries the exact
    /// [`CoreError::WipLimitExceeded`] [`WorkflowConfig::check_wip_limit`]
    /// produced, computed from the count read inside the same transaction
    /// that decided not to write, so callers get the engine's own error
    /// text rather than a re-derived approximation.
    Rejected(CoreError),
}

impl Repository {
    /// Create an item via the ordinary path — always `ItemSource::Manual`
    /// (the operator's own words, typed or spoken by them: the UI, `tack
    /// add`, the MCP tool, and the Alexa skill all go through this).
    /// External-data import paths must call
    /// [`create_item_with_source`](Self::create_item_with_source) instead so
    /// the item's provenance is recorded truthfully — see TODO.md's C2 card
    /// and `tack_core::models::ItemSource`.
    #[instrument(skip(self))]
    pub async fn create_item(
        &self,
        project_id: Uuid,
        initial_status: &str,
        input: CreateItem,
    ) -> Result<Item, sqlx::Error> {
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
    ) -> Result<Item, sqlx::Error> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        let now_str = now.to_rfc3339();
        let item_type = input.item_type.unwrap_or(ItemType::Task);
        let priority = input.priority.unwrap_or_default();
        let estimate_unit = input.estimate_unit.unwrap_or_default();
        let tags = input.tags.unwrap_or_default();
        let tags_json = serde_json::to_string(&tags).unwrap();
        let item_type_str = item_type.to_string();
        let priority_str = priority.to_string();
        let estimate_unit_str = serde_json::to_string(&estimate_unit).unwrap();

        // Get next sort order
        let max_sort: Option<i32> = sqlx::query_scalar(
            "SELECT MAX(sort_order) FROM items WHERE project_id = ? AND status = ?",
        )
        .bind(project_id.to_string())
        .bind(initial_status)
        .fetch_one(self.pool())
        .await?;
        let sort_order = max_sort.unwrap_or(0) + 1;

        let source_str = source.to_string();

        sqlx::query(
            "INSERT INTO items (id, project_id, parent_id, title, description, item_type, status, priority, estimate, estimate_unit, tags, sort_order, sprint_id, assignee, due_date, source, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
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
        .bind(&source_str)
        .bind(&now_str)
        .bind(&now_str)
        .execute(self.pool())
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
            started_at: None,
            completed_at: None,
            source,
            created_at: now,
            updated_at: now,
        })
    }

    #[instrument(skip(self))]
    pub async fn get_item(&self, id: Uuid) -> Result<Option<Item>, sqlx::Error> {
        let row = sqlx::query_as::<_, ItemRow>(
            "SELECT id, project_id, parent_id, title, description, item_type, status, priority, estimate, estimate_unit, tags, sort_order, sprint_id, assignee, due_date, source, started_at, completed_at, created_at, updated_at
             FROM items WHERE id = ?"
        )
        .bind(id.to_string())
        .fetch_optional(self.pool())
        .await?;

        Ok(row.map(|r| r.into_item()))
    }

    #[instrument(skip(self))]
    pub async fn list_items(
        &self,
        project_id: Uuid,
        filter: &ItemFilter,
    ) -> Result<Vec<Item>, sqlx::Error> {
        let (where_clause, binds) = item_filter_clause(project_id, filter);
        let mut query = format!(
            "SELECT id, project_id, parent_id, title, description, item_type, status, priority, estimate, estimate_unit, tags, sort_order, sprint_id, assignee, due_date, source, started_at, completed_at, created_at, updated_at
             FROM items{where_clause} ORDER BY sort_order ASC"
        );

        let per_page = filter.effective_per_page() as i64;
        let page = filter.effective_page() as i64;
        let offset = (page - 1) * per_page;
        query.push_str(&format!(" LIMIT {per_page} OFFSET {offset}"));

        // Build the query dynamically
        let mut q = sqlx::query_as::<_, ItemRow>(&query);
        for bind in &binds {
            q = q.bind(bind);
        }

        let rows = q.fetch_all(self.pool()).await?;
        Ok(rows.into_iter().map(|r| r.into_item()).collect())
    }

    /// Total number of items matching `filter` (ignoring pagination). Used to
    /// build the `{ data, total, page, per_page }` list envelope so clients can
    /// page through everything instead of silently truncating at one page.
    #[instrument(skip(self))]
    pub async fn count_items(
        &self,
        project_id: Uuid,
        filter: &ItemFilter,
    ) -> Result<i64, sqlx::Error> {
        let (where_clause, binds) = item_filter_clause(project_id, filter);
        let query = format!("SELECT COUNT(*) FROM items{where_clause}");
        let mut q = sqlx::query_scalar::<_, i64>(&query);
        for bind in &binds {
            q = q.bind(bind);
        }
        q.fetch_one(self.pool()).await
    }

    /// All items assigned to `sprint_id`, unpaginated, ordered by board
    /// position (`sort_order`). Card C3 (sprint DAG dispatch) needs the
    /// *complete* sprint, not one `ItemFilter::MAX_PER_PAGE`-sized page of
    /// it — a sprint dispatch that silently dropped items past page 1 would
    /// be exactly the kind of "looks like it worked" bug the dry-run mode
    /// exists to prevent.
    #[instrument(skip(self))]
    pub async fn list_items_for_sprint(&self, sprint_id: Uuid) -> Result<Vec<Item>, sqlx::Error> {
        let rows = sqlx::query_as::<_, ItemRow>(
            "SELECT id, project_id, parent_id, title, description, item_type, status, priority, estimate, estimate_unit, tags, sort_order, sprint_id, assignee, due_date, source, started_at, completed_at, created_at, updated_at
             FROM items WHERE sprint_id = ? ORDER BY sort_order ASC"
        )
        .bind(sprint_id.to_string())
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|r| r.into_item()).collect())
    }

    /// Return all incomplete items whose `due_date` falls in `[from, to)`.
    /// Used by the background webhook task to fire `item.due_soon` events.
    pub async fn list_items_due_soon(
        &self,
        from: chrono::DateTime<chrono::Utc>,
        to: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<Item>, sqlx::Error> {
        let rows = sqlx::query_as::<_, ItemRow>(
            "SELECT id, project_id, parent_id, title, description, item_type, status, priority, estimate, estimate_unit, tags, sort_order, sprint_id, assignee, due_date, source, started_at, completed_at, created_at, updated_at
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
        Ok(rows.into_iter().map(|r| r.into_item()).collect())
    }

    #[instrument(skip(self))]
    pub async fn update_item(
        &self,
        id: Uuid,
        input: UpdateItem,
    ) -> Result<Option<Item>, sqlx::Error> {
        let now = Utc::now().to_rfc3339();

        if let Some(ref title) = input.title {
            sqlx::query("UPDATE items SET title = ?, updated_at = ? WHERE id = ?")
                .bind(title)
                .bind(&now)
                .bind(id.to_string())
                .execute(self.pool())
                .await?;
        }
        if let Some(ref description) = input.description {
            sqlx::query("UPDATE items SET description = ?, updated_at = ? WHERE id = ?")
                .bind(description)
                .bind(&now)
                .bind(id.to_string())
                .execute(self.pool())
                .await?;
        }
        if let Some(ref status) = input.status {
            sqlx::query("UPDATE items SET status = ?, updated_at = ? WHERE id = ?")
                .bind(status)
                .bind(&now)
                .bind(id.to_string())
                .execute(self.pool())
                .await?;
        }
        if let Some(ref priority) = input.priority {
            sqlx::query("UPDATE items SET priority = ?, updated_at = ? WHERE id = ?")
                .bind(priority.to_string())
                .bind(&now)
                .bind(id.to_string())
                .execute(self.pool())
                .await?;
        }
        if let Some(ref item_type) = input.item_type {
            sqlx::query("UPDATE items SET item_type = ?, updated_at = ? WHERE id = ?")
                .bind(item_type.to_string())
                .bind(&now)
                .bind(id.to_string())
                .execute(self.pool())
                .await?;
        }
        if let Some(estimate) = input.estimate {
            sqlx::query("UPDATE items SET estimate = ?, updated_at = ? WHERE id = ?")
                .bind(estimate)
                .bind(&now)
                .bind(id.to_string())
                .execute(self.pool())
                .await?;
        }
        if let Some(ref tags) = input.tags {
            let tags_json = serde_json::to_string(tags).unwrap();
            sqlx::query("UPDATE items SET tags = ?, updated_at = ? WHERE id = ?")
                .bind(&tags_json)
                .bind(&now)
                .bind(id.to_string())
                .execute(self.pool())
                .await?;
        }
        if let Some(sort_order) = input.sort_order {
            sqlx::query("UPDATE items SET sort_order = ?, updated_at = ? WHERE id = ?")
                .bind(sort_order)
                .bind(&now)
                .bind(id.to_string())
                .execute(self.pool())
                .await?;
        }
        if input.assignee.is_some() {
            sqlx::query("UPDATE items SET assignee = ?, updated_at = ? WHERE id = ?")
                .bind(&input.assignee)
                .bind(&now)
                .bind(id.to_string())
                .execute(self.pool())
                .await?;
        }
        // Double-`Option` fields: outer `Some` means "the client addressed this
        // field", inner `Option` carries the new value (`None` ⇒ clear to NULL).
        if let Some(sprint_id) = input.sprint_id {
            sqlx::query("UPDATE items SET sprint_id = ?, updated_at = ? WHERE id = ?")
                .bind(sprint_id.map(|s| s.to_string()))
                .bind(&now)
                .bind(id.to_string())
                .execute(self.pool())
                .await?;
        }
        if let Some(due_date) = input.due_date {
            sqlx::query("UPDATE items SET due_date = ?, updated_at = ? WHERE id = ?")
                .bind(due_date.map(|d| d.to_rfc3339()))
                .bind(&now)
                .bind(id.to_string())
                .execute(self.pool())
                .await?;
        }
        if let Some(estimate_unit) = input.estimate_unit {
            // `estimate_unit` is stored as a JSON string; `None` clears it to SQL NULL.
            let value = estimate_unit.map(|u| serde_json::to_string(&u).unwrap());
            sqlx::query("UPDATE items SET estimate_unit = ?, updated_at = ? WHERE id = ?")
                .bind(value)
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
                        "UPDATE items SET started_at = COALESCE(started_at, ?), completed_at = NULL, updated_at = ? WHERE id = ?",
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
                        "UPDATE items SET completed_at = COALESCE(completed_at, ?), updated_at = ? WHERE id = ?",
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
                        "UPDATE items SET completed_at = NULL, updated_at = ? WHERE id = ?",
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

    #[instrument(skip(self))]
    pub async fn delete_item(&self, id: Uuid) -> Result<bool, sqlx::Error> {
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
    ) -> Result<i64, sqlx::Error> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM items WHERE project_id = ? AND status = ?")
                .bind(project_id.to_string())
                .bind(status)
                .fetch_one(self.pool())
                .await?;
        Ok(count)
    }

    /// Atomically check `target_status`'s WIP limit (per `workflow`) and,
    /// only if it isn't exceeded, apply the status transition — all inside
    /// one `BEGIN IMMEDIATE` SQLite write transaction, so the count read the
    /// limit check depends on can never be interleaved with another
    /// writer racing the same column.
    ///
    /// Card R2 (2026-08-05): `dispatcher::apply_mapped_status` used to do
    /// this as two separate steps — [`Repository::count_items_by_status`]
    /// then a plain [`Repository::update_item`] — with no lock spanning
    /// them. Two concurrent callers moving *different* items into the same
    /// WIP-limited column could each read "under the limit" before either
    /// had written, and both would then commit, pushing the column over its
    /// configured limit. `BEGIN IMMEDIATE` (rather than the plain deferred
    /// `BEGIN` [`Repository::upsert_orch_tasks`] and friends use, which only
    /// takes SQLite's write lock on the *first* write inside the
    /// transaction) acquires the write lock up front, at the count read —
    /// so a second concurrent caller's own `BEGIN IMMEDIATE` blocks until
    /// the first transaction commits or rolls back, rather than both
    /// proceeding as if uncontended and one of them hitting a deferred
    /// transaction's read-to-write lock upgrade conflict later.
    ///
    /// Card R3 (2026-08-05) closed the gap this doc comment used to flag:
    /// `handlers::items::update_item` (the human/board-drag path) and
    /// `handlers::alexa` (the voice "mark done" path) used to do the same
    /// unguarded two-step `count_items_by_status` + `update_item` — the
    /// identical race, on the two call sites hit far more often than
    /// dispatch. Both now call this method too. See TODO.md's R3 handoff.
    ///
    /// Only touches the fields `dispatcher::apply_mapped_status` needs
    /// (status, and the status-category-derived started_at/completed_at) —
    /// not the full field set `update_item` handles. If a future caller
    /// needs more fields updated atomically alongside the WIP check, extend
    /// this method rather than composing it with `update_item`'s separate,
    /// unguarded writes.
    #[instrument(skip(self, workflow))]
    pub async fn update_item_status_checked(
        &self,
        id: Uuid,
        project_id: Uuid,
        target_status: &str,
        status_category: Option<StatusCategory>,
        workflow: &WorkflowConfig,
    ) -> Result<Option<StatusUpdateOutcome>, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM items WHERE project_id = ? AND status = ?")
                .bind(project_id.to_string())
                .bind(target_status)
                .fetch_one(&mut *tx)
                .await?;

        if let Err(e) = workflow.check_wip_limit(target_status, count as usize) {
            tx.rollback().await?;
            return Ok(Some(StatusUpdateOutcome::Rejected(e)));
        }

        sqlx::query("UPDATE items SET status = ?, updated_at = ? WHERE id = ?")
            .bind(target_status)
            .bind(&now)
            .bind(id.to_string())
            .execute(&mut *tx)
            .await?;

        if let Some(category) = status_category {
            match category {
                StatusCategory::InProgress => {
                    sqlx::query(
                        "UPDATE items SET started_at = COALESCE(started_at, ?), completed_at = NULL, updated_at = ? WHERE id = ?",
                    )
                    .bind(&now)
                    .bind(&now)
                    .bind(id.to_string())
                    .execute(&mut *tx)
                    .await?;
                }
                StatusCategory::Done => {
                    sqlx::query(
                        "UPDATE items SET completed_at = COALESCE(completed_at, ?), updated_at = ? WHERE id = ?",
                    )
                    .bind(&now)
                    .bind(&now)
                    .bind(id.to_string())
                    .execute(&mut *tx)
                    .await?;
                }
                StatusCategory::Todo => {
                    sqlx::query(
                        "UPDATE items SET completed_at = NULL, updated_at = ? WHERE id = ?",
                    )
                    .bind(&now)
                    .bind(id.to_string())
                    .execute(&mut *tx)
                    .await?;
                }
            }
        }

        tx.commit().await?;

        Ok(self
            .get_item(id)
            .await?
            .map(|item| StatusUpdateOutcome::Applied(Box::new(item))))
    }

    #[instrument(skip(self))]
    pub async fn search_items(
        &self,
        project_id: Uuid,
        query: &str,
    ) -> Result<Vec<Item>, sqlx::Error> {
        let rows = sqlx::query_as::<_, ItemRow>(
            "SELECT i.id, i.project_id, i.parent_id, i.title, i.description, i.item_type, i.status, i.priority, i.estimate, i.estimate_unit, i.tags, i.sort_order, i.sprint_id, i.assignee, i.due_date, i.source, i.started_at, i.completed_at, i.created_at, i.updated_at
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

        Ok(rows.into_iter().map(|r| r.into_item()).collect())
    }

    #[instrument(skip(self))]
    pub async fn search_items_global(
        &self,
        workspace_id: Uuid,
        query: &str,
    ) -> Result<Vec<Item>, sqlx::Error> {
        let rows = sqlx::query_as::<_, ItemRow>(
            "SELECT i.id, i.project_id, i.parent_id, i.title, i.description, i.item_type, i.status, i.priority, i.estimate, i.estimate_unit, i.tags, i.sort_order, i.sprint_id, i.assignee, i.due_date, i.source, i.started_at, i.completed_at, i.created_at, i.updated_at
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

        Ok(rows.into_iter().map(|r| r.into_item()).collect())
    }

    #[instrument(skip(self))]
    pub async fn get_item_tree(&self, project_id: Uuid) -> Result<Vec<Item>, sqlx::Error> {
        let rows = sqlx::query_as::<_, ItemRow>(
            "SELECT id, project_id, parent_id, title, description, item_type, status, priority, estimate, estimate_unit, tags, sort_order, sprint_id, assignee, due_date, source, started_at, completed_at, created_at, updated_at
             FROM items WHERE project_id = ? ORDER BY parent_id NULLS FIRST, sort_order ASC"
        )
        .bind(project_id.to_string())
        .fetch_all(self.pool())
        .await?;

        Ok(rows.into_iter().map(|r| r.into_item()).collect())
    }

    /// Returns true if every direct child of `parent_id` has exactly `done_status` as its status.
    /// Returns false when the parent has no children (nothing to complete).
    #[instrument(skip(self))]
    pub async fn siblings_all_done(
        &self,
        parent_id: Uuid,
        done_status: &str,
    ) -> Result<bool, sqlx::Error> {
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
    ) -> Result<bool, sqlx::Error> {
        // Get all children of this parent
        let children = sqlx::query_as::<_, ItemRow>(
            "SELECT id, project_id, parent_id, title, description, item_type, status, priority, estimate, estimate_unit, tags, sort_order, sprint_id, assignee, due_date, source, started_at, completed_at, created_at, updated_at
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
                "UPDATE items SET status = ?, completed_at = ?, updated_at = ? WHERE id = ?",
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
    source: String,
    started_at: Option<String>,
    completed_at: Option<String>,
    created_at: String,
    updated_at: String,
}

impl ItemRow {
    fn into_item(self) -> Item {
        Item {
            id: Uuid::parse_str(&self.id).unwrap(),
            project_id: Uuid::parse_str(&self.project_id).unwrap(),
            parent_id: self.parent_id.and_then(|s| Uuid::parse_str(&s).ok()),
            title: self.title,
            description: self.description,
            item_type: parse_item_type(&self.item_type),
            status: self.status,
            priority: parse_priority(&self.priority),
            estimate: self.estimate,
            estimate_unit: serde_json::from_str(&self.estimate_unit).unwrap_or_default(),
            tags: serde_json::from_str(&self.tags).unwrap_or_default(),
            sort_order: self.sort_order,
            sprint_id: self.sprint_id.and_then(|s| Uuid::parse_str(&s).ok()),
            assignee: self.assignee,
            due_date: self.due_date.and_then(|s| {
                chrono::DateTime::parse_from_rfc3339(&s)
                    .ok()
                    .map(|d| d.with_timezone(&Utc))
            }),
            // `FromStr` for `ItemSource` is infallible (unrecognised text
            // degrades to `Unknown`, i.e. untrusted) — see its own doc
            // comment for why that's the safe direction.
            source: self.source.parse().unwrap_or_default(),
            started_at: self.started_at.and_then(|s| {
                chrono::DateTime::parse_from_rfc3339(&s)
                    .ok()
                    .map(|d| d.with_timezone(&Utc))
            }),
            completed_at: self.completed_at.and_then(|s| {
                chrono::DateTime::parse_from_rfc3339(&s)
                    .ok()
                    .map(|d| d.with_timezone(&Utc))
            }),
            created_at: chrono::DateTime::parse_from_rfc3339(&self.created_at)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            updated_at: chrono::DateTime::parse_from_rfc3339(&self.updated_at)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        }
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

fn parse_priority(s: &str) -> Priority {
    match s {
        "critical" => Priority::Critical,
        "high" => Priority::High,
        "medium" => Priority::Medium,
        "low" => Priority::Low,
        "none" => Priority::None,
        _ => Priority::Medium,
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
