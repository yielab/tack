//! Rows a test plants directly, so a test that reads a route's answer never
//! needs to name SQL. One function per table, named for the table. They write
//! below the store's own commands, which is the point: the test controls the
//! column the route under test reads (a `created_at`, a stored state).

use tack_db::Repository;
use tack_db::repo::execution::state;
use tack_protocol::execution::ExecutionState;
use uuid::Uuid;

/// One `execution_artifacts` manifest row for `attempt_id`, created at `created_at`.
pub async fn execution_artifact(
    repo: &Repository,
    attempt_id: &str,
    artifact_id: &str,
    created_at: &str,
) {
    sqlx::query(
        "INSERT INTO execution_artifacts \
         (id, attempt_id, artifact_id, kind, name, size_bytes, sha256, created_at) \
         VALUES (?, ?, ?, 'patch', 'changes.patch', 42, ?, ?)",
    )
    .bind(format!("art_row_{}", Uuid::new_v4()))
    .bind(attempt_id)
    .bind(artifact_id)
    .bind("a".repeat(64))
    .bind(created_at)
    .execute(repo.pool())
    .await
    .expect("seed execution_artifacts");
}

/// One pending `execution_decisions` row for `attempt_id`, created at `created_at`.
pub async fn execution_decision(
    repo: &Repository,
    attempt_id: &str,
    decision_id: &str,
    created_at: &str,
) {
    sqlx::query(
        "INSERT INTO execution_decisions \
         (id, attempt_id, decision_id, kind, prompt, created_at, updated_at) \
         VALUES (?, ?, ?, 'approval', 'Proceed?', ?, ?)",
    )
    .bind(format!("dec_row_{}", Uuid::new_v4()))
    .bind(attempt_id)
    .bind(decision_id)
    .bind(created_at)
    .bind(created_at)
    .execute(repo.pool())
    .await
    .expect("seed execution_decisions");
}

/// Puts an `execution_attempts` row in `state` with `terminal_reason` (raw JSON text).
pub async fn execution_attempt_ended(
    repo: &Repository,
    attempt_id: &str,
    state: ExecutionState,
    terminal_reason: &str,
) {
    sqlx::query("UPDATE execution_attempts SET state = ?, terminal_reason = ? WHERE id = ?")
        .bind(state::text(state))
        .bind(terminal_reason)
        .bind(attempt_id)
        .execute(repo.pool())
        .await
        .expect("seed execution_attempts state");
}

/// Stores `usage` (raw JSON text) on an `execution_attempts` row.
pub async fn execution_attempt_usage(repo: &Repository, attempt_id: &str, usage: &str) {
    sqlx::query("UPDATE execution_attempts SET usage = ? WHERE id = ?")
        .bind(usage)
        .bind(attempt_id)
        .execute(repo.pool())
        .await
        .expect("seed execution_attempts usage");
}

// ─── agent_profiles, agent_runners, roles ───────────────────────────────────

/// Removes every `agent_profiles` row, the built-in ones included.
pub async fn agent_profiles_cleared(repo: &Repository) {
    sqlx::query("DELETE FROM agent_profiles")
        .execute(repo.pool())
        .await
        .expect("clear agent_profiles");
}

/// Removes one `agent_profiles` row whatever its kind.
pub async fn agent_profile_removed(repo: &Repository, profile_id: &str) {
    sqlx::query("DELETE FROM agent_profiles WHERE id = ?")
        .bind(profile_id)
        .execute(repo.pool())
        .await
        .expect("remove agent_profiles row");
}

/// Marks a runner revoked at `revoked_at` without touching its enrollment tokens.
pub async fn agent_runner_revoked(repo: &Repository, runner_id: &str, revoked_at: &str) {
    sqlx::query("UPDATE agent_runners SET state='revoked', revoked_at=? WHERE id=?")
        .bind(revoked_at)
        .bind(runner_id)
        .execute(repo.pool())
        .await
        .expect("seed agent_runners revoked");
}

/// Stores `heartbeat_at` as the last heartbeat of one runner.
pub async fn agent_runner_heartbeat(repo: &Repository, runner_id: &str, heartbeat_at: &str) {
    sqlx::query("UPDATE agent_runners SET last_heartbeat_at = ? WHERE id = ?")
        .bind(heartbeat_at)
        .bind(runner_id)
        .execute(repo.pool())
        .await
        .expect("seed agent_runners heartbeat");
}

/// Clears the last heartbeat of every runner, so none reads as online.
pub async fn agent_runners_silent(repo: &Repository) {
    sqlx::query("UPDATE agent_runners SET last_heartbeat_at = NULL")
        .execute(repo.pool())
        .await
        .expect("seed agent_runners silent");
}

/// Renames a runner directly, bypassing the unique-name checks of a route.
pub async fn agent_runner_named(repo: &Repository, runner_id: &str, name: &str) {
    sqlx::query("UPDATE agent_runners SET name = ? WHERE id = ?")
        .bind(name)
        .bind(runner_id)
        .execute(repo.pool())
        .await
        .expect("seed agent_runners name");
}

/// A write lock held on the `agent_runners` table, released by [`RunnerRowHold::release`].
pub struct RunnerRowHold(sqlx::Transaction<'static, sqlx::Sqlite>);

impl RunnerRowHold {
    /// Commits the held transaction and lets blocked writers go on.
    pub async fn release(self) {
        self.0.commit().await.expect("release the hold");
    }
}

/// Opens a `BEGIN IMMEDIATE` transaction that has written to `runner_id`'s row
/// and keeps it open, so any other writer of that row waits.
pub async fn agent_runner_row_held(repo: &Repository, runner_id: &str) -> RunnerRowHold {
    let mut tx = repo
        .pool()
        .begin_with("BEGIN IMMEDIATE")
        .await
        .expect("hold the runner row");
    sqlx::query("UPDATE agent_runners SET updated_at=updated_at WHERE id=?")
        .bind(runner_id)
        .execute(&mut *tx)
        .await
        .expect("no-op hold write");
    RunnerRowHold(tx)
}

/// Points a role at an agent profile.
pub async fn role_agent_profile(repo: &Repository, role_id: &str, profile_id: &str) {
    sqlx::query("UPDATE roles SET agent_profile_id = ? WHERE id = ?")
        .bind(profile_id)
        .bind(role_id)
        .execute(repo.pool())
        .await
        .expect("seed roles agent_profile_id");
}

// ─── items and projects ─────────────────────────────────────────────────────

/// Sets an item's description.
pub async fn item_description(repo: &Repository, item_id: &str, description: &str) {
    sqlx::query("UPDATE items SET description = ? WHERE id = ?")
        .bind(description)
        .bind(item_id)
        .execute(repo.pool())
        .await
        .expect("seed items description");
}

/// Sets an item's title and description.
pub async fn item_card(repo: &Repository, item_id: &str, title: &str, description: &str) {
    sqlx::query("UPDATE items SET description = ?, title = ? WHERE id = ?")
        .bind(description)
        .bind(title)
        .bind(item_id)
        .execute(repo.pool())
        .await
        .expect("seed items card");
}

/// Sets an item's type and priority.
pub async fn item_type_and_priority(
    repo: &Repository,
    item_id: &str,
    item_type: &str,
    priority: &str,
) {
    sqlx::query("UPDATE items SET item_type = ?, priority = ? WHERE id = ?")
        .bind(item_type)
        .bind(priority)
        .bind(item_id)
        .execute(repo.pool())
        .await
        .expect("seed items type and priority");
}

/// Sets, or clears with `None`, a project's definition of done.
pub async fn project_definition_of_done(
    repo: &Repository,
    project_id: &str,
    definition_of_done: Option<&str>,
) {
    sqlx::query("UPDATE projects SET definition_of_done = ? WHERE id = ?")
        .bind(definition_of_done)
        .bind(project_id)
        .execute(repo.pool())
        .await
        .expect("seed projects definition_of_done");
}

/// Sets a project's default agent profile.
pub async fn project_default_profile(repo: &Repository, project_id: &str, profile_id: &str) {
    sqlx::query("UPDATE projects SET default_profile_id = ? WHERE id = ?")
        .bind(profile_id)
        .bind(project_id)
        .execute(repo.pool())
        .await
        .expect("seed projects default_profile_id");
}

/// Sets a project's default agent profile to the profile of `kind`.
pub async fn project_default_profile_of_kind(repo: &Repository, project_id: &str, kind: &str) {
    sqlx::query(
        "UPDATE projects SET default_profile_id = (SELECT id FROM agent_profiles WHERE kind = ?) WHERE id = ?",
    )
    .bind(kind)
    .bind(project_id)
    .execute(repo.pool())
    .await
    .expect("seed projects default_profile_id by kind");
}

/// Sets where a project's code lives, with `push_after_run` on and no default branch.
pub async fn project_code(
    repo: &Repository,
    project_id: &str,
    code_origin: &str,
    repository: Option<&str>,
    workspace_mode: &str,
) {
    sqlx::query("UPDATE projects SET code_origin=?, repository=?, default_branch=NULL, workspace_mode=?, push_after_run=1 WHERE id=?")
        .bind(code_origin)
        .bind(repository)
        .bind(workspace_mode)
        .bind(project_id)
        .execute(repo.pool())
        .await
        .expect("seed projects code");
}

/// Sets a project's install command.
pub async fn project_prepare_command(repo: &Repository, project_id: &str, command: &str) {
    sqlx::query("UPDATE projects SET prepare_command = ? WHERE id = ?")
        .bind(command)
        .bind(project_id)
        .execute(repo.pool())
        .await
        .expect("seed projects prepare_command");
}

/// Sets a project's default agent.
pub async fn project_default_harness(repo: &Repository, project_id: &str, harness: &str) {
    sqlx::query("UPDATE projects SET default_harness = ? WHERE id = ?")
        .bind(harness)
        .bind(project_id)
        .execute(repo.pool())
        .await
        .expect("seed projects default_harness");
}

/// Turns auto-start on for a project at `status`, running `harness` by default.
pub async fn project_auto_start(repo: &Repository, project_id: &str, status: &str, harness: &str) {
    sqlx::query(
        "UPDATE projects SET auto_start_enabled = 1, auto_start_status = ?, default_harness = ? WHERE id = ?",
    )
    .bind(status)
    .bind(harness)
    .bind(project_id)
    .execute(repo.pool())
    .await
    .expect("seed projects auto start");
}

/// Sets how many runs a project keeps open at once when auto-starting.
pub async fn project_auto_start_max_open(repo: &Repository, project_id: &str, max_open: i64) {
    sqlx::query("UPDATE projects SET auto_start_max_open = ? WHERE id = ?")
        .bind(max_open)
        .bind(project_id)
        .execute(repo.pool())
        .await
        .expect("seed projects auto_start_max_open");
}

// ─── execution requests, attempts, audits, merges ───────────────────────────

/// One `execution_attempts` row to plant: every timestamp column is `at`.
pub struct AttemptSeed<'a> {
    pub id: &'a str,
    pub request_id: &'a str,
    pub runner_id: &'a str,
    pub state: ExecutionState,
    pub at: &'a str,
    pub usage: Option<&'a str>,
    pub actual_execution: Option<&'a str>,
}

/// Plants an attempt number 1 with fencing token 1.
pub async fn execution_attempt(repo: &Repository, attempt: AttemptSeed<'_>) {
    sqlx::query(
        "INSERT INTO execution_attempts (id,request_id,attempt_number,runner_id,fencing_token,state,lease_issued_at,lease_expires_at,created_at,updated_at,usage,actual_execution) VALUES (?,?,1,?,1,?,?,?,?,?,?,?)",
    )
    .bind(attempt.id)
    .bind(attempt.request_id)
    .bind(attempt.runner_id)
    .bind(state::text(attempt.state))
    .bind(attempt.at)
    .bind(attempt.at)
    .bind(attempt.at)
    .bind(attempt.at)
    .bind(attempt.usage)
    .bind(attempt.actual_execution)
    .execute(repo.pool())
    .await
    .expect("seed execution_attempts");
}

/// Puts an `execution_attempts` row in `state`.
pub async fn execution_attempt_state(repo: &Repository, attempt_id: &str, state: ExecutionState) {
    sqlx::query("UPDATE execution_attempts SET state = ? WHERE id = ?")
        .bind(state::text(state))
        .bind(attempt_id)
        .execute(repo.pool())
        .await
        .expect("seed execution_attempts state");
}

/// Puts an `execution_requests` row in `state`.
pub async fn execution_request_state(repo: &Repository, request_id: &str, state: ExecutionState) {
    sqlx::query("UPDATE execution_requests SET state = ? WHERE id = ?")
        .bind(state::text(state))
        .bind(request_id)
        .execute(repo.pool())
        .await
        .expect("seed execution_requests state");
}

/// Stamps an `execution_requests` row as created and last updated at `at`.
pub async fn execution_request_created_at(repo: &Repository, request_id: &str, at: &str) {
    sqlx::query("UPDATE execution_requests SET created_at = ?, updated_at = ? WHERE id = ?")
        .bind(at)
        .bind(at)
        .bind(request_id)
        .execute(repo.pool())
        .await
        .expect("seed execution_requests created_at");
}

/// Stores the reason a poll gave for a request waiting.
pub async fn execution_request_waiting_reason(repo: &Repository, request_id: &str, reason: &str) {
    sqlx::query("UPDATE execution_requests SET waiting_reason = ? WHERE id = ?")
        .bind(reason)
        .bind(request_id)
        .execute(repo.pool())
        .await
        .expect("seed execution_requests waiting_reason");
}

/// Marks a request as started by auto-start for the docket `team`.
pub async fn execution_request_auto_started(repo: &Repository, request_id: &str, team: &str) {
    sqlx::query(
        "UPDATE execution_requests SET metadata = '{\"started_by\":\"auto_start\"}', \
         request_snapshot = json_set(request_snapshot, '$.resolved_agent_profile.tool_policy.docket.team', ?) \
         WHERE id = ?",
    )
    .bind(team)
    .bind(request_id)
    .execute(repo.pool())
    .await
    .expect("seed execution_requests auto start");
}

/// Plants `count` queued requests named `{prefix}-{i}` against `item_id` and
/// the runner `runner-active`, created one second apart from `base`. Values are
/// generated here, never caller input.
pub async fn execution_requests_queued(
    repo: &Repository,
    prefix: &str,
    item_id: &str,
    count: usize,
    base: chrono::DateTime<chrono::Utc>,
) {
    let rows: Vec<String> = (0..count)
        .map(|i| {
            let id = format!("{prefix}-{i}");
            let ts = (base + chrono::Duration::seconds(1 + i as i64)).to_rfc3339();
            format!(
                "('{id}','{item_id}','{prefix}','{id}','{id}','queued','exact_runner','runner-active','{{}}','{{}}','{{}}','{ts}','{ts}')"
            )
        })
        .collect();
    let sql = format!(
        "INSERT INTO execution_requests \
         (id, item_id, idempotency_scope, idempotency_key, request_fingerprint, state, \
          selector_kind, selector_id, agent_profile_snapshot, repository_snapshot, \
          permission_policy, created_at, updated_at) VALUES {}",
        rows.join(",")
    );
    sqlx::query(sqlx::AssertSqlSafe(sql))
        .execute(repo.pool())
        .await
        .expect("seed queued execution_requests");
}

/// One `execution_recovery_audits` row for `attempt_id`, created at `created_at`.
pub async fn execution_recovery_audit(
    repo: &Repository,
    attempt_id: &str,
    recovery_key: &str,
    classification: &str,
    created_at: &str,
) {
    sqlx::query(
        "INSERT INTO execution_recovery_audits(attempt_id,recovery_key,classification,details,fingerprint,response,created_at) VALUES (?,?,?,'{}','fingerprint','{}',?)",
    )
    .bind(attempt_id)
    .bind(recovery_key)
    .bind(classification)
    .bind(created_at)
    .execute(repo.pool())
    .await
    .expect("seed execution_recovery_audits");
}

/// One automatic `tack_merges` row for `attempt_id` in `project_id`.
pub async fn tack_merge_auto(repo: &Repository, attempt_id: &str, project_id: &str, at: &str) {
    sqlx::query("INSERT INTO tack_merges (attempt_id, project_id, trigger, checked_at, merged_at) VALUES (?, ?, 'auto', ?, ?)")
        .bind(attempt_id)
        .bind(project_id)
        .bind(at)
        .bind(at)
        .execute(repo.pool())
        .await
        .expect("seed tack_merges");
}
