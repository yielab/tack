use std::collections::HashMap;

use tack_core::autonomy::Domain;

use crate::repo::autonomy::{
    AttemptCeilingInputs, AttemptContext, MergeRow, PendingDemotion, Settings, level_of, list_of,
};
use crate::{Repository, StoreError};

/// The Automation settings of a project, `None` when the project does not exist.
pub async fn project_settings(
    repo: &Repository,
    project_id: &str,
) -> Result<Option<Settings>, StoreError> {
    let row: Option<(String, String, Option<String>, i64, i64)> = sqlx::query_as(
        "SELECT autonomy_level, verify_require, attestation_public_key,
                verify_loop_max_attempts, merge_on_done
         FROM projects WHERE id = ?",
    )
    .bind(project_id)
    .fetch_optional(repo.pool())
    .await?;
    let Some((level, require, key, loop_max, on_done)) = row else {
        return Ok(None);
    };
    let domains: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT name, path_globs, item_types, level FROM autonomy_domains
         WHERE project_id = ? ORDER BY name",
    )
    .bind(project_id)
    .fetch_all(repo.pool())
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
        budgets: super::budgets::project_budgets(repo, project_id)
            .await?
            .unwrap_or_default(),
        merge_on_done: on_done != 0,
    }))
}

/// Per domain: the changes Tack merged by itself, and how many of those were undone (reverted
/// or CI red, which demoted the domain).
pub async fn domain_counts(
    repo: &Repository,
    project_id: &str,
) -> Result<HashMap<String, (i64, i64)>, StoreError> {
    let rows: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT domain, SUM(merged_at IS NOT NULL), SUM(demoted_at IS NOT NULL)
         FROM tack_merges
         WHERE project_id = ? AND trigger = 'auto' AND domain IS NOT NULL
         GROUP BY domain",
    )
    .bind(project_id)
    .fetch_all(repo.pool())
    .await?;
    Ok(rows.into_iter().map(|(d, m, u)| (d, (m, u))).collect())
}

/// The profiles a project's tasks run with, as (profile name, docket team): the profile of each
/// role, and the project's default profile, which a task with no role runs with.
pub async fn project_profile_teams(
    repo: &Repository,
    project_id: &str,
) -> Result<Vec<(String, Option<String>)>, StoreError> {
    Ok(sqlx::query_as(
        "SELECT DISTINCT p.name, json_extract(p.tool_policy, '$.docket.team')
         FROM agent_profiles p
         WHERE p.id IN (SELECT agent_profile_id FROM roles WHERE project_id = ?1)
            OR p.id = (SELECT default_profile_id FROM projects WHERE id = ?1)
         ORDER BY p.name",
    )
    .bind(project_id)
    .fetch_all(repo.pool())
    .await?)
}

/// The capability snapshot of every active, unrevoked runner.
pub async fn active_runner_snapshots(repo: &Repository) -> Result<Vec<String>, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT capability_snapshot FROM agent_runners
         WHERE state = 'active' AND revoked_at IS NULL ORDER BY name",
    )
    .fetch_all(repo.pool())
    .await?)
}

/// The run an attempt belongs to, as the merge reads it; `None` for an unknown attempt.
pub async fn attempt_context(
    repo: &Repository,
    attempt_id: &str,
) -> Result<Option<AttemptContext>, StoreError> {
    Ok(sqlx::query_as(
        "SELECT r.id AS request_id, i.id AS item_id, i.project_id, i.item_type,
                a.actual_execution, r.request_snapshot
         FROM execution_attempts a
         JOIN execution_requests r ON r.id = a.request_id
         JOIN items i ON i.id = r.item_id
         WHERE a.id = ?",
    )
    .bind(attempt_id)
    .fetch_optional(repo.pool())
    .await?)
}

/// The merge ledger's row of an attempt, `None` when the merge was never tried.
pub async fn merge_of_attempt(
    repo: &Repository,
    attempt_id: &str,
) -> Result<Option<MergeRow>, StoreError> {
    Ok(
        sqlx::query_as("SELECT * FROM tack_merges WHERE attempt_id = ?")
            .bind(attempt_id)
            .fetch_optional(repo.pool())
            .await?,
    )
}

/// What the team's ceiling is read from for one attempt.
pub async fn attempt_ceiling_inputs(
    repo: &Repository,
    attempt_id: &str,
) -> Result<Option<AttemptCeilingInputs>, StoreError> {
    Ok(sqlx::query_as(
        "SELECT ru.capability_snapshot AS capability_snapshot,
                json_extract(r.request_snapshot, '$.requested_harness_kind') AS harness_kind,
                json_extract(r.request_snapshot, '$.resolved_agent_profile.tool_policy.docket.team') AS team
         FROM execution_attempts a
         JOIN execution_requests r ON r.id = a.request_id
         JOIN agent_runners ru ON ru.id = a.runner_id
         WHERE a.id = ?",
    )
    .bind(attempt_id)
    .fetch_optional(repo.pool())
    .await?)
}

/// The automatic merges whose pull request was reverted or went red, not yet charged to their
/// domain.
pub async fn pending_demotions(repo: &Repository) -> Result<Vec<PendingDemotion>, StoreError> {
    Ok(sqlx::query_as(
        "SELECT m.attempt_id, m.project_id, m.domain, r.item_id, pr.repo, pr.number,
                pr.state = 'reverted' AS reverted
         FROM tack_merges m
         JOIN pull_requests pr ON pr.attempt_id = m.attempt_id
         JOIN execution_attempts a ON a.id = m.attempt_id
         JOIN execution_requests r ON r.id = a.request_id
         WHERE m.trigger = 'auto' AND m.merged_at IS NOT NULL AND m.demoted_at IS NULL
           AND (pr.state = 'reverted' OR pr.red_ci_at IS NOT NULL)",
    )
    .fetch_all(repo.pool())
    .await?)
}

/// The attempts a move to Done tried to merge and left held, for the tick to retry.
pub async fn held_done_candidates(repo: &Repository) -> Result<Vec<String>, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT attempt_id FROM tack_merges
         WHERE trigger = 'done' AND merged_at IS NULL AND ci_state = 'pending'
         ORDER BY checked_at",
    )
    .fetch_all(repo.pool())
    .await?)
}

/// The attempts that may merge by themselves now: succeeded, with a merge-readiness pack, no
/// human review and no merge yet, in a project (or a domain of one) above L3. An attempt whose
/// ledger row belongs to a human trigger is theirs. Only the newest finished attempt of an
/// unfinished item's newest request is one: a run a newer one replaced, or an item already Done,
/// is never merged.
pub async fn auto_candidates(repo: &Repository) -> Result<Vec<String>, StoreError> {
    Ok(sqlx::query_scalar(
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
    .fetch_all(repo.pool())
    .await?)
}

/// The newest uploaded artifact of the attempt whose kind is `kind`: its id and the reference
/// to its stored content.
pub async fn newest_artifact_of_kind(
    repo: &Repository,
    attempt_id: &str,
    kind: &str,
) -> Result<Option<(String, String)>, StoreError> {
    Ok(sqlx::query_as(
        "SELECT artifact_id, content_reference FROM execution_artifacts
         WHERE attempt_id = ? AND kind = ? AND content_reference IS NOT NULL
         ORDER BY created_at DESC, rowid DESC LIMIT 1",
    )
    .bind(attempt_id)
    .bind(kind)
    .fetch_optional(repo.pool())
    .await?)
}

/// The newest uploaded artifact of the attempt with this media type: its id and the reference
/// to its stored content.
pub async fn newest_artifact_of_media_type(
    repo: &Repository,
    attempt_id: &str,
    media_type: &str,
) -> Result<Option<(String, String)>, StoreError> {
    Ok(sqlx::query_as(
        "SELECT artifact_id, content_reference FROM execution_artifacts
         WHERE attempt_id = ? AND media_type = ? AND content_reference IS NOT NULL
         ORDER BY created_at DESC, rowid DESC LIMIT 1",
    )
    .bind(attempt_id)
    .bind(media_type)
    .fetch_optional(repo.pool())
    .await?)
}

/// How many merge ledger rows came from `trigger` (`button`, `done` or `auto`).
pub async fn merges_by_trigger(repo: &Repository, trigger: &str) -> Result<i64, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM tack_merges WHERE trigger = ?")
            .bind(trigger)
            .fetch_one(repo.pool())
            .await?,
    )
}
