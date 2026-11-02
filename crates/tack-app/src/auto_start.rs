//! A project that starts its own work: the tick that turns a task in the project's
//! auto-start status, with a role or a profile chosen on it and no run yet, into a run. The task stays
//! in its status: only a person moves it. Which tasks and how many is `tack_core::auto_start::pick`'s;
//! this is the rows that feed it and the enqueue that follows.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use tack_core::auto_start::{Candidate, Facts, Limits, Why, pick, why};
use tack_core::dependency::{DependencyEdge, DependencyGraph};
use tack_core::models::{DependencyType, Priority, Project};
use tack_core::run_defaults::{Resolved, Source};
use tack_db::{Repository, queries};
use tack_protocol::Clock;
use uuid::Uuid;

use crate::background::{Loops, Start};
use crate::runs::start_run::{
    PermissionRequest, RunRequest, default_selector, resolve_profile, start_run,
};
use crate::{AppError, merge};
use tack_protocol::execution::Approvals;

/// The principal a run started by the board itself is recorded under.
const PRINCIPAL: &str = "auto_start";

/// What one pass did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Outcome {
    /// Runs the pass started.
    pub started: usize,
}

/// The loop `tack serve` starts: `tick` every `interval`, the first pass one interval after
/// the start, a failed pass logged. The caller does not start it when auto-start is off
/// (`auto_start_seconds` is 0).
pub fn spawn(loops: &mut Loops, repo: Repository, clock: Arc<dyn Clock>, interval: Duration) {
    loops.spawn_tick("auto_start", Start::AfterInterval, interval, move || {
        let (repo, clock) = (repo.clone(), clock.clone());
        async move {
            if let Err(error) = tick(&repo, clock.as_ref()).await {
                tracing::warn!(%error, "auto-start: could not list projects");
            }
        }
    });
}

/// One pass over every project with auto-start on.
pub async fn tick(repo: &Repository, clock: &dyn Clock) -> Result<Outcome, AppError> {
    let ids = queries::projects::auto_start_project_ids(repo).await?;
    let mut outcome = Outcome::default();
    for id in ids.iter().filter_map(|id| id.parse::<Uuid>().ok()) {
        match repo.get_project(id).await {
            Ok(Some(project)) => match project_tick(repo, clock, &project).await {
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

/// What `pick` is given for one project, and the numbers its sentences quote.
struct Plan {
    candidates: Vec<Candidate>,
    limits: Limits,
    facts: Facts,
}

/// The tasks waiting in the project's start status with someone to do them, and the limits they
/// meet. `None` when nothing waits.
async fn plan(
    repo: &Repository,
    clock: &dyn Clock,
    project: &Project,
) -> Result<Option<Plan>, AppError> {
    let Some(waiting_in) = project.auto_start_status.as_deref() else {
        return Ok(None);
    };
    let project_id = project.id.to_string();
    let rows = queries::items::waiting_with_someone_to_do_it(repo, &project_id, waiting_in).await?;
    if rows.is_empty() {
        return Ok(None);
    }
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
            Some(resolved) => default_selector(repo, resolved.fleet_id.clone(), clock.now())
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
    };
    let facts = Facts {
        max_open: limits.max_open,
        open_now: limits.open_now,
    };
    Ok(Some(Plan {
        candidates,
        limits,
        facts,
    }))
}

/// The waiting tasks of a project that the next pass would not start, most urgent first, each
/// with why. Nothing is started or written.
pub async fn explain(
    repo: &Repository,
    clock: &dyn Clock,
    project: &Project,
) -> Result<Vec<(Uuid, Why)>, AppError> {
    let Some(plan) = plan(repo, clock, project).await? else {
        return Ok(Vec::new());
    };
    let (_, skipped) = pick(&plan.candidates, &plan.limits);
    Ok(skipped
        .into_iter()
        .map(|(item_id, skip)| (item_id, why(&skip, &plan.facts)))
        .collect())
}

/// The tasks of one project to start now. How many runs it started.
async fn project_tick(
    repo: &Repository,
    clock: &dyn Clock,
    project: &Project,
) -> Result<usize, AppError> {
    let Some(Plan {
        candidates, limits, ..
    }) = plan(repo, clock, project).await?
    else {
        return Ok(0);
    };
    let (picked, skipped) = pick(&candidates, &limits);
    for (item_id, why) in skipped {
        tracing::debug!(%item_id, ?why, "auto-start: skipped");
    }
    let mut count = 0;
    for item_id in picked {
        if let Some(candidate) = candidates.iter().find(|c| c.item_id == item_id)
            && start(repo, clock, project, candidate).await
        {
            count += 1;
        }
    }
    Ok(count)
}

/// What a run started without a person ticking boxes is allowed: the profile's tools for its
/// agent, the network only if its role needs it, and a question before each action where some
/// eligible machine does not sandbox the agent.
pub(crate) async fn permissions_for(
    repo: &Repository,
    project: &Project,
    resolved: &Resolved,
) -> Permitted {
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
    let tools: Vec<String> = policy
        .as_ref()
        .and_then(|policy| serde_json::from_value(policy["tools"][harness.as_str()].clone()).ok())
        .unwrap_or_default();
    // Nobody ticks a box here, so the profile's role says whether the run may reach the network.
    let network = policy
        .as_ref()
        .is_some_and(tack_core::run_defaults::profile_network);
    // The Run dialog's rule: auto where the machine sandboxes this agent, ask
    // where it does not. Absent `approvals` means auto, so it is set only when
    // some eligible machine reports no sandbox for the agent.
    let mut permissions = PermissionRequest::new(tools, Some(network));
    if !sandboxed_everywhere(repo, resolved.fleet_id.as_deref(), &harness).await {
        permissions.approvals = Some(Approvals::Ask);
    }
    Permitted {
        permissions,
        policy,
        harness,
    }
}

/// What [`permissions_for`] settled, with the two facts it read to settle it.
pub(crate) struct Permitted {
    pub permissions: PermissionRequest,
    /// The profile's tool policy, when it has one.
    pub policy: Option<Value>,
    /// The agent the run goes to.
    pub harness: String,
}

/// Enqueue the run of one picked task and say so on its activity. The task's status is not
/// touched: it stays where the person put it. Whether the run was started by this pass.
async fn start(
    repo: &Repository,
    clock: &dyn Clock,
    project: &Project,
    candidate: &Candidate,
) -> bool {
    let Some(resolved) = &candidate.profile else {
        return false;
    };
    let Permitted {
        permissions,
        policy,
        harness,
    } = permissions_for(repo, project, resolved).await;
    let input = RunRequest {
        item_id: candidate.item_id,
        idempotency_key: format!("auto-start:{}:{}", candidate.item_id, candidate.version),
        selector: None,
        agent_profile_id: None,
        requested_harness_kind: None,
        requested_model: None,
        agent_profile_snapshot: None,
        repository_snapshot: None,
        permission_policy: permissions,
        budgets: json!({}),
        environment: json!({}),
        metadata: json!({ "started_by": "auto_start" }),
        timeout_seconds: None,
        verify: None,
        push_branch: None,
        autonomy: None,
        origin: None,
    };
    match start_run(repo, clock.now(), PRINCIPAL, input).await {
        Ok(created) if !created.replayed => {}
        Ok(_) => return false,
        Err(refusal) => {
            tracing::warn!(
                item_id = %candidate.item_id,
                status = refusal.status(),
                code = ?refusal.code,
                message = %refusal.message,
                "auto-start: could not create the run"
            );
            return false;
        }
    }
    let waiting_in = project.auto_start_status.as_deref().unwrap_or_default();
    let role = match &resolved.source {
        Source::Role(name) => Some(name.clone()),
        _ => repo
            .profiles_for_item(candidate.item_id)
            .await
            .ok()
            .and_then(|roles| roles.into_iter().next().map(|(name, _)| name)),
    };
    let who = match role {
        Some(role) => format!("with the role {role}"),
        None => {
            format!(
                "with no role, and goes to {}",
                destination(policy.as_ref(), &harness)
            )
        }
    };
    merge::tell(
        repo,
        &candidate.item_id.to_string(),
        format!("Started by itself: it reached {waiting_in} {who}"),
    )
    .await;
    true
}

/// Where a task with no role goes, in words: the team its profile names, else the agent.
fn destination(policy: Option<&Value>, harness: &str) -> String {
    let team = policy
        .and_then(|policy| policy["docket"]["team"].as_str())
        .filter(|team| !team.is_empty());
    match team {
        Some(team) => format!("the team {team}"),
        None => agent_name(harness),
    }
}

/// The name a person reads for an agent kind.
fn agent_name(kind: &str) -> String {
    match kind {
        "claude-code" => "Claude Code",
        "codex" => "Codex",
        "opencode" => "OpenCode",
        "docket" => "Docket",
        other => other,
    }
    .to_owned()
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
