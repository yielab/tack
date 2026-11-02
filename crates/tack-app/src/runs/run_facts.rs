//! Two facts about a run the board's card shows and the run's own row does not carry: whether
//! its newest attempt was turned away before it started, and what the verifier recommended.
//! The list of runs reads them here, once for the whole batch, so no screen matches a reason's
//! text or opens a pack itself.

use std::collections::HashMap;

use serde_json::Value;
use tack_core::run_outcome::never_started;
use tack_db::Repository;
use tack_db::queries::execution_attempts::newest_attempts_of_requests;
use tack_protocol::execution::ExecutionState;
use tack_protocol::mrp::{MergeDecision, MergeReadinessPack};

use crate::AppError;
use crate::autonomy::MRP_MEDIA_TYPE;
use crate::ports::Artifacts;

/// What a run's newest attempt came to, as the run list states it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RunFacts {
    /// The newest attempt ended without ever starting work (see
    /// [`tack_core::run_outcome::never_started`]).
    pub never_started: bool,
    /// What the verifier's pack for the newest attempt recommends; `None` while the attempt has
    /// not succeeded, or when it left no readable pack.
    pub verifier_recommendation: Option<MergeDecision>,
}

/// The facts of each request's newest attempt, in one query and one read per pack. A request with
/// no attempt yet has no entry. Without `artifacts` (a board with no stored files wired in) no
/// pack is read and no recommendation is given.
pub async fn run_facts(
    repo: &Repository,
    artifacts: Option<&dyn Artifacts>,
    request_ids: &[String],
) -> Result<HashMap<String, RunFacts>, AppError> {
    let mut facts = HashMap::new();
    for attempt in newest_attempts_of_requests(repo, request_ids, MRP_MEDIA_TYPE).await? {
        let reason: Option<Value> = attempt
            .terminal_reason
            .as_deref()
            .and_then(|raw| serde_json::from_str(raw).ok());
        let recommendation = match (artifacts, &attempt.artifact_reference, attempt.state) {
            (Some(artifacts), Some(reference), ExecutionState::Succeeded) => artifacts
                .read(reference)
                .await
                .and_then(|bytes| serde_json::from_slice::<MergeReadinessPack>(&bytes).ok())
                .map(|pack| pack.recommendation.decision),
            _ => None,
        };
        facts.insert(
            attempt.request_id,
            RunFacts {
                never_started: never_started(attempt.state, reason.as_ref()),
                verifier_recommendation: recommendation,
            },
        );
    }
    Ok(facts)
}
