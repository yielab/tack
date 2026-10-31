//! A project's autonomy settings (migration 087's columns on `projects`,
//! `autonomy_domains` and `project_budgets`), the merge ledger (`tack_merges`)
//! and the rows the automatic merge and demotion read and write. Policy lives in
//! `tack_core::autonomy`; nothing here decides anything.

use chrono::{DateTime, Utc};
use tack_core::autonomy::{Domain, Level};

use super::budgets::ProjectBudgets;

/// Everything the Automation panel edits for one project.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub level: Level,
    pub domains: Vec<Domain>,
    pub verify_require: Vec<String>,
    pub attestation_public_key: Option<String>,
    pub verify_loop_max_attempts: i64,
    pub budgets: ProjectBudgets,
    pub merge_on_done: bool,
}

pub(crate) fn level_of(text: &str) -> Level {
    text.parse().unwrap_or(Level::L3)
}

pub(crate) fn list_of(json: &str) -> Vec<String> {
    serde_json::from_str(json).unwrap_or_default()
}

fn to_json(list: &[String]) -> String {
    serde_json::to_string(list).unwrap_or_else(|_| "[]".to_owned())
}

/// One row of the merge ledger: a merge Tack made, or the first condition that
/// kept it from making one.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct MergeRow {
    pub attempt_id: String,
    pub project_id: String,
    pub trigger: String,
    pub domain: Option<String>,
    pub level: Option<String>,
    pub policy_snapshot: Option<String>,
    pub pack_artifact_id: Option<String>,
    pub attestation_keyid: Option<String>,
    pub ci_state: Option<String>,
    pub held_reason: Option<String>,
    pub checked_at: String,
    pub merged_at: Option<String>,
    pub merge_sha: Option<String>,
    pub demoted_at: Option<String>,
}

/// The run an attempt belongs to, as the merge reads it.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AttemptContext {
    pub request_id: String,
    pub item_id: String,
    pub project_id: String,
    pub item_type: String,
    pub actual_execution: Option<String>,
    pub request_snapshot: String,
}

/// What the team's ceiling is read from, for one attempt: the capability
/// snapshot of the runner that ran it, the harness the request asked for and the
/// docket team its resolved profile names.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AttemptCeilingInputs {
    pub capability_snapshot: String,
    pub harness_kind: Option<String>,
    pub team: Option<String>,
}

/// An automatic merge whose pull request was reverted or went red on its merge
/// commit and has not yet cost its domain a level.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PendingDemotion {
    pub attempt_id: String,
    pub project_id: String,
    pub domain: Option<String>,
    pub item_id: String,
    pub repo: String,
    pub number: i64,
    /// Reverted, as against CI gone red on the merge commit.
    pub reverted: bool,
}

impl crate::Repository {
    /// Replace the project's settings, its domains all at once. Whether the project exists.
    pub async fn put_autonomy_settings(
        &self,
        project_id: &str,
        settings: &Settings,
    ) -> Result<bool, crate::StoreError> {
        let mut tx = self.pool().begin().await?;
        let updated = sqlx::query(
            "UPDATE projects SET autonomy_level = ?, verify_require = ?, attestation_public_key = ?,
                    verify_loop_max_attempts = ?, merge_on_done = ? WHERE id = ?",
        )
        .bind(settings.level.to_string())
        .bind(to_json(&settings.verify_require))
        .bind(&settings.attestation_public_key)
        .bind(settings.verify_loop_max_attempts)
        .bind(i64::from(settings.merge_on_done))
        .bind(project_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if updated == 0 {
            return Ok(false);
        }
        // A held merge keeps the key id it was verified with so a tick need not run
        // prill again; any change here may replace the key, so every unmerged
        // attempt of the project is verified afresh.
        sqlx::query(
            "UPDATE tack_merges SET attestation_keyid = NULL WHERE project_id = ? AND merged_at IS NULL",
        )
        .bind(project_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM autonomy_domains WHERE project_id = ?")
            .bind(project_id)
            .execute(&mut *tx)
            .await?;
        for domain in &settings.domains {
            sqlx::query(
                "INSERT INTO autonomy_domains (id, project_id, name, path_globs, item_types, level)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(project_id)
            .bind(&domain.name)
            .bind(to_json(&domain.path_globs))
            .bind(to_json(&domain.item_types))
            .bind(domain.level.to_string())
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        self.put_project_budgets(project_id, &settings.budgets)
            .await?;
        Ok(true)
    }

    /// Write the attempt's ledger row, replacing the earlier check. `demoted_at` is never
    /// touched here.
    pub async fn record_merge(&self, row: &MergeRow) -> Result<(), crate::StoreError> {
        sqlx::query(
            "INSERT INTO tack_merges (attempt_id, project_id, trigger, domain, level, policy_snapshot,
                pack_artifact_id, attestation_keyid, ci_state, held_reason, checked_at, merged_at, merge_sha)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(attempt_id) DO UPDATE SET
                trigger = excluded.trigger, domain = excluded.domain, level = excluded.level,
                policy_snapshot = excluded.policy_snapshot, pack_artifact_id = excluded.pack_artifact_id,
                attestation_keyid = excluded.attestation_keyid, ci_state = excluded.ci_state,
                held_reason = excluded.held_reason, checked_at = excluded.checked_at,
                merged_at = excluded.merged_at, merge_sha = excluded.merge_sha",
        )
        .bind(&row.attempt_id)
        .bind(&row.project_id)
        .bind(&row.trigger)
        .bind(&row.domain)
        .bind(&row.level)
        .bind(&row.policy_snapshot)
        .bind(&row.pack_artifact_id)
        .bind(&row.attestation_keyid)
        .bind(&row.ci_state)
        .bind(&row.held_reason)
        .bind(&row.checked_at)
        .bind(&row.merged_at)
        .bind(&row.merge_sha)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Take the merge's domain (the project, when it had none) one level down and stamp
    /// `demoted_at`. The levels before and after, `None` when the domain no longer exists.
    pub async fn demote_after_revert(
        &self,
        pending: &PendingDemotion,
        now: DateTime<Utc>,
    ) -> Result<Option<(Level, Level)>, crate::StoreError> {
        let mut tx = self.pool().begin().await?;
        let current: Option<String> = match &pending.domain {
            Some(name) => {
                sqlx::query_scalar(
                    "SELECT level FROM autonomy_domains WHERE project_id = ? AND name = ?",
                )
                .bind(&pending.project_id)
                .bind(name)
                .fetch_optional(&mut *tx)
                .await?
            }
            None => {
                sqlx::query_scalar("SELECT autonomy_level FROM projects WHERE id = ?")
                    .bind(&pending.project_id)
                    .fetch_optional(&mut *tx)
                    .await?
            }
        };
        let outcome = current.map(|text| {
            let from = level_of(&text);
            (from, from.demoted())
        });
        if let Some((_, to)) = outcome {
            match &pending.domain {
                Some(name) => {
                    sqlx::query(
                        "UPDATE autonomy_domains SET level = ?, updated_at = datetime('now')
                         WHERE project_id = ? AND name = ?",
                    )
                    .bind(to.to_string())
                    .bind(&pending.project_id)
                    .bind(name)
                    .execute(&mut *tx)
                    .await?;
                }
                None => {
                    sqlx::query("UPDATE projects SET autonomy_level = ? WHERE id = ?")
                        .bind(to.to_string())
                        .bind(&pending.project_id)
                        .execute(&mut *tx)
                        .await?;
                }
            }
        }
        sqlx::query("UPDATE tack_merges SET demoted_at = ? WHERE attempt_id = ?")
            .bind(now.to_rfc3339())
            .bind(&pending.attempt_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(outcome)
    }
}
