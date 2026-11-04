//! The teams the board's agents report, with the profiles that name them (ADR 0032 decision 1).

use std::collections::BTreeMap;

use axum::Extension;
use axum::extract::State;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::instrument;
use utoipa::{IntoParams, ToSchema};

use tack_core::outline::{BundleTeam, TEAM_SHAPES};
use tack_core::principal::{Capability, Principal, may};
use tack_db::queries;
use tack_protocol::execution::{
    ActivityTask, CapabilitySupport, CurrentStep, FinishedStep, HarnessCapability, TeamMisfit,
    TeamNeeds, TeamReport,
};

use crate::error::{ApiError, ApiResult};
use crate::extract::{Json, Path, Query};
use crate::router::AppState;

/// A profile that names a team.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TeamProfile {
    pub id: String,
    pub name: String,
    /// What the profile needs that the team says it cannot do; a run of it ends before the team
    /// gets the task. Empty when the team meets every need, or did not say enough to tell.
    #[serde(default)]
    pub misfits: Vec<TeamMisfit>,
}

/// One team: what an agent reported about it, or only the name a profile gives it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TeamEntry {
    pub name: String,
    /// The agent that runs the team; `null` for a name only a profile that names no agent uses.
    pub harness: Option<String>,
    /// Whether an active runner reports this team. When `false`, everything below besides
    /// `profiles` is `null`: the board knows the name and nothing else.
    pub reported: bool,
    /// The runner whose report this is: the newest one when several report the team.
    pub runner: Option<String>,
    /// When that runner's agent last reported.
    pub reported_at: Option<DateTime<Utc>>,
    /// Everything the agent said of the team; a field it did not say is absent.
    pub report: Option<TeamReport>,
    /// The profiles that name the team.
    pub profiles: Vec<TeamProfile>,
}

/// Every team any agent reports.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TeamList {
    pub teams: Vec<TeamEntry>,
}

/// One agent an active runner reports, with the runner that reports it.
pub(crate) struct ReportedHarness {
    pub runner: String,
    pub kind: String,
    pub capability: HarnessCapability,
}

/// Every agent every active runner reports.
pub(crate) async fn reported_harnesses(state: &AppState) -> ApiResult<Vec<ReportedHarness>> {
    reported_harnesses_in(&state.repo).await
}

/// [`reported_harnesses`], for a caller that holds the repository and not the whole state.
pub(crate) async fn reported_harnesses_in(
    repo: &tack_db::Repository,
) -> ApiResult<Vec<ReportedHarness>> {
    let mut found = Vec::new();
    for runner in repo.list_runners().await? {
        if runner.state != "active" || runner.revoked_at.is_some() {
            continue;
        }
        for capability in tack_app::autonomy::harnesses_of(&runner.capability_snapshot) {
            let Some(kind) = serde_json::to_value(&capability.harness_kind)
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned))
            else {
                continue;
            };
            found.push(ReportedHarness {
                runner: runner.name.clone(),
                kind,
                capability,
            });
        }
    }
    Ok(found)
}

/// What the runners report of the teams a project file may name, in the file's words: the pods of
/// an agent that has them, never the team of one a local agent is. A shape the file has no word for
/// is left out.
pub(crate) async fn reported_bundle_teams(state: &AppState) -> ApiResult<Vec<BundleTeam>> {
    let mut teams: Vec<BundleTeam> = Vec::new();
    for harness in reported_harnesses(state).await? {
        for team in harness.capability.teams.unwrap_or_default() {
            if team.is_of_agent(&harness.kind) || teams.iter().any(|seen| seen.name == team.name) {
                continue;
            }
            teams.push(BundleTeam {
                name: team.name,
                shape: team
                    .shape
                    .filter(|shape| TEAM_SHAPES.contains(&shape.as_str())),
                delivers: team.delivers.map(|delivers| delivers.word().to_owned()),
                methods: team
                    .methods
                    .unwrap_or_default()
                    .into_iter()
                    .map(|method| method.name)
                    .collect(),
            });
        }
    }
    Ok(teams)
}

/// The methods a profile names for its kinds of card (`docket.pipeline_by_type`), once each.
fn methods_named(policy: &Value) -> Vec<String> {
    let mut methods: Vec<String> = policy
        .get("docket")
        .and_then(|docket| docket.get("pipeline_by_type"))
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|by_type| by_type.values())
        .filter_map(|method| method.as_str().map(str::to_owned))
        .collect();
    methods.sort();
    methods.dedup();
    methods
}

/// What a profile of `kind` with this policy needs that `team` says it cannot do: the profile's
/// own needs, then those of each method it names.
fn profile_misfits(team: &TeamReport, kind: &str, policy: &Value) -> Vec<TeamMisfit> {
    let web = tack_core::run_defaults::profile_network(policy);
    let mut misfits = team.misfits(&TeamNeeds::of(kind, web, None));
    for method in methods_named(policy) {
        for misfit in team.misfits(&TeamNeeds::of(kind, web, Some(&method))) {
            if !misfits.contains(&misfit) {
                misfits.push(misfit);
            }
        }
    }
    misfits
}

/// GET /api/teams
#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/teams",
    tag = "teams",
    responses(
        (status = 200, description = "Every team any active runner reports, once per name and agent, with the profiles that name it; plus the names profiles use that no runner reports", body = TeamList),
    ),
)]
pub async fn list_teams(State(state): State<AppState>) -> ApiResult<Json<TeamList>> {
    // The newest report of each team, by agent and name.
    let mut reported: BTreeMap<(String, String), TeamEntry> = BTreeMap::new();
    for harness in reported_harnesses(&state).await? {
        let probed_at = harness.capability.probed_at;
        for team in harness.capability.teams.unwrap_or_default() {
            let key = (harness.kind.clone(), team.name.clone());
            if reported
                .get(&key)
                .and_then(|seen| seen.reported_at)
                .is_some_and(|seen| seen > probed_at)
            {
                continue;
            }
            reported.insert(
                key,
                TeamEntry {
                    name: team.name.clone(),
                    harness: Some(harness.kind.clone()),
                    reported: true,
                    runner: Some(harness.runner.clone()),
                    reported_at: Some(probed_at),
                    report: Some(team),
                    profiles: Vec::new(),
                },
            );
        }
    }
    // A profile names a team under its agent, or under every agent that reports the name when it
    // names none; a name nobody reports stays a team of its own, marked as not reported. A profile
    // that names no team belongs to the team of one its agent reports, and to nothing when that
    // agent reports none.
    let mut unreported: BTreeMap<(Option<String>, String), TeamEntry> = BTreeMap::new();
    for profile in queries::agent_profiles::profiles_by_name(&state.repo).await? {
        let policy: Value = serde_json::from_str(&profile.tool_policy).unwrap_or_default();
        let line = TeamProfile {
            id: profile.id,
            name: profile.name,
            misfits: Vec::new(),
        };
        let Some((team, agent)) = tack_core::run_defaults::named_team(&policy) else {
            let agent = tack_core::run_defaults::profile_agent(&policy).to_owned();
            if let Some(entry) = reported.get_mut(&(agent.clone(), agent.clone()))
                && let Some(report) = entry.report.as_ref().filter(|r| r.is_of_agent(&agent))
            {
                let misfits = profile_misfits(report, &profile.kind, &policy);
                entry.profiles.push(TeamProfile { misfits, ..line });
            }
            continue;
        };
        let mut found = false;
        for ((kind, name), entry) in &mut reported {
            if *name == team && agent.as_ref().is_none_or(|agent| agent == kind) {
                let misfits = entry
                    .report
                    .as_ref()
                    .map(|report| profile_misfits(report, &profile.kind, &policy))
                    .unwrap_or_default();
                entry.profiles.push(TeamProfile {
                    misfits,
                    ..line.clone()
                });
                found = true;
            }
        }
        if !found {
            unreported
                .entry((agent.clone(), team.clone()))
                .or_insert_with(|| TeamEntry {
                    name: team,
                    harness: agent,
                    reported: false,
                    runner: None,
                    reported_at: None,
                    report: None,
                    profiles: Vec::new(),
                })
                .profiles
                .push(line);
        }
    }
    let mut teams: Vec<TeamEntry> = reported
        .into_values()
        .chain(unreported.into_values())
        .collect();
    teams.sort_by(|a, b| (&a.name, &a.harness).cmp(&(&b.name, &b.harness)));
    Ok(Json(TeamList { teams }))
}

/// Which agent's team is meant when two report a team of the same name.
#[derive(Debug, Deserialize, IntoParams)]
pub struct ActivityQuery {
    pub harness: Option<String>,
}

/// What a team is doing, as its agent last reported it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TeamActivityEntry {
    pub team: String,
    pub harness: String,
    /// Whether the agent can say what its teams are doing. When `false`, `tasks` is empty and
    /// `reason` says why.
    pub supported: bool,
    pub reason: Option<String>,
    /// The runner whose report this is: the newest one when several report the team.
    pub runner: Option<String>,
    /// When the board last heard what the team is doing; `null` when it never has.
    pub reported_at: Option<DateTime<Utc>>,
    /// The tasks the team holds, the ones still going first. A task with `on_board` `null` is work
    /// the board did not start: a person, a routine or another channel gave it to the team
    /// directly. Tasks that ended stay for 14 days.
    pub tasks: Vec<ActivityTask>,
}

/// The text of a stored enum word as the enum.
fn word<T: serde::de::DeserializeOwned>(text: &str) -> Option<T> {
    serde_json::from_value(Value::String(text.to_owned())).ok()
}

fn instant(text: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|at| at.with_timezone(&Utc))
}

/// GET /api/teams/{id}/activity
#[instrument(skip(state, principal))]
#[utoipa::path(
    get,
    path = "/api/teams/{id}/activity",
    tag = "teams",
    params(("id" = String, Path, description = "The team's name"), ActivityQuery),
    responses(
        (status = 200, description = "What the team is doing: every task it holds with the step each is in and the steps it finished, the work the board did not start included (`on_board` is `null`). An agent credential gets the tasks of the projects it may read, and the work the board did not start only if it may read every project. `supported` is `false`, with its reason, for an agent that cannot say", body = TeamActivityEntry),
        (status = 403, description = "An agent credential with the scope `read` on no project", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "not_found (no active runner reports a team of that name)"),
    ),
)]
pub async fn team_activity(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(team): Path<String>,
    Query(query): Query<ActivityQuery>,
) -> ApiResult<Json<TeamActivityEntry>> {
    // The newest report of the team, by the agents that report it.
    let mut found: Option<(String, String, DateTime<Utc>, Option<String>)> = None;
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
            let reports = harness
                .teams
                .as_ref()
                .is_some_and(|teams| teams.iter().any(|reported| reported.name == team));
            if !reports
                || query.harness.as_ref().is_some_and(|wanted| *wanted != kind)
                || found
                    .as_ref()
                    .is_some_and(|(_, _, seen, _)| *seen > harness.probed_at)
            {
                continue;
            }
            let activity = harness.features().and_then(|features| features.activity);
            let unsupported = match activity {
                Some(value) if value.support == CapabilitySupport::Supported => None,
                Some(value) => Some(value.reason.unwrap_or_default()),
                None => Some("this agent does not say".to_owned()),
            };
            found = Some((kind, runner.name.clone(), harness.probed_at, unsupported));
        }
    }
    let Some((harness, runner, _, unsupported)) = found else {
        return Err(ApiError::NotFound(format!(
            "no active runner reports a team named {team}"
        )));
    };
    if unsupported.is_some() {
        return Ok(Json(TeamActivityEntry {
            team,
            harness,
            supported: false,
            reason: unsupported,
            runner: Some(runner),
            reported_at: None,
            tasks: Vec::new(),
        }));
    }
    let mut rows = queries::team_activity::team_tasks(&state.repo, &team, Some(&harness)).await?;
    // A team's tasks cross projects. A caller sees the ones the board started for an item of a
    // project it may read; the work the board did not start belongs to no project and is shown
    // only to a caller that may read them all (the digest's rule).
    if !principal.may_everywhere(Capability::Read) {
        let projects: std::collections::HashMap<String, Option<uuid::Uuid>> =
            queries::team_activity::team_task_projects(&state.repo, &team, &harness)
                .await?
                .into_iter()
                .map(|(task, project)| (task, project.and_then(|id| id.parse().ok())))
                .collect();
        rows.retain(|row| {
            projects
                .get(&row.task_ref)
                .copied()
                .flatten()
                .is_some_and(|project| may(&principal, project, Capability::Read))
        });
    }
    let mut steps = std::collections::HashMap::<String, Vec<FinishedStep>>::new();
    for row in queries::team_activity::team_task_steps(&state.repo, &harness, &team).await? {
        if let Ok(step) = serde_json::from_str::<FinishedStep>(&row.step) {
            steps.entry(row.task_ref).or_default().push(step);
        }
    }
    let reported_at = rows
        .iter()
        .filter_map(|row| instant(&row.reported_at))
        .max();
    let mut tasks: Vec<ActivityTask> = rows
        .into_iter()
        .filter_map(|row| {
            Some(ActivityTask {
                state: word(&row.state)?,
                waits_on: row.waits_on.as_deref().and_then(word),
                current_step: row
                    .current_step
                    .as_deref()
                    .and_then(|text| serde_json::from_str::<CurrentStep>(text).ok()),
                finished_steps: steps.remove(&row.task_ref).unwrap_or_default(),
                changed_at: row.changed_at.as_deref().and_then(instant),
                on_board: row.on_board,
                task_ref: row.task_ref,
            })
        })
        .collect();
    // The ones still going first; the rows come newest change first and the sort keeps that.
    tasks.sort_by_key(|task| task.state.over());
    Ok(Json(TeamActivityEntry {
        team,
        harness,
        supported: true,
        reason: None,
        runner: Some(runner),
        reported_at,
        tasks,
    }))
}
