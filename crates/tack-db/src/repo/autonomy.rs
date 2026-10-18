//! A project's autonomy settings (migration 087's columns on `projects`,
//! `autonomy_domains` and `project_budgets`), the merge ledger (`tack_merges`)
//! and the queries the automatic merge and demotion read. Policy lives in
//! `tack_core::autonomy`; nothing here decides anything.

use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use tack_core::autonomy::{Domain, Level};

use super::budgets::{self, ProjectBudgets};

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

fn level_of(text: &str) -> Level {
    text.parse().unwrap_or(Level::L3)
}

fn list_of(json: &str) -> Vec<String> {
    serde_json::from_str(json).unwrap_or_default()
}

fn to_json(list: &[String]) -> String {
    serde_json::to_string(list).unwrap_or_else(|_| "[]".to_owned())
}

/// The project's settings; `None` when the project does not exist.
pub async fn get_settings(
    pool: &SqlitePool,
    project_id: &str,
) -> Result<Option<Settings>, sqlx::Error> {
    let row: Option<(String, String, Option<String>, i64, i64)> = sqlx::query_as(
        "SELECT autonomy_level, verify_require, attestation_public_key,
                verify_loop_max_attempts, merge_on_done
         FROM projects WHERE id = ?",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await?;
    let Some((level, require, key, loop_max, on_done)) = row else {
        return Ok(None);
    };
    let domains: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT name, path_globs, item_types, level FROM autonomy_domains
         WHERE project_id = ? ORDER BY name",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    Ok(Some(Settings {
        level: level_of(&level),
        domains: domains
            .into_iter()
            .map(|(name, globs, types, level)| Domain {
                name,
                path_globs: list_of(&globs),
                item_types: list_of(&types),
                level: level_of(&level),
            })
            .collect(),
        verify_require: list_of(&require),
        attestation_public_key: key,
        verify_loop_max_attempts: loop_max,
        budgets: budgets::get(pool, project_id)
            .await?
            .unwrap_or(ProjectBudgets {
                tokens_per_day: None,
                auto_merges_per_day: None,
                queue_items_max: None,
            }),
        merge_on_done: on_done != 0,
    }))
}

/// Replace the project's settings, its domains all at once. Returns whether the
/// project exists.
pub async fn put_settings(
    pool: &SqlitePool,
    project_id: &str,
    settings: &Settings,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
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
    budgets::put(pool, project_id, &settings.budgets).await?;
    Ok(true)
}

/// Per domain: the changes Tack merged by itself, and how many of those were
/// undone (reverted or CI red, which demoted the domain).
pub async fn domain_counts(
    pool: &SqlitePool,
    project_id: &str,
) -> Result<std::collections::HashMap<String, (i64, i64)>, sqlx::Error> {
    let rows: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT domain, SUM(merged_at IS NOT NULL), SUM(demoted_at IS NOT NULL)
         FROM tack_merges
         WHERE project_id = ? AND trigger = 'auto' AND domain IS NOT NULL
         GROUP BY domain",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(d, m, u)| (d, (m, u))).collect())
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

pub async fn get_merge(
    pool: &SqlitePool,
    attempt_id: &str,
) -> Result<Option<MergeRow>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM tack_merges WHERE attempt_id = ?")
        .bind(attempt_id)
        .fetch_optional(pool)
        .await
}

/// Write the attempt's ledger row, replacing the earlier check. `demoted_at`
/// is never touched here.
pub async fn record_merge(pool: &SqlitePool, row: &MergeRow) -> Result<(), sqlx::Error> {
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
    .execute(pool)
    .await?;
    Ok(())
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

pub async fn attempt_context(
    pool: &SqlitePool,
    attempt_id: &str,
) -> Result<Option<AttemptContext>, sqlx::Error> {
    sqlx::query_as(
        "SELECT r.id AS request_id, i.id AS item_id, i.project_id, i.item_type,
                a.actual_execution, r.request_snapshot
         FROM execution_attempts a
         JOIN execution_requests r ON r.id = a.request_id
         JOIN items i ON i.id = r.item_id
         WHERE a.id = ?",
    )
    .bind(attempt_id)
    .fetch_optional(pool)
    .await
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

pub async fn attempt_ceiling_inputs(
    pool: &SqlitePool,
    attempt_id: &str,
) -> Result<Option<AttemptCeilingInputs>, sqlx::Error> {
    sqlx::query_as(
        "SELECT ru.capability_snapshot AS capability_snapshot,
                json_extract(r.request_snapshot, '$.requested_harness_kind') AS harness_kind,
                json_extract(r.request_snapshot, '$.resolved_agent_profile.tool_policy.docket.team') AS team
         FROM execution_attempts a
         JOIN execution_requests r ON r.id = a.request_id
         JOIN agent_runners ru ON ru.id = a.runner_id
         WHERE a.id = ?",
    )
    .bind(attempt_id)
    .fetch_optional(pool)
    .await
}

/// The profiles a project's roles point at, as (profile name, docket team).
pub async fn role_profile_teams(
    pool: &SqlitePool,
    project_id: &str,
) -> Result<Vec<(String, Option<String>)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT DISTINCT p.name, json_extract(p.tool_policy, '$.docket.team')
         FROM roles r JOIN agent_profiles p ON p.id = r.agent_profile_id
         WHERE r.project_id = ? ORDER BY p.name",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
}

/// The capability snapshot of every active, unrevoked runner.
pub async fn active_runner_snapshots(pool: &SqlitePool) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT capability_snapshot FROM agent_runners
         WHERE state = 'active' AND revoked_at IS NULL ORDER BY name",
    )
    .fetch_all(pool)
    .await
}

/// Attempts the automatic merge looks at: succeeded, with a merge-readiness
/// pack, no human review and no merge yet, in a project (or a domain of one)
/// above L3. An attempt whose ledger row belongs to a human trigger is theirs.
/// Only the newest finished attempt of an unfinished item's newest request is
/// one: a run a newer one replaced, or an item already Done, is never merged.
pub async fn auto_candidates(pool: &SqlitePool) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT a.id FROM execution_attempts a
         JOIN execution_requests r ON r.id = a.request_id
         JOIN items i ON i.id = r.item_id
         JOIN projects p ON p.id = i.project_id
         WHERE a.state = 'succeeded'
           AND p.archived = 0
           AND i.completed_at IS NULL
           AND r.id = (SELECT r2.id FROM execution_requests r2 WHERE r2.item_id = i.id
                        ORDER BY r2.created_at DESC, r2.rowid DESC LIMIT 1)
           AND a.id = (SELECT a2.id FROM execution_attempts a2 WHERE a2.request_id = r.id
                         AND a2.state IN ('succeeded','failed','cancelled')
                        ORDER BY a2.attempt_number DESC LIMIT 1)
           AND (p.autonomy_level != 'L3' OR EXISTS (
                SELECT 1 FROM autonomy_domains d WHERE d.project_id = p.id AND d.level != 'L3'))
           AND EXISTS (SELECT 1 FROM execution_artifacts x
                       WHERE x.attempt_id = a.id AND x.media_type = 'application/vnd.tack.mrp+json')
           AND NOT EXISTS (SELECT 1 FROM mrp_reviews m WHERE m.attempt_id = a.id)
           AND NOT EXISTS (SELECT 1 FROM attempt_reviews v WHERE v.attempt_id = a.id)
           AND NOT EXISTS (SELECT 1 FROM tack_merges t WHERE t.attempt_id = a.id
                           AND (t.merged_at IS NOT NULL OR t.trigger != 'auto'))
         ORDER BY a.created_at",
    )
    .fetch_all(pool)
    .await
}

/// Attempts a Done move asked to merge that are waiting for CI.
pub async fn held_done_candidates(pool: &SqlitePool) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT attempt_id FROM tack_merges
         WHERE trigger = 'done' AND merged_at IS NULL AND ci_state = 'pending'
         ORDER BY checked_at",
    )
    .fetch_all(pool)
    .await
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

pub async fn pending_demotions(pool: &SqlitePool) -> Result<Vec<PendingDemotion>, sqlx::Error> {
    sqlx::query_as(
        "SELECT m.attempt_id, m.project_id, m.domain, r.item_id, pr.repo, pr.number,
                pr.state = 'reverted' AS reverted
         FROM tack_merges m
         JOIN pull_requests pr ON pr.attempt_id = m.attempt_id
         JOIN execution_attempts a ON a.id = m.attempt_id
         JOIN execution_requests r ON r.id = a.request_id
         WHERE m.trigger = 'auto' AND m.merged_at IS NOT NULL AND m.demoted_at IS NULL
           AND (pr.state = 'reverted' OR pr.red_ci_at IS NOT NULL)",
    )
    .fetch_all(pool)
    .await
}

/// Take the merge's domain (the project, when it had none) one level down and
/// stamp `demoted_at`. Returns the levels before and after, or `None` when the
/// domain no longer exists.
pub async fn demote(
    pool: &SqlitePool,
    pending: &PendingDemotion,
    now: DateTime<Utc>,
) -> Result<Option<(Level, Level)>, sqlx::Error> {
    let mut tx = pool.begin().await?;
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

/// The newest uploaded artifact of the attempt whose `kind` is `kind`: its id
/// and the reference to its stored content.
pub async fn newest_artifact_of_kind(
    pool: &SqlitePool,
    attempt_id: &str,
    kind: &str,
) -> Result<Option<(String, String)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT artifact_id, content_reference FROM execution_artifacts
         WHERE attempt_id = ? AND kind = ? AND content_reference IS NOT NULL
         ORDER BY created_at DESC, rowid DESC LIMIT 1",
    )
    .bind(attempt_id)
    .bind(kind)
    .fetch_optional(pool)
    .await
}

/// The newest uploaded artifact of the attempt with this media type.
pub async fn newest_artifact_of_media_type(
    pool: &SqlitePool,
    attempt_id: &str,
    media_type: &str,
) -> Result<Option<(String, String)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT artifact_id, content_reference FROM execution_artifacts
         WHERE attempt_id = ? AND media_type = ? AND content_reference IS NOT NULL
         ORDER BY created_at DESC, rowid DESC LIMIT 1",
    )
    .bind(attempt_id)
    .bind(media_type)
    .fetch_optional(pool)
    .await
}
