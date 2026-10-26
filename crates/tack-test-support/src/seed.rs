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

/// Sets when an `execution_attempts` row's lease expires, apart from when it was issued.
pub async fn execution_attempt_lease_expires_at(
    repo: &Repository,
    attempt_id: &str,
    lease_expires_at: &str,
) {
    sqlx::query("UPDATE execution_attempts SET lease_expires_at = ? WHERE id = ?")
        .bind(lease_expires_at)
        .bind(attempt_id)
        .execute(repo.pool())
        .await
        .expect("seed execution_attempts lease_expires_at");
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

/// Registers one runner and one agent profile, the two rows a request needs before
/// [`running_attempt`] can claim it.
pub async fn runner_with_profile(
    repo: &Repository,
    clock: &dyn tack_protocol::Clock,
    runner_id: &str,
    profile_id: &str,
    credential_hash: &str,
    capacity: i64,
) {
    use tack_db::repo::execution::{NewAgentProfile, NewRunner};
    repo.register_runner(
        NewRunner {
            id: runner_id,
            name: "Seeded runner",
            credential_hash,
            labels: "{}",
            total_capacity: capacity,
            available_capacity: capacity,
            capability_snapshot: "{}",
            protocol_version: 1,
        },
        clock,
    )
    .await
    .expect("seed runner");
    repo.create_agent_profile(
        NewAgentProfile {
            id: profile_id,
            name: "Seeded profile",
            instructions: "work",
            tool_policy: r#"{"mode":"safe"}"#,
            limits: "{}",
        },
        clock,
    )
    .await
    .expect("seed profile");
}

/// Enqueues a request for `item_id` pinned to `runner_id`, claims it through the real claim path
/// and puts the attempt in `running`. The request is `req-<tag>`, the attempt `att-<tag>`
/// (returned). The runner and profile come from [`runner_with_profile`].
pub async fn running_attempt(
    repo: &Repository,
    clock: &dyn tack_protocol::Clock,
    runner_id: &str,
    profile_id: &str,
    item_id: &str,
    tag: &str,
) -> String {
    use chrono::Duration;
    use tack_db::repo::execution::{NewExecutionRequest, RequestSelection};

    let request_id = format!("req-{tag}");
    let snapshot = format!(
        r#"{{"request_id":"{request_id}","item_id":"{item_id}","idempotency_key":"{request_id}","created_by":{{"source":"test","subject_id":"seed"}},"created_at":"2026-08-12T12:00:00Z","selector":{{"kind":"exact_runner","runner_id":"{runner_id}"}},"agent_profile_id":"{profile_id}","resolved_agent_profile":{{"name":"P","instructions":"work","tool_policy":{{"mode":"safe"}},"timeout_seconds":60,"budgets":{{}}}},"requested_harness_kind":"codex","requested_model": null,"repository":{{"kind":"git","remote":"https://example.test/seed.git","base_revision":"abc123","subdirectory":null}},"permission_policy":{{"tools":[],"network":false}},"timeout_seconds":60,"budgets":{{}},"status_map_policy_id":null,"environment":{{}},"metadata":{{}}}}"#
    );
    repo.enqueue_execution(
        NewExecutionRequest {
            id: &request_id,
            item_id,
            idempotency_scope: "item",
            idempotency_key: &request_id,
            request_fingerprint: &request_id,
            selector_kind: "exact_runner",
            selector_id: runner_id,
            agent_profile_id: Some(profile_id),
            agent_profile_snapshot: r#"{"name":"P","instructions":"work","tool_policy":{"mode":"safe"},"timeout_seconds":60,"budgets":{}}"#,
            requested_harness_kind: Some("codex"),
            requested_model: None,
            repository_snapshot: r#"{"kind":"git","remote":"https://example.test/seed.git","base_revision":"abc123","subdirectory":null}"#,
            permission_policy: r#"{"tools":[],"network":false}"#,
            timeout_seconds: Some(60),
            budgets: "{}",
            status_map_policy_id: None,
            environment: "{}",
            metadata: "{}",
            request_snapshot: &snapshot,
        },
        clock,
    )
    .await
    .expect("seed enqueue");
    let attempt_id = format!("att-{tag}");
    let claim = repo
        .claim_execution_idempotent_with_snapshot(
            runner_id,
            &attempt_id,
            &attempt_id,
            Duration::seconds(300),
            clock,
            RequestSelection::Naive,
        )
        .await
        .expect("seed claim")
        .expect("work available");
    execution_attempt_state(repo, &attempt_id, ExecutionState::Running).await;
    claim.lease.attempt_id
}

/// Raises a pending decision against a running attempt through the real insert path (which also
/// parks the attempt in `waiting_decision`). `options` is JSON text.
pub async fn pending_decision(
    repo: &Repository,
    clock: &dyn tack_protocol::Clock,
    runner_id: &str,
    attempt_id: &str,
    decision_id: &str,
    options: &str,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
) {
    use tack_db::repo::execution::NewDecision;
    let row_id = format!("row-{}", Uuid::new_v4());
    let written = repo
        .create_execution_decision(
            runner_id,
            attempt_id,
            1,
            NewDecision {
                id: &row_id,
                decision_id,
                kind: "tool_permission",
                prompt: "Allow the harness to run the focused test suite?",
                options,
                metadata: "{}",
                expires_at,
                recommendation: None,
            },
            clock,
        )
        .await
        .expect("seed decision insert");
    assert!(written, "the seeded decision must land");
}

// ─── runner-protocol fixtures ───────────────────────────────────────────────

/// Sets how many runs a runner can hold at all and how many it can still take.
pub async fn agent_runner_capacity(
    repo: &Repository,
    runner_id: &str,
    total_capacity: i64,
    available_capacity: i64,
) {
    sqlx::query("UPDATE agent_runners SET total_capacity = ?, available_capacity = ? WHERE id = ?")
        .bind(total_capacity)
        .bind(available_capacity)
        .bind(runner_id)
        .execute(repo.pool())
        .await
        .expect("seed agent_runners capacity");
}

/// Marks a decision resolved by an operator, with `answer` and `resolved_by` as raw JSON text,
/// every timestamp being `at`.
pub async fn execution_decision_resolved(
    repo: &Repository,
    attempt_id: &str,
    decision_id: &str,
    answer: &str,
    resolved_by: &str,
    at: &str,
) {
    sqlx::query(
        "UPDATE execution_decisions SET state='resolved', answer=?, resolved_at=?, resolved_by=?, updated_at=? \
         WHERE attempt_id=? AND decision_id=?",
    )
    .bind(answer)
    .bind(at)
    .bind(resolved_by)
    .bind(at)
    .bind(attempt_id)
    .bind(decision_id)
    .execute(repo.pool())
    .await
    .expect("seed execution_decisions resolved");
}

/// A Merge-Readiness Pack artifact of `attempt_id` whose verified content is stored at
/// `content_reference`.
pub async fn execution_pack_artifact(
    repo: &Repository,
    attempt_id: &str,
    artifact_id: &str,
    content_reference: &str,
    created_at: &str,
) {
    sqlx::query(
        "INSERT INTO execution_artifacts \
         (id, attempt_id, artifact_id, kind, name, media_type, size_bytes, sha256, content_reference, created_at) \
         VALUES (?, ?, ?, 'mrp', 'mrp.json', 'application/vnd.tack.mrp+json', 1, 'x', ?, ?)",
    )
    .bind(format!("art_row_{}", Uuid::new_v4()))
    .bind(attempt_id)
    .bind(artifact_id)
    .bind(content_reference)
    .bind(created_at)
    .execute(repo.pool())
    .await
    .expect("seed execution_artifacts pack");
}

/// Sets the status a project moves an item to when a run leaves evidence.
pub async fn project_on_finish_status(repo: &Repository, project_id: &str, status: &str) {
    sqlx::query("UPDATE projects SET on_finish_status = ? WHERE id = ?")
        .bind(status)
        .bind(project_id)
        .execute(repo.pool())
        .await
        .expect("seed projects on_finish_status");
}

/// Replaces the stored snapshot (raw text, valid or not) of an `execution_requests` row.
pub async fn execution_request_snapshot(repo: &Repository, request_id: &str, snapshot: &str) {
    sqlx::query("UPDATE execution_requests SET request_snapshot = ? WHERE id = ?")
        .bind(snapshot)
        .bind(request_id)
        .execute(repo.pool())
        .await
        .expect("seed execution_requests snapshot");
}

/// Sets how many follow-ups a rejected run of this project's items may start.
pub async fn project_verify_loop_max_attempts(repo: &Repository, project_id: &str, cap: i64) {
    sqlx::query("UPDATE projects SET verify_loop_max_attempts = ? WHERE id = ?")
        .bind(cap)
        .bind(project_id)
        .execute(repo.pool())
        .await
        .expect("seed projects verify_loop_max_attempts");
}

/// A runner registered under `runner_id` through the store's own command: active, one slot, no
/// labels. The row a request pinned to that runner needs before it can be queued.
pub async fn registered_runner(repo: &Repository, runner_id: &str) {
    repo.register_runner(
        tack_db::repo::execution::NewRunner {
            id: runner_id,
            name: "local-1",
            credential_hash: "h",
            labels: "{}",
            total_capacity: 1,
            available_capacity: 1,
            capability_snapshot: "{}",
            protocol_version: 1,
        },
        &tack_protocol::SystemClock,
    )
    .await
    .expect("seed registered runner");
}
