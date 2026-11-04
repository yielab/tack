//! What became of a run, as a rule the whole product shares: whether it was turned away before
//! the agent did anything. The server computes it once, for the lightweight run list and for
//! each attempt, and every screen reads the flag; none matches the reason's text itself.

use serde_json::Value;
use tack_protocol::execution::ExecutionState;

/// The code a runner reports when the computer refused the run (the agent kind did not match,
/// or the agent was not installed).
const HARNESS_REJECTED: &str = "harness_rejected";

/// The code a runner reports when the team refused the task at claim, before any step, because
/// the method it names cannot run on that team.
const PIPELINE_REFUSED: &str = "pipeline";

/// Whether an attempt ended without ever starting work: it failed, and the computer refused it
/// (`code` is `harness_rejected`), the team refused the method the task named (`code` is
/// `pipeline`) or the team's own ceiling did (docket words it "ceiling L<n>" in the `message`,
/// and sends no code). Such a run used nothing and changed nothing, so it is
/// "Not started", never a failure of the work. An attempt in any other state is never one, whatever
/// its reason says.
pub fn never_started(state: ExecutionState, terminal_reason: Option<&Value>) -> bool {
    if state != ExecutionState::Failed {
        return false;
    }
    let Some(reason) = terminal_reason else {
        return false;
    };
    matches!(
        reason.get("code").and_then(Value::as_str),
        Some(HARNESS_REJECTED | PIPELINE_REFUSED)
    ) || reason
        .get("message")
        .and_then(Value::as_str)
        .is_some_and(names_a_ceiling)
}

/// Whether `message` names the level a team's ceiling stopped at: the word "ceiling" (any case),
/// whitespace, `L` and a digit, on a word boundary (`ceiling L3`, not `ceiling Lx` or `ceilings L3`).
fn names_a_ceiling(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    let mut from = 0;
    while let Some(found) = lower[from..].find("ceiling") {
        let start = from + found;
        let end = start + "ceiling".len();
        from = end;
        let before_ok = lower[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        let rest = &lower[end..];
        let after_space = rest.trim_start();
        if !before_ok || after_space.len() == rest.len() {
            continue;
        }
        let mut chars = after_space.chars();
        if chars.next() == Some('l')
            && chars.next().is_some_and(|c| c.is_ascii_digit())
            && chars
                .next()
                .is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_refused_failed_attempt_never_started() {
        let failed = ExecutionState::Failed;
        assert!(never_started(
            failed,
            Some(&json!({"code": "harness_rejected"}))
        ));
        assert!(never_started(failed, Some(&json!({"code": "pipeline"}))));
        assert!(never_started(
            failed,
            Some(&json!({"message": "run refused: ceiling L3 (needs approval)"}))
        ));
        assert!(never_started(
            failed,
            Some(&json!({"message": "Team Ceiling   l2."}))
        ));
    }

    #[test]
    fn anything_else_started() {
        let failed = ExecutionState::Failed;
        assert!(!never_started(failed, Some(&json!({"code": "timeout"}))));
        assert!(!never_started(
            failed,
            Some(&json!({"message": "ceilings L3"}))
        ));
        assert!(!never_started(
            failed,
            Some(&json!({"message": "ceiling Lx"}))
        ));
        assert!(!never_started(
            failed,
            Some(&json!({"message": "ceiling L35"}))
        ));
        assert!(!never_started(failed, Some(&json!({"message": "ceiling"}))));
        assert!(!never_started(failed, None));
        assert!(!never_started(failed, Some(&json!("harness_rejected"))));
        for state in [
            ExecutionState::Succeeded,
            ExecutionState::Cancelled,
            ExecutionState::Lost,
        ] {
            assert!(
                !never_started(state, Some(&json!({"code": "harness_rejected"}))),
                "{state:?}"
            );
        }
    }
}
