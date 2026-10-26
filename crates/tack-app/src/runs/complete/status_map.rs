use serde_json::Value;
use tack_core::workflow::{StatusMapEvent, StatusMapPolicy};
use tack_db::repo::items::AtomicItemUpdateOutcome;
use tack_db::{Repository, queries};
use uuid::Uuid;

/// A board item the completion moved to another status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemMoved {
    pub project_id: Uuid,
    pub item_id: Uuid,
    pub old_status: String,
    pub new_status: String,
}

/// Best-effort: moves the attempt's item per the request snapshot's `status_map_policy_id`,
/// else to the project's `on_finish_status` when the attempt left a patch or a kept workspace.
/// A refused transition or any lookup failure is logged at `warn` and never fails the
/// completion.
pub(super) async fn apply(repo: &Repository, attempt_id: &str) -> Option<ItemMoved> {
    let facts = match queries::execution_requests::status_map_facts_of_attempt(repo, attempt_id)
        .await
    {
        Ok(Some(found)) => found,
        Ok(None) => return None,
        Err(err) => {
            tracing::warn!(attempt_id, error = %err, "status map: could not read the request snapshot");
            return None;
        }
    };
    let policy = match facts
        .policy_id
        .as_deref()
        .map(str::parse::<StatusMapPolicy>)
    {
        Some(Ok(policy)) => Some(policy),
        Some(Err(_)) => {
            tracing::warn!(
                attempt_id,
                policy_id = facts.policy_id,
                "status map: unknown policy id, item untouched"
            );
            return None;
        }
        None => None,
    };
    let item_uuid = facts.item_id.parse::<Uuid>().ok()?;
    let item = repo.get_item(item_uuid).await.ok()??;
    let project = repo.get_project(item.project_id).await.ok()??;
    let left_evidence = facts
        .terminal_reason
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .is_some_and(|reason| {
            reason["workspace_kept_at"].is_string()
                || reason["artifacts"].as_array().is_some_and(|artifacts| {
                    artifacts.iter().any(|artifact| {
                        artifact["kind"] == "patch" && artifact["size_bytes"].as_i64() > Some(0)
                    })
                })
        });
    let on_finish = project
        .on_finish_status
        .as_deref()
        .filter(|_| left_evidence)
        .and_then(|name| {
            project
                .workflow
                .statuses
                .iter()
                .find(|status| status.name.eq_ignore_ascii_case(name))
                .map(|status| status.name.clone())
        });
    let target = policy
        .and_then(|policy| {
            policy.target_status(&project.workflow, StatusMapEvent::AttemptSucceeded)
        })
        .or(on_finish)?;
    if item.status == target {
        return None;
    }
    let update = tack_core::models::UpdateItem {
        status: Some(target.clone()),
        ..Default::default()
    };
    match repo
        .update_item_atomically(item_uuid, update, &project.workflow, None)
        .await
    {
        Ok(AtomicItemUpdateOutcome::Updated {
            item, old_status, ..
        }) => Some(ItemMoved {
            project_id: item.project_id,
            item_id: item.id,
            old_status,
            new_status: item.status.clone(),
        }),
        Ok(outcome) => {
            tracing::warn!(attempt_id, %item_uuid, target, ?outcome, "status map: transition refused, item untouched");
            None
        }
        Err(err) => {
            tracing::warn!(attempt_id, %item_uuid, target, error = %err, "status map: item update failed");
            None
        }
    }
}
