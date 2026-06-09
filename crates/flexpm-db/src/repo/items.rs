use chrono::Utc;
use tracing::{debug, instrument};
use uuid::Uuid;

use flexpm_core::models::{CreateItem, Item, ItemFilter, ItemType, Priority, UpdateItem};

use super::Repository;

impl Repository {
    #[instrument(skip(self))]
    pub async fn create_item(
        &self,
        project_id: Uuid,
        initial_status: &str,
        input: CreateItem,
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

        sqlx::query(
            "INSERT INTO items (id, project_id, parent_id, title, description, item_type, status, priority, estimate, estimate_unit, tags, sort_order, sprint_id, assignee, due_date, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
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
        .bind(&now_str)
        .bind(&now_str)
        .execute(self.pool())
        .await?;

        debug!(item_id = %id, title = %input.title, "Item created");

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
            created_at: now,
            updated_at: now,
        })
    }

    #[instrument(skip(self))]
    pub async fn get_item(&self, id: Uuid) -> Result<Option<Item>, sqlx::Error> {
        let row = sqlx::query_as::<_, ItemRow>(
            "SELECT id, project_id, parent_id, title, description, item_type, status, priority, estimate, estimate_unit, tags, sort_order, sprint_id, assignee, due_date, started_at, completed_at, created_at, updated_at
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
        let mut query = String::from(
            "SELECT id, project_id, parent_id, title, description, item_type, status, priority, estimate, estimate_unit, tags, sort_order, sprint_id, assignee, due_date, started_at, completed_at, created_at, updated_at
             FROM items WHERE project_id = ?"
        );
        let mut binds: Vec<String> = vec![project_id.to_string()];

        if let Some(ref status) = filter.status {
            query.push_str(" AND status = ?");
            binds.push(status.clone());
        }
        if let Some(ref item_type) = filter.item_type {
            query.push_str(" AND item_type = ?");
            binds.push(item_type.to_string());
        }
        if let Some(ref priority) = filter.priority {
            query.push_str(" AND priority = ?");
            binds.push(priority.to_string());
        }
        if let Some(ref sprint_id) = filter.sprint_id {
            query.push_str(" AND sprint_id = ?");
            binds.push(sprint_id.to_string());
        }
        if let Some(ref parent_id) = filter.parent_id {
            query.push_str(" AND parent_id = ?");
            binds.push(parent_id.to_string());
        }
        if let Some(ref assignee) = filter.assignee {
            query.push_str(" AND assignee = ?");
            binds.push(assignee.clone());
        }

        query.push_str(" ORDER BY sort_order ASC");

        let per_page = filter.per_page.unwrap_or(100).min(500) as i64;
        let page = filter.page.unwrap_or(1).max(1) as i64;
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

    #[instrument(skip(self))]
    pub async fn search_items(
        &self,
        project_id: Uuid,
        query: &str,
    ) -> Result<Vec<Item>, sqlx::Error> {
        let rows = sqlx::query_as::<_, ItemRow>(
            "SELECT i.id, i.project_id, i.parent_id, i.title, i.description, i.item_type, i.status, i.priority, i.estimate, i.estimate_unit, i.tags, i.sort_order, i.sprint_id, i.assignee, i.due_date, i.started_at, i.completed_at, i.created_at, i.updated_at
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
            "SELECT i.id, i.project_id, i.parent_id, i.title, i.description, i.item_type, i.status, i.priority, i.estimate, i.estimate_unit, i.tags, i.sort_order, i.sprint_id, i.assignee, i.due_date, i.started_at, i.completed_at, i.created_at, i.updated_at
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
            "SELECT id, project_id, parent_id, title, description, item_type, status, priority, estimate, estimate_unit, tags, sort_order, sprint_id, assignee, due_date, started_at, completed_at, created_at, updated_at
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
            "SELECT id, project_id, parent_id, title, description, item_type, status, priority, estimate, estimate_unit, tags, sort_order, sprint_id, assignee, due_date, started_at, completed_at, created_at, updated_at
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
