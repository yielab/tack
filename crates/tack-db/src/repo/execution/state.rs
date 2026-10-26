//! The one place an `ExecutionState` becomes text for a column and back. A
//! write binds [`text`]; a read hands the column to [`parse`]. No other file
//! spells a state name.

use tack_protocol::execution::ExecutionState;

use crate::StoreError;

/// The text a column holds for this state.
pub fn text(state: ExecutionState) -> &'static str {
    state.as_str()
}

/// Whether a column's text names a state in which an attempt holds a lease.
pub fn is_live(text: &str) -> bool {
    matches!(
        parse(text),
        Ok(ExecutionState::Leased
            | ExecutionState::Preparing
            | ExecutionState::Running
            | ExecutionState::WaitingDecision)
    )
}

/// Whether a column's text names the state that waits for an operator.
pub fn is_needs_operator(text: &str) -> bool {
    matches!(parse(text), Ok(ExecutionState::NeedsOperator))
}

/// The state a column's text names. Text that names none is a corrupt row, so
/// it surfaces as a decode failure rather than a guess.
pub fn parse(text: &str) -> Result<ExecutionState, StoreError> {
    text.parse::<ExecutionState>()
        .map_err(|unknown| sqlx::Error::Decode(Box::new(unknown)).into())
}
