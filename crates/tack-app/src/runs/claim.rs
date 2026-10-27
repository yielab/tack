use chrono::{DateTime, Duration, Utc};
use tack_db::Repository;
use tack_db::repo::execution::{ClaimedExecution, RequestSelection};
use tack_orch::scheduler::SchedulingPolicy;
use uuid::Uuid;

use crate::AppError;
use crate::scheduler_wiring::choose_request_for_runner;

/// Leases the queued request the scheduler picks for `runner_id`, or answers `None` when there
/// is nothing this runner may take. Idempotent on `claim_request_id`: a replay returns the
/// lease the first call issued. The scheduler only chooses; the claim itself re-checks that the
/// choice is still queued and eligible, so a choice that lost a race yields `None`, never a
/// second lease. A docket task's snapshot carries the autonomy level a person asked for, or none.
pub async fn claim(
    repo: &Repository,
    now: DateTime<Utc>,
    runner_id: &str,
    claim_request_id: &str,
    lease_duration: Duration,
) -> Result<Option<ClaimedExecution>, AppError> {
    let scheduled_request_id =
        choose_request_for_runner(repo, runner_id, now, &SchedulingPolicy::default()).await?;
    let attempt_id = format!("att_{}", Uuid::new_v4());
    let claimed = repo
        .claim_execution_idempotent_with_snapshot(
            runner_id,
            claim_request_id,
            &attempt_id,
            lease_duration,
            now,
            RequestSelection::Scheduled(scheduled_request_id.as_deref()),
        )
        .await?;
    let Some(mut claimed) = claimed else {
        return Ok(None);
    };
    // Only a level a person asked for reaches docket, as it is; the project's level never does.
    let level = crate::autonomy::level_to_send(&claimed.request_snapshot);
    if let Some(object) = claimed.request_snapshot.as_object_mut() {
        match level {
            Some(level) => object.insert("autonomy".to_owned(), level.to_string().into()),
            None => object.remove("autonomy"),
        };
    }
    Ok(Some(claimed))
}
