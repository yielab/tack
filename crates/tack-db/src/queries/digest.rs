//! The rows the digest is computed from (ADR 0034, decision 6): each one the narrowest question
//! that answers a section, so a big board is not read whole to say a few lines about it.

use sqlx::AssertSqlSafe;
use tack_protocol::execution::ActivityState;

use crate::repo::items::needs_review_sql;
use crate::{Repository, StoreError};

/// An item that is not in a done status.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct OpenItemRow {
    pub id: String,
    pub title: String,
    pub status: String,
    pub updated_at: String,
}

/// The project's items in no status of `done`, in no particular order.
pub async fn open_items(
    repo: &Repository,
    project_id: &str,
    done: &[String],
) -> Result<Vec<OpenItemRow>, StoreError> {
    // The statuses travel as one JSON array, so their number never meets a limit on parameters.
    let done = serde_json::to_string(done).unwrap_or_else(|_| "[]".into());
    Ok(sqlx::query_as(
        "SELECT id, title, status, updated_at FROM items
         WHERE project_id = ? AND status NOT IN (SELECT value FROM json_each(?))",
    )
    .bind(project_id)
    .bind(done)
    .fetch_all(repo.pool())
    .await?)
}

/// An item that waits for a person, and since when.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct WaitRow {
    pub item_id: String,
    pub title: String,
    /// The run's state (`waiting_decision` or `needs_operator`), or `review` for finished work.
    pub kind: String,
    pub since: String,
}

/// What the project's runs wait on a person for: a run that asks, a run that needs the operator.
pub async fn runs_waiting_on_a_person(
    repo: &Repository,
    project_id: &str,
) -> Result<Vec<WaitRow>, StoreError> {
    Ok(sqlx::query_as(crate::repo::execution::state::sql(
        "SELECT r.item_id AS item_id, i.title AS title, r.state AS kind,
                COALESCE(r.waiting_since, r.updated_at) AS since
         FROM execution_requests r JOIN items i ON i.id = r.item_id
         WHERE i.project_id = ? AND r.state IN ({waiting_decision}, {needs_operator})",
    ))
    .bind(project_id)
    .fetch_all(repo.pool())
    .await?)
}

/// The project's unfinished items whose last run ended and left work nobody has reviewed, and
/// when that run ended.
pub async fn items_awaiting_review(
    repo: &Repository,
    project_id: &str,
) -> Result<Vec<WaitRow>, StoreError> {
    // The fragment is a constant of this crate; only `project_id` is bound.
    Ok(sqlx::query_as(AssertSqlSafe(format!(
        "SELECT id AS item_id, title, 'review' AS kind,
                COALESCE((SELECT r.updated_at FROM execution_requests r WHERE r.item_id = items.id
                          ORDER BY r.created_at DESC, r.rowid DESC LIMIT 1), updated_at) AS since
         FROM items WHERE project_id = ? AND completed_at IS NULL AND {}",
        needs_review_sql()
    )))
    .bind(project_id)
    .fetch_all(repo.pool())
    .await?)
}

/// A run that failed.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct FailedRunRow {
    pub id: String,
    pub item_id: String,
    pub title: String,
    pub failed_at: String,
    pub reason: Option<String>,
}

/// The project's runs that failed at or after `since` (an RFC 3339 instant). A request that
/// reached a terminal state is not written to again, so its `updated_at` is when it ended.
pub async fn failed_runs_since(
    repo: &Repository,
    project_id: &str,
    since: &str,
) -> Result<Vec<FailedRunRow>, StoreError> {
    Ok(sqlx::query_as(crate::repo::execution::state::sql(
        "SELECT r.id AS id, r.item_id AS item_id, i.title AS title, r.updated_at AS failed_at,
                r.waiting_reason AS reason
         FROM execution_requests r JOIN items i ON i.id = r.item_id
         WHERE i.project_id = ? AND r.state = {failed}
           AND julianday(r.updated_at) >= julianday(?)",
    ))
    .bind(project_id)
    .bind(since)
    .fetch_all(repo.pool())
    .await?)
}

/// The title of an item.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct TitleRow {
    pub id: String,
    pub title: String,
}

/// The titles of the items among `ids` that exist.
pub async fn item_titles(repo: &Repository, ids: &[String]) -> Result<Vec<TitleRow>, StoreError> {
    let ids = serde_json::to_string(ids).unwrap_or_else(|_| "[]".into());
    Ok(
        sqlx::query_as("SELECT id, title FROM items WHERE id IN (SELECT value FROM json_each(?))")
            .bind(ids)
            .fetch_all(repo.pool())
            .await?,
    )
}

/// A task a team holds that has not ended, with the item it was started for. `current_step`
/// stays the raw JSON text: the caller owns parsing it.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct OpenTeamTaskRow {
    pub harness: String,
    pub team: String,
    pub task_ref: String,
    pub item_id: Option<String>,
    pub project_id: Option<String>,
    pub title: Option<String>,
    pub state: String,
    pub waits_on: Option<String>,
    pub current_step: Option<String>,
    pub changed_at: Option<String>,
}

/// Every task of every team that is not over, with its item and the item's project when the
/// board started it for one that still exists.
pub async fn open_team_tasks(repo: &Repository) -> Result<Vec<OpenTeamTaskRow>, StoreError> {
    let over = ActivityState::OVER
        .iter()
        .map(|state| format!("'{}'", state.as_str()))
        .collect::<Vec<_>>()
        .join(",");
    // The fragment is built from the states' own names; nothing is bound into it.
    Ok(sqlx::query_as(AssertSqlSafe(format!(
        "SELECT t.harness AS harness, t.team AS team, t.task_ref AS task_ref,
                i.id AS item_id, i.project_id AS project_id, i.title AS title,
                t.state AS state, t.waits_on AS waits_on, t.current_step AS current_step,
                t.changed_at AS changed_at
         FROM team_tasks t LEFT JOIN items i ON i.id = t.on_board
         WHERE t.state NOT IN ({over})"
    )))
    .fetch_all(repo.pool())
    .await?)
}
