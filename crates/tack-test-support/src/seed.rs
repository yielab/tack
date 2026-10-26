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
