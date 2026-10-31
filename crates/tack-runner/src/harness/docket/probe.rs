//! What a running docket reports about itself: its API version, its teams and
//! what keeps each from running lights-out. Everything is read over HTTP;
//! nothing here starts docket or spends a token.

use std::collections::BTreeSet;

use chrono::Utc;
use tack_protocol::execution::{
    DocsLinks, HarnessCapability, HarnessKind as DomainHarnessKind, ModelListSource, ModelsReport,
    SignInReport, SignInState, TeamDecides, TeamMember, TeamNetwork, TeamQueue, TeamRecordSet,
    TeamReport, TeamStep, TeamStepCheck, TeamStepKind, TeamViolation, WorksIn,
};

use super::{
    client::Client,
    wire::{PodList, PodReport, PodRow, PodStep, RecordList, StatusAnswer},
};

/// The oldest operator API version this harness speaks; a later one is accepted too. It is the
/// first whose docket keeps the escalation record and whose task body carries `prepare`.
pub const MIN_API_VERSION: u32 = 6;

/// The profile a team must hold to run without a person.
const LIGHTS_OUT: &str = "lights-out";

fn needs_beta_6(found: &str) -> String {
    format!("needs docket 0.2.0-beta.6 (API {MIN_API_VERSION}); this docket answers API {found}")
}

fn without_escalations(found: u32) -> String {
    format!(
        "needs docket 0.2.0-beta.6 with the escalation record; this docket answers API {found} without it"
    )
}

/// What `GET /status.json` says.
pub(super) struct Status {
    /// The model ids its team members run on.
    pub models: Vec<String>,
    /// docket's own version, when it sends one.
    pub version: Option<String>,
    /// The operator API version it answered.
    pub api: u32,
}

/// `GET /status.json`, or the sentence saying why this docket cannot be used.
pub(super) async fn status(client: &Client) -> Result<Status, String> {
    let reply = client
        .status()
        .await
        .map_err(|why| format!("docket is not answering at {}: {why}", client.base()))?;
    if reply.status != 200 {
        return Err(format!(
            "docket is not answering at {}: {}",
            client.base(),
            reply.error()
        ));
    }
    let answer: StatusAnswer = reply
        .read()
        .map_err(|why| format!("docket's status could not be read: {why}"))?;
    let api = match answer.api_version.parse::<u32>() {
        Ok(found) if found >= MIN_API_VERSION => found,
        _ => return Err(needs_beta_6(&answer.api_version)),
    };
    let models: BTreeSet<String> = answer
        .agents
        .into_iter()
        .filter_map(|agent| agent.model)
        .filter(|model| !model.is_empty())
        .collect();
    Ok(Status {
        models: models.into_iter().collect(),
        version: answer.version.filter(|version| !version.is_empty()),
        api,
    })
}

/// Whether docket keeps the escalation record, which every question and approval is read from.
/// `apiVersion` is the same with and without it, so the route decides: `GET /escalations` answers
/// `200` or this docket cannot be used.
pub(super) async fn escalation_record(client: &Client, api: u32) -> Result<(), String> {
    let reply = client
        .escalations(0, None)
        .await
        .map_err(|why| format!("docket is not answering at {}: {why}", client.base()))?;
    if reply.status == 200 {
        Ok(())
    } else {
        Err(without_escalations(api))
    }
}

/// The record sets a team declares, with how many rows each holds. `None` when docket has no such
/// route (an older one answers `404`) or its answer cannot be read: that is not "no sets".
async fn record_sets(client: &Client, name: &str) -> Option<Vec<TeamRecordSet>> {
    let reply = client
        .records(name)
        .await
        .ok()
        .filter(|r| r.status == 200)?;
    let list: RecordList = reply
        .read()
        .inspect_err(|why| tracing::warn!(why, "docket's record sets could not be read"))
        .ok()?;
    Some(
        list.records
            .into_iter()
            .map(|set| TeamRecordSet {
                name: set.name,
                description: set.description.unwrap_or_default(),
                rows: set.rows,
            })
            .collect(),
    )
}

/// Where the team works: the explicit key when docket sends one, else what `repoConfigDir` says.
/// Neither is "did not say", which is not the same as a repository.
fn works_in(row: &PodRow, report: &PodReport) -> Option<WorksIn> {
    match report
        .workspace_kind
        .as_deref()
        .or(row.workspace_kind.as_deref())
    {
        Some("workdir") => return Some(WorksIn::Folder),
        Some("codebase") => return Some(WorksIn::Repository),
        _ => {}
    }
    report.repo_config_dir.as_deref().map(|dir| {
        if dir.is_empty() {
            WorksIn::Folder
        } else {
            WorksIn::Repository
        }
    })
}

/// A kind of step docket names, in Tack's words; `None` for a kind Tack has no word for.
fn step_kind(kind: &str) -> Option<TeamStepKind> {
    Some(match kind {
        "role" | "agent" => TeamStepKind::Agent,
        "run" | "command" => TeamStepKind::Command,
        "input" | "ask" => TeamStepKind::Ask,
        "wait" => TeamStepKind::Wait,
        "parallel" => TeamStepKind::Parallel,
        _ => return None,
    })
}

/// The steps of a pod's pipeline. A step Tack cannot read or has no word for makes the whole list
/// absent: a pipeline with a step missing would read as a different one.
fn steps(raw: Vec<serde_json::Value>) -> Option<Vec<TeamStep>> {
    raw.into_iter()
        .map(|value| {
            let step: PodStep = serde_json::from_value(value).ok()?;
            Some(TeamStep {
                kind: step_kind(&step.kind)?,
                name: step.id,
                role: step.role,
                checked_by: step.gate.as_deref().and_then(|gate| match gate {
                    "mechanical" => Some(TeamStepCheck::Command),
                    "verdict" => Some(TeamStepCheck::Reviewer),
                    "approval" => Some(TeamStepCheck::Person),
                    "external" => Some(TeamStepCheck::External),
                    _ => None,
                }),
                // The first route is the one shown; docket lists the verdict's own first.
                sends_back_to: step.rework.first().map(|rework| rework.to.clone()),
                sends_back_max: step.rework.first().and_then(|rework| rework.max),
            })
        })
        .collect::<Option<Vec<_>>>()
        .or_else(|| {
            tracing::warn!("docket's pipeline steps could not be read");
            None
        })
}

/// One team as docket reports it: its profile with the violations, the folders its sandbox reads,
/// where it works, who is in it, how it decides, what it holds and what it keeps.
async fn team(client: &Client, row: &PodRow) -> Result<TeamReport, String> {
    let name = &row.name;
    let reply = client.pod(name).await?;
    if reply.status != 200 {
        return Err(reply.error());
    }
    let report: PodReport = reply.read()?;
    let works_in = works_in(row, &report);
    let mut team = TeamReport {
        name: name.clone(),
        profile: report.profile.name,
        violations: report
            .profile
            .violations
            .into_iter()
            .map(|violation| TeamViolation {
                requirement: violation.requirement,
                fix: violation.fix,
            })
            .collect(),
        // An older docket sends no key; that stays unknown rather than becoming "none".
        read_only: report.read_only,
        works_in,
        records: record_sets(client, name).await,
        members: report.members.map(|members| {
            members
                .into_iter()
                .map(|member| TeamMember {
                    role: member.role,
                    model: member.model.and_then(|model| model.value),
                })
                .collect()
        }),
        decides: None,
        network: report
            .network
            .and_then(|network| match network.mode.as_str() {
                "none" => Some(TeamNetwork::Closed),
                "limited" => Some(TeamNetwork::Limited),
                "open" => Some(TeamNetwork::Open),
                _ => None,
            }),
        queue: row.queue.as_ref().map(|queue| TeamQueue {
            waiting: queue.pending + queue.waiting_approval + queue.waiting_input,
            running: queue.running,
        }),
        shape: report.blueprint.filter(|shape| !shape.is_empty()),
        steps: report
            .pipeline
            .and_then(|pipeline| pipeline.steps)
            .and_then(steps),
    };
    team.decides = Some(if capped(&team) {
        TeamDecides::AsksAPerson
    } else {
        TeamDecides::ToTheEnd
    });
    Ok(team)
}

/// The teams docket reports, each with its profile's violations and the folders its sandbox reads.
pub(super) async fn teams(client: &Client) -> Result<Vec<TeamReport>, String> {
    let reply = client.pods().await?;
    if reply.status != 200 {
        return Err(reply.error());
    }
    let list: PodList = reply.read()?;
    let mut teams = Vec::new();
    for pod in &list.pods {
        teams.push(team(client, pod).await?);
    }
    Ok(teams)
}

/// A team that needs a person during a run: attended, or with a violation.
fn capped(team: &TeamReport) -> bool {
    team.profile != LIGHTS_OUT || !team.violations.is_empty()
}

pub(super) async fn probe(
    client: &Client,
    decisions: tack_protocol::execution::CapabilityValue,
) -> HarnessCapability {
    let mut capability = HarnessCapability {
        harness_kind: DomainHarnessKind::new(super::DESCRIPTOR.kind),
        installed_version: String::new(),
        probe_error: None,
        probed_at: Utc::now(),
        decisions: Some(decisions),
        requires_model: false,
        requires_model_reason: None,
        models: None,
        default_model: None,
        sign_in: None,
        docs: Some(DocsLinks {
            models: super::DESCRIPTOR.models_docs.to_owned(),
            sign_in: super::DESCRIPTOR.sign_in_docs.to_owned(),
        }),
        autonomy_ceiling: None,
        teams: None,
        additional: [
            ("sandbox".to_owned(), serde_json::json!("harness")),
            (
                "sandbox_reason".to_owned(),
                serde_json::json!("docket confines its own agents in a jail it sets up"),
            ),
        ]
        .into(),
    };
    let Status {
        models,
        version,
        api,
    } = match status(client).await {
        Ok(answer) => answer,
        Err(why) => {
            // An answer from the wrong API version still says which one.
            if why.starts_with("needs docket")
                && let Some(found) = why.rsplit("answers API ").next()
            {
                capability.set_api_version(found);
            }
            capability.probe_error = Some(why);
            return capability;
        }
    };
    capability.installed_version = version.unwrap_or_default();
    capability.set_api_version(&api.to_string());
    if let Err(why) = escalation_record(client, api).await {
        capability.probe_error = Some(why);
        return capability;
    }
    let signed_in = !models.is_empty();
    capability.sign_in = Some(SignInReport {
        state: if signed_in {
            SignInState::SignedIn
        } else {
            SignInState::Unknown
        },
        detail: if signed_in {
            format!("docket's team members run on {}", models.join(", "))
        } else {
            "docket reports no model for its team members".to_owned()
        },
        command: None,
    });
    capability.models = Some(ModelsReport {
        source: if signed_in {
            ModelListSource::Listed
        } else {
            ModelListSource::None
        },
        reason: (!signed_in).then(|| "docket reports no model for its team members".to_owned()),
        ids: models,
    });
    match teams(client).await {
        Ok(teams) => {
            if !teams.is_empty() {
                capability.autonomy_ceiling =
                    Some(if teams.iter().any(capped) { "L3" } else { "L5" }.to_owned());
            }
            capability.teams = Some(teams);
        }
        Err(why) => {
            capability.probe_error = Some(format!("docket's teams could not be read: {why}"))
        }
    }
    capability
}
