use tack_core::decision::DecisionResolvedBy;
use tack_db::Repository;
use tack_protocol::Clock;

use crate::AppError;

/// Fail-closed expiry of every still-`pending` decision whose `expires_at` has passed: each
/// becomes `expired` with no answer and a `system` `resolved_by`, the same terminal shape
/// `resolve_decision` writes lazily for a single decision, and each attempt left with no
/// pending decision resumes. One transaction. Returns how many decisions expired.
pub async fn expire_decisions(repo: &Repository, clock: &dyn Clock) -> Result<u64, AppError> {
    let resolved_by =
        serde_json::to_string(&DecisionResolvedBy::expiry()).expect("resolved_by serializes");
    Ok(repo
        .expire_overdue_execution_decisions(clock.now(), &resolved_by)
        .await?)
}
