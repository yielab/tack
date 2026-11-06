//! What the work under a group says about itself, added up by status: its estimate, start and due
//! date, the team (or single agent) its latest run went to, whether a run of it is still open and
//! whether it asks a person for something. The rules that read these facts are
//! `tack_core::children`'s.
//!
//! "Under" is the whole subtree, to any depth, without passing through a hidden item. The work of a
//! subtree is its items that are work: a type that does not hold work, and no children.

use std::collections::HashMap;

use tack_core::children::Tally;
use uuid::Uuid;

use crate::repo::execution::state;
use crate::{Repository, StoreError, decode};

const TABLE: &str = "items";

#[derive(sqlx::FromRow)]
struct TallyRow {
    group_id: String,
    status: String,
    count: i64,
    estimate: Option<f64>,
    starts: Option<String>,
    due: Option<String>,
    teams: String,
    open_runs: i64,
    needs_you: i64,
}

/// The work under every group that has `seed` as a row of `items` (a `WHERE` fragment over
/// `items` with one `?` for `bound`), added up by status, per group. The facts of each work item
/// are read once, however many groups it sits under.
async fn tallies_where(
    repo: &Repository,
    seed: &str,
    bound: &str,
) -> Result<HashMap<Uuid, Vec<Tally>>, StoreError> {
    let sql = state::sql(&format!(
        "WITH RECURSIVE sub(group_id, id) AS (
             SELECT parent_id, id FROM items WHERE parent_id IS NOT NULL AND hidden = 0 AND {seed}
             UNION ALL
             SELECT sub.group_id, c.id FROM sub JOIN items c ON c.parent_id = sub.id WHERE c.hidden = 0
         ),
         work AS MATERIALIZED (
             SELECT i.id, i.status, i.estimate,
                    COALESCE(i.started_at, i.created_at) AS starts, i.due_date AS due,
                    (SELECT COALESCE(NULLIF(json_extract(r.request_snapshot, '$.resolved_agent_profile.tool_policy.docket.team'), ''),
                                     NULLIF(json_extract(r.request_snapshot, '$.resolved_agent_profile.tool_policy.agent'), ''))
                       FROM execution_requests r WHERE r.item_id = i.id
                      ORDER BY r.created_at DESC, r.rowid DESC LIMIT 1) AS team,
                    EXISTS (SELECT 1 FROM execution_requests r
                             WHERE r.item_id = i.id AND r.state IN ({{open}})) AS open_run,
                    EXISTS (SELECT 1 FROM notifications n
                             WHERE n.item_id = i.id AND n.level = 'needs_you' AND n.resolved_at IS NULL) AS needs_you
               FROM items i
              WHERE i.id IN (SELECT id FROM sub)
                AND COALESCE((SELECT t.level FROM item_types t
                               WHERE t.project_id = i.project_id AND t.key = i.item_type), 'work') <> 'group'
                AND NOT EXISTS (SELECT 1 FROM items k WHERE k.parent_id = i.id AND k.hidden = 0)
         )
         SELECT sub.group_id, w.status, COUNT(*) AS count, SUM(w.estimate) AS estimate,
                MIN(w.starts) AS starts, MAX(w.due) AS due,
                json_group_array(DISTINCT w.team) AS teams,
                SUM(w.open_run) AS open_runs, SUM(w.needs_you) AS needs_you
           FROM sub JOIN work w ON w.id = sub.id
          GROUP BY sub.group_id, w.status"
    ));
    let rows: Vec<TallyRow> = sqlx::query_as(sql)
        .bind(bound)
        .fetch_all(repo.pool())
        .await?;
    let mut by_group: HashMap<Uuid, Vec<Tally>> = HashMap::new();
    for row in rows {
        let group = decode::uuid(TABLE, "parent_id", &row.group_id)?;
        let teams: Vec<Option<String>> = decode::json(TABLE, "teams", &row.teams)?;
        let count = |n: i64| u32::try_from(n).unwrap_or(u32::MAX);
        by_group.entry(group).or_default().push(Tally {
            status: row.status,
            count: count(row.count),
            estimate: row.estimate,
            starts: decode::optional_instant(TABLE, "started_at", row.starts.as_deref())?,
            due: decode::optional_instant(TABLE, "due_date", row.due.as_deref())?,
            teams: teams.into_iter().flatten().collect(),
            open_runs: count(row.open_runs),
            needs_you: count(row.needs_you),
        });
    }
    Ok(by_group)
}

/// The work under every group of the project, by group.
pub async fn of_project(
    repo: &Repository,
    project_id: Uuid,
) -> Result<HashMap<Uuid, Vec<Tally>>, StoreError> {
    tallies_where(repo, "project_id = ?", &project_id.to_string()).await
}

/// The work under each of `groups`, by group; a group with none is left out.
pub async fn of_groups(
    repo: &Repository,
    groups: &[Uuid],
) -> Result<HashMap<Uuid, Vec<Tally>>, StoreError> {
    if groups.is_empty() {
        return Ok(HashMap::new());
    }
    let ids: Vec<String> = groups.iter().map(Uuid::to_string).collect();
    let ids =
        serde_json::to_string(&ids).map_err(|error| StoreError::corrupt(TABLE, "id", error))?;
    tallies_where(repo, "parent_id IN (SELECT value FROM json_each(?))", &ids).await
}

/// The work under one group.
pub async fn of_parent(repo: &Repository, group_id: Uuid) -> Result<Vec<Tally>, StoreError> {
    Ok(of_groups(repo, &[group_id])
        .await?
        .remove(&group_id)
        .unwrap_or_default())
}
