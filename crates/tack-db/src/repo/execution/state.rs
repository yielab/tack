//! The one place an `ExecutionState` becomes text for a column and back. A
//! write binds [`text`]; a read hands the column to [`parse`]. No other file
//! spells a state name.

use tack_protocol::execution::{self, ExecutionState, TransitionActor};

use crate::StoreError;

/// The SQL spelling of one state: `'queued'`.
pub fn literal(state: ExecutionState) -> String {
    format!("'{}'", state.as_str())
}

/// The SQL spelling of a list of states, for `IN (...)`: `'leased', 'preparing'`.
pub fn list(states: &[ExecutionState]) -> String {
    states
        .iter()
        .map(|state| literal(*state))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The states in which an attempt holds a lease, as the list an `IN (...)` takes.
pub fn held() -> String {
    list(&execution::lease_held())
}

/// The states an attempt never leaves, as the list an `IN (...)` takes.
pub fn terminal_states() -> String {
    list(&execution::terminal())
}

/// A query written with the state names as tokens: `{queued}`, `{running}`, ... for one state,
/// `{held}` for the states in which an attempt holds a lease, `{terminal}` for the ones it never
/// leaves and `{open}` for a request still being worked on. The states come from the enum, so a
/// query never spells one. The tokens hold only the constants of this crate, never a value.
pub fn sql(template: &str) -> sqlx::AssertSqlSafe<String> {
    sqlx::AssertSqlSafe(expand(template))
}

/// [`sql`] as plain text, for a query that is built further.
pub fn expand(template: &str) -> String {
    let mut text = template.to_owned();
    for state in ExecutionState::ALL {
        text = text.replace(&format!("{{{}}}", state.as_str()), &literal(state));
    }
    let open: Vec<_> = ExecutionState::ALL
        .into_iter()
        .filter(|state| {
            !state.is_terminal()
                && !matches!(state, ExecutionState::Lost | ExecutionState::NeedsOperator)
        })
        .collect();
    text = text
        .replace("{held}", &held())
        .replace("{terminal}", &terminal_states())
        .replace("{open}", &list(&open));
    text
}

/// One state change the store writes with an `UPDATE ... WHERE state ...` guard. The guard is the
/// lifecycle's own table read through [`Move::from`], so a state is never spelled in a query and
/// a change the table forbids cannot be written. `Move::ALL` is every move the store makes; one
/// test checks each against [`execution::validate_transition`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    /// A queued request is leased to a runner.
    Lease,
    /// The runner accepted the lease and prepared a workspace.
    Prepare,
    /// The agent started.
    Start,
    /// The agent asked a question and waits for the answer.
    AskDecision,
    /// The answer arrived and the agent goes on.
    ResumeAfterDecision,
    /// The lease owner ends the attempt in `to` (a terminal state).
    Finish(ExecutionState),
    /// An operator cancels a request no runner holds.
    CancelQueued,
    /// Recovery gives up an attempt whose lease ran out: it is `to` (`Lost` or `NeedsOperator`).
    Abandon(ExecutionState),
    /// An operator sends a request that waits for them back to the queue.
    Requeue,
}

impl Move {
    pub const ALL: [Move; 12] = [
        Move::Lease,
        Move::Prepare,
        Move::Start,
        Move::AskDecision,
        Move::ResumeAfterDecision,
        Move::Finish(ExecutionState::Succeeded),
        Move::Finish(ExecutionState::Failed),
        Move::Finish(ExecutionState::Cancelled),
        Move::CancelQueued,
        Move::Abandon(ExecutionState::Lost),
        Move::Abandon(ExecutionState::NeedsOperator),
        Move::Requeue,
    ];

    pub fn to(self) -> ExecutionState {
        match self {
            Move::Lease => ExecutionState::Leased,
            Move::Prepare => ExecutionState::Preparing,
            Move::Start | Move::ResumeAfterDecision => ExecutionState::Running,
            Move::AskDecision => ExecutionState::WaitingDecision,
            Move::Finish(to) | Move::Abandon(to) => to,
            Move::CancelQueued => ExecutionState::Cancelled,
            Move::Requeue => ExecutionState::Queued,
        }
    }

    pub fn actor(self) -> TransitionActor {
        match self {
            Move::Lease => TransitionActor::Scheduler,
            Move::CancelQueued | Move::Requeue => TransitionActor::Operator,
            Move::Abandon(_) => TransitionActor::RecoveryService,
            _ => TransitionActor::LeaseOwner,
        }
    }

    /// The states this move starts from: the lifecycle's sources for it, narrowed to the one the
    /// move names where two moves share a target (`Start` and `ResumeAfterDecision` both end in
    /// `Running`).
    pub fn from(self) -> Vec<ExecutionState> {
        let narrowed = match self {
            Move::Start => Some(ExecutionState::Preparing),
            Move::ResumeAfterDecision => Some(ExecutionState::WaitingDecision),
            Move::Requeue => Some(ExecutionState::NeedsOperator),
            _ => None,
        };
        let sources = execution::sources(self.to(), self.actor());
        match narrowed {
            Some(only) => sources.into_iter().filter(|state| *state == only).collect(),
            None => sources,
        }
    }

    /// The `state IN (...)` condition of the move's `UPDATE`, to put after `AND`.
    pub fn guard(self) -> String {
        format!("state IN ({})", list(&self.from()))
    }

    /// The assignment of the move's target, to put after `SET`.
    pub fn set(self) -> String {
        format!("state = {}", literal(self.to()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_guarded_move_is_one_the_lifecycle_table_allows() {
        for step in Move::ALL {
            let from = step.from();
            assert!(!from.is_empty(), "{step:?} starts from no state");
            for state in from {
                assert!(
                    execution::validate_transition(state, step.to(), step.actor()).is_ok(),
                    "{step:?} is guarded to start from {state:?}, which the lifecycle forbids"
                );
            }
        }
    }

    #[test]
    fn a_move_that_shares_its_target_is_narrowed_not_widened() {
        assert_eq!(Move::Start.from(), [ExecutionState::Preparing]);
        assert_eq!(
            Move::ResumeAfterDecision.from(),
            [ExecutionState::WaitingDecision]
        );
        assert!(
            Move::Finish(ExecutionState::Succeeded)
                .from()
                .contains(&ExecutionState::Running)
        );
        assert!(
            !Move::Finish(ExecutionState::Succeeded)
                .from()
                .contains(&ExecutionState::Leased)
        );
    }

    /// No query outside this file and the migrations spells an execution state: the guards come
    /// from the lifecycle (ADR 0028, decision 3).
    #[test]
    fn no_sql_string_in_the_store_spells_a_state() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = vec![src.clone()];
        let mut checked = 0;
        while let Some(dir) = files.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                if path.is_dir() {
                    files.push(path);
                    continue;
                }
                if !name.ends_with(".rs")
                    || name == "state.rs"
                    || name == "migrations.rs"
                    || name == "tests.rs"
                    || path.components().any(|c| c.as_os_str() == "tests")
                {
                    continue;
                }
                checked += 1;
                let text = std::fs::read_to_string(&path).unwrap();
                for state in ExecutionState::ALL {
                    let quoted = format!("'{}'", state.as_str());
                    assert!(
                        !text.contains(&quoted),
                        "{} spells the state {quoted} in a string; build it from the lifecycle",
                        path.display()
                    );
                }
            }
        }
        assert!(checked > 20, "the walk found only {checked} files");
    }
}

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
