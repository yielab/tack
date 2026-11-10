use serde_json::{Map, Value};
use tack_db::{Repository, StoreError, queries};
use tack_protocol::mrp::MergeDecision;

/// What a Reviewer run said about the run it was started from, decided before the completion
/// commits and written in it.
pub(super) struct ReviewerWord {
    reviewed_attempt_id: String,
    decision: MergeDecision,
}

impl ReviewerWord {
    pub(super) fn follow_up(&self) -> tack_db::repo::execution::ReviewerVerdict<'_> {
        tack_db::repo::execution::ReviewerVerdict {
            reviewed_attempt_id: &self.reviewed_attempt_id,
            decision: match self.decision {
                MergeDecision::Merge => "merge",
                MergeDecision::Review => "review",
                MergeDecision::Reject => "reject",
            },
        }
    }
}

/// The word of a Reviewer run that succeeded: its result ends with a verdict line
/// (`MergeDecision::from_reviewer`), and the run it reviewed is the one it was started from
/// (`metadata.from_run`), its newest attempt. Nothing otherwise: any other profile, a run that
/// was not started from another, or a result with no verdict line.
pub(super) async fn plan(
    repo: &Repository,
    attempt_id: &str,
    terminal_reason: &Map<String, Value>,
) -> Result<Option<ReviewerWord>, StoreError> {
    let Some(decision) = terminal_reason
        .get("result")
        .and_then(Value::as_str)
        .and_then(MergeDecision::from_reviewer)
    else {
        return Ok(None);
    };
    let request = queries::execution_requests::request_of_attempt(repo, attempt_id).await?;
    let Ok(snapshot) = serde_json::from_str::<Value>(&request.request_snapshot) else {
        return Ok(None);
    };
    let is_reviewer = snapshot["resolved_agent_profile"]["kind"].as_str() == Some("reviewer");
    let Some(from_run) = snapshot["metadata"]["from_run"]
        .as_str()
        .filter(|_| is_reviewer)
    else {
        return Ok(None);
    };
    let reviewed = queries::execution_attempts::latest_attempt_of_request(repo, from_run).await?;
    Ok(reviewed
        .filter(|reviewed| reviewed != attempt_id)
        .map(|reviewed_attempt_id| ReviewerWord {
            reviewed_attempt_id,
            decision,
        }))
}
