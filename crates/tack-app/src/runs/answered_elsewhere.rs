use chrono::{DateTime, Utc};
use tack_core::decision::DecisionResolvedBy;
use tack_db::Repository;
use tack_db::repo::execution::DecisionSettlement;
use tack_protocol::execution::AttemptId;

use crate::AppError;

/// What came of telling the board a decision was answered in the agent's own tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnsweredElsewhere {
    /// The attempt asked no decision with this id.
    NotFound,
    /// The decision is closed: `state` is `answered_elsewhere`, whether this call closed it or an
    /// earlier one did, or the state another road had already given it (`resolved`, `expired`),
    /// which this call leaves as it was. `closed_now` is true only for the call that wrote it.
    Settled {
        state: String,
        resolved_at: String,
        closed_now: bool,
    },
}

/// Closes a still-`pending` decision as `answered_elsewhere`, naming `source` (the tool that took
/// the answer) as who closed it, and resumes the attempt when it was the last pending decision,
/// in the same transaction. A decision already settled is left as it is and reported as it
/// stands, so the runner telling the board twice, or after a person answered, changes nothing.
pub async fn answered_elsewhere(
    repo: &Repository,
    now: DateTime<Utc>,
    attempt_id: &AttemptId,
    decision_id: &str,
    source: &str,
) -> Result<AnsweredElsewhere, AppError> {
    let now_s = now.to_rfc3339();
    let resolved_by = serde_json::to_string(&DecisionResolvedBy::harness(source))
        .expect("resolved_by serializes");
    let outcome = repo
        .settle_execution_decision(attempt_id, decision_id, now, |decision| {
            if decision.state == "pending" {
                (
                    DecisionSettlement::AnsweredElsewhere { resolved_by },
                    AnsweredElsewhere::Settled {
                        state: "answered_elsewhere".to_owned(),
                        resolved_at: now_s.clone(),
                        closed_now: true,
                    },
                )
            } else {
                (
                    DecisionSettlement::Keep,
                    AnsweredElsewhere::Settled {
                        state: decision.state.clone(),
                        resolved_at: decision
                            .resolved_at
                            .clone()
                            .unwrap_or_else(|| decision.updated_at.clone()),
                        closed_now: false,
                    },
                )
            }
        })
        .await?;
    Ok(outcome.unwrap_or(AnsweredElsewhere::NotFound))
}
