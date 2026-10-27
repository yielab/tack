//! What an accepted merge-readiness pack does to its item.

use tack_core::models::UpdateItem;
use tack_core::workflow::{StatusMapEvent, StatusMapPolicy};
use tack_db::Repository;
use tack_db::queries::execution_requests::{self, RequestItemPolicy};
use tack_db::repo::items::AtomicItemUpdateOutcome;

use crate::AppError;
use crate::ports::BoardEvents;

/// Moves the request's item to the first Done status when the request's
/// `status_map_policy_id` is `done_on_mrp_accepted`; any other policy leaves it. The open boards
/// are told through `events` when the item moved.
pub async fn apply_accepted_checked(
    repo: &Repository,
    events: &dyn BoardEvents,
    request_id: &str,
) -> Result<(), AppError> {
    let row = execution_requests::item_and_status_map_policy(repo, request_id).await?;
    let Some(RequestItemPolicy {
        item_id,
        status_map_policy_id: Some(policy_id),
    }) = row
    else {
        return Ok(());
    };
    let Ok(policy) = policy_id.parse::<StatusMapPolicy>() else {
        return Ok(());
    };
    let Ok(item_id) = item_id.parse::<uuid::Uuid>() else {
        return Ok(());
    };
    let Some(item) = repo.get_item(item_id).await? else {
        return Ok(());
    };
    let Some(project) = repo.get_project(item.project_id).await? else {
        return Ok(());
    };
    let Some(target) = policy.target_status(&project.workflow, StatusMapEvent::MrpAccepted) else {
        return Ok(());
    };
    if item.status == target {
        return Ok(());
    }
    let update = UpdateItem {
        status: Some(target.clone()),
        ..Default::default()
    };
    match repo
        .update_item_atomically(item_id, update, &project.workflow, None)
        .await?
    {
        AtomicItemUpdateOutcome::Updated {
            item, old_status, ..
        } => events.item_moved(item.project_id, item.id, old_status, item.status.clone()),
        outcome => {
            tracing::warn!(request_id, %item_id, target, ?outcome, "mrp accepted: transition refused, item untouched");
        }
    }
    Ok(())
}

/// [`apply_accepted_checked`] for the merge, which has already merged: a failure to read the
/// store is logged, never undoes the merge.
pub async fn apply_accepted(repo: &Repository, events: &dyn BoardEvents, request_id: &str) {
    if let Err(error) = apply_accepted_checked(repo, events, request_id).await {
        tracing::warn!(request_id, %error, "mrp accepted: could not move the item");
    }
}
