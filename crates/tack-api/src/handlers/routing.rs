//! Who would do each open card of a project (ADR 0036, decision 6): the one answer, so Flow, the
//! Map and the item page draw the lane a card runs in. The choice is
//! `tack_core::run_defaults::resolve`'s, the same a run takes; this joins the chosen profile's
//! team and agent to what the computers report.

use std::collections::{HashMap, HashSet};

use axum::extract::State;
use serde::Serialize;
use serde_json::Value;
use tracing::instrument;
use utoipa::ToSchema;
use uuid::Uuid;

use tack_app::runs::start_run::{profile_candidates, profile_source_label};
use tack_core::run_defaults::{named_team, resolve};
use tack_db::queries;

use crate::error::{ApiError, ApiResult};
use crate::extract::{Json, Path};
use crate::router::AppState;

/// Why nobody would take a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum NobodyReason {
    /// No profile chosen on the card, it has no role and neither the project nor the board has a
    /// profile to fall back on.
    NoRole,
    /// Its roles name no profile, and neither does the project nor the board.
    RoleWithoutProfile,
    /// The profile that would do it no longer exists.
    ProfileMissing,
    /// The profile names a team that no active computer reports.
    TeamNotReported,
    /// The profile runs an agent that no active computer reports.
    HarnessNotReported,
}

/// The profile that would do one open card, and where it would run.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Routed {
    pub item_id: Uuid,
    /// The profile chosen; `null` when nobody would take the card and none is chosen.
    pub profile_id: Option<String>,
    /// Where the choice came from: `run_settings` (chosen on the card), `role:<name>`,
    /// `project_default`, `implementer` (the built-in profile, when nothing else is named) or
    /// `ambiguous` (its roles name different profiles, so the project's default is used).
    pub source: Option<String>,
    /// The role the profile came from; with `nobody` `role_without_profile`, the role that names
    /// none.
    pub role: Option<String>,
    /// The team the profile names; the card runs in it, or nobody takes it when no computer
    /// reports it. `null` for a profile that names none: the card runs in its single agent.
    pub team: Option<String>,
    /// The agent the profile runs.
    pub harness: Option<String>,
    /// Why nobody would take the card; `null` when someone would.
    pub nobody: Option<NobodyReason>,
}

/// What the active computers report: the agents they have and the teams under each.
struct Reported {
    harnesses: HashSet<String>,
    teams: HashSet<(String, String)>,
}

impl Reported {
    async fn read(state: &AppState) -> ApiResult<Reported> {
        let mut reported = Reported {
            harnesses: HashSet::new(),
            teams: HashSet::new(),
        };
        for runner in state.repo.list_runners().await? {
            if runner.state != "active" || runner.revoked_at.is_some() {
                continue;
            }
            for harness in tack_app::autonomy::harnesses_of(&runner.capability_snapshot) {
                let Some(kind) = serde_json::to_value(&harness.harness_kind)
                    .ok()
                    .and_then(|value| value.as_str().map(str::to_owned))
                else {
                    continue;
                };
                for team in harness.teams.unwrap_or_default() {
                    reported.teams.insert((kind.clone(), team.name));
                }
                reported.harnesses.insert(kind);
            }
        }
        Ok(reported)
    }

    /// The agent that reports `team`: the named one, else the first that does.
    fn agent_of(&self, team: &str, agent: Option<&str>) -> Option<String> {
        match agent {
            Some(agent) => self
                .teams
                .contains(&(agent.to_owned(), team.to_owned()))
                .then(|| agent.to_owned()),
            None => self
                .teams
                .iter()
                .filter(|(_, name)| name == team)
                .map(|(kind, _)| kind.clone())
                .min(),
        }
    }
}

/// GET /api/projects/{id}/routing
#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/projects/{id}/routing",
    tag = "projects",
    params(("id" = Uuid, Path, description = "Project ID")),
    responses(
        (status = 200, description = "For each open card of the project, the profile that would do it, where that choice came from, and the team or agent it runs in; or why nobody would take it. The same choice a run takes", body = Vec<Routed>),
        (status = 404, description = "No such project", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn get_routing(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<Routed>>> {
    let project = state
        .repo
        .get_project(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {id} not found")))?;
    let items = state.repo.get_item_tree(id).await?;
    let implementer = queries::agent_profiles::implementer_profile_id(&state.repo).await?;
    let candidates = profile_candidates(&state.repo).await?;
    let policies: HashMap<String, Value> = queries::agent_profiles::profile_candidates(&state.repo)
        .await?
        .into_iter()
        .map(|row| {
            (
                row.id,
                serde_json::from_str(&row.tool_policy).unwrap_or_default(),
            )
        })
        .collect();
    let reported = Reported::read(&state).await?;

    let profile_of_role: HashMap<Uuid, (String, Option<String>)> = state
        .repo
        .list_roles(id)
        .await?
        .into_iter()
        .map(|role| (role.id, (role.name, role.agent_profile_id)))
        .collect();
    let mut roles_of: HashMap<Uuid, Vec<(String, Option<String>)>> = HashMap::new();
    for (item, role) in state.repo.item_roles_for_project(id).await? {
        if let Some(found) = profile_of_role.get(&role) {
            roles_of.entry(item).or_default().push(found.clone());
        }
    }

    let routed = items
        .iter()
        .filter(|item| !project.workflow.is_done_status(&item.status))
        .map(|item| {
            let mut roles = roles_of.remove(&item.id).unwrap_or_default();
            roles.sort();
            let resolved = resolve(
                None,
                item.run_settings
                    .as_ref()
                    .and_then(|settings| settings.agent_profile_id.as_deref()),
                &roles,
                project.default_profile_id.as_deref(),
                implementer.as_deref(),
                &candidates,
            );
            let Some(resolved) = resolved else {
                let bare = roles.first().map(|(name, _)| name.clone());
                return Routed {
                    item_id: item.id,
                    profile_id: None,
                    source: None,
                    role: bare.clone(),
                    team: None,
                    harness: None,
                    nobody: Some(if bare.is_some() {
                        NobodyReason::RoleWithoutProfile
                    } else {
                        NobodyReason::NoRole
                    }),
                };
            };
            let role = match &resolved.source {
                tack_core::run_defaults::Source::Role(name) => Some(name.clone()),
                _ => None,
            };
            let mut routed = Routed {
                item_id: item.id,
                profile_id: Some(resolved.profile_id.clone()),
                source: profile_source_label(&resolved.source),
                role,
                team: None,
                harness: resolved.harness.clone(),
                nobody: None,
            };
            let Some(policy) = policies.get(&resolved.profile_id) else {
                routed.nobody = Some(NobodyReason::ProfileMissing);
                return routed;
            };
            match named_team(policy) {
                Some((team, agent)) => {
                    let reporting = reported.agent_of(&team, agent.as_deref());
                    if reporting.is_none() {
                        routed.nobody = Some(NobodyReason::TeamNotReported);
                    }
                    routed.harness = reporting.or(agent);
                    routed.team = Some(team);
                }
                None => {
                    if routed
                        .harness
                        .as_ref()
                        .is_some_and(|harness| !reported.harnesses.contains(harness))
                    {
                        routed.nobody = Some(NobodyReason::HarnessNotReported);
                    }
                }
            }
            routed
        })
        .collect();
    Ok(Json(routed))
}
