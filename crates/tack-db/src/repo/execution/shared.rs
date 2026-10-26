//! What more than one stage of the lifecycle needs.

use crate::StoreError;
use sqlx::SqliteConnection;
use tack_protocol::Clock;
use tack_protocol::execution::ExecutionState;

pub(super) fn stamp(clock: &dyn Clock) -> String {
    clock.now().to_rfc3339()
}

pub(super) fn terminal(state: &str) -> bool {
    super::state::parse(state).is_ok_and(ExecutionState::is_terminal)
}

/// Sets the request that owns `attempt_id` to `state`. A request's state
/// mirrors its live attempt's, so every attempt transition that is not a
/// claim or a terminal/recovery move calls this in the same transaction.
pub(super) async fn mirror_request_state(
    conn: &mut SqliteConnection,
    attempt_id: &str,
    state: ExecutionState,
    now: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE execution_requests SET state=?,updated_at=? \
         WHERE id=(SELECT request_id FROM execution_attempts WHERE id=?)",
    )
    .bind(super::state::text(state))
    .bind(now)
    .bind(attempt_id)
    .execute(conn)
    .await?;
    Ok(())
}

pub(super) fn canonical_json(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut entries: Vec<_> = map.into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            serde_json::Value::Object(
                entries
                    .into_iter()
                    .map(|(k, v)| (k, canonical_json(v)))
                    .collect(),
            )
        }
        serde_json::Value::Array(v) => {
            serde_json::Value::Array(v.into_iter().map(canonical_json).collect())
        }
        v => v,
    }
}
