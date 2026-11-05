use std::collections::HashMap;

use sqlx::SqliteConnection;
use tack_core::hierarchy::{Entry, Placement, Target};
use tack_core::item_types::Level;
use uuid::Uuid;

use crate::{Repository, StoreError, decode};

/// A task waiting in a status, as auto-start picks from them.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WaitingItem {
    pub id: String,
    pub priority: String,
    pub sort_order: i32,
    pub version: i64,
}

/// The tasks of the project in `status` that say who does them: at least one role, or a profile
/// chosen on the task itself (`run_settings.agent_profile_id`), and that have never had a run: a
/// task with any execution request, open or ended, is not waiting to start.
pub async fn waiting_with_someone_to_do_it(
    repo: &Repository,
    project_id: &str,
    status: &str,
) -> Result<Vec<WaitingItem>, StoreError> {
    Ok(sqlx::query_as(
        "SELECT id, priority, sort_order, version FROM items
         WHERE project_id = ? AND status = ?
           AND (EXISTS (SELECT 1 FROM item_roles ir WHERE ir.item_id = items.id)
                OR COALESCE(json_extract(items.run_settings, '$.agent_profile_id'), '') <> '')
           AND NOT EXISTS (SELECT 1 FROM execution_requests r WHERE r.item_id = items.id)",
    )
    .bind(project_id)
    .bind(status)
    .fetch_all(repo.pool())
    .await?)
}

/// The id and status of one task.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ItemStatus {
    pub id: String,
    pub status: String,
}

/// The status of every task of the project.
pub async fn statuses_of_project(
    repo: &Repository,
    project_id: &str,
) -> Result<Vec<ItemStatus>, StoreError> {
    Ok(
        sqlx::query_as("SELECT id, status FROM items WHERE project_id = ? AND hidden = 0")
            .bind(project_id)
            .fetch_all(repo.pool())
            .await?,
    )
}

/// The status of every task that recurs from `item_id`.
pub async fn statuses_of_recurrences(
    repo: &Repository,
    item_id: &str,
) -> Result<Vec<String>, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT status FROM items WHERE recurrence_of = ?")
            .bind(item_id)
            .fetch_all(repo.pool())
            .await?,
    )
}

/// How far down the tree facts are read: past the deepest tree the rule allows, so an item that is
/// already too deep still reads as such, and a stored loop ends.
const WALK_LIMIT: usize = 20;

#[derive(sqlx::FromRow)]
struct TreeFacts {
    project_id: String,
    title: String,
    type_name: Option<String>,
    level: Option<String>,
}

impl TreeFacts {
    fn level(&self) -> Result<Level, StoreError> {
        self.level
            .as_deref()
            .and_then(Level::parse)
            .ok_or_else(|| StoreError::corrupt("item_types", "level", "no type has this level"))
    }
}

async fn tree_facts(
    conn: &mut SqliteConnection,
    id: Uuid,
) -> Result<Option<TreeFacts>, StoreError> {
    Ok(sqlx::query_as(
        "SELECT i.project_id, i.title, t.name AS type_name, t.level
           FROM items i
           LEFT JOIN item_types t ON t.project_id = i.project_id AND t.key = i.item_type
          WHERE i.id = ?",
    )
    .bind(id.to_string())
    .fetch_optional(&mut *conn)
    .await?)
}

/// The item as the tree rule sees it when it is put under a parent: its type, its project and
/// how many levels lie below it. `None` when there is no such item.
pub(crate) async fn placement_in(
    conn: &mut SqliteConnection,
    id: Uuid,
) -> Result<Option<Placement>, StoreError> {
    let Some(facts) = tree_facts(conn, id).await? else {
        return Ok(None);
    };
    let height: i64 = sqlx::query_scalar(
        "WITH RECURSIVE below(id, depth) AS (
             SELECT id, 1 FROM items WHERE parent_id = ?1
             UNION ALL
             SELECT i.id, below.depth + 1 FROM items i JOIN below ON i.parent_id = below.id
              WHERE below.depth < ?2)
         SELECT COALESCE(MAX(depth), 0) FROM below",
    )
    .bind(id.to_string())
    .bind(WALK_LIMIT as i64)
    .fetch_one(&mut *conn)
    .await?;
    Ok(Some(Placement {
        item: id,
        level: facts.level()?,
        project: decode::uuid("items", "project_id", &facts.project_id)?,
        title: facts.title,
        type_name: facts.type_name.unwrap_or_default(),
        subtree_height: usize::try_from(height).unwrap_or(0),
    }))
}

/// The item as the tree rule sees it when it would become a parent: its type, its project and its
/// parents, nearest first. `None` when there is no such item.
pub(crate) async fn target_in(
    conn: &mut SqliteConnection,
    id: Uuid,
) -> Result<Option<Target>, StoreError> {
    let Some(facts) = tree_facts(conn, id).await? else {
        return Ok(None);
    };
    let above: Vec<String> = sqlx::query_scalar(
        "WITH RECURSIVE above(id, depth) AS (
             SELECT parent_id, 1 FROM items WHERE id = ?1 AND parent_id IS NOT NULL
             UNION ALL
             SELECT i.parent_id, above.depth + 1 FROM items i JOIN above ON i.id = above.id
              WHERE i.parent_id IS NOT NULL AND above.depth < ?2)
         SELECT id FROM above ORDER BY depth",
    )
    .bind(id.to_string())
    .bind(WALK_LIMIT as i64)
    .fetch_all(&mut *conn)
    .await?;
    Ok(Some(Target {
        id,
        level: facts.level()?,
        project: decode::uuid("items", "project_id", &facts.project_id)?,
        type_name: facts.type_name.unwrap_or_default(),
        ancestors: above
            .iter()
            .map(|id| decode::uuid("items", "parent_id", id))
            .collect::<Result<_, _>>()?,
    }))
}

/// One item of a project's tree.
#[derive(Debug, Clone)]
pub struct TreeRow {
    pub id: Uuid,
    /// The key of its type in the project's catalog.
    pub item_type: String,
    pub entry: Entry,
    /// On no board or list: a thread's anchor.
    pub hidden: bool,
}

#[derive(sqlx::FromRow)]
struct TreeRowRaw {
    id: String,
    parent_id: Option<String>,
    title: String,
    item_type: String,
    type_name: Option<String>,
    level: Option<String>,
    hidden: bool,
}

/// Every item of the project with its parent and its type's name and level, in tree order: each
/// item comes before what it holds, and siblings follow their sort order.
pub async fn tree_of_project(
    repo: &Repository,
    project_id: Uuid,
) -> Result<Vec<TreeRow>, StoreError> {
    let rows: Vec<TreeRowRaw> = sqlx::query_as(
        "SELECT i.id, i.parent_id, i.title, i.item_type, t.name AS type_name, t.level, i.hidden
           FROM items i
           LEFT JOIN item_types t ON t.project_id = i.project_id AND t.key = i.item_type
          WHERE i.project_id = ?
          ORDER BY i.sort_order, i.created_at, i.id",
    )
    .bind(project_id.to_string())
    .fetch_all(repo.pool())
    .await?;
    let mut by_parent: HashMap<Option<Uuid>, Vec<TreeRow>> = HashMap::new();
    let mut ids = Vec::with_capacity(rows.len());
    for row in rows {
        let id = decode::uuid("items", "id", &row.id)?;
        let parent = decode::optional_uuid("items", "parent_id", row.parent_id.as_deref())?;
        let level =
            row.level.as_deref().and_then(Level::parse).ok_or_else(|| {
                StoreError::corrupt("item_types", "level", "no type has this level")
            })?;
        ids.push(id);
        by_parent.entry(parent).or_default().push(TreeRow {
            id,
            item_type: row.item_type,
            hidden: row.hidden,
            entry: Entry {
                parent,
                title: row.title,
                type_name: row.type_name.unwrap_or_default(),
                level,
            },
        });
    }
    // The roots are the items whose parent is absent or not of this project.
    let known: std::collections::HashSet<Uuid> = ids.into_iter().collect();
    let mut ordered = Vec::new();
    let mut stack: Vec<TreeRow> = Vec::new();
    let mut roots: Vec<TreeRow> = by_parent.remove(&None).unwrap_or_default();
    let orphans: Vec<Option<Uuid>> = by_parent
        .keys()
        .filter(|parent| parent.is_some_and(|p| !known.contains(&p)))
        .copied()
        .collect();
    for parent in orphans {
        roots.extend(by_parent.remove(&parent).unwrap_or_default());
    }
    stack.extend(roots.into_iter().rev());
    while let Some(row) = stack.pop() {
        let children = by_parent.remove(&Some(row.id)).unwrap_or_default();
        ordered.push(row);
        stack.extend(children.into_iter().rev());
    }
    Ok(ordered)
}
