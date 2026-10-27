//! What more than one stage of the lifecycle needs.

use crate::StoreError;
use sqlx::SqliteConnection;
use tack_protocol::execution::{AttemptId, ExecutionState, FencingToken};

pub(super) fn terminal(state: &str) -> bool {
    super::state::parse(state).is_ok_and(ExecutionState::is_terminal)
}

/// A fencing token as the integer column holds it.
pub(crate) fn token_column(token: FencingToken) -> Result<i64, StoreError> {
    i64::try_from(token.get()).map_err(|_| StoreError::malformed("fencing token is out of range"))
}

/// A fencing token read from the integer column; a negative one is a corrupt row.
pub(super) fn token_from_column(value: i64) -> Result<FencingToken, StoreError> {
    u64::try_from(value)
        .map(FencingToken::new)
        .map_err(|_| StoreError::malformed("fencing token column is negative"))
}

/// Sets the request that owns `attempt_id` to `state`. A request's state
/// mirrors its live attempt's, so every attempt transition that is not a
/// claim or a terminal/recovery move calls this in the same transaction.
pub(super) async fn mirror_request_state(
    conn: &mut SqliteConnection,
    attempt_id: &AttemptId,
    state: ExecutionState,
    now: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE execution_requests SET state=?,updated_at=? \
         WHERE id=(SELECT request_id FROM execution_attempts WHERE id=?)",
    )
    .bind(super::state::text(state))
    .bind(now)
    .bind(attempt_id.as_str())
    .execute(conn)
    .await?;
    Ok(())
}
