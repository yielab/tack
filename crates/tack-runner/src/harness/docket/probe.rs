//! What a running docket reports about itself: its API version, its teams and
//! what keeps each from running lights-out. Everything is read over HTTP;
//! nothing here starts docket or spends a token.

use std::collections::BTreeSet;

use chrono::Utc;
use tack_protocol::execution::{
    DocsLinks, HarnessCapability, HarnessKind as DomainHarnessKind, MemoryDecision, MemoryEntry,
    MemoryEvidence, ModelListSource, ModelsReport, SignInReport, SignInState, TeamBudget,
    TeamDecides, TeamDelivers, TeamMember, TeamMemory, TeamMethod, TeamNeed, TeamNetwork,
    TeamPause, TeamQueue, TeamRecordSet, TeamReport, TeamStep, TeamStepCheck, TeamStepKind,
    TeamViolation, TeamWebMode, TeamWebRead, WorksIn,
};

use super::{
    client::Client,
    wire::{
        MemberReach, MemoryItem, MemoryList, PodBudget, PodList, PodMember, PodMethod, PodNeed,
        PodNetwork, PodReport, PodRow, PodStep, PodWebRead, RecordList, StatusAnswer,
    },
};

/// The oldest operator API version this harness speaks; a later one is accepted too. It is the
/// first whose docket keeps the escalation record and whose task body carries `prepare`.
pub const MIN_API_VERSION: u32 = 6;

/// The profile a team must hold to run without a person.
const LIGHTS_OUT: &str = "lights-out";

fn needs_beta_6(found: &str) -> String {
    format!("needs docket 0.3.0-beta.1 (API {MIN_API_VERSION}); this docket answers API {found}")
}

fn without_escalations(found: u32) -> String {
    format!(
        "needs docket 0.3.0-beta.1 with the escalation record; this docket answers API {found} without it"
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

/// The record sets a team declares, with how many rows each holds, and whether the proposals are
/// shared across its tasks. `None` when docket has no such route (an older one answers `404`) or
/// its answer cannot be read: that is not "no sets".
async fn record_sets(client: &Client, name: &str) -> Option<(Vec<TeamRecordSet>, Option<bool>)> {
    let reply = client
        .records(name)
        .await
        .ok()
        .filter(|r| r.status == 200)?;
    let list: RecordList = reply
        .read()
        .inspect_err(|why| tracing::warn!(why, "docket's record sets could not be read"))
        .ok()?;
    // Said only when the listing carries the proposals' `scope`: a set kept per team is shared
    // across tasks, one kept per task is not. An older docket sends no scope, and says nothing.
    let shared = list
        .records
        .iter()
        .find(|set| set.name == super::records::PROPOSALS)
        .and_then(|set| match set.scope.as_deref() {
            Some("pod") => Some(true),
            Some("task") => Some(false),
            _ => None,
        });
    Some((
        list.records
            .into_iter()
            .map(|set| TeamRecordSet {
                name: set.name,
                description: set.description.unwrap_or_default(),
                rows: set.rows,
            })
            .collect(),
        shared,
    ))
}

/// What a member's role reaches, in Tack's words: whether it can propose items (its role writes
/// the proposals' record set), whether it can change the project (a tool of the `write` or
/// `exec` kind) and whether it reads the web (it holds `fetch`, which runs outside the jail). A
/// member whose `capabilities` is absent or cannot be read says none of them.
pub(super) fn member(member: PodMember) -> TeamMember {
    let reach: Option<MemberReach> = member
        .capabilities
        .and_then(|raw| serde_json::from_value(raw).ok());
    TeamMember {
        role: member.role,
        model: member.model.and_then(|model| model.value),
        can_propose: reach
            .as_ref()
            .and_then(|reach| reach.records.as_ref()?.write.as_ref())
            .map(|sets| sets.iter().any(|set| set == super::records::PROPOSALS)),
        writes_project: reach
            .as_ref()
            .and_then(|reach| reach.kinds.as_ref())
            .map(|kinds| kinds.iter().any(|kind| kind == "write" || kind == "exec")),
        reads_web: reach
            .as_ref()
            .and_then(|reach| reach.tools.as_ref())
            .map(|tools| tools.iter().any(|tool| tool == "fetch")),
        web_hosts: reach
            .and_then(|reach| reach.fetch_domains)
            .filter(|hosts| !hosts.is_empty()),
    }
}

/// How the team reads the web, in docket's own sentence (`webRead`). Absent from a docket that does
/// not list the feature, and when the answer cannot be read: the team's report never fails for it.
pub(super) fn web_read(report: &mut PodReport, listed: bool) -> Option<TeamWebRead> {
    let raw = report.web_read.take().filter(|_| listed)?;
    let read: PodWebRead = serde_json::from_value(raw)
        .inspect_err(|why| tracing::warn!(%why, "docket's webRead could not be read"))
        .ok()?;
    let mode = match read.mode.as_str() {
        "open" => TeamWebMode::Open,
        "listed" => TeamWebMode::Listed,
        "none" => TeamWebMode::Closed,
        _ => return None,
    };
    Some(TeamWebRead {
        mode,
        reason: read.reason,
        fix: read.fix,
    })
}

/// How far the team reaches, in Tack's words. A `none` that docket says no jail enforces is open,
/// as docket itself words it; a mode Tack has no word for stays unsaid.
pub(super) fn network(network: Option<&PodNetwork>) -> Option<TeamNetwork> {
    let network = network?;
    match network.mode.as_str() {
        "none" if network.enforced == Some(false) => Some(TeamNetwork::Open),
        "none" => Some(TeamNetwork::Closed),
        "limited" => Some(TeamNetwork::Limited),
        "open" => Some(TeamNetwork::Open),
        _ => None,
    }
}

/// Why the team is paused, when it is: the report's word, else the row's. A reason Tack has no
/// word for is `Other`, and a pause with no reason at all is too.
fn paused(row: &PodRow, report: &PodReport) -> Option<TeamPause> {
    let paused = report.paused.or(row.paused)?;
    if !paused {
        return None;
    }
    let reason = report
        .paused_reason
        .as_deref()
        .or(row.paused_reason.as_deref());
    Some(match reason {
        Some("budget") => TeamPause::Budget,
        _ => TeamPause::Other,
    })
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

/// What a task on the team hands back, in Tack's words. docket sends a list whose first entry is
/// `branch` or `report`; a `records` after it says the team keeps record sets, which the team's
/// `records` already carries. `None` for a list that does not start with a word Tack has.
fn delivers(raw: &serde_json::Value) -> Option<TeamDelivers> {
    match raw.as_array()?.first()?.as_str()? {
        "branch" => Some(TeamDelivers::Branch),
        "report" => Some(TeamDelivers::Report),
        _ => None,
    }
}

/// Every entry of a list docket sent, each read on its own; one that cannot be read makes the
/// whole list absent, since a list with an entry missing would read as a different one.
fn all<T: serde::de::DeserializeOwned>(raw: Vec<serde_json::Value>, what: &str) -> Option<Vec<T>> {
    raw.into_iter()
        .map(|value| serde_json::from_value(value).ok())
        .collect::<Option<Vec<_>>>()
        .or_else(|| {
            tracing::warn!("docket's {what} could not be read");
            None
        })
}

/// What a team's report says of its contract (`podContract`): what it delivers, the methods a task
/// may name and what its work still needs. All three are absent from a report of a docket that
/// does not list the feature, whatever the report holds: the keys are read only when announced.
#[derive(Default)]
pub(super) struct Contract {
    pub delivers: Option<TeamDelivers>,
    pub methods: Option<Vec<TeamMethod>>,
    pub needs: Option<Vec<TeamNeed>>,
}

pub(super) fn contract(report: &mut PodReport, listed: bool) -> Contract {
    if !listed {
        return Contract::default();
    }
    Contract {
        delivers: report.delivers.as_ref().and_then(delivers),
        methods: report
            .methods
            .take()
            .and_then(|raw| all::<PodMethod>(raw, "methods"))
            .map(|methods| {
                methods
                    .into_iter()
                    .map(|method| TeamMethod {
                        name: method.name,
                        source: method.source,
                        default: method.default,
                        missing_roles: method.missing_roles,
                        error: method.error,
                    })
                    .collect()
            }),
        needs: report
            .needs
            .take()
            .and_then(|raw| all::<PodNeed>(raw, "needs"))
            .map(|needs| {
                needs
                    .into_iter()
                    .map(|need| TeamNeed {
                        kind: need.kind,
                        name: need.name,
                        fix: need.fix,
                        satisfied: need.satisfied,
                        blocking: need.blocking,
                    })
                    .collect()
            }),
    }
}

/// What a team may spend and has spent (`podResume`). Absent from a report of a docket that does
/// not list the feature, whatever the report holds, and when the key cannot be read.
fn budget(report: &mut PodReport, listed: bool) -> Option<TeamBudget> {
    if !listed {
        return None;
    }
    let raw = report.budget.take()?;
    match serde_json::from_value::<PodBudget>(raw) {
        Ok(budget) => Some(TeamBudget {
            tokens: budget.tokens,
            used: budget.used,
            window: budget.window,
            window_start: budget.window_start,
        }),
        Err(why) => {
            tracing::warn!(%why, "docket's budget could not be read");
            None
        }
    }
}

/// The entries of a team's memory in one status, oldest first. `None` when docket cannot say: the
/// route fails, answers something else or holds an entry Tack cannot read. A list with an entry
/// missing would read as a different one, and a pending entry is what a person is waiting to be
/// told of.
async fn memory_entries(client: &Client, name: &str, status: &str) -> Option<Vec<MemoryEntry>> {
    let reply = client
        .pod_memory(name, status)
        .await
        .inspect_err(|why| tracing::warn!(why, team = name, "docket's memory could not be read"))
        .ok()?;
    if reply.status != 200 {
        tracing::warn!(
            team = name,
            status = reply.status,
            "docket's memory could not be read"
        );
        return None;
    }
    let list: MemoryList = reply
        .read()
        .inspect_err(|why| tracing::warn!(why, team = name, "docket's memory could not be read"))
        .ok()?;
    let items = all::<MemoryItem>(list.entries, "memory entries")?;
    Some(
        items
            .into_iter()
            .map(|item| MemoryEntry {
                id: item.id,
                kind: item.kind,
                subject: item.subject,
                note: item.note,
                reason: item.reason,
                stale: item.stale,
                provenance: item.provenance,
                evidence: item
                    .evidence
                    .filter(|proof| proof.exit.is_some() || proof.ok.is_some())
                    .map(|proof| MemoryEvidence {
                        exit: proof.exit,
                        ok: proof.ok,
                    }),
            })
            .collect(),
    )
}

/// What a team has learnt and what waits for a person (`memoryStatus`). Absent from a docket that
/// does not list the feature, and when either list cannot be read: the team's report never fails
/// for its memory.
async fn memory(client: &Client, name: &str, listed: Listed) -> Option<TeamMemory> {
    if !listed.memory {
        return None;
    }
    Some(TeamMemory {
        pending: memory_entries(client, name, "pending").await?,
        accepted: memory_entries(client, name, "accepted").await?,
        can_decide: listed.memory_decide,
    })
}

/// What `GET /health` lists of the features a team's report depends on.
#[derive(Clone, Copy)]
struct Listed {
    pause: bool,
    contract: bool,
    resume: bool,
    memory: bool,
    memory_decide: bool,
    web_read: bool,
}

impl Listed {
    async fn of(client: &Client) -> Listed {
        Listed {
            pause: client.has_feature(super::POD_PAUSE).await,
            contract: client.has_feature(super::POD_CONTRACT).await,
            resume: client.has_feature(super::POD_RESUME).await,
            memory: client.has_feature(super::MEMORY_STATUS).await,
            memory_decide: client.has_feature(super::MEMORY_DECIDE).await,
            web_read: client.has_feature(super::WEB_READ).await,
        }
    }
}

/// One team as docket reports it: its profile with the violations, the folders its sandbox reads,
/// where it works, who is in it, how it decides, what it holds and what it keeps.
async fn team(client: &Client, row: &PodRow, listed: Listed) -> Result<TeamReport, String> {
    let name = &row.name;
    let reply = client.pod(name).await?;
    if reply.status != 200 {
        return Err(reply.error());
    }
    let mut report: PodReport = reply.read()?;
    let contract = contract(&mut report, listed.contract);
    let budget = budget(&mut report, listed.resume);
    let web_read = web_read(&mut report, listed.web_read);
    let memory = memory(client, name, listed).await;
    let works_in = works_in(row, &report);
    let (records, proposals_shared_across_tasks) = match record_sets(client, name).await {
        Some((sets, shared)) => (Some(sets), shared),
        None => (None, None),
    };
    // Said only by a docket that lists `podPause`, and only while the team is paused.
    let paused = listed.pause.then(|| paused(row, &report)).flatten();
    let mut team = TeamReport {
        name: name.clone(),
        profile: Some(report.profile.name),
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
        records,
        members: report
            .members
            .map(|members| members.into_iter().map(member).collect()),
        decides: None,
        network: network(report.network.as_ref()),
        queue: row.queue.as_ref().map(|queue| TeamQueue {
            waiting: queue.pending + queue.waiting_approval + queue.waiting_input,
            running: queue.running,
        }),
        shape: report.blueprint.filter(|shape| !shape.is_empty()),
        steps: report
            .pipeline
            .and_then(|pipeline| pipeline.steps)
            .and_then(steps),
        proposals_shared_across_tasks,
        paused,
        folder: report.work_dir.filter(|dir| !dir.is_empty()),
        delivers: contract.delivers,
        methods: contract.methods,
        needs: contract.needs,
        budget,
        memory,
        web_read,
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
    let listed = Listed::of(client).await;
    let mut teams = Vec::new();
    for pod in &list.pods {
        teams.push(team(client, pod, listed).await?);
    }
    Ok(teams)
}

/// Lets a paused team claim tasks again (`POST /pods/<pod>/resume`) and reads its report
/// afterwards, or says in a person's words why docket did not. A docket that does not list
/// `podResume` is not asked: it has no such route, and the sentence says where the operator
/// resumes the team instead.
pub(super) async fn resume(client: &Client, name: &str) -> Result<Option<TeamReport>, String> {
    if !client.has_feature(super::POD_RESUME).await {
        return Err(format!(
            "This docket cannot resume a team from the board. Run `docket run --pod {name} --resume` on the runner's computer."
        ));
    }
    let reply = client.resume_pod(name).await?;
    match reply.status {
        200 | 204 => {}
        404 => return Err(format!("docket has no team named {name}")),
        _ => return Err(reply.error()),
    }
    report_of(
        client,
        name,
        Listed {
            resume: true,
            ..Listed::of(client).await
        },
    )
    .await
}

/// The team's report as docket gives it now, or `None` when the list no longer holds the team.
async fn report_of(
    client: &Client,
    name: &str,
    listed: Listed,
) -> Result<Option<TeamReport>, String> {
    let reply = client.pods().await?;
    if reply.status != 200 {
        return Err(reply.error());
    }
    let list: PodList = reply.read()?;
    let Some(row) = list.pods.iter().find(|pod| pod.name == name) else {
        return Ok(None);
    };
    team(client, row, listed).await.map(Some)
}

/// Approves or refuses one pending memory entry of a team (`POST /pods/<pod>/memory/<id>`) and
/// reads the team's report afterwards, or says in a person's words why docket did not. A docket
/// that does not list `memoryDecide` is not asked: it has no such route, and the sentence says
/// which command decides the entry instead.
pub(super) async fn decide_memory(
    client: &Client,
    name: &str,
    entry: &str,
    decision: MemoryDecision,
    reason: Option<&str>,
) -> Result<Option<TeamReport>, String> {
    let (word, command) = match decision {
        MemoryDecision::Approve => (
            "approve",
            format!("docket pod memory approve {entry} --pod {name}"),
        ),
        MemoryDecision::Refuse => (
            "refuse",
            format!("docket pod memory refuse {entry} --pod {name} --reason \"<why>\""),
        ),
    };
    if !client.has_feature(super::MEMORY_DECIDE).await {
        return Err(format!(
            "This docket cannot decide a memory entry from the board. Run `{command}` on the runner's computer."
        ));
    }
    let reason = reason.map(str::trim).filter(|reason| !reason.is_empty());
    if decision == MemoryDecision::Refuse && reason.is_none() {
        return Err("A refusal needs a reason.".to_owned());
    }
    let reply = client.decide_memory(name, entry, word, reason).await?;
    match reply.status {
        200 => {}
        404 => {
            return Err(format!(
                "docket has no team named {name}, or the team has no entry {entry}"
            ));
        }
        409 => return Err("That entry is no longer waiting.".to_owned()),
        403 => {
            return Err(
                "docket refused: the token the runner holds may only read. Give it the full token."
                    .to_owned(),
            );
        }
        _ => return Err(reply.error()),
    }
    report_of(client, name, Listed::of(client).await).await
}

/// A team that needs a person during a run: attended, or with a violation.
fn capped(team: &TeamReport) -> bool {
    team.profile.as_deref() != Some(LIGHTS_OUT) || !team.violations.is_empty()
}

pub(super) async fn probe(
    client: &Client,
    features: tack_protocol::execution::FeatureCapabilities,
) -> HarnessCapability {
    let mut capability = HarnessCapability {
        harness_kind: DomainHarnessKind::new(super::DESCRIPTOR.kind),
        installed_version: String::new(),
        probe_error: None,
        probed_at: Utc::now(),
        decisions: Some(features.decisions.clone()),
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
    capability.set_features(&features);
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
    // The board offers to make a team only where docket can.
    let mut listed = Vec::new();
    if client.has_feature(super::apply::POD_SHAPE).await {
        listed.push(super::apply::POD_SHAPE);
    }
    capability.set_api_features(&listed);
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
