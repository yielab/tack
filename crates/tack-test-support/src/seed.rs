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

/// Sets the state an `execution_requests` row is in.
pub async fn execution_request_state(repo: &Repository, request_id: &str, state: ExecutionState) {
    sqlx::query("UPDATE execution_requests SET state = ? WHERE id = ?")
        .bind(state::text(state))
        .bind(request_id)
        .execute(repo.pool())
        .await
        .expect("seed execution_requests state");
}

/// One `execution_attempts` row, number 1 of `request_id`, held by `runner_id` in `state`, whose
/// lease was issued at `now` and expires at `lease_expires_at`.
pub async fn execution_attempt(
    repo: &Repository,
    request_id: &str,
    attempt_id: &str,
    runner_id: &str,
    state: ExecutionState,
    now: &str,
    lease_expires_at: &str,
) {
    sqlx::query(
        "INSERT INTO execution_attempts \
         (id, request_id, attempt_number, runner_id, fencing_token, state, lease_issued_at, \
          lease_expires_at, created_at, updated_at) \
         VALUES (?, ?, 1, ?, 1, ?, ?, ?, ?, ?)",
    )
    .bind(attempt_id)
    .bind(request_id)
    .bind(runner_id)
    .bind(state::text(state))
    .bind(now)
    .bind(lease_expires_at)
    .bind(now)
    .bind(now)
    .execute(repo.pool())
    .await
    .expect("seed execution_attempts");
}

/// Moves an `execution_attempts` row to `state`.
pub async fn execution_attempt_state(repo: &Repository, attempt_id: &str, state: ExecutionState) {
    sqlx::query("UPDATE execution_attempts SET state = ? WHERE id = ?")
        .bind(state::text(state))
        .bind(attempt_id)
        .execute(repo.pool())
        .await
        .expect("seed execution_attempts state");
}

/// The `needs_operator` recovery audit a runner's authoritative report leaves for `attempt_id`,
/// which an operator's requeue requires.
pub async fn authoritative_recovery_audit(
    repo: &Repository,
    attempt_id: &str,
    recovery_key: &str,
    created_at: &str,
) {
    sqlx::query(
        "INSERT INTO execution_recovery_audits \
         (attempt_id, recovery_key, classification, details, fingerprint, response, created_at) \
         VALUES (?, ?, 'needs_operator', '{}', 'fingerprint', '{}', ?)",
    )
    .bind(attempt_id)
    .bind(recovery_key)
    .bind(created_at)
    .execute(repo.pool())
    .await
    .expect("seed execution_recovery_audits");
}

/// Stamps the last heartbeat of an `agent_runners` row.
pub async fn runner_heartbeat(repo: &Repository, runner_id: &str, at: &str) {
    sqlx::query("UPDATE agent_runners SET last_heartbeat_at = ? WHERE id = ?")
        .bind(at)
        .bind(runner_id)
        .execute(repo.pool())
        .await
        .expect("seed agent_runners heartbeat");
}
