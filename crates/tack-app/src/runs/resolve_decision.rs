use chrono::{DateTime, Utc};
use serde_json::Value;
use tack_core::decision::DecisionResolvedBy;
use tack_db::Repository;
use tack_db::repo::execution::DecisionSettlement;

use crate::AppError;
use tack_db::canonical_json;

/// What came of an answer to a decision. A rejection is a value, not an error: the caller maps
/// each to its own response.
#[derive(Debug, Clone, PartialEq)]
pub enum ResolveOutcome {
    /// No decision exists for this exact `(attempt_id, decision_id)` pair. That covers both
    /// "never existed" and "exists under a different attempt"; nothing is written and the two
    /// cannot be told apart, so guessing another attempt's `decision_id` teaches nothing.
    NotFound,
    /// `answer.option_id` is not one of the decision's recorded `options`. Only checked when
    /// the decision has options: a freeform one has none to check against.
    InvalidOption,
    /// Terminal and fail-closed: the decision is expired, already or by this very call, and no
    /// answer can resolve it.
    Expired,
    /// Already resolved with a different answer than the one submitted.
    IdempotencyConflict,
    /// A fresh resolution, or the byte-identical replay of one (`replayed` tells them apart).
    Resolved {
        resolved_at: String,
        resolved_by: Value,
        answer: Value,
        replayed: bool,
    },
    /// A stored `state` nothing writes. Never treated as any of the above.
    UnknownState(String),
}

/// Answers one decision, scoped to the exact `(attempt_id, decision_id)` pair. The read, the
/// rule and the write are one transaction, so a concurrent answer or expiry cannot land between
/// them. Settling the attempt's last pending decision, by answer or by expiry, resumes the
/// attempt and its request from `waiting_decision` to `running` in that same transaction.
///
/// The rules, by the decision's stored state:
/// - `expired`: refused, whatever the answer.
/// - `resolved`: a replay of the same answer (compared regardless of key order) returns the
///   stored resolution; a different answer is a conflict.
/// - `pending` and overdue (`expires_at <= now`): expired on the spot and refused, even for an
///   answer that would otherwise have been valid.
/// - `pending`: the answer's `option_id` must be one of the decision's options, if it has any;
///   then it is resolved.
pub async fn resolve_decision(
    repo: &Repository,
    now: DateTime<Utc>,
    attempt_id: &str,
    decision_id: &str,
    answer: &Value,
    resolved_by: &DecisionResolvedBy,
) -> Result<ResolveOutcome, AppError> {
    let now_s = now.to_rfc3339();
    let outcome = repo
        .settle_execution_decision(attempt_id, decision_id, now, |decision| {
            match decision.state.as_str() {
                "expired" => (DecisionSettlement::Keep, ResolveOutcome::Expired),
                "resolved" => {
                    let stored_answer = decision
                        .answer
                        .as_deref()
                        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                        .unwrap_or(Value::Null);
                    let outcome = if canonical_string(&stored_answer) == canonical_string(answer) {
                        ResolveOutcome::Resolved {
                            resolved_at: decision.resolved_at.clone().unwrap_or_default(),
                            resolved_by: decision
                                .resolved_by
                                .as_deref()
                                .and_then(|raw| serde_json::from_str(raw).ok())
                                .unwrap_or(Value::Null),
                            answer: stored_answer,
                            replayed: true,
                        }
                    } else {
                        ResolveOutcome::IdempotencyConflict
                    };
                    (DecisionSettlement::Keep, outcome)
                }
                "pending" => {
                    let overdue = decision
                        .expires_at
                        .as_deref()
                        .is_some_and(|value| value <= now_s.as_str());
                    if overdue {
                        let expiry = serde_json::to_string(&DecisionResolvedBy::expiry())
                            .expect("resolved_by serializes");
                        return (
                            DecisionSettlement::Expire {
                                resolved_by: expiry,
                            },
                            ResolveOutcome::Expired,
                        );
                    }
                    let options: Vec<Value> =
                        serde_json::from_str(&decision.options).unwrap_or_default();
                    let option_id = answer
                        .get("option_id")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let option_known = options
                        .iter()
                        .any(|opt| opt.get("option_id").and_then(Value::as_str) == Some(option_id));
                    if !options.is_empty() && !option_known {
                        return (DecisionSettlement::Keep, ResolveOutcome::InvalidOption);
                    }
                    let settlement = DecisionSettlement::Resolve {
                        answer: serde_json::to_string(answer).expect("answer serializes"),
                        resolved_by: serde_json::to_string(resolved_by)
                            .expect("resolved_by serializes"),
                    };
                    let outcome = ResolveOutcome::Resolved {
                        resolved_at: now_s.clone(),
                        resolved_by: serde_json::to_value(resolved_by)
                            .expect("resolved_by serializes"),
                        answer: answer.clone(),
                        replayed: false,
                    };
                    (settlement, outcome)
                }
                other => (
                    DecisionSettlement::Keep,
                    ResolveOutcome::UnknownState(other.to_string()),
                ),
            }
        })
        .await?;
    Ok(outcome.unwrap_or(ResolveOutcome::NotFound))
}

/// The answer as a string that does not depend on the order of its keys, so two submissions of
/// the same answer compare equal.
fn canonical_string(value: &Value) -> String {
    serde_json::to_string(&canonical_json(value.clone())).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn canonical_string_is_stable_across_key_order() {
        let a = json!({"option_id": "allow_once", "text": null});
        let b = json!({"text": null, "option_id": "allow_once"});
        assert_eq!(canonical_string(&a), canonical_string(&b));
        let c = json!({"option_id": "deny", "text": null});
        assert_ne!(canonical_string(&a), canonical_string(&c));
    }
}
