//! The one place an `ExecutionState` becomes text for a column and back. A
//! write binds [`text`]; a read hands the column to [`parse`]. No other file
//! spells a state name.

use tack_protocol::execution::ExecutionState;

use crate::StoreError;

/// The text a column holds for this state.
pub fn text(state: ExecutionState) -> &'static str {
    state.as_str()
}

/// The state a column's text names. Text that names none is a corrupt row, so
/// it surfaces as a decode failure rather than a guess.
pub fn parse(text: &str) -> Result<ExecutionState, StoreError> {
    text.parse::<ExecutionState>()
        .map_err(|unknown| sqlx::Error::Decode(Box::new(unknown)).into())
}
