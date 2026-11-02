use crate::{Repository, StoreError};

/// One task a team holds, as its runner last reported it. `current_step` stays the raw JSON text:
/// the caller owns parsing it.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct TeamTaskRow {
    pub harness: String,
    pub team: String,
    pub task_ref: String,
    pub runner_id: String,
    pub on_board: Option<String>,
    pub state: String,
    pub waits_on: Option<String>,
    pub current_step: Option<String>,
    pub changed_at: Option<String>,
    pub reported_at: String,
}

/// One step a task finished; `step` is the raw JSON text.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct TeamTaskStepRow {
    pub task_ref: String,
    pub finished_at: String,
    pub step: String,
}

/// The tasks a team holds, the newest change first. `harness` narrows to one agent when two
/// report a team of the same name.
pub async fn team_tasks(
    repo: &Repository,
    team: &str,
    harness: Option<&str>,
) -> Result<Vec<TeamTaskRow>, StoreError> {
    Ok(sqlx::query_as::<_, TeamTaskRow>(
        "SELECT harness,team,task_ref,runner_id,on_board,state,waits_on,current_step,changed_at,reported_at
         FROM team_tasks
         WHERE team = ? AND (? IS NULL OR harness = ?)
         ORDER BY COALESCE(changed_at, reported_at) DESC, task_ref",
    )
    .bind(team)
    .bind(harness)
    .bind(harness)
    .fetch_all(repo.pool())
    .await?)
}

/// The steps the tasks of a team finished, oldest first within each task.
pub async fn team_task_steps(
    repo: &Repository,
    harness: &str,
    team: &str,
) -> Result<Vec<TeamTaskStepRow>, StoreError> {
    Ok(sqlx::query_as::<_, TeamTaskStepRow>(
        "SELECT task_ref,finished_at,step FROM team_task_steps
         WHERE harness = ? AND team = ? ORDER BY task_ref, position",
    )
    .bind(harness)
    .bind(team)
    .fetch_all(repo.pool())
    .await?)
}

/// The project of the item the board started each task of a team for. `None` for work the board
/// did not start, and for a task whose item no longer exists.
pub async fn team_task_projects(
    repo: &Repository,
    team: &str,
    harness: &str,
) -> Result<Vec<(String, Option<String>)>, StoreError> {
    Ok(sqlx::query_as(
        "SELECT t.task_ref, i.project_id
         FROM team_tasks t LEFT JOIN items i ON i.id = t.on_board
         WHERE t.team = ? AND t.harness = ?",
    )
    .bind(team)
    .bind(harness)
    .fetch_all(repo.pool())
    .await?)
}
