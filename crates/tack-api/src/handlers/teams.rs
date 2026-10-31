//! The teams the board's agents report, with the profiles that name them (ADR 0032 decision 1).

use std::collections::BTreeMap;

use axum::extract::State;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::instrument;
use utoipa::ToSchema;

use tack_db::queries;
use tack_protocol::execution::TeamReport;

use crate::error::ApiResult;
use crate::extract::Json;
use crate::router::AppState;

/// A profile that names a team.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TeamProfile {
    pub id: String,
    pub name: String,
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

/// A profile's team and agent, as its tool policy stores them.
fn named_team(tool_policy: &str) -> Option<(String, Option<String>)> {
    let policy: Value = serde_json::from_str(tool_policy).ok()?;
    let team = policy.get("docket")?.get("team")?.as_str()?;
    let agent = policy.get("agent").and_then(Value::as_str);
    (!team.is_empty()).then(|| (team.to_owned(), agent.map(str::to_owned)))
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
                let key = (kind.clone(), team.name.clone());
                if reported
                    .get(&key)
                    .and_then(|seen| seen.reported_at)
                    .is_some_and(|seen| seen > harness.probed_at)
                {
                    continue;
                }
                reported.insert(
                    key,
                    TeamEntry {
                        name: team.name.clone(),
                        harness: Some(kind.clone()),
                        reported: true,
                        runner: Some(runner.name.clone()),
                        reported_at: Some(harness.probed_at),
                        report: Some(team),
                        profiles: Vec::new(),
                    },
                );
            }
        }
    }
    // A profile names a team under its agent, or under every agent that reports the name when it
    // names none; a name nobody reports stays a team of its own, marked as not reported.
    let mut unreported: BTreeMap<(Option<String>, String), TeamEntry> = BTreeMap::new();
    for profile in queries::agent_profiles::profiles_by_name(&state.repo).await? {
        let Some((team, agent)) = named_team(&profile.tool_policy) else {
            continue;
        };
        let line = TeamProfile {
            id: profile.id,
            name: profile.name,
        };
        let mut found = false;
        for ((kind, name), entry) in &mut reported {
            if *name == team && agent.as_ref().is_none_or(|agent| agent == kind) {
                entry.profiles.push(line.clone());
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
