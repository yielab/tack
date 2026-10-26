//! A project that starts its own work: the tick that turns a task in the project's
//! auto-start status, with a role, into a run and moves it to the first in-progress
//! status. Which tasks and how many is `tack_core::auto_start::pick`'s; this is the
//! rows that feed it and the enqueue and the move that follow.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use tack_core::auto_start::{Candidate, Limits, pick};
use tack_core::dependency::{DependencyEdge, DependencyGraph};
use tack_core::models::{DependencyType, Priority, Project, UpdateItem};
use tack_core::run_defaults::Source;
use tack_core::workflow::StatusCategory;
use tack_db::repo::items::AtomicItemUpdateOutcome;
use tack_db::{Repository, queries};
use tack_protocol::Clock;
use uuid::Uuid;

use crate::host::Host;
use crate::runs::start_run::{RunRequest, default_selector, resolve_profile, start_run};
use crate::{AppError, merge};

/// The principal a run started by the board itself is recorded under.
const PRINCIPAL: &str = "auto_start";

/// What one pass did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Outcome {
    /// Runs the pass started.
    pub started: usize,
}

/// The loop `tack serve` spawns: `tick` every `interval`, the first pass one interval after
/// the start, a failed pass logged. The caller does not spawn it when auto-start is off
/// (`auto_start_seconds` is 0).
pub async fn run(repo: Repository, clock: Arc<dyn Clock>, host: Arc<dyn Host>, interval: Duration) {
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ticker.tick().await; // skip the first immediate tick
    loop {
        ticker.tick().await;
        if let Err(error) = tick(&repo, clock.as_ref(), host.as_ref()).await {
            tracing::warn!(%error, "auto-start: could not list projects");
        }
    }
}

/// One pass over every project with auto-start on.
pub async fn tick(
    repo: &Repository,
    clock: &dyn Clock,
    host: &dyn Host,
) -> Result<Outcome, AppError> {
    let ids = queries::projects::auto_start_project_ids(repo).await?;
    let mut outcome = Outcome::default();
    for id in ids.iter().filter_map(|id| id.parse::<Uuid>().ok()) {
        match repo.get_project(id).await {
            Ok(Some(project)) => match project_tick(repo, clock, host, &project).await {
                Ok(started) => outcome.started += started,
                Err(error) => {
                    tracing::warn!(project_id = %id, %error, "auto-start: project pass failed");
                }
            },
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(project_id = %id, %error, "auto-start: could not load project")
            }
        }
    }
    Ok(outcome)
}

/// The tasks of one project to start now. How many runs it started.
async fn project_tick(
    repo: &Repository,
    clock: &dyn Clock,
    host: &dyn Host,
    project: &Project,
) -> Result<usize, AppError> {
    let Some(waiting_in) = project.auto_start_status.as_deref() else {
        return Ok(0);
    };
    let project_id = project.id.to_string();
    let rows = queries::items::waiting_with_a_role(repo, &project_id, waiting_in).await?;
    if rows.is_empty() {
        return Ok(0);
    }
    let Some(started) = project
        .workflow
        .statuses
        .iter()
        .filter(|status| status.category == StatusCategory::InProgress)
        .min_by_key(|status| status.order)
    else {
        tracing::debug!(project_id = %project.id, "auto-start: the workflow has no in-progress status");
        return Ok(0);
    };
    let wip_room = match started.wip_limit {
        Some(limit) => {
            let taken = repo
                .count_items_by_status(project.id, &started.name)
                .await?;
            Some(limit.saturating_sub(usize::try_from(taken).unwrap_or(usize::MAX)))
        }
        None => None,
    };
    let open = queries::execution_requests::items_with_an_open_request(repo, &project_id).await?;
    let budget_reached = crate::scheduler_wiring::budget_reached(repo, &project_id, clock.now())
        .await?
        .is_some();

    let item_ids: Vec<Uuid> = rows.iter().filter_map(|row| row.id.parse().ok()).collect();
    let edges: Vec<DependencyEdge> = repo
        .list_dependencies_for_items(&item_ids)
        .await?
        .into_iter()
        .filter(|d| {
            matches!(
                d.dependency_type,
                DependencyType::Blocks | DependencyType::IsBlockedBy
            )
        })
        .map(|d| DependencyEdge {
            source: d.source_item_id,
            target: d.target_item_id,
            dep_type: d.dependency_type,
        })
        .collect();
    let graph = DependencyGraph::from_edges(&edges);
    let statuses: HashMap<Uuid, String> = queries::items::statuses_of_project(repo, &project_id)
        .await?
        .into_iter()
        .filter_map(|row| Some((row.id.parse().ok()?, row.status)))
        .collect();
    let status_of = |id: Uuid| statuses.get(&id).cloned();

    let mut candidates = Vec::new();
    for row in rows {
        let Ok(item_id) = row.id.parse::<Uuid>() else {
            continue;
        };
        let Some(item) = repo.get_item(item_id).await? else {
            continue;
        };
        // Without an agent to name, the run could not be created at all.
        let profile = resolve_profile(repo, &item, project, None)
            .await
            .ok()
            .flatten()
            .filter(|resolved| resolved.harness.is_some() || project.default_harness.is_some());
        let has_machine = match &profile {
            Some(resolved) => default_selector(repo, resolved.fleet_id.clone())
                .await
                .is_ok(),
            None => false,
        };
        candidates.push(Candidate {
            item_id,
            priority: serde_json::from_value::<Priority>(Value::String(row.priority))
                .unwrap_or_default(),
            sort_order: row.sort_order,
            version: row.version,
            has_open_request: open.contains(&row.id),
            unfinished_blockers: graph
                .unfinished_blockers(item_id, &status_of, &project.workflow)
                .len(),
            profile,
            has_machine,
        });
    }
    let limits = Limits {
        max_open: usize::try_from(project.auto_start_max_open).unwrap_or(1),
        open_now: open.len(),
        budget_reached,
        wip_room,
    };
    let (picked, skipped) = pick(&candidates, &limits);
    for (item_id, why) in skipped {
        tracing::debug!(%item_id, ?why, "auto-start: skipped");
    }
    let mut count = 0;
    for item_id in picked {
        if let Some(candidate) = candidates.iter().find(|c| c.item_id == item_id)
            && start(repo, clock, host, project, candidate, &started.name).await
        {
            count += 1;
        }
    }
    Ok(count)
}

/// Enqueue the run of one picked task, then move it to the in-progress status. Whether the run
/// was started by this pass.
async fn start(
    repo: &Repository,
    clock: &dyn Clock,
    host: &dyn Host,
    project: &Project,
    candidate: &Candidate,
    started: &str,
) -> bool {
    let Some(resolved) = &candidate.profile else {
        return false;
    };
    let harness = resolved
        .harness
        .clone()
        .or_else(|| project.default_harness.clone())
        .unwrap_or_default();
    // The profile's tools for this agent, as the Run dialog sends them.
    let policy: Option<String> = queries::agent_profiles::tool_policy(repo, &resolved.profile_id)
        .await
        .ok()
        .flatten();
    let policy = policy.and_then(|text| serde_json::from_str::<Value>(&text).ok());
    let tools = policy
        .as_ref()
        .map(|policy| policy["tools"][harness.as_str()].clone())
        .filter(Value::is_array)
        .unwrap_or_else(|| json!([]));
    // Nobody ticks a box here, so the profile's role says whether the run may reach the network.
    let network = policy
        .as_ref()
        .is_some_and(tack_core::run_defaults::profile_network);
    // The Run dialog's rule: auto where the machine sandboxes this agent, ask
    // where it does not. Absent `approvals` means auto, so it is set only when
    // some eligible machine reports no sandbox for the agent.
    let approvals = if sandboxed_everywhere(repo, resolved.fleet_id.as_deref(), &harness).await {
        json!({ "tools": tools, "network": network })
    } else {
        json!({ "tools": tools, "network": network, "approvals": "ask" })
    };
    let input = RunRequest {
        item_id: candidate.item_id,
        idempotency_key: format!("auto-start:{}:{}", candidate.item_id, candidate.version),
        selector_kind: None,
        selector_id: None,
        agent_profile_id: None,
        requested_harness_kind: None,
        requested_model: None,
        agent_profile_snapshot: None,
        repository_snapshot: None,
        permission_policy: approvals,
        budgets: json!({}),
        environment: json!({}),
        metadata: json!({ "started_by": "auto_start" }),
        timeout_seconds: None,
        status_map_policy_id: None,
        verify: None,
        push_branch: None,
        autonomy: None,
        origin: None,
    };
    match start_run(repo, clock, PRINCIPAL, input).await {
        Ok(created) if !created.replayed => {}
        Ok(_) => return false,
        Err(refusal) => {
            tracing::warn!(
                item_id = %candidate.item_id,
                status = refusal.status,
                code = ?refusal.code,
                message = %refusal.message,
                "auto-start: could not create the run"
            );
            return false;
        }
    }
    let moved = repo
        .update_item_atomically(
            candidate.item_id,
            UpdateItem {
                status: Some(started.to_owned()),
                ..UpdateItem::default()
            },
            &project.workflow,
            None,
        )
        .await;
    let (item, old_status) = match moved {
        Ok(AtomicItemUpdateOutcome::Updated {
            item, old_status, ..
        }) => (*item, old_status),
        other => {
            tracing::warn!(item_id = %candidate.item_id, outcome = ?other, "auto-start: could not move the task");
            return true;
        }
    };
    host.item_moved(
        item.project_id,
        item.id,
        old_status.clone(),
        item.status.clone(),
    );
    let role = match &resolved.source {
        Source::Role(name) => Some(name.clone()),
        _ => repo
            .profiles_for_item(item.id)
            .await
            .ok()
            .and_then(|roles| roles.into_iter().next().map(|(name, _)| name)),
    };
    merge::tell(
        repo,
        &item.id.to_string(),
        format!(
            "Started by itself: it reached {old_status} with the role {}",
            role.unwrap_or_default()
        ),
    )
    .await;
    true
}

/// Whether every machine the run may land on (the profile's fleet, else the only
/// active runner) reports the agent as sandboxed, by bwrap or by its own jail.
async fn sandboxed_everywhere(repo: &Repository, fleet_id: Option<&str>, harness: &str) -> bool {
    let snapshots = queries::agent_runners::capability_snapshots_of_active_runners(repo, fleet_id)
        .await
        .unwrap_or_default();
    !snapshots.is_empty()
        && snapshots.iter().all(|snapshot| {
            crate::autonomy::harnesses_of(snapshot)
                .iter()
                .find(|h| crate::autonomy::kind_is(h, harness))
                .and_then(|h| h.additional.get("sandbox").and_then(Value::as_str))
                .is_some_and(|sandbox| sandbox == "bwrap" || sandbox == "harness")
        })
}
