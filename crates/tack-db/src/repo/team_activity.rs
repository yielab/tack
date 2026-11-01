use crate::{Repository, StoreError};

/// A task a runner reported, ready to store. JSON values arrive as text, so this crate parses
/// none of them.
pub struct TeamTaskReport<'a> {
    pub runner_id: &'a str,
    pub harness: &'a str,
    pub team: &'a str,
    pub task_ref: &'a str,
    pub on_board: Option<&'a str>,
    pub state: &'a str,
    pub waits_on: Option<&'a str>,
    pub current_step: Option<&'a str>,
    pub changed_at: Option<&'a str>,
    pub reported_at: &'a str,
    /// The finished steps in order: when each ended, and the step as JSON.
    pub steps: Vec<(String, String)>,
}

impl Repository {
    /// Stores a task as reported, replacing what was held for it, in one transaction: the same
    /// report twice leaves the same rows.
    pub async fn record_team_task(&self, task: &TeamTaskReport<'_>) -> Result<(), StoreError> {
        let mut tx = self.pool().begin().await?;
        sqlx::query(
            "INSERT INTO team_tasks
                (harness,team,task_ref,runner_id,on_board,state,waits_on,current_step,changed_at,reported_at)
             VALUES (?,?,?,?,?,?,?,?,?,?)
             ON CONFLICT (harness,team,task_ref) DO UPDATE SET
                runner_id = excluded.runner_id, on_board = excluded.on_board, state = excluded.state,
                waits_on = excluded.waits_on, current_step = excluded.current_step,
                changed_at = excluded.changed_at, reported_at = excluded.reported_at",
        )
        .bind(task.harness)
        .bind(task.team)
        .bind(task.task_ref)
        .bind(task.runner_id)
        .bind(task.on_board)
        .bind(task.state)
        .bind(task.waits_on)
        .bind(task.current_step)
        .bind(task.changed_at)
        .bind(task.reported_at)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM team_task_steps WHERE harness = ? AND team = ? AND task_ref = ?")
            .bind(task.harness)
            .bind(task.team)
            .bind(task.task_ref)
            .execute(&mut *tx)
            .await?;
        for (position, (finished_at, step)) in (0_i64..).zip(&task.steps) {
            sqlx::query(
                "INSERT INTO team_task_steps (harness,team,task_ref,position,finished_at,step)
                 VALUES (?,?,?,?,?,?)",
            )
            .bind(task.harness)
            .bind(task.team)
            .bind(task.task_ref)
            .bind(position)
            .bind(finished_at)
            .bind(step)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Forgets the tasks in a state of `over` (the states a task does not leave) that last changed
    /// before `cutoff`, with their steps, and the steps of the rest that ended before it. Returns
    /// how many tasks went.
    pub async fn prune_team_activity(
        &self,
        cutoff: &str,
        over: &[&str],
    ) -> Result<u64, StoreError> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM team_task_steps WHERE finished_at < ?")
            .bind(cutoff)
            .execute(&mut *tx)
            .await?;
        let marks = vec!["?"; over.len()].join(",");
        let delete = format!(
            "DELETE FROM team_tasks WHERE state IN ({marks}) AND COALESCE(changed_at, reported_at) < ?"
        );
        let mut gone = sqlx::query(sqlx::AssertSqlSafe(delete));
        for state in over {
            gone = gone.bind(*state);
        }
        let gone = gone.bind(cutoff).execute(&mut *tx).await?.rows_affected();
        tx.commit().await?;
        Ok(gone)
    }
}
