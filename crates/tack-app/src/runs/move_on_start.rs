//! A run starting can move its item: the project's `on_start_status` is where the work now is, so
//! the board stops saying it waits. It is the counterpart of the move a finished run makes in
//! `complete`, and goes through the same write as any other edit of the item.

use tack_core::models::UpdateItem;
use tack_db::Repository;
use tack_db::queries;
use tack_db::repo::items::AtomicItemUpdateOutcome;
use uuid::Uuid;

use super::complete::ItemMoved;

/// Best-effort: moves the attempt's item to the project's `on_start_status`. `None` when there is
/// nothing to move: no status set, the item is already there, or it is in a done status (a rerun
/// of finished work does not reopen it). A refused move or any lookup failure is logged at `warn`
/// and never fails the start the runner reported.
pub async fn move_item(repo: &Repository, attempt_id: &str) -> Option<ItemMoved> {
    let facts = match queries::execution_requests::status_map_facts_of_attempt(repo, attempt_id)
        .await
    {
        Ok(Some(found)) => found,
        Ok(None) => return None,
        Err(err) => {
            tracing::warn!(attempt_id, error = %err, "on start: could not read the request snapshot");
            return None;
        }
    };
    let item_id = facts.item_id.parse::<Uuid>().ok()?;
    let item = repo.get_item(item_id).await.ok()??;
    let project = repo.get_project(item.project_id).await.ok()??;
    let target = project.on_start_status.as_deref().and_then(|name| {
        project
            .workflow
            .statuses
            .iter()
            .find(|status| status.name.eq_ignore_ascii_case(name))
            .map(|status| status.name.clone())
    })?;
    if item.status == target || project.workflow.is_done_status(&item.status) {
        return None;
    }
    let outcome = repo
        .update_item_atomically(
            item_id,
            UpdateItem {
                status: Some(target.clone()),
                ..Default::default()
            },
            &project.workflow,
            None,
        )
        .await;
    match outcome {
        Ok(AtomicItemUpdateOutcome::Updated {
            item, old_status, ..
        }) => Some(ItemMoved {
            project_id: item.project_id,
            item_id: item.id,
            old_status,
            new_status: item.status.clone(),
        }),
        Ok(outcome) => {
            tracing::warn!(attempt_id, %item_id, target, ?outcome, "on start: transition refused, item untouched");
            None
        }
        Err(err) => {
            tracing::warn!(attempt_id, %item_id, error = %err, "on start: could not move the item");
            None
        }
    }
}
