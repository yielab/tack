use chrono::Duration;
use tack_db::Repository;
use tack_db::repo::execution::{ClaimedExecution, RequestSelection};
use tack_orch::scheduler::SchedulingPolicy;
use tack_protocol::Clock;
use uuid::Uuid;

use crate::AppError;
use crate::scheduler_wiring::choose_request_for_runner;

/// Leases the queued request the scheduler picks for `runner_id`, or answers `None` when there
/// is nothing this runner may take. Idempotent on `claim_request_id`: a replay returns the
/// lease the first call issued. The scheduler only chooses; the claim itself re-checks that the
/// choice is still queued and eligible, so a choice that lost a race yields `None`, never a
/// second lease. A docket task's snapshot carries the autonomy level it may run at, or none.
pub async fn claim(
    repo: &Repository,
    clock: &dyn Clock,
    runner_id: &str,
    claim_request_id: &str,
    lease_duration: Duration,
) -> Result<Option<ClaimedExecution>, AppError> {
    let scheduled_request_id =
        choose_request_for_runner(repo, runner_id, clock.now(), &SchedulingPolicy::default())
            .await?;
    let attempt_id = format!("att_{}", Uuid::new_v4());
    let claimed = repo
        .claim_execution_idempotent_with_snapshot(
            runner_id,
            claim_request_id,
            &attempt_id,
            lease_duration,
            clock,
            RequestSelection::Scheduled(scheduled_request_id.as_deref()),
        )
        .await?;
    let Some(mut claimed) = claimed else {
        return Ok(None);
    };
    // The level a docket task may run at is read now, from the project and the runner that
    // took it; the request keeps only the cap a person asked for.
    let level =
        crate::autonomy::level_to_send(repo, &claimed.lease.attempt_id, &claimed.request_snapshot)
            .await;
    if let Some(object) = claimed.request_snapshot.as_object_mut() {
        match level {
            Some(level) => object.insert("autonomy".to_owned(), level.to_string().into()),
            None => object.remove("autonomy"),
        };
    }
    Ok(Some(claimed))
}
