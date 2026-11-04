//! What a parent item's children say about themselves: their status, the team (or single agent)
//! their latest run went to, and whether a run of theirs is still open. The rules that read these
//! facts are `tack_core::children`'s.

use std::collections::HashMap;

use tack_core::children::Child;
use uuid::Uuid;

use crate::repo::execution::state;
use crate::{Repository, StoreError};

#[derive(sqlx::FromRow)]
struct ChildRow {
    parent_id: String,
    status: String,
    team: Option<String>,
    open_run: bool,
}

/// The facts of every non-hidden child whose parent satisfies `parent_filter`, a `WHERE` fragment
/// over `items` with one `?` for `bound`. The team is the docket team the newest request's
/// profile names, else the agent that profile runs.
async fn children_where(
    repo: &Repository,
    parent_filter: &str,
    bound: &str,
) -> Result<HashMap<Uuid, Vec<Child>>, StoreError> {
    let sql = state::sql(&format!(
        "SELECT parent_id, status,
                (SELECT COALESCE(NULLIF(json_extract(r.request_snapshot, '$.resolved_agent_profile.tool_policy.docket.team'), ''),
                                 NULLIF(json_extract(r.request_snapshot, '$.resolved_agent_profile.tool_policy.agent'), ''))
                   FROM execution_requests r WHERE r.item_id = items.id
                  ORDER BY r.created_at DESC, r.rowid DESC LIMIT 1) AS team,
                EXISTS (SELECT 1 FROM execution_requests r
                         WHERE r.item_id = items.id AND r.state IN ({{open}})) AS open_run
           FROM items
          WHERE parent_id IS NOT NULL AND hidden = 0 AND {parent_filter}
          ORDER BY sort_order ASC"
    ));
    let rows: Vec<ChildRow> = sqlx::query_as(sql)
        .bind(bound)
        .fetch_all(repo.pool())
        .await?;
    let mut children: HashMap<Uuid, Vec<Child>> = HashMap::new();
    for row in rows {
        let Ok(parent) = row.parent_id.parse() else {
            continue;
        };
        children.entry(parent).or_default().push(Child {
            status: row.status,
            team: row.team,
            open_run: row.open_run,
        });
    }
    Ok(children)
}

/// The children of every parent in the project, by parent.
pub async fn of_project(
    repo: &Repository,
    project_id: Uuid,
) -> Result<HashMap<Uuid, Vec<Child>>, StoreError> {
    children_where(repo, "project_id = ?", &project_id.to_string()).await
}

/// The children of one parent.
pub async fn of_parent(repo: &Repository, parent_id: Uuid) -> Result<Vec<Child>, StoreError> {
    Ok(
        children_where(repo, "parent_id = ?", &parent_id.to_string())
            .await?
            .remove(&parent_id)
            .unwrap_or_default(),
    )
}
