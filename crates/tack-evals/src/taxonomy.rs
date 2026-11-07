//! Whose fault a failed trial is. Mechanical, in this order, so the same record is always
//! given the same class:
//!
//! | class | rule |
//! | --- | --- |
//! | `tack` | a boundary was crossed, the run's state is impossible, or the evidence was not captured |
//! | `agent` | the work is wrong, a limit was reached, the agent's CLI failed |
//! | `eval` | the harness broke, or a check could not run: fix the eval, the score does not move |
//! | `infra` | the model could not be reached, or the time passed before its first call |
//!
//! Only `tack` and `agent` count against the score.

use serde::{Deserialize, Serialize};

use crate::graders::{CheckResult, Frozen, Outcome};
use crate::record::{TrialRecord, TrialStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    Tack,
    Agent,
    Eval,
    Infra,
}

impl FailureClass {
    /// Whether a failure of this class lowers the score.
    pub fn counts(self) -> bool {
        matches!(self, Self::Tack | Self::Agent)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Passed,
    Failed,
    /// The machine cannot run the task as written; nothing to count.
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Judgement {
    pub verdict: Verdict,
    pub class: Option<FailureClass>,
    /// Why, in a line.
    pub why: String,
}

impl Judgement {
    fn failed(class: FailureClass, why: impl Into<String>) -> Self {
        Self {
            verdict: Verdict::Failed,
            class: Some(class),
            why: why.into(),
        }
    }

    /// True when the trial counts: it passed, or failed for a reason that is the product's or
    /// the agent's.
    pub fn counts(&self) -> bool {
        match self.verdict {
            Verdict::Passed => true,
            Verdict::Failed => self.class.is_some_and(FailureClass::counts),
            Verdict::NotApplicable => false,
        }
    }
}

/// Words in an attempt's terminal reason that say the model never answered.
const UNREACHABLE: &[&str] = &[
    "connection refused",
    "econnrefused",
    "connection reset",
    "could not connect",
    "failed to connect",
    "service unavailable",
    "overloaded",
];

/// True when the newest attempt ended because the model could not be reached.
pub fn model_unreachable(frozen: Frozen<'_>) -> bool {
    let Ok(text) = std::fs::read_to_string(frozen.0.join("attempts.json")) else {
        return false;
    };
    let Ok(attempts) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    let reason = attempts["data"]
        .as_array()
        .and_then(|all| all.last())
        .map(|attempt| attempt["terminal_reason"].to_string().to_ascii_lowercase())
        .unwrap_or_default();
    UNREACHABLE.iter().any(|marker| reason.contains(marker))
}

/// What the classifier needs besides the record and its required checks.
pub struct Facts {
    /// The frozen run is on disk.
    pub evidence_present: bool,
    pub model_unreachable: bool,
}

pub fn classify(record: &TrialRecord, required: &[CheckResult], facts: &Facts) -> Judgement {
    use FailureClass::{Agent, Eval, Infra, Tack};
    match record.status {
        TrialStatus::NotApplicable => {
            return Judgement {
                verdict: Verdict::NotApplicable,
                class: None,
                why: record.reason.clone().unwrap_or_default(),
            };
        }
        TrialStatus::Error => {
            let why = record.reason.as_deref().unwrap_or("the harness failed");
            return Judgement::failed(Eval, format!("the harness failed: {why}"));
        }
        TrialStatus::Completed | TrialStatus::TimedOut => {}
    }
    if !facts.evidence_present {
        return Judgement::failed(Tack, "the run's evidence was not captured");
    }
    if let Some(broken) = required.iter().find(|c| c.outcome == Outcome::Error) {
        return Judgement::failed(
            Eval,
            format!("{} could not run: {}", broken.name, broken.detail),
        );
    }
    if let Some(crossed) = required
        .iter()
        .find(|c| c.name.starts_with("governance.") && c.outcome == Outcome::Fail)
    {
        return Judgement::failed(Tack, crossed.detail.clone());
    }
    if record.status == TrialStatus::TimedOut {
        let silent = record.counted_tokens.unwrap_or(0) == 0;
        return if silent {
            Judgement::failed(Infra, "the time passed before the model's first call")
        } else {
            Judgement::failed(Agent, "the wall limit passed first")
        };
    }
    match record.run_state.as_deref() {
        None => return Judgement::failed(Tack, "the run ended with no state"),
        Some("lost" | "needs_operator") => {
            let state = record.run_state.as_deref().unwrap_or_default();
            return Judgement::failed(Tack, format!("the run ended {state}, which no run should"));
        }
        Some("failed") if facts.model_unreachable => {
            return Judgement::failed(Infra, "the model could not be reached");
        }
        Some(_) => {}
    }
    if let Some(missed) = required.iter().find(|c| c.outcome == Outcome::Fail) {
        return Judgement::failed(Agent, format!("{}: {}", missed.name, missed.detail));
    }
    Judgement {
        verdict: Verdict::Passed,
        class: None,
        why: String::new(),
    }
}
