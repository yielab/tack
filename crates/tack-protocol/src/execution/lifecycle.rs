//! Lifecycle validation for execution attempts.

use super::types::ExecutionState;

/// The authority requesting or observing a lifecycle change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionActor {
    Scheduler,
    Operator,
    LeaseOwner,
    RecoveryService,
}

/// A denied transition with a stable reason suitable for protocol errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleError {
    pub from: ExecutionState,
    pub to: ExecutionState,
    pub actor: TransitionActor,
    /// Fixed protocol reason. Do not replace with prose that callers would
    /// have to parse.
    pub reason: &'static str,
}

impl LifecycleError {
    pub const STABLE_REASON: &'static str = "invalid_transition";
}

/// Validates a non-replay state transition against runner protocol v1.
///
/// Two moves the store makes are in the table beyond the lease owner's and recovery's: the
/// scheduler fails a queued request no runner could ever claim (`queued` -> `failed`), and an
/// operator cancels a run nobody holds any more (`needs_operator`, or a lease that ran out)
/// without waiting for a runner to observe it (`* -> cancelled`). The table cannot say that a
/// lease has expired; the store checks it.
///
/// Idempotent reports are intentionally not transitions: endpoint code returns
/// the original success before calling this function, as specified by the
/// frozen lifecycle fixture.
pub fn validate_transition(
    from: ExecutionState,
    to: ExecutionState,
    actor: TransitionActor,
) -> Result<(), LifecycleError> {
    use ExecutionState::{
        Cancelled, Failed, Leased, Lost, NeedsOperator, Preparing, Queued, Running, Succeeded,
        WaitingDecision,
    };
    use TransitionActor::{LeaseOwner, Operator, RecoveryService, Scheduler};

    let allowed = matches!(
        (from, to, actor),
        (Queued, Leased | Failed, Scheduler)
            | (Queued, Cancelled, Operator)
            | (Leased, Preparing | Failed | Cancelled, LeaseOwner)
            | (Leased, Lost | NeedsOperator, RecoveryService)
            | (
                Leased | Preparing | Running | WaitingDecision | NeedsOperator,
                Cancelled,
                Operator
            )
            | (Preparing, Running | Failed | Cancelled, LeaseOwner)
            | (Preparing, Lost | NeedsOperator, RecoveryService)
            | (
                Running,
                WaitingDecision | Succeeded | Failed | Cancelled,
                LeaseOwner
            )
            | (Running, Lost | NeedsOperator, RecoveryService)
            | (
                WaitingDecision,
                Running | Succeeded | Failed | Cancelled,
                LeaseOwner
            )
            | (WaitingDecision, Lost | NeedsOperator, RecoveryService)
            | (Lost, Queued, RecoveryService)
            | (Lost, Queued | Failed, Operator)
            | (NeedsOperator, Queued | Failed, Operator)
    );

    if allowed {
        Ok(())
    } else {
        Err(LifecycleError {
            from,
            to,
            actor,
            reason: LifecycleError::STABLE_REASON,
        })
    }
}

/// The states a transition to `to` by `actor` may start from, read off the table in
/// [`validate_transition`]. A store builds the guard of every `UPDATE` that moves a state from
/// this list, so the rules are written once.
pub fn sources(to: ExecutionState, actor: TransitionActor) -> Vec<ExecutionState> {
    ExecutionState::ALL
        .into_iter()
        .filter(|from| validate_transition(*from, to, actor).is_ok())
        .collect()
}

/// The states in which a runner holds a lease: the ones its lease owner can still move out of.
pub fn lease_held() -> Vec<ExecutionState> {
    ExecutionState::ALL
        .into_iter()
        .filter(|from| {
            ExecutionState::ALL
                .into_iter()
                .any(|to| validate_transition(*from, to, TransitionActor::LeaseOwner).is_ok())
        })
        .collect()
}

/// The states no actor can move out of.
pub fn terminal() -> Vec<ExecutionState> {
    ExecutionState::ALL
        .into_iter()
        .filter(|state| state.is_terminal())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    struct FixtureRule {
        from: ExecutionState,
        allow: BTreeMap<ExecutionState, Vec<String>>,
        deny: Vec<ExecutionState>,
        #[serde(flatten, default)]
        additional: BTreeMap<String, serde_json::Value>,
    }

    #[derive(Serialize, Deserialize)]
    struct LifecycleFixture {
        rules: Vec<FixtureRule>,
        #[serde(flatten, default)]
        additional: BTreeMap<String, serde_json::Value>,
    }

    const ALL_ACTORS: [TransitionActor; 4] = [
        TransitionActor::Scheduler,
        TransitionActor::Operator,
        TransitionActor::LeaseOwner,
        TransitionActor::RecoveryService,
    ];

    fn actor(name: &str) -> TransitionActor {
        match name {
            "scheduler" => TransitionActor::Scheduler,
            "operator" => TransitionActor::Operator,
            "lease_owner" => TransitionActor::LeaseOwner,
            "recovery_service" => TransitionActor::RecoveryService,
            other => panic!("unknown frozen transition actor: {other}"),
        }
    }

    #[test]
    fn known_fixture_transitions_are_accepted() {
        assert!(
            validate_transition(
                ExecutionState::Running,
                ExecutionState::WaitingDecision,
                TransitionActor::LeaseOwner,
            )
            .is_ok()
        );
        assert!(
            validate_transition(
                ExecutionState::NeedsOperator,
                ExecutionState::Queued,
                TransitionActor::Operator,
            )
            .is_ok()
        );
    }

    #[test]
    fn illegal_transition_has_a_stable_reason() {
        let error = validate_transition(
            ExecutionState::Succeeded,
            ExecutionState::Running,
            TransitionActor::LeaseOwner,
        )
        .expect_err("terminal execution must not reopen");
        assert_eq!(error.reason, "invalid_transition");
    }

    #[test]
    fn sources_and_held_states_are_read_off_the_table() {
        use ExecutionState::{
            Leased, Lost, NeedsOperator, Preparing, Queued, Running, WaitingDecision,
        };
        assert_eq!(
            lease_held(),
            [Leased, Preparing, Running, WaitingDecision],
            "the states a lease owner can move out of"
        );
        assert_eq!(
            sources(Lost, TransitionActor::RecoveryService),
            lease_held()
        );
        assert_eq!(
            sources(Running, TransitionActor::LeaseOwner),
            [Preparing, WaitingDecision]
        );
        assert_eq!(
            sources(Queued, TransitionActor::Operator),
            [Lost, NeedsOperator]
        );
        assert_eq!(
            terminal(),
            [
                ExecutionState::Succeeded,
                ExecutionState::Failed,
                ExecutionState::Cancelled
            ]
        );
    }

    #[test]
    fn frozen_lifecycle_fixture_is_implemented_exactly() {
        let raw = include_str!("../../../../docs/contracts/runner-v1/lifecycle-transitions.json");
        let original: serde_json::Value = serde_json::from_str(raw).expect("fixture JSON");
        let fixture: LifecycleFixture = serde_json::from_str(raw).expect("lifecycle fixture");
        assert_eq!(
            serde_json::to_value(&fixture).expect("serialize lifecycle fixture"),
            original,
            "lifecycle fixture must round-trip without dropping additive fields"
        );

        for rule in fixture.rules {
            for to in ExecutionState::ALL {
                for transition_actor in ALL_ACTORS {
                    let expected = rule.allow.get(&to).is_some_and(|allowed| {
                        allowed.iter().any(|name| actor(name) == transition_actor)
                    });
                    assert_eq!(
                        validate_transition(rule.from, to, transition_actor).is_ok(),
                        expected,
                        "fixture mismatch for {from:?} -> {to:?} by {transition_actor:?}",
                        from = rule.from,
                    );
                }
            }
        }
    }
}
