//! A project that starts its own work: the tick that turns a task in the project's
//! auto-start status, with a role, into a run and moves it to the first in-progress
//! status. Which tasks and how many is `tack_core::auto_start::pick`'s; this is the
//! rows that feed it and the enqueue and the move that follow.

use std::collections::HashMap;
use std::sync::Arc;
use tack_protocol::SystemClock;

use chrono::Utc;
use serde_json::{Value, json};
use tack_core::auto_start::{Candidate, Limits, pick};
use tack_core::dependency::{DependencyEdge, DependencyGraph};
use tack_core::models::{DependencyType, Priority, Project, UpdateItem};
use tack_core::run_defaults::Source;
use tack_core::workflow::StatusCategory;
use tack_db::repo::items::AtomicItemUpdateOutcome;
use uuid::Uuid;

use crate::handlers::executions::{
    CreateExecution, OperatorExecutionState, create_as, default_selector, resolve_profile,
};
use crate::handlers::websocket::{self, BoardEvent};
use crate::router::AppState;

/// The principal a run started by the board itself is recorded under.
const PRINCIPAL: &str = "auto_start";

/// The states of a request that still occupies a run.
const OPEN_STATES: &str = "('queued','leased','preparing','running','waiting_decision')";

/// Background task: `tick` every `config.auto_start_seconds`. Returns at once when that is 0.
pub async fn run_auto_start_task(state: AppState) {
    if state.config.auto_start_seconds == 0 {
        return;
    }
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(
        state.config.auto_start_seconds,
    ));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    interval.tick().await; // skip the first immediate tick
    loop {
        interval.tick().await;
        tick(&state).await;
    }
}

/// One pass over every project with auto-start on.
pub async fn tick(state: &AppState) {
    let ids: Vec<String> = match sqlx::query_scalar(
        "SELECT id FROM projects WHERE auto_start_enabled = 1 AND archived = 0 AND auto_start_status IS NOT NULL",
    )
    .fetch_all(state.repo.pool())
    .await
    {
        Ok(ids) => ids,
        Err(error) => {
            tracing::warn!(%error, "auto-start: could not list projects");
            return;
        }
    };
    for id in ids.iter().filter_map(|id| id.parse::<Uuid>().ok()) {
        match state.repo.get_project(id).await {
            Ok(Some(project)) => {
                if let Err(error) = project_tick(state, &project).await {
                    tracing::warn!(project_id = %id, %error, "auto-start: project pass failed");
                }
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(project_id = %id, %error, "auto-start: could not load project")
            }
        }
    }
}

async fn project_tick(state: &AppState, project: &Project) -> Result<(), sqlx::Error> {
    let Some(waiting_in) = project.auto_start_status.as_deref() else {
        return Ok(());
    };
    let pool = state.repo.pool();
    let project_id = project.id.to_string();
    let rows: Vec<(String, String, i32, i64)> = sqlx::query_as(
        "SELECT id, priority, sort_order, version FROM items
         WHERE project_id = ? AND status = ?
           AND EXISTS (SELECT 1 FROM item_roles ir WHERE ir.item_id = items.id)",
    )
    .bind(&project_id)
    .bind(waiting_in)
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        return Ok(());
    }
    let Some(started) = project
        .workflow
        .statuses
        .iter()
        .filter(|status| status.category == StatusCategory::InProgress)
        .min_by_key(|status| status.order)
    else {
        tracing::debug!(project_id = %project.id, "auto-start: the workflow has no in-progress status");
        return Ok(());
    };
    let wip_room = match started.wip_limit {
        Some(limit) => {
            let taken = state
                .repo
                .count_items_by_status(project.id, &started.name)
                .await?;
            Some(limit.saturating_sub(usize::try_from(taken).unwrap_or(usize::MAX)))
        }
        None => None,
    };
    let open: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT r.item_id FROM execution_requests r JOIN items i ON i.id = r.item_id
         WHERE i.project_id = ? AND r.state IN {OPEN_STATES}"
    )))
    .bind(&project_id)
    .fetch_all(pool)
    .await?;
    let budget_reached =
        tack_orch::scheduler::wiring::budget_reached(&state.repo, &project_id, Utc::now())
            .await?
            .is_some();

    let item_ids: Vec<Uuid> = rows.iter().filter_map(|(id, ..)| id.parse().ok()).collect();
    let edges: Vec<DependencyEdge> = state
        .repo
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
    let statuses: HashMap<Uuid, String> =
        sqlx::query_as::<_, (String, String)>("SELECT id, status FROM items WHERE project_id = ?")
            .bind(&project_id)
            .fetch_all(pool)
            .await?
            .into_iter()
            .filter_map(|(id, status)| Some((id.parse().ok()?, status)))
            .collect();
    let status_of = |id: Uuid| statuses.get(&id).cloned();

    let mut candidates = Vec::new();
    for (id, priority, sort_order, version) in rows {
        let Ok(item_id) = id.parse::<Uuid>() else {
            continue;
        };
        let Some(item) = state.repo.get_item(item_id).await? else {
            continue;
        };
        // Without an agent to name, the run could not be created at all.
        let profile = resolve_profile(&state.repo, &item, project, None)
            .await
            .ok()
            .flatten()
            .filter(|resolved| resolved.harness.is_some() || project.default_harness.is_some());
        let has_machine = match &profile {
            Some(resolved) => default_selector(&state.repo, resolved.fleet_id.clone())
                .await
                .is_ok(),
            None => false,
        };
        candidates.push(Candidate {
            item_id,
            priority: serde_json::from_value::<Priority>(Value::String(priority))
                .unwrap_or_default(),
            sort_order,
            version,
            has_open_request: open.contains(&id),
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
    for item_id in picked {
        if let Some(candidate) = candidates.iter().find(|c| c.item_id == item_id) {
            start(state, project, candidate, &started.name).await;
        }
    }
    Ok(())
}

/// Enqueue the run of one picked task, then move it to the in-progress status.
async fn start(state: &AppState, project: &Project, candidate: &Candidate, started: &str) {
    let Some(resolved) = &candidate.profile else {
        return;
    };
    let harness = resolved
        .harness
        .clone()
        .or_else(|| project.default_harness.clone())
        .unwrap_or_default();
    // The profile's tools for this agent, as the Run dialog sends them.
    let policy: Option<String> =
        sqlx::query_scalar("SELECT tool_policy FROM agent_profiles WHERE id = ?")
            .bind(&resolved.profile_id)
            .fetch_optional(state.repo.pool())
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
    let approvals = if sandboxed_everywhere(state, resolved.fleet_id.as_deref(), &harness).await {
        json!({ "tools": tools, "network": network })
    } else {
        json!({ "tools": tools, "network": network, "approvals": "ask" })
    };
    let input = CreateExecution {
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
        origin: None,
    };
    let operator = OperatorExecutionState::with_clock(state.repo.clone(), Arc::new(SystemClock));
    match create_as(operator, PRINCIPAL.to_owned(), input).await {
        Ok(axum::Json(created)) if !created.replayed => {}
        Ok(_) => return,
        Err((status, body)) => {
            tracing::warn!(item_id = %candidate.item_id, %status, body = %body.0, "auto-start: could not create the run");
            return;
        }
    }
    let moved = state
        .repo
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
            return;
        }
    };
    websocket::broadcast_event(
        state,
        BoardEvent::ItemUpdated {
            project_id: item.project_id,
            item_id: item.id,
            old_status: Some(old_status.clone()),
            new_status: item.status.clone(),
        },
    );
    let role = match &resolved.source {
        Source::Role(name) => Some(name.clone()),
        _ => state
            .repo
            .profiles_for_item(item.id)
            .await
            .ok()
            .and_then(|roles| roles.into_iter().next().map(|(name, _)| name)),
    };
    crate::merge::tell(
        state,
        &item.id.to_string(),
        format!(
            "Started by itself: it reached {old_status} with the role {}",
            role.unwrap_or_default()
        ),
    )
    .await;
}

/// Whether every machine the run may land on (the profile's fleet, else the only
/// active runner) reports the agent as sandboxed, by bwrap or by its own jail.
async fn sandboxed_everywhere(state: &AppState, fleet_id: Option<&str>, harness: &str) -> bool {
    let sql = match fleet_id {
        Some(_) => {
            "SELECT r.capability_snapshot FROM agent_runners r \
             JOIN agent_fleet_members m ON m.runner_id = r.id \
             WHERE m.fleet_id = ? AND r.state = 'active' AND r.revoked_at IS NULL"
        }
        None => {
            "SELECT capability_snapshot FROM agent_runners \
             WHERE ? IS NULL AND state = 'active' AND revoked_at IS NULL"
        }
    };
    let snapshots: Vec<String> = sqlx::query_scalar(sql)
        .bind(fleet_id)
        .fetch_all(state.repo.pool())
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
