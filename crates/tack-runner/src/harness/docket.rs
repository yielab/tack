//! docket as a harness: a daemon the runner talks to over HTTP, not a process
//! it spawns. [`DocketAdapter`] implements [`HarnessAdapter`] and
//! [`HarnessProbe`] directly against docket's operator API (version 6, with
//! the escalation record): it posts the attempt as a task to the team the
//! profile names, follows the task's hops on the trace, carries docket's
//! escalations (every approval and question) to the person, and names the
//! finished branch's commit in the user's repository, where the evidence, the
//! verifier and the push read it. The runner makes no copy of the code for a
//! docket run.
//!
//! What the API and the fake that stands in for it are built on is in
//! `fixtures/docket/v5/README.md`.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use chrono::{DateTime, Local, Utc};
use serde_json::{Value, json};
use tack_protocol::conversation::{ConverseMode, REPLY_REASON_KEY, TurnNote};
use tack_protocol::event_kind;
use tack_protocol::execution::{
    ActualExecution, CapabilitySupport, CapabilityValue, FeatureCapabilities, HarnessCapability,
    HarnessKind as DomainHarnessKind, Measurement, MeasurementSource, OperationOutcome, ParityCell,
    ParityRule, TeamActivity, TeamFacts, TeamMember, TeamMisfit, TeamNeeds, TeamReport, Usage,
    WorkspaceId as DomainWorkspaceId, team_misfits,
};
use tokio::sync::{Mutex, mpsc};

use crate::{
    client::{AttemptState, ClaimedWork, Recommendation, Timestamp},
    config::DocketConfig,
    harness::{
        AnsweredElsewhere, AttemptJournal, CancelObservation, CancellationEvidence, DecisionAnswer,
        DecisionOption, ExecutionSpec, HarnessAdapter, HarnessError, HarnessOutcome, HarnessProbe,
        LocalRunHandle, ModelSource, Progress, Question, RecoveryObservation,
        local_process::{HarnessDescriptor, ModelSelection, capability, policy_capability},
        redact::SecretMaterial,
    },
};

mod activity;
mod apply;
mod client;
pub mod daemon;
mod probe;
mod records;
mod steps;
mod task;
mod wire;

use client::Client;
use wire::{
    BudgetOutcome, Enqueued, Escalation, EscalationDetail, EscalationEvent, EscalationList,
    EscalationState, HopEvent, PodReport, TaskDiff, TaskUsage, TaskView, Tokens, TraceEvent,
    TracePage,
};

pub static DESCRIPTOR: HarnessDescriptor = HarnessDescriptor {
    kind: "docket",
    program: "docket",
    // A request naming no model runs on the team's own configuration.
    model_selection: ModelSelection::Optional,
    inherited_env: &[],
    probe_notes: &[],
    credential_note: "docket holds its own model credentials in its home; Tack holds none and \
                      reaches docket with the daemon's bearer token.",
    reports_process_groups: false,
    cancels_over_api: true,
    reports_progress: true,
    sandbox_binds: &[],
    parity: &[
        (
            ParityRule::SandboxOrNoRun,
            ParityCell::missing(
                "GR8: docket's isolation is assumed, not read from what it reports",
            ),
        ),
        (
            ParityRule::NoSandboxMeansAsk,
            ParityCell::missing(
                "GR8: docket's sandbox posture is not read, so the dialog cannot default to ask",
            ),
        ),
        (
            ParityRule::EgressByAllowList,
            ParityCell::delegated("docket"),
        ),
        (
            ParityRule::UntrustedItemAsks,
            ParityCell::missing("GR12: the item's trust is not passed to docket"),
        ),
        (ParityRule::AgentConfigIsolated, ParityCell::Runner),
        (
            ParityRule::TokenLimitWhileRunning,
            ParityCell::delegated("docket"),
        ),
        (
            ParityRule::KillEverythingOnCancel,
            ParityCell::delegated("docket"),
        ),
        (
            ParityRule::OutputRules,
            ParityCell::missing(
                "GR17: the Planner and Researcher rules and the no-change rule are not applied to docket's diff",
            ),
        ),
        (
            ParityRule::ChecksChangedHoldsMerge,
            ParityCell::missing("GR9: the merge does not yet hold when the checks changed"),
        ),
        (ParityRule::Redaction, ParityCell::Runner),
        (ParityRule::StepsVisible, ParityCell::delegated("docket")),
    ],
    readiness: None,
    models_docs: "https://github.com/yielab/docket/blob/develop/docs/CONFIGURATION.md#31-choose-which-model-each-agent-runs-on",
    sign_in_docs: "https://github.com/yielab/docket/blob/develop/docs/MODEL-GATEWAYS.md",
};

/// The feature of `GET /health` under which a team's `paused` is sent.
const POD_PAUSE: &str = "podPause";

/// The feature of `GET /health` under which a team's report carries `delivers`, `methods` and
/// `needs`.
const POD_CONTRACT: &str = "podContract";

/// The feature of `GET /health` under which a team's report carries `webRead` (how the team reads
/// the web, and why) and each of its needs says whether it `blocking`.
const WEB_READ: &str = "webRead";

/// The feature of `GET /health` under which a paused team can be resumed over HTTP and its report
/// carries `budget`.
const POD_RESUME: &str = "podResume";

/// The feature of `GET /health` under which `GET /pods/<pod>/memory` takes a `status`, so a team's
/// pending and accepted entries can be read apart.
const MEMORY_STATUS: &str = "memoryStatus";

/// The feature of `GET /health` under which `POST /pods/<pod>/memory/<id>` approves or refuses a
/// pending memory entry.
const MEMORY_DECIDE: &str = "memoryDecide";

/// The feature of `GET /health` under which a pod declares the hosts its `fetch` reads.
const POD_FETCH: &str = "podFetch";

/// The feature of `GET /health` under which a task can continue turn by turn: it is sent with
/// `conversation: {maxTurns}` and each turn ends on an escalation of kind `turn`.
const CONVERSATION: &str = "conversation";

/// The most turns docket takes for one conversation task (`MAX_CONVERSATION_TURNS`).
const MAX_CONVERSATION_TURNS: u32 = 20;

/// The kind of the escalation a conversation task waits on between two turns.
const TURN_KIND: &str = "turn";

/// The `code` of the `400` docket answers to a conversation task whose method has several steps.
const CONVERSATION_NEEDS_ONE_STEP: &str = "conversation_needs_one_step";

/// How often a running task's trace, inbox and record are read.
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// How long after a task is seen ended its trace is still read for the hops it has not yet
/// shown. docket hands out an event only once its second has closed and a further second has
/// passed, so an event written before the task ended is readable within two seconds of it.
const TRACE_SETTLE: Duration = Duration::from_secs(3);

/// How often the trace is read again while it settles.
const TRACE_SETTLE_POLL: Duration = Duration::from_millis(250);

/// The task states in which docket waits for a person and the agent does no work.
fn waiting_on_a_person(status: &str) -> bool {
    matches!(status, "waiting_input" | "waiting_approval")
}

/// Progress lines the engine has not yet taken, before more are dropped.
const PROGRESS_BUFFER: usize = 256;

/// What a person reads when an attempt fails before docket ran anything.
const WRONG_WORKSPACE: &str =
    "the docket team works on a branch of your folder; choose that under Settings → Automation";

/// What a person reads when docket ended a task for needing a person and gave no sentence.
const NEEDS_A_PERSON: &str = "docket stopped the task because a step needed a person and the task ran at a level that asks nobody";

/// The task states in which docket will do nothing more by itself.
fn terminal(status: &str) -> bool {
    matches!(status, "done" | "failed" | "blocked" | "cancelled")
}

fn rejected(reason: String) -> HarnessError {
    tracing::warn!(
        reason,
        harness = DESCRIPTOR.kind,
        "request rejected before start"
    );
    HarnessError::Rejected { reason }
}

/// Whether a team works in a repository: `workspaceKind` when docket sends it, else what
/// `repoConfigDir` says. A team that could not be read is taken to have one, as every team did
/// before.
fn in_repository(report: Option<&PodReport>) -> bool {
    let Some(report) = report else {
        return true;
    };
    match report.workspace_kind.as_deref() {
        Some("workdir") => false,
        Some(_) => true,
        None => report
            .repo_config_dir
            .as_deref()
            .is_none_or(|dir| !dir.is_empty()),
    }
}

/// Whether the task's last hop failed and kept its work: docket pins the tree of an Implementer
/// stopped by its budget, its transport or a gap in the team's setup, and lists what it changed.
fn kept_work(task: &TaskView) -> bool {
    task.hops
        .last()
        .filter(|hop| hop.ok == Some(false))
        .and_then(|hop| hop.artifact.as_ref())
        .is_some_and(|artifact| !artifact.files_changed.is_empty())
}

/// The roles of the team that can propose items, once each, in the order docket lists them.
/// `None` when docket did not say of any member whether it can.
fn proposers(members: Option<&[TeamMember]>) -> Option<Vec<String>> {
    let members = members?;
    if members.iter().all(|member| member.can_propose.is_none()) {
        return None;
    }
    let mut roles: Vec<String> = Vec::new();
    for member in members
        .iter()
        .filter(|member| member.can_propose == Some(true))
    {
        if !roles.contains(&member.role) {
            roles.push(member.role.clone());
        }
    }
    Some(roles)
}

/// Why a run did not go to `pod`, one sentence per need the team cannot meet, each with the change
/// in docket that meets it. With `podFetch` a pod reads the web through its own `fetch` hosts, which
/// opens no more than reading; the whole network, which also opens `bash`, steps and MCP servers to
/// every host, comes second. `facts` is what the team said, for the sentences that quote it.
fn misfit_sentence(
    pod: &str,
    misfits: &[TeamMisfit],
    pod_fetch: bool,
    facts: &TeamFacts<'_>,
) -> String {
    let lines: Vec<String> = misfits
        .iter()
        .map(|misfit| match misfit {
            TeamMisfit::NoWeb => match facts.web_read {
                // docket's own sentence: why the team reads none and what changes that.
                Some(web) => format!(
                    "The team {pod} cannot reach the web and this run needs it: {} {}",
                    web.reason, web.fix
                )
                .trim_end()
                .to_owned(),
                None if pod_fetch => format!(
                    "The team {pod} cannot reach the web and this run needs it: docket reports its \
                     network as none and no member holds `fetch`. Give it the hosts it reads with \
                     `docket pod set fetchDomains <host> --pod {pod}` (`'*' --widen` for any host), \
                     open its whole network with `docket pod set network open --widen --pod {pod}`, \
                     or turn the network off in the profile."
                ),
                None => format!(
                    "The team {pod} cannot reach the web and this run needs it: docket reports its \
                     network as none and no member holds `fetch`. Name the hosts `fetch` reads in \
                     docket's FETCH_ALLOWED_DOMAINS (every team's), open the team's network with \
                     `docket pod set network open --widen --pod {pod}`, or turn the network off in \
                     the profile."
                ),
            },
            TeamMisfit::NoProposals => format!(
                "The team {pod} cannot propose items and a Planner delivers nothing else: no \
                 member can write the record set `tack_proposals`. Declare it in the team's \
                 `records/tack_proposals.yaml`, let a role write it and apply the team."
            ),
            TeamMisfit::NoRepository => format!(
                "The team {pod} delivers a report and this run changes the repository. Point the \
                 profile at a team that works in the repository, or give the run a Planner or \
                 Researcher profile."
            ),
            TeamMisfit::UnknownMethod { name } => format!(
                "The team {pod} has no method named {name}; it can run {}. Name one of those in \
                 the profile's `pipeline_by_type`, or add the recipe with \
                 `docket pod update {name} --pod {pod}`.",
                facts
                    .methods
                    .unwrap_or_default()
                    .iter()
                    .filter(|method| method.missing_roles.is_empty() && method.error.is_none())
                    .map(|method| method.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            TeamMisfit::StepsSkipped { method, roles } => format!(
                "The method {method} cannot run on the team {pod}: no member plays {}, and docket \
                 refuses a task that names it. Add the role to the team and apply it, or name \
                 another method.",
                roles.join(", ")
            ),
            TeamMisfit::MethodBroken { method, error } => format!(
                "The method {method} of the team {pod} does not load, and docket refuses a task \
                 that names it: {} Fix its file in the team's pipelines folder and check it \
                 with `docket pod validate`.",
                error.lines().next().unwrap_or_default()
            ),
            TeamMisfit::NeedsUnmet { name } => format!(
                "The team {pod} still needs {name}: {}",
                facts
                    .needs
                    .unwrap_or_default()
                    .iter()
                    .find(|need| need.name == *name)
                    .map_or("docket gave no fix", |need| need.fix.as_str())
            ),
        })
        .collect();
    format!("Not sent to docket. {}", lines.join(" "))
}

fn measurement<T>(value: Option<T>) -> Measurement<T> {
    Measurement {
        source: if value.is_some() {
            MeasurementSource::Measured
        } else {
            MeasurementSource::NotMeasured
        },
        value,
        additional: BTreeMap::new(),
    }
}

/// Why a turn is a new task: the way every team converses, and the way one whose docket does not
/// list `conversation` has to.
const REPLAYS: &str = "every turn is a new task given the conversation so far, and its reply is \
     the summary of the team's last step";

/// How a thread with a team continues when docket lists `conversation`.
const CONTINUES: &str = "a thread is one docket task that continues turn by turn: each turn \
     answers the task's `turn` escalation, and its reply is the summary of the team's last step; \
     a team whose method has several steps replays instead";

/// What docket offers over its API, as every docket does. `converse` is the way every team can,
/// by replaying; [`probe`](HarnessProbe::probe) raises it for a docket that lists `conversation`.
fn capabilities() -> FeatureCapabilities {
    FeatureCapabilities {
        cancel: capability(
            CapabilitySupport::Supported,
            "a cancellation is a call to docket's own cancel route, which stops the running hop \
             and its process group",
        ),
        resume: capability(
            CapabilitySupport::Supported,
            "a task still running when the runner restarts is taken back from the journal's \
             handle and followed to its end; the time it ran before is not counted against it",
        ),
        decisions: capability(
            CapabilitySupport::Supported,
            "a tool call docket's policy gates, and a question a step parks on, are escalations \
             docket records, which reach the person and are answered over docket's one \
             escalation route",
        ),
        artifacts: capability(
            CapabilitySupport::Supported,
            "the evidence is the diff, in the repository, from the base commit to the task's \
             branch, which is what docket committed",
        ),
        usage: capability(
            CapabilitySupport::Supported,
            "the task record carries the tokens used, in total and by model",
        ),
        proposals: Some(capability(
            CapabilitySupport::Supported,
            "through a team's `tack_proposals` record set",
        )),
        activity: Some(capability(
            CapabilitySupport::Supported,
            "the team's task list, with the step each task is in and the steps it finished, \
             including the work its CLI, a routine or another channel gave it",
        )),
        converse: Some(ConverseMode::Replays.capability(REPLAYS)),
        additional: policy_capability(
            CapabilitySupport::Supported,
            "the profile's denied tools and the network flag become the task's restrictions, \
             which docket applies on top of the team's own",
        ),
    }
}

/// The per-attempt facts `start` fixes and `wait` reads.
struct RunInfo {
    pod: String,
    task: String,
    attempt_id: String,
    workspace: PathBuf,
    started_at: DateTime<Utc>,
    timeout: Duration,
    /// docket refused the task with this sentence; there is nothing to follow.
    refused: Option<String>,
    /// The operator API version docket answered when the task started, when it could be asked.
    api: Option<u32>,
    /// The team works in a repository. False only when docket says the team has none; a docket
    /// that does not say is taken to have one, as every team did before.
    in_repository: bool,
    /// The run is one turn of a thread, which answers with the last step's summary.
    turn: bool,
    /// This run continues a task another run already started (decision 6, Phase 87): the tokens
    /// that task had used (`input`, `output`) when the retry was sent, read back out of the
    /// task's own cumulative `usage` so the attempt counts only its own new hops.
    baseline_usage: Option<(u64, u64)>,
    /// How this run's thread converses, when it differs from replaying: continuing the thread's
    /// task, or replaying because docket refused the task as a conversation.
    converse: Option<CapabilityValue>,
    /// The thread this run is a turn of, when it is one.
    thread: Option<String>,
    /// A line for the timeline before anything else: a turn that could not continue its task
    /// says it starts a new one.
    notice: Option<String>,
    /// What docket's text for this run is scrubbed of before it leaves the adapter, as a local
    /// agent's output is: the token docket is read with, the request's literal environment and
    /// the resolved prompt.
    secrets: SecretMaterial,
}

/// The ends of the channels `wait` drives.
struct WaitSide {
    questions: mpsc::Sender<Question>,
    answers: mpsc::Receiver<DecisionAnswer>,
    progress: mpsc::Sender<Progress>,
    elsewhere: mpsc::Sender<AnsweredElsewhere>,
}

struct Live {
    info: Arc<RunInfo>,
    decisions: Option<(mpsc::Receiver<Question>, mpsc::Sender<DecisionAnswer>)>,
    progress: Option<mpsc::Receiver<Progress>>,
    elsewhere: Option<mpsc::Receiver<AnsweredElsewhere>>,
    wait: Option<WaitSide>,
}

/// What a run that stopped for a question nobody answered says: the question and when it was
/// asked, and how long the team stood still; without the question, only that it expired.
fn unanswered_text(
    asked: Option<&(String, DateTime<Utc>, bool)>,
    unanswered_until: Option<DateTime<Utc>>,
) -> String {
    let Some((question, at, _)) = asked else {
        return super::not_answered(unanswered_until);
    };
    let (at, now) = (at.with_timezone(&Local), Utc::now().with_timezone(&Local));
    let seconds = (now - at).num_seconds().max(0);
    let (count, unit) = if seconds < 60 {
        (seconds, "second")
    } else {
        ((seconds + 30) / 60, "minute")
    };
    format!(
        "Nobody answered '{question}' (asked {}); the team stopped after {count} {unit}{}.",
        at.format(if at.date_naive() == now.date_naive() {
            "%H:%M"
        } else {
            "%Y-%m-%d %H:%M"
        }),
        if count == 1 { "" } else { "s" },
    )
}

/// What a step's own budget stop says: the bound that ran out, the figures, and where a person
/// raises it. Never called with `bound: "tokens"` (that stop keeps the run's token limit, which
/// is Tack's own, not a sentence built from docket's `budget`).
fn budget_message(budget: &BudgetOutcome, hop_error: &str) -> String {
    format!(
        "The step ran out of its {}: {} of {}. {}",
        budget_bound_phrase(&budget.bound),
        budget.used,
        budget.limit,
        match budget.raise.as_deref() {
            Some(raise) if !raise.is_empty() => format!("Raise {raise} to allow more."),
            _ => budget_origin_sentence(hop_error, &budget.source),
        },
    )
}

/// The bound's name in a sentence. `other` is not expected on the wire (docket's `bound` is a
/// closed set of four), but a limit is still named rather than left blank if docket ever adds
/// one Tack has not read about yet.
fn budget_bound_phrase(bound: &str) -> String {
    match bound {
        "rounds" => "round limit".to_owned(),
        "toolCalls" => "tool-call limit".to_owned(),
        "time" => "time limit".to_owned(),
        "tokens" => "token limit".to_owned(),
        other => format!("{other} limit"),
    }
}

/// Where to raise the limit, for the rare hop whose `budget.raise` is absent (Card L's live proof
/// against docket `develop` `fa06ce05`, P59-6/7/8, confirms `raise` is always present today — this
/// is a fallback, in practice near-dead code, kept for an older docket or one that regresses the
/// field). docket's stop message also carries a
/// `"(source: <source>; raise <key>)"` tail for a bound its machine or its pipeline sized
/// (`agent/loop_types.py::limit_origin`); reading that key out of it, rather than keeping a
/// table of Tack's own, means a key docket renames or retires (ADR 0038 decision 5, which is
/// already dropping `maxIterations`/`maxToolCalls` as declarable keys) changes this sentence by
/// itself. Falls back to a source-only sentence when the hop's `error` carries no such tail —
/// an older docket, or a bound (`tokens`) docket's loop never tags this way.
fn budget_origin_sentence(hop_error: &str, source: &str) -> String {
    if let Some(start) = hop_error.find("(source:")
        && let Some(end) = hop_error[start..].find(')')
        && let Some((_, raise)) = hop_error[start..start + end].split_once("raise ")
    {
        return format!("Raise {} to allow more.", raise.trim());
    }
    match source {
        "step" => "Declared on the team's pipeline step.".to_owned(),
        "pod" => "Raise the team's turnTimeoutS.".to_owned(),
        "default" => "It is the machine's default.".to_owned(),
        "task" => "It is this task's own limit.".to_owned(),
        "" => "docket did not say where this limit comes from.".to_owned(),
        other => format!("Set by the team's {other}."),
    }
}

/// What a person's answer will be posted as: docket's escalation and what it takes.
struct Pending {
    /// docket's id for the escalation, which is also the route its answer goes to.
    escalation: String,
    /// The ids of docket's own options.
    options: Vec<String>,
    /// docket offers only to decline, and the person may answer in text instead.
    by_text: bool,
    /// The one property the answer's text fills, and whether docket requires it.
    property: String,
    required: bool,
    expires_at: Option<DateTime<Utc>>,
}

pub struct DocketAdapter {
    client: Client,
    state_dir: PathBuf,
    ensured: Mutex<HashSet<String>>,
    live: Mutex<HashMap<String, Live>>,
    /// Where the activity reader is in each team's tasks and trace.
    activity: Mutex<HashMap<String, activity::TeamCursor>>,
    /// How long recovery waits for an unreachable docket, and whether it already has.
    recovery_wait: Duration,
    recovery_waited: tokio::sync::OnceCell<()>,
}

impl DocketAdapter {
    /// An adapter for the docket `config` names. `state_dir` is the runner's
    /// own, where each attempt's trace cursor is kept.
    pub fn new(config: DocketConfig, state_dir: PathBuf) -> Self {
        Self {
            client: Client::new(&config),
            state_dir,
            ensured: Mutex::new(HashSet::new()),
            live: Mutex::new(HashMap::new()),
            activity: Mutex::new(HashMap::new()),
            recovery_wait: RECOVERY_WAIT,
            recovery_waited: tokio::sync::OnceCell::new(),
        }
    }

    /// The same adapter that waits `within` for a docket it cannot reach when it recovers a task.
    #[must_use]
    pub fn with_recovery_wait(mut self, within: Duration) -> Self {
        self.recovery_wait = within;
        self
    }

    /// Where one kind of progress for an attempt is kept: `cursor` for the trace, `since` for the
    /// escalations.
    fn state_path(&self, attempt_id: &str, extension: &str) -> PathBuf {
        let safe: String = attempt_id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        self.state_dir
            .join("docket")
            .join(format!("{safe}.{extension}"))
    }

    fn read_cursor(&self, attempt_id: &str) -> String {
        std::fs::read_to_string(self.state_path(attempt_id, "cursor"))
            .map(|text| text.trim().to_owned())
            .unwrap_or_default()
    }

    /// How many finished hops of this attempt's task have reached the timeline.
    fn read_hops_finished(&self, attempt_id: &str) -> usize {
        std::fs::read_to_string(self.state_path(attempt_id, "hops"))
            .ok()
            .and_then(|text| text.trim().parse().ok())
            .unwrap_or(0)
    }

    /// The `seq` below which this attempt's escalations are all settled.
    fn read_since(&self, attempt_id: &str) -> u64 {
        std::fs::read_to_string(self.state_path(attempt_id, "since"))
            .ok()
            .and_then(|text| text.trim().parse().ok())
            .unwrap_or(0)
    }

    /// Written after what it follows has been delivered, through a rename so a crash leaves the
    /// old value or the new one, never half of one.
    fn write_state(&self, attempt_id: &str, extension: &str, value: &str) {
        let path = self.state_path(attempt_id, extension);
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| {
                let partial = path.with_extension(format!("{extension}.partial"));
                std::fs::write(&partial, value)?;
                std::fs::rename(&partial, &path)
            });
        if written.is_err() {
            tracing::warn!(
                harness = DESCRIPTOR.kind,
                extension,
                "the attempt's progress could not be saved"
            );
        }
    }

    /// The team a profile names, when it names one.
    fn named_team(spec: &ExecutionSpec) -> Option<String> {
        tack_core::run_defaults::named_team(&spec.work.request.resolved_agent_profile.tool_policy)
            .map(|(team, _)| team)
    }

    /// The pod a task goes to: the profile's team, else the project's default
    /// pod, named for the repository folder.
    fn team(spec: &ExecutionSpec) -> Result<String, HarnessError> {
        if let Some(team) = Self::named_team(spec) {
            return Ok(team);
        }
        let repository = spec.work.workspace_repository().map_err(|error| {
            rejected(format!("the claim's repository facts disagree ({error})"))
        })?;
        Ok(default_pod(
            repository
                .repository_path
                .as_deref()
                .unwrap_or(Path::new("")),
        ))
    }

    async fn run_info(&self, process_id: &str) -> Option<Arc<RunInfo>> {
        self.live
            .lock()
            .await
            .get(process_id)
            .map(|live| Arc::clone(&live.info))
    }

    /// Makes the task known to `wait`, `cancel` and the channels the engine
    /// takes, under the handle `pod/task`.
    async fn register(&self, info: RunInfo) -> LocalRunHandle {
        let process_id = format!("{}/{}", info.pod, info.task);
        let (questions_tx, questions_rx) = mpsc::channel(1);
        let (answers_tx, answers_rx) = mpsc::channel(1);
        let (progress_tx, progress_rx) = mpsc::channel(PROGRESS_BUFFER);
        let (elsewhere_tx, elsewhere_rx) = mpsc::channel(1);
        self.live.lock().await.insert(
            process_id.clone(),
            Live {
                info: Arc::new(info),
                decisions: Some((questions_rx, answers_tx)),
                progress: Some(progress_rx),
                elsewhere: Some(elsewhere_rx),
                wait: Some(WaitSide {
                    questions: questions_tx,
                    answers: answers_rx,
                    progress: progress_tx,
                    elsewhere: elsewhere_tx,
                }),
            },
        );
        LocalRunHandle { process_id }
    }

    /// The commit the task's branch stands on in the repository, which is
    /// where docket left the work. Nothing is checked out or moved.
    async fn branch_head(repository: &Path, branch: &str) -> Result<String, String> {
        if branch.is_empty() || branch.starts_with('-') {
            return Err("docket named no usable branch for the task".to_owned());
        }
        let output = tokio::process::Command::new("git")
            .arg("-C")
            .arg(repository)
            .args([
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch}^{{commit}}"),
            ])
            .output()
            .await
            .map_err(|error| format!("git could not be run ({})", error.kind()))?;
        let head = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if output.status.success() && !head.is_empty() {
            Ok(head)
        } else {
            Err(format!(
                "the task's branch {branch:?} is not in the repository"
            ))
        }
    }

    /// Whether the team works in a repository, read once when its run starts: `workspaceKind` when
    /// docket sends it, else `repoConfigDir` (`""` for a folder). Anything docket does not say
    /// counts as yes: a folder team is the one thing it has to say outright.
    async fn has_repository(&self, pod: &str) -> bool {
        in_repository(self.read_team(pod).await.as_ref())
    }

    /// `GET /pods/<pod>`, or `None` when docket does not answer it or its answer cannot be read.
    async fn read_team(&self, pod: &str) -> Option<PodReport> {
        let reply = self.client.pod(pod).await.ok()?;
        if reply.status != 200 {
            return None;
        }
        reply.read::<PodReport>().ok()
    }

    /// The commit the request's base revision names in the repository, which
    /// is where the task branch is cut; the revision as written when git
    /// cannot say.
    async fn base_commit(spec: &ExecutionSpec) -> String {
        let revision = &spec.workspace.base_revision;
        let commit = tokio::process::Command::new("git")
            .arg("-C")
            .arg(&spec.workspace.path)
            .args(["rev-parse", "--verify", "--quiet"])
            .arg(format!("{revision}^{{commit}}"))
            .output()
            .await
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
            .filter(|commit| !commit.is_empty());
        commit.unwrap_or_else(|| revision.clone())
    }
}

/// The name of the project's default pod: the repository folder's basename as
/// lowercase `[a-z0-9-]`, 64 characters at most.
pub fn default_pod(repository: &Path) -> String {
    let name = repository
        .file_name()
        .map(|name| name.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let mut slug = String::new();
    for c in name.chars() {
        let c = if c.is_ascii_lowercase() || c.is_ascii_digit() {
            c
        } else {
            '-'
        };
        if c != '-' || !(slug.is_empty() || slug.ends_with('-')) {
            slug.push(c);
        }
    }
    let slug: String = slug.trim_end_matches('-').chars().take(64).collect();
    let slug = slug.trim_end_matches('-').to_owned();
    if slug.is_empty() {
        "default".to_owned()
    } else {
        slug
    }
}

/// docket's cache figures in a hop's or a task's `usage`: `cached` is the part of
/// `input` a cache served and `cacheWrite` the part written to one, both already
/// inside `input` (`TokenUsage` in docket's `core/llm.py`, whose `counted_tokens` is
/// `input - cached + output`; Tack's `Usage::counted` is the same sum over
/// `tokens_in` and `cache_read_tokens`). docket writes 0 both for an endpoint that
/// reports no figure and for a real zero, which the wire cannot tell apart, so a 0
/// is kept as 0; only a docket that sends no key leaves "cache not reported".
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct CacheFigures {
    read: Option<u64>,
    write: Option<u64>,
}

impl CacheFigures {
    /// A hop's, or one model's, `usage`.
    fn of_hop(usage: &Tokens) -> Self {
        Self {
            read: usage.cached,
            write: usage.cache_write,
        }
    }

    /// A task's `usage`: its own `cached`/`cacheWrite` when docket sends them, else
    /// the sums of its `byModel` entries, which carry them since docket's `da556114`.
    fn of_task(usage: &TaskUsage) -> Self {
        let sum = |figure: fn(&Tokens) -> Option<u64>| {
            usage
                .by_model
                .values()
                .filter_map(figure)
                .reduce(|a, b| a + b)
        };
        Self {
            read: usage.total.cached.or_else(|| sum(|used| used.cached)),
            write: usage
                .total
                .cache_write
                .or_else(|| sum(|used| used.cache_write)),
        }
    }

    /// The keys `Usage.additional` carries them under.
    fn put(self, additional: &mut BTreeMap<String, Value>) {
        for (key, tokens) in [
            ("cache_read_tokens", self.read),
            ("cache_write_tokens", self.write),
        ] {
            if let Some(tokens) = tokens {
                additional.insert(key.to_owned(), tokens.into());
            }
        }
    }
}

fn hop_role(role: &str) -> String {
    let mut chars = role.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

/// The timeline line a `hop_started` or `hop_finished` record makes.
fn hop_progress(finished: bool, hop: &HopEvent) -> Progress {
    let who = format!(
        "{} ({})",
        hop_role(&hop.role),
        hop.model.as_deref().unwrap_or("no model named")
    );
    if !finished {
        return Progress {
            kind: event_kind::AGENT_HOP_STARTED.to_owned(),
            text: format!("{who} started"),
            usage: None,
        };
    }
    let mut parts = vec![match &hop.verdict {
        Some(verdict) => verdict.clone(),
        None if hop.ok == Some(false) => match hop.error.as_deref() {
            Some(error) if !error.is_empty() => format!("failed: {error}"),
            _ => "failed".to_owned(),
        },
        None => "done".to_owned(),
    }];
    if let Some(cycle) = hop.cycle.filter(|cycle| *cycle > 0) {
        parts.push(format!("cycle {cycle}"));
    }
    if let Some(gate) = &hop.gate {
        parts.push(format!("gate {gate}"));
    }
    let mut usage = None;
    if let Some(used) = &hop.usage
        && let (Some(input), Some(output)) = (used.input, used.output)
    {
        let cache = CacheFigures::of_hop(used);
        parts.push(match cache.read {
            Some(read) => {
                format!("{input} tokens in ({read} read from cache), {output} out")
            }
            None => format!("{input} tokens in, {output} out"),
        });
        // docket's `input` includes the cache reads and writes, as `tokens_in` does.
        let mut additional = BTreeMap::new();
        cache.put(&mut additional);
        usage = Some(Usage {
            tokens_in: measurement(Some(input)),
            tokens_out: measurement(Some(output)),
            duration_ms: measurement(hop.duration_ms),
            cost_usd: measurement(None),
            additional,
        });
    }
    Progress {
        kind: event_kind::AGENT_HOP_FINISHED.to_owned(),
        text: format!("{who}: {}", parts.join(", ")),
        usage,
    }
}

/// The labels docket gave the options of the escalations Tack has read, by escalation and
/// option, so a timeline line names what a person picked and never docket's id for it.
type OptionLabels = HashMap<(String, String), String>;

/// The timeline line an `escalation_*` record makes. `escalation_delivered` says only that a
/// channel carried a notification, which the line for the question already tells.
fn escalation_progress(event: &TraceEvent, labels: &OptionLabels) -> Option<Progress> {
    let (kind, text) = match event {
        TraceEvent::EscalationAsked(asked) => (
            event_kind::ESCALATION_ASKED,
            format!(
                "docket asked: {}",
                asked.prompt.as_deref().unwrap_or("it names no question")
            ),
        ),
        TraceEvent::EscalationResolved(resolved) => (
            event_kind::ESCALATION_RESOLVED,
            match picked_label(resolved, labels) {
                Some(label) => format!("a person answered: {label}"),
                None => "a person answered".to_owned(),
            },
        ),
        TraceEvent::EscalationExpired(_) => (
            event_kind::ESCALATION_EXPIRED,
            "nobody answered before it expired".to_owned(),
        ),
        TraceEvent::EscalationDecided(_) => (
            event_kind::ESCALATION_DECIDED,
            "docket decided this on its own".to_owned(),
        ),
        _ => return None,
    };
    Some(Progress {
        kind: kind.to_owned(),
        text,
        usage: None,
    })
}

/// The label docket gave the option an escalation was resolved with, when Tack has read it.
fn picked_label<'a>(event: &EscalationEvent, labels: &'a OptionLabels) -> Option<&'a str> {
    let key = (event.escalation.clone()?, event.option_id.clone()?);
    labels.get(&key).map(String::as_str)
}

/// One of docket's options for an escalation, worded as docket words it.
fn escalation_option(option: &wire::EscalationOption) -> DecisionOption {
    let text = |value: &Option<String>| value.clone().filter(|text| !text.is_empty());
    DecisionOption {
        option_id: option.id.clone(),
        label: text(&option.label).unwrap_or_else(|| option.id.clone()),
        description: text(&option.description),
        risks: Some(option.risks.clone()).filter(|risks| !risks.is_empty()),
        estimated_tokens: option.estimated_tokens,
    }
}

/// docket's recommendation, when it names one of the options offered.
fn escalation_recommendation(
    record: &Escalation,
    options: &[DecisionOption],
) -> Option<Recommendation> {
    let recommendation = record.recommendation.as_ref()?;
    options
        .iter()
        .find(|option| option.option_id == recommendation.option_id)?;
    Some(Recommendation {
        option_id: recommendation.option_id.clone(),
        rationale: recommendation
            .rationale
            .clone()
            .filter(|text| !text.is_empty()),
        evidence_refs: Some(recommendation.evidence_refs.clone())
            .filter(|evidence| !evidence.is_empty()),
    })
}

/// The id Tack offers for answering in text where docket's only option is to decline.
const TEXT_ANSWER: &str = "accept";

/// The kinds of escalation docket records (`operator-loop.spec.md`), which Tack puts to a person.
const KNOWN_KINDS: [&str; 5] = ["approval", "gate", "check_changed", "consult", "input"];

/// The kinds whose answer's text is a reason.
const REASON_KINDS: [&str; 3] = ["approval", "gate", "check_changed"];

/// A pipeline step that parks the task until a time. docket's sweep ends it and nobody is asked,
/// so it is never one of the questions above.
const WAIT_KIND: &str = "wait";

/// What a person reads when docket marks an approval as leaving the machine but does not show the
/// call: approving what cannot be seen is the one thing this asks nobody to do.
const CALL_NOT_SHOWN: &str =
    "docket did not show the call that would leave; answer it in docket, where it is shown";

/// The timeline line for a wait: when it ends, in the person's own clock, with the date only when
/// it is not today.
fn wait_line(until: Option<DateTime<Utc>>, now: DateTime<Utc>) -> String {
    let Some(until) = until else {
        return "The team waits. Nobody needs to answer.".to_owned();
    };
    let (until, now) = (until.with_timezone(&Local), now.with_timezone(&Local));
    let format = if until.date_naive() == now.date_naive() {
        "%H:%M"
    } else {
        "%Y-%m-%d %H:%M"
    };
    format!(
        "The team waits until {}. Nobody needs to answer.",
        until.format(format)
    )
}

/// The question one of docket's escalations is, and what answering it posts. The options are
/// docket's own, in its order, so the last one is the way out; a question whose only option is to
/// decline but which takes text is offered an answer in text beside it. A kind Tack does not know
/// is the sentence saying so, and nothing is asked. An approval docket marks `outward` is asked
/// only with the `call` it would make, which rides in the question's metadata; without it the
/// sentence says to answer in docket.
fn escalation_question(
    record: &Escalation,
    call: Option<&str>,
) -> Result<(Question, Pending), String> {
    let kind = record.kind.clone();
    if !KNOWN_KINDS.contains(&kind.as_str()) {
        return Err(format!(
            "docket asked something of kind {kind:?}, which this Tack cannot put to you; answer it in docket"
        ));
    }
    let leaving = record.outward && kind == "approval";
    let call = call.filter(|call| !call.trim().is_empty());
    if leaving && call.is_none() {
        return Err(CALL_NOT_SHOWN.to_owned());
    }
    let mut options: Vec<DecisionOption> = record.options.iter().map(escalation_option).collect();
    let docket_ids: Vec<String> = options
        .iter()
        .map(|option| option.option_id.clone())
        .collect();
    let by_text = docket_ids.iter().all(|id| id == "decline") && record.affordances.accepts_text;
    if by_text {
        let own = |id: &str, label: &str| DecisionOption {
            option_id: id.to_owned(),
            label: label.to_owned(),
            description: None,
            risks: None,
            estimated_tokens: None,
        };
        options = vec![own(TEXT_ANSWER, "Answer"), own("decline", "Decline")];
    }
    let recommendation = escalation_recommendation(record, &options);
    // What the answer's text fills: an approval takes a reason, a question the one property
    // its schema names.
    let schema = record.requested_schema.as_ref();
    let property = if REASON_KINDS.contains(&kind.as_str()) {
        "reason".to_owned()
    } else {
        schema
            .and_then(|schema| schema.properties.keys().next().cloned())
            .unwrap_or_else(|| "answer".to_owned())
    };
    let required = schema.is_some_and(|schema| schema.required.contains(&property));
    let mut metadata = serde_json::Map::new();
    // Where the answer's text goes, so the screen opens its field under the choice it belongs to
    // and nowhere else: a refusal takes a reason, a question that offers only to decline takes
    // the answer itself, and a question that takes text takes it with any choice but declining.
    let reason = REASON_KINDS.contains(&kind.as_str());
    let text_options: Vec<&str> = if by_text {
        vec![TEXT_ANSWER]
    } else if reason {
        docket_ids
            .iter()
            .map(String::as_str)
            .filter(|id| *id == "deny")
            .collect()
    } else if record.affordances.accepts_text {
        docket_ids
            .iter()
            .map(String::as_str)
            .filter(|id| *id != "decline")
            .collect()
    } else {
        Vec::new()
    };
    if !text_options.is_empty() {
        metadata.insert(
            "text".to_owned(),
            json!({
                "label": if reason { "Reason" } else { "Your answer" },
                "required": required && !reason,
                "options": text_options,
            }),
        );
    }
    for (key, value) in [
        ("step", &record.step),
        ("role", &record.role),
        ("policy", &record.policy),
        ("level", &record.level),
    ] {
        if let Some(value) = value.as_deref().filter(|value| !value.is_empty()) {
            metadata.insert(key.to_owned(), value.into());
        }
    }
    if let Some(call) = call.filter(|_| leaving) {
        metadata.insert("outward".to_owned(), true.into());
        metadata.insert("call".to_owned(), call.into());
    }
    // One question of several the same step asked at once: the step waits for every one, so the
    // person is told which this is. A consult that asked one says nothing.
    if let Some(part) = record.part.filter(|part| part.count > 1) {
        metadata.insert(
            "part".to_owned(),
            json!({"index": part.index, "count": part.count}),
        );
    }
    Ok((
        Question {
            vendor_id: record.id.clone(),
            kind,
            prompt: record
                .prompt
                .as_deref()
                .filter(|prompt| !prompt.is_empty())
                .unwrap_or("docket asks a question")
                .to_owned(),
            options,
            recommendation,
            metadata,
        },
        Pending {
            escalation: record.id.clone(),
            options: docket_ids,
            by_text,
            property,
            required,
            expires_at: record.expires_at,
        },
    ))
}

/// What the one record of an `outward` approval shows of the call.
enum Call {
    Shown(String),
    Missing,
    Settled,
}

/// What one read of the task's escalations found.
#[derive(Default)]
struct Escalations {
    /// The first one waiting for a person that has not been asked yet.
    next: Option<(Question, Pending)>,
    /// Pending ones Tack cannot put to a person, by docket's id, and the sentence saying why.
    unaskable: Vec<(String, String)>,
    /// Pending waits not yet reported, by docket's id, and the time each ends.
    waits: Vec<(String, Option<DateTime<Utc>>)>,
    /// The label of every option of every record read.
    labels: OptionLabels,
    /// The held one has expired in docket's own record.
    expired: bool,
    /// The held one has been settled in docket's own record (answered at its CLI, or by a
    /// policy) while Tack still holds it open.
    settled: bool,
    /// The highest `seq` below the first record still pending: where a restarted adapter may
    /// read from without missing a question.
    settled_through: u64,
    /// docket's ids of the conversation's `turn` records, settled or not: its answers are the
    /// turn's reply and its reply is the next message, neither a question for a person.
    turns: Vec<String>,
    /// A `turn` record is pending: the conversation waits for the next message.
    parked: bool,
}

impl DocketAdapter {
    /// The task's escalations past `since`, from docket's one record: the question to ask next, if
    /// one is waiting and not yet asked, and whether the one held has expired.
    async fn read_escalations(
        &self,
        info: &RunInfo,
        since: u64,
        asked: &HashSet<String>,
        held: Option<&Pending>,
    ) -> Option<Escalations> {
        let reply = self
            .client
            .escalations(since, Some(&info.task))
            .await
            .ok()
            .filter(|r| r.status == 200)?;
        let list: EscalationList = reply
            .read()
            .inspect_err(|why| tracing::warn!(why, "docket's escalations could not be read"))
            .ok()?;
        let mut found = Escalations {
            settled_through: since,
            ..Escalations::default()
        };
        let mut open = false;
        for raw in list.escalations {
            let record = match serde_json::from_value::<Escalation>(raw.clone()) {
                Ok(record) => record,
                Err(why) => {
                    // A pending record that cannot be read is still one a person must be told of.
                    if let Some(id) = raw["id"].as_str()
                        && raw["state"] == json!("pending")
                        && !asked.contains(id)
                    {
                        open = true;
                        found.unaskable.push((
                            id.to_owned(),
                            format!(
                                "docket asked something this Tack could not read ({why}); answer it in docket"
                            ),
                        ));
                    }
                    continue;
                }
            };
            for option in &record.options {
                if let Some(label) = option.label.as_deref().filter(|label| !label.is_empty()) {
                    found
                        .labels
                        .insert((record.id.clone(), option.id.clone()), label.to_owned());
                }
            }
            if let Some(held) = held
                && record.id == held.escalation
                && matches!(record.state, EscalationState::Expired)
            {
                found.expired = true;
            }
            if let Some(held) = held
                && record.id == held.escalation
                && matches!(record.state, EscalationState::Settled)
            {
                found.settled = true;
            }
            if record.kind == TURN_KIND {
                found.turns.push(record.id.clone());
            }
            if matches!(record.state, EscalationState::Pending) {
                open = true;
                if record.kind == TURN_KIND {
                    found.parked = true;
                } else if record.kind == WAIT_KIND {
                    if !asked.contains(&record.id) {
                        found.waits.push((record.id.clone(), record.expires_at));
                    }
                } else if held.is_none() && found.next.is_none() && !asked.contains(&record.id) {
                    let call = if record.outward && record.kind == "approval" {
                        match self.call_that_leaves(&record.id).await {
                            Call::Shown(call) => Some(call),
                            Call::Missing => None,
                            // Answered in docket between the list and this read.
                            Call::Settled => continue,
                        }
                    } else {
                        None
                    };
                    let call = call.map(|call| info.secrets.scrub(&call));
                    match escalation_question(&record, call.as_deref()) {
                        Ok((mut question, pending)) => {
                            question.prompt = info.secrets.scrub(&question.prompt);
                            let mut metadata =
                                Value::Object(std::mem::take(&mut question.metadata));
                            info.secrets.scrub_json(&mut metadata);
                            if let Value::Object(metadata) = metadata {
                                question.metadata = metadata;
                            }
                            found.next = Some((question, pending));
                        }
                        Err(sentence) => found.unaskable.push((record.id.clone(), sentence)),
                    }
                }
            } else if !open {
                found.settled_through = found.settled_through.max(record.seq);
            }
        }
        Some(found)
    }

    /// The call an `outward` approval would make, from the one record that carries it. A read
    /// that fails, a `404` or an answer with no call is `Missing`: the person is never asked to
    /// approve what Tack cannot show.
    async fn call_that_leaves(&self, id: &str) -> Call {
        let detail = match self.client.escalation(id).await {
            Ok(reply) if reply.status == 200 => reply.read::<EscalationDetail>().ok(),
            _ => None,
        };
        match detail {
            Some(detail) if !matches!(detail.view.state, EscalationState::Pending) => Call::Settled,
            Some(EscalationDetail {
                call: Some(call), ..
            }) if !call.trim().is_empty() => Call::Shown(call),
            _ => Call::Missing,
        }
    }

    /// What the text docket sends back for this run is scrubbed of, as a local agent's output is.
    fn run_secrets(&self, work: &ClaimedWork) -> SecretMaterial {
        let mut secrets = self.client.secrets();
        for value in work.request.environment.values() {
            if let Some(literal) = &value.value {
                secrets.register(literal.clone());
            }
        }
        secrets.register(super::local_prompt(work));
        secrets
    }

    /// Posts the person's answer where docket reads it. A `409` or a `404` means docket had
    /// already settled it, or no longer has it, which is not a failure.
    async fn deliver(&self, pending: &Pending, answer: &DecisionAnswer) -> Option<String> {
        let chosen = answer.option_id.as_deref();
        let option = match chosen {
            Some(TEXT_ANSWER) if pending.by_text => None,
            Some(id) if pending.options.iter().any(|own| own == id) => Some(id),
            // Whatever the person picked that docket does not offer is a refusal.
            _ if pending.options.iter().any(|own| own == "deny") => Some("deny"),
            _ => Some("decline"),
        };
        let mut body = json!({"actor": "tack"});
        if let Some(option) = option {
            body["optionId"] = json!(option);
        }
        let text = answer.text.as_deref().filter(|text| !text.is_empty());
        if option != Some("decline")
            && (text.is_some() || (pending.required && option != Some("deny")))
        {
            body["content"] = json!({pending.property.as_str(): text.unwrap_or_default()});
        }
        match self.client.resolve(&pending.escalation, body).await {
            Ok(reply) if matches!(reply.status, 200 | 404 | 409) => None,
            Ok(reply) => Some(format!("docket did not take the answer: {}", reply.error())),
            Err(why) => Some(format!("the answer could not reach docket: {why}")),
        }
    }

    async fn outcome(
        &self,
        info: &RunInfo,
        state: AttemptState,
        reason: Value,
        task: Option<&TaskView>,
        model: Option<String>,
    ) -> HarnessOutcome {
        let usage = task.and_then(|task| task.usage.as_ref());
        let by_model: BTreeMap<String, Value> = usage
            .into_iter()
            .flat_map(|usage| &usage.by_model)
            .map(|(name, used)| {
                // `by_model` keeps the input the cache did not serve, with the cache
                // figures on top (`ModelUsage`); docket's `input` has them inside.
                let cache = CacheFigures::of_hop(used);
                let input = used.input.unwrap_or(0);
                let mut entry = json!({
                    "input": input
                        .saturating_sub(cache.read.unwrap_or(0))
                        .saturating_sub(cache.write.unwrap_or(0)),
                    "output": used.output.unwrap_or(0),
                });
                if let Some(read) = cache.read {
                    entry["cache_read"] = read.into();
                }
                if let Some(write) = cache.write {
                    entry["cache_write"] = write.into();
                }
                (name.clone(), entry)
            })
            .collect();
        let mut additional = BTreeMap::new();
        if let Some(usage) = usage {
            CacheFigures::of_task(usage).put(&mut additional);
        }
        if !by_model.is_empty() {
            additional.insert("by_model".to_owned(), json!(by_model));
        }
        // What each step was made of, from the record of a task that has ended.
        let mut reason = reason;
        if let Some(task) = task.filter(|task| terminal(&task.status))
            && !task.hops.is_empty()
            && let Some(object) = reason.as_object_mut()
        {
            let hop_times = self.client.has_feature("hopTimes").await;
            object.insert(
                "steps".to_owned(),
                Value::Array(steps::steps(&task.hops, hop_times)),
            );
        }
        info.secrets.scrub_json(&mut reason);
        let ended_at = Utc::now();
        let elapsed =
            u64::try_from((ended_at - info.started_at).num_milliseconds().max(0)).unwrap_or(0);
        HarnessOutcome {
            terminal_state: state,
            terminal_reason: reason,
            final_checkpoint: None,
            actual_execution: ActualExecution {
                harness_kind: DomainHarnessKind::new(DESCRIPTOR.kind),
                harness_version: info
                    .api
                    .map_or_else(|| "API unknown".to_owned(), |api| format!("API {api}")),
                model_source: if model.is_some() {
                    ModelSource::Reported
                } else {
                    ModelSource::AgentDefault
                }
                .as_str()
                .to_owned(),
                model,
                capability_snapshot: {
                    let mut features = capabilities();
                    if let Some(converse) = &info.converse {
                        features.converse = Some(converse.clone());
                    }
                    features
                },
                // Overwritten by the engine, which owns workspace facts.
                workspace_id: DomainWorkspaceId::new(""),
                base_revision: String::new(),
                started_at: info.started_at,
                ended_at,
                additional: BTreeMap::from([("team".to_owned(), json!(info.pod))]),
            },
            usage: Usage {
                // A continuation's task carries the whole task's usage, old hops included
                // (decision 6): its own baseline, what the task had already used when the retry
                // was sent, is taken back out so the run counts only its new hops.
                tokens_in: measurement(
                    usage
                        .and_then(|usage| usage.total.input)
                        .map(|input| input.saturating_sub(info.baseline_usage.map_or(0, |b| b.0))),
                ),
                tokens_out: measurement(
                    usage.and_then(|usage| usage.total.output).map(|output| {
                        output.saturating_sub(info.baseline_usage.map_or(0, |b| b.1))
                    }),
                ),
                duration_ms: measurement(Some(elapsed)),
                cost_usd: measurement(None),
                additional,
            },
        }
    }

    /// A thread's turn answers with what the team's last step handed off: its `handoff` summary,
    /// which docket fills with the step's whole reply when the step made no handoff call. A
    /// repository team's turn has a branch, so the engine would never look for a reply file, and
    /// docket's agent is not told to write one.
    fn reply(info: &RunInfo, task: &TaskView, reason: &mut Value) {
        if info.turn
            && let Some(summary) = steps::result(&task.hops)
        {
            reason[REPLY_REASON_KEY] = json!(summary);
        }
    }

    /// What `next.continue` on a budget stop says: whether a request may follow this task with
    /// `continue_task`, and if not, why (decision 5's table, "Continue or Start over" in
    /// `docs/closed-cycles/plans/phase-87.md`). `hop_budget` is the last hop's `budget`, when `budgetOutcome`
    /// read one; without it (an older docket, or one that never wrote the key on this hop) Tack
    /// cannot tell a step's own budget stop from a confused model, so continuing is never
    /// offered — the same case as a docket without `retryContinues`.
    async fn continue_offer(&self, hop_budget: Option<&BudgetOutcome>) -> Value {
        const UPDATE_DOCKET: &str = "This team's docket cannot continue a step. Update docket.";
        let Some(budget) = hop_budget else {
            return json!({"possible": false, "why": UPDATE_DOCKET});
        };
        if budget.bound == "tokens" {
            return json!({"possible": false,
                           "why": "The task's token limit is spent, and continuing cannot raise it."});
        }
        if !budget.tree_changed {
            return json!({"possible": false,
                           "why": "The step changed nothing; the task may be unclear. Reword it before running again."});
        }
        if !self.client.has_feature("retryContinues").await {
            return json!({"possible": false, "why": UPDATE_DOCKET});
        }
        json!({"possible": true})
    }

    /// Adds `branch`/`head_commit`/`base_commit` to a budget stop's `reason` when the task's
    /// worktree has a branch to read (decision 3, Phase 87): any budget-stopped hop that pinned
    /// its tree — an Implementer's, whatever bound it hit, `tokens` included — leaves work worth
    /// showing, not only a `step_budget` one. Card L's live proof found a tokens-bound stop with
    /// `treeChanged: true` left with neither key, because only the `step_budget` arm ever called
    /// this; `from_run` then refused the very run it should have offered first (decision 5's
    /// table, row 2). Called only once the caller has already checked `budget.tree_changed`.
    async fn add_kept_branch(&self, reason: &mut Value, task: &TaskView, workspace: &Path) {
        let branch = task
            .worktree
            .as_ref()
            .map_or("", |worktree| worktree.branch.as_str());
        if let Ok(head) = Self::branch_head(workspace, branch).await {
            reason["branch"] = json!(branch);
            reason["head_commit"] = json!(head);
            reason["base_commit"] = json!(
                task.worktree
                    .as_ref()
                    .and_then(|worktree| worktree.base_commit.clone())
            );
        }
    }

    /// The task has ended: what that makes of the attempt.
    async fn finish(
        &self,
        info: &RunInfo,
        task: &TaskView,
        model: Option<String>,
        progress: &mpsc::Sender<Progress>,
        unanswered_until: Option<DateTime<Utc>>,
        // The question last put to the person, when it was asked, and whether an answer went back.
        asked: Option<&(String, DateTime<Utc>, bool)>,
    ) -> HarnessOutcome {
        // docket's task record says why it ended in `reason`, for a failed and a blocked task
        // alike; its hops carry their own `error`, the task has none.
        let why = task.reason.as_deref().unwrap_or_default();
        let failure_kind = task.failure_kind.as_deref();
        let (state, mut reason) = match task.status.as_str() {
            // A team in a folder leaves no branch: what it did is what its last step handed off,
            // and there is nothing for the engine to capture or merge.
            "done" if task.worktree.is_none() && !info.in_repository => {
                let mut reason = json!({"code": "done", "outside_repository": true,
                                        "team": info.pod, "task": info.task,
                                        "summary": steps::result(&task.hops)});
                Self::reply(info, task, &mut reason);
                (AttemptState::Succeeded, reason)
            }
            "done" => {
                let branch = task
                    .worktree
                    .as_ref()
                    .map_or("", |worktree| worktree.branch.as_str());
                match Self::branch_head(&info.workspace, branch).await {
                    Ok(head) => {
                        let mut untracked = 0;
                        if let Ok(reply) = self.client.task_diff(&info.pod, &info.task).await
                            && reply.status == 200
                            && let Ok(diff) = reply.read::<TaskDiff>()
                            && !diff.untracked.is_empty()
                        {
                            let count = diff.untracked.len();
                            untracked = count;
                            let text = if count == 1 {
                                "1 file docket did not commit is not in the evidence".to_owned()
                            } else {
                                format!(
                                    "{count} files docket did not commit are not in the evidence"
                                )
                            };
                            let _ = progress.try_send(Progress {
                                kind: event_kind::AGENT_UNTRACKED.to_owned(),
                                text,
                                usage: None,
                            });
                        }
                        // `head_commit` is where the engine reads the work from.
                        let mut reason = json!({"code": "done", "team": info.pod,
                                                "task": info.task, "branch": branch,
                                                "head_commit": head,
                                                "base_commit": task.worktree.as_ref()
                                                    .and_then(|worktree| worktree.base_commit.clone())});
                        // The engine names them when the branch turns out to equal its base.
                        if untracked > 0 {
                            reason["untracked_files"] = json!(untracked);
                        }
                        Self::reply(info, task, &mut reason);
                        (AttemptState::Succeeded, reason)
                    }
                    Err(message) => (
                        AttemptState::Failed,
                        json!({"code": "branch_not_applied", "message": message,
                               "task": info.task}),
                    ),
                }
            }
            "cancelled" => (
                AttemptState::Cancelled,
                json!({"code": "cancelled", "task": info.task}),
            ),
            // docket's own sweep ended a question nobody answered in time.
            "blocked" if task.blocked_reason.as_deref() == Some("input_expired") => (
                AttemptState::Failed,
                json!({"code": "not_answered",
                       "message": super::not_answered(unanswered_until),
                       "task": info.task}),
            ),
            // A wait ran out and the pipeline names no step to go to next.
            "blocked" if task.blocked_reason.as_deref() == Some("wait_expired") => (
                AttemptState::Failed,
                json!({"code": "wait_expired",
                       "message": "The team's wait ran out and its pipeline does not say what comes next.",
                       "task": info.task}),
            ),
            // The team's budget was spent before a step (`failureKinds`): the run says so and how
            // it goes on, and not docket's text, which keeps a dollar figure Tack does not report.
            "blocked"
                if failure_kind == Some("pod_budget")
                    && self.client.has_feature("failureKinds").await =>
            {
                (
                    AttemptState::Failed,
                    json!({"code": "pod_budget",
                           "message": format!(
                               "The team's budget is spent, so docket started nothing more. \
                                Resume it with `docket run --pod {} --resume` once the budget allows.",
                               info.pod),
                           "task": info.task}),
                )
            }
            "blocked" => (
                AttemptState::Failed,
                json!({"code": "blocked", "message": format!("docket said: {why}"), "task": info.task}),
            ),
            // At L5 a tool call needed approval and docket asks nobody; the task is final until
            // someone retries it, so the attempt ends here and nothing queues it again.
            _ if failure_kind == Some("needs_human") => (
                AttemptState::Failed,
                json!({"code": "needs_human",
                       "message": if why.is_empty() { NEEDS_A_PERSON } else { why },
                       "task": info.task}),
            ),
            // `pipelineChecked`: the claim refused the method the task names before any step ran,
            // because the team lacks a role of it or its file does not load. docket's reason names
            // the pipeline, where it comes from and each missing role.
            _ if failure_kind == Some("pipeline") => (
                AttemptState::Failed,
                json!({"code": "pipeline",
                       "message": format!(
                           "The team {} refused the method before any step ran: {why}",
                           info.pod),
                       "task": info.task}),
            ),
            // The install failed before any hop; docket's reason names the command and its output.
            _ if failure_kind == Some("prepare") => (
                AttemptState::Failed,
                json!({"code": "prepare",
                       "message": format!("Could not prepare the dependencies (docket): {why}"),
                       "task": info.task}),
            ),
            // docket's approval timeout ends the task as a denial. Tack sent none, so nobody
            // refused anything: a question went unanswered.
            _ if failure_kind == Some("approval_denied")
                && !asked.is_some_and(|(_, _, answered)| *answered) =>
            {
                (
                    AttemptState::Failed,
                    json!({"code": "not_answered",
                           "message": unanswered_text(asked, unanswered_until),
                           "task": info.task}),
                )
            }
            // docket's own deadline for the approval passed with nobody answering. Its reason
            // ("approval expired: nobody answered by <time>") is kept after the question.
            _ if failure_kind == Some("approval_expired") => {
                let base = unanswered_text(asked, unanswered_until);
                let message = if why.is_empty() {
                    base
                } else {
                    format!("{}. docket said: {why}", base.trim_end_matches('.'))
                };
                (
                    AttemptState::Failed,
                    json!({"code": "not_answered", "message": message, "task": info.task}),
                )
            }
            // A denial a person gave: docket's own text carries their reason.
            _ if failure_kind == Some("approval_denied") => (
                AttemptState::Failed,
                json!({"code": "approval_denied", "message": why, "task": info.task}),
            ),
            // `budgetOutcome`: a step's own budget ran out, not the model's call. A `tokens`
            // bound keeps today's code and meaning (Tack's own token limit, read below), so this
            // arm is only reached for `rounds`, `toolCalls` or `time`. The run stays Failed
            // either way: nothing here is verified, published or merged (decision 3).
            _ if failure_kind == Some("budget")
                && self.client.has_feature("budgetOutcome").await
                && task
                    .hops
                    .last()
                    .and_then(|hop| hop.budget.as_ref())
                    .is_some_and(|budget| budget.bound != "tokens") =>
            {
                let last_hop = task.hops.last().expect("checked by this arm's own guard");
                let budget = last_hop
                    .budget
                    .as_ref()
                    .expect("checked by this arm's own guard");
                let mut reason = json!({"code": "step_budget",
                                        "message": budget_message(budget, &last_hop.error),
                                        "task": info.task});
                // The budget hop pinned the task's work (an Implementer hop, docket's own
                // narrowing): the branch and its head go in, read the same way a `done` run's
                // are, so the engine captures the patch. Without that pin, the branch carries
                // nothing of this step and the reason names neither.
                if budget.tree_changed {
                    self.add_kept_branch(&mut reason, task, &info.workspace)
                        .await;
                }
                (AttemptState::Failed, reason)
            }
            // A `tokens` bound keeps code `budget` (that limit is Tack's own), but a kept tree is
            // still a kept tree: without this arm the generic catch-all below never looks, and
            // `from_run` refuses the very run decision 5's table says to offer it from first
            // (found live, Card L, docs/closed-cycles/plans/phase-87.md — a real tokens-bound stop left with
            // neither key despite a real commit).
            _ if failure_kind == Some("budget")
                && self.client.has_feature("budgetOutcome").await
                && task
                    .hops
                    .last()
                    .and_then(|hop| hop.budget.as_ref())
                    .is_some_and(|budget| budget.bound == "tokens" && budget.tree_changed) =>
            {
                let mut reason = json!({"code": "budget",
                                        "message": format!("docket said: {why}"),
                                        "task": info.task});
                self.add_kept_branch(&mut reason, task, &info.workspace)
                    .await;
                (AttemptState::Failed, reason)
            }
            // `capabilityMissing`: a step met a gap in the team's own setup and docket ended the
            // task instead of asking a person. The change that closes it is the run's message.
            _ if failure_kind == Some("capability_missing") => {
                let mut reason = json!({"code": "capability_missing",
                                        "message": format!(
                                            "The team {} stopped on a gap in its own setup: {why}",
                                            info.pod),
                                        "task": info.task});
                if let Some(gap) = task
                    .hops
                    .iter()
                    .rev()
                    .find_map(|hop| hop.capability_missing.as_ref())
                {
                    reason["missing"] = json!({"kind": gap.kind, "name": gap.name, "fix": gap.fix});
                }
                if kept_work(task) {
                    self.add_kept_branch(&mut reason, task, &info.workspace)
                        .await;
                }
                (AttemptState::Failed, reason)
            }
            // Any other end is docket's, in docket's words and labelled as such. An Implementer
            // stopped by its transport keeps its work, which the engine captures from the branch.
            _ => {
                let mut reason = json!({"code": failure_kind.unwrap_or("failed"),
                                        "message": format!("docket said: {why}"),
                                        "task": info.task});
                if task.status == "failed" && kept_work(task) {
                    self.add_kept_branch(&mut reason, task, &info.workspace)
                        .await;
                }
                (AttemptState::Failed, reason)
            }
        };
        // Decision 5's table, "Continue or Start over": only a budget stop ever offers it, and
        // only docket's own last word on this hop's `budget` (or its absence) decides whether it
        // is possible, and why not.
        if failure_kind == Some("budget")
            && let Some(object) = reason.as_object_mut()
        {
            let hop_budget = task.hops.last().and_then(|hop| hop.budget.as_ref());
            object.insert(
                "next".to_owned(),
                json!({"continue": self.continue_offer(hop_budget).await}),
            );
        }
        let mut outcome = self.outcome(info, state, reason, Some(task), model).await;
        // What the team wrote in this task, beside the steps. The task has ended, so no row
        // changes under the read.
        let records::Kept {
            sets: mut records,
            proposals_unread,
        } = records::kept(&self.client, &info.pod, &info.task).await;
        for set in &mut records {
            info.secrets.scrub_json(set);
        }
        if let Some(object) = outcome.terminal_reason.as_object_mut() {
            // The proposal the rows make: the capture stages it as a file's plan would be, or
            // says why it cannot. Rows the run could not read are a proposal that is not there,
            // and the person is told why.
            match records::proposal(&records) {
                Some(Ok(plan)) => object.insert("plan".to_owned(), plan),
                Some(Err(why)) => object.insert("plan_invalid".to_owned(), Value::String(why)),
                None => proposals_unread
                    .and_then(|why| object.insert("plan_invalid".to_owned(), Value::String(why))),
            };
            if !records.is_empty() {
                object.insert("records".to_owned(), Value::Array(records));
            }
        }
        outcome
    }
}

#[async_trait]
impl HarnessProbe for DocketAdapter {
    fn harness_kind(&self) -> DomainHarnessKind {
        DomainHarnessKind::new(DESCRIPTOR.kind)
    }

    async fn probe(&self) -> HarnessCapability {
        let mut features = capabilities();
        if self.client.has_feature(CONVERSATION).await {
            features.converse = Some(ConverseMode::Continues.capability(CONTINUES));
        }
        probe::probe(&self.client, features).await
    }

    fn declared_capabilities(&self) -> FeatureCapabilities {
        capabilities()
    }

    async fn activity(&self) -> Option<Vec<TeamActivity>> {
        let mut cursors = self.activity.lock().await;
        Some(activity::read(&self.client, &mut cursors, Utc::now()).await)
    }

    async fn resume_team(&self, team: &str) -> Result<Option<TeamReport>, String> {
        probe::resume(&self.client, team).await
    }

    async fn decide_memory(
        &self,
        team: &str,
        entry: &str,
        decision: tack_protocol::execution::MemoryDecision,
        reason: Option<&str>,
    ) -> Result<Option<TeamReport>, String> {
        probe::decide_memory(&self.client, team, entry, decision, reason).await
    }

    async fn apply_team(
        &self,
        team: &str,
        shape: &str,
        directory: &str,
        accept: Option<&str>,
    ) -> OperationOutcome {
        apply::apply_team(&self.client, team, shape, directory, accept).await
    }
}

#[async_trait]
impl HarnessAdapter for DocketAdapter {
    async fn validate(&self, spec: &ExecutionSpec) -> Result<(), HarnessError> {
        let repository = spec.work.workspace_repository().map_err(|error| {
            rejected(format!("the claim's repository facts disagree ({error})"))
        })?;
        if repository.workspace_mode != crate::client::WorkspaceMode::Repository {
            return Err(rejected(WRONG_WORKSPACE.to_owned()));
        }
        let team = Self::named_team(spec);
        // The version first: a docket older than API 6 gets no task, and neither does one
        // without the escalation record, which is where every question is read from.
        let api = probe::status(&self.client).await.map_err(rejected)?.api;
        probe::escalation_record(&self.client, api)
            .await
            .map_err(rejected)?;
        // The default pod is made when the task starts, like any other.
        let Some(team) = team else {
            return Ok(());
        };
        let teams = probe::teams(&self.client)
            .await
            .map_err(|why| rejected(format!("docket's teams could not be read: {why}")))?;
        if !teams.iter().any(|known| known.name == team) {
            return Err(rejected(format!(
                "docket does not report the team {team:?}; check the profile's team"
            )));
        }
        Ok(())
    }

    async fn start(&self, spec: &ExecutionSpec) -> Result<LocalRunHandle, HarnessError> {
        let request = &spec.work.request;
        // `continue_task`: this request follows a task another run already started (decision 6,
        // Phase 87), instead of posting a new one. The app layer put it there once it read the
        // stopped run's own team and task; nothing here looks either up on its own.
        if let Some(process_id) = request
            .metadata
            .get("continue_task")
            .and_then(Value::as_str)
        {
            return self.start_continuation(spec, process_id).await;
        }
        let attempt_id = spec.work.lease.attempt_id.as_str().to_owned();
        let pod = Self::team(spec)?;
        let repository = spec.work.workspace_repository().map_err(|error| {
            rejected(format!("the claim's repository facts disagree ({error})"))
        })?;
        let location = repository
            .repository_path
            .as_deref()
            .unwrap_or(&spec.workspace.path)
            .display()
            .to_string();

        // Once per team per process; docket answers 200 for one it has. The set is not held across
        // the call, so a slow docket delays only the run that waits on it.
        let known = self.ensured.lock().await.contains(&pod);
        if !known {
            match self.client.ensure_pod(&pod, Some(&location), None).await {
                Ok(reply) if reply.status == 200 || reply.status == 201 => {
                    self.ensured.lock().await.insert(pod.clone());
                }
                Ok(reply) => {
                    return Err(rejected(format!(
                        "docket did not accept the team {pod:?}: {}",
                        reply.error()
                    )));
                }
                Err(why) => {
                    tracing::warn!(why, "docket could not be reached to ensure the team");
                    return Err(HarnessError::Process);
                }
            }
        }

        let api = probe::status(&self.client).await.map_err(rejected)?.api;
        let team = self.read_team(&pod).await;
        // A team in a folder has no commit to start from and no install to make: docket refuses
        // the task that names either, so neither the base nor the rework branch is worked out.
        let in_repository = in_repository(team.as_ref());
        // What the team says it can do, read now and not from the last probe: a need it cannot
        // meet ends the run here, before the team spends a token to ask a person for it.
        let (network, members, contract, web_read) = match team {
            Some(mut report) => {
                let listed = self.client.has_feature(POD_CONTRACT).await;
                let contract = probe::contract(&mut report, listed);
                let web_read =
                    probe::web_read(&mut report, self.client.has_feature(WEB_READ).await);
                (
                    probe::network(report.network.as_ref()),
                    report
                        .members
                        .map(|members| members.into_iter().map(probe::member).collect::<Vec<_>>()),
                    contract,
                    web_read,
                )
            }
            None => (None, None, probe::Contract::default(), None),
        };
        let facts = TeamFacts {
            network,
            members: members.as_deref(),
            delivers: contract.delivers,
            methods: contract.methods.as_deref(),
            needs: contract.needs.as_deref(),
            web_read: web_read.as_ref(),
        };
        let misfits = team_misfits(
            facts,
            &TeamNeeds::of(
                super::profile_kind(request),
                request.permission_policy.network,
                task::method(request),
            ),
        );
        if !misfits.is_empty() {
            let pod_fetch = self.client.has_feature(POD_FETCH).await;
            return Err(rejected(misfit_sentence(&pod, &misfits, pod_fetch, &facts)));
        }
        // The roles that propose, named in the task so no other role calls the tool; a team whose
        // members say nothing of it is told as before, naming none.
        let proposers = proposers(members.as_deref());
        // A thread on a team whose docket lists `conversation` is one task that continues turn by
        // turn: the first turn sends it, each later one answers its `turn` escalation.
        let note = TurnNote::of(&request.metadata);
        let conversing = note.is_some() && self.client.has_feature(CONVERSATION).await;
        let mut notice = None;
        let mut continued = None;
        if conversing && let Some(note) = note.as_ref().filter(|note| note.turn > 1) {
            match self.answer_turn(&pod, note, &attempt_id).await? {
                Turn::Answered(task) => continued = Some(task),
                Turn::Starts(why) => notice = Some(why),
            }
        }
        let (task, refused, converse) = match continued {
            Some(task) => (
                task,
                None,
                Some(ConverseMode::Continues.capability(CONTINUES)),
            ),
            None => {
                let base_ref = if in_repository {
                    Some(
                        match request.metadata.get("rework_base").and_then(Value::as_str) {
                            Some(branch) if !branch.is_empty() => branch.to_owned(),
                            _ => Self::base_commit(spec).await,
                        },
                    )
                } else {
                    None
                };
                let mut body = task::body(
                    request,
                    &attempt_id,
                    spec.work.lease.attempt_number,
                    base_ref.as_deref(),
                    request.autonomy(),
                    proposers.as_deref(),
                );
                let max_turns = note
                    .as_ref()
                    .filter(|_| conversing)
                    .map(|note| note.max_turns.clamp(1, MAX_CONVERSATION_TURNS));
                if let Some(max_turns) = max_turns {
                    body["conversation"] = json!({"maxTurns": max_turns});
                }
                let mut converse = max_turns.map(|_| ConverseMode::Continues.capability(CONTINUES));
                let mut replayed = false;
                let mut reply = self.send_task(&pod, &body).await?;
                // A team whose method has several steps cannot continue: nothing was enqueued, so
                // the same task goes again as today's replay, and the run says why.
                if max_turns.is_some()
                    && reply.status == 400
                    && reply.body["code"] == json!(CONVERSATION_NEEDS_ONE_STEP)
                {
                    converse = Some(ConverseMode::Replays.capability(&format!(
                        "{REPLAYS}; docket would not continue this team's task: {}",
                        reply.error()
                    )));
                    replayed = true;
                    // A team that replays has no conversation to restart.
                    notice = None;
                    if let Some(object) = body.as_object_mut() {
                        object.remove("conversation");
                    }
                    reply = self.send_task(&pod, &body).await?;
                }
                let (task, refused) = match reply.status {
                    200 | 201 => match reply.read::<Enqueued>() {
                        Ok(enqueued) => (enqueued.task, None),
                        Err(why) => {
                            tracing::warn!(why, "docket's answer to the task names no task");
                            return Err(HarnessError::Process);
                        }
                    },
                    // The task is never retried: the attempt fails with docket's words.
                    400 | 422 => (format!("rejected-{attempt_id}"), Some(reply.error())),
                    401 => return Err(rejected("docket refused the runner's token".to_owned())),
                    _ => {
                        return Err(rejected(format!(
                            "docket refused the task: {}",
                            reply.error()
                        )));
                    }
                };
                if let Some(note) = note
                    .as_ref()
                    .filter(|_| max_turns.is_some() && !replayed && refused.is_none())
                {
                    self.write_thread(&note.id, &ThreadTask::new(&pod, &task));
                }
                (task, refused, converse)
            }
        };

        Ok(self
            .register(RunInfo {
                pod,
                task,
                attempt_id,
                workspace: spec.workspace.path.clone(),
                started_at: Utc::now(),
                timeout: Duration::from_secs(request.timeout_seconds),
                refused,
                api: Some(api),
                in_repository,
                turn: note.is_some(),
                baseline_usage: None,
                converse,
                thread: note.map(|note| note.id),
                notice,
                secrets: self.run_secrets(&spec.work),
            })
            .await)
    }

    async fn decision_channels(
        &self,
        handle: &LocalRunHandle,
    ) -> Option<(mpsc::Receiver<Question>, mpsc::Sender<DecisionAnswer>)> {
        self.live
            .lock()
            .await
            .get_mut(&handle.process_id)?
            .decisions
            .take()
    }

    async fn sandbox(&self, _handle: &LocalRunHandle) -> Option<String> {
        Some("harness".to_owned())
    }

    async fn progress_channel(&self, handle: &LocalRunHandle) -> Option<mpsc::Receiver<Progress>> {
        self.live
            .lock()
            .await
            .get_mut(&handle.process_id)?
            .progress
            .take()
    }

    async fn answered_elsewhere_channel(
        &self,
        handle: &LocalRunHandle,
    ) -> Option<mpsc::Receiver<AnsweredElsewhere>> {
        self.live
            .lock()
            .await
            .get_mut(&handle.process_id)?
            .elsewhere
            .take()
    }

    async fn cancel(&self, handle: &LocalRunHandle) -> Result<CancellationEvidence, HarnessError> {
        let info = self
            .run_info(&handle.process_id)
            .await
            .ok_or(HarnessError::Process)?;
        let observation = if info.refused.is_some() {
            CancelObservation::AlreadyTerminal
        } else {
            match self
                .client
                .cancel_task(&info.pod, &info.task, "cancelled from Tack")
                .await
            {
                Ok(reply) => cancel_observation(reply.status, &reply.body),
                Err(_) => CancelObservation::Ambiguous,
            }
        };
        Ok(CancellationEvidence {
            observation,
            observed_at: Timestamp::new(
                Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            ),
            details: serde_json::Map::from_iter([
                ("team".to_owned(), json!(info.pod)),
                ("task".to_owned(), json!(info.task)),
            ]),
        })
    }

    async fn wait(&self, handle: &LocalRunHandle) -> Result<HarnessOutcome, HarnessError> {
        let (info, side) = {
            let mut live = self.live.lock().await;
            let entry = live
                .get_mut(&handle.process_id)
                .ok_or(HarnessError::Process)?;
            (
                Arc::clone(&entry.info),
                entry.wait.take().ok_or(HarnessError::Process)?,
            )
        };
        let outcome = self.follow(&info, side).await;
        self.live.lock().await.remove(&handle.process_id);
        outcome
    }

    async fn reconcile(
        &self,
        journal: &AttemptJournal,
    ) -> Result<RecoveryObservation, HarnessError> {
        let Some(process_id) = journal.process_id.as_deref() else {
            return Ok(RecoveryObservation::ProcessStopped);
        };
        let Some((pod, task)) = process_id.split_once('/') else {
            return Err(HarnessError::RecoveryUnavailable);
        };
        if task.starts_with("rejected-") {
            return Ok(RecoveryObservation::ProcessStopped);
        }
        // A docket restarting beside this runner is out of reach for a few seconds, and the task it
        // holds is intact: the first record to find it unreachable waits for the daemon for all of
        // them, then every record reads once. An answer that is not a task is never waited for.
        let mut reply = self.client.task(pod, task).await;
        if reply.is_err() {
            self.recovery_waited
                .get_or_init(|| self.client.wait_until_healthy(self.recovery_wait))
                .await;
            reply = self.client.task(pod, task).await;
        }
        Ok(match reply {
            Ok(reply) if reply.status == 200 => match reply.read::<TaskView>() {
                Ok(view) if terminal(&view.status) => RecoveryObservation::ProcessStopped,
                Ok(_) => RecoveryObservation::ProcessRunning,
                Err(why) => {
                    tracing::warn!(%why, pod, task, "docket's task could not be read; recovery cannot tell if it runs");
                    RecoveryObservation::Ambiguous
                }
            },
            Ok(reply) => {
                tracing::warn!(
                    status = reply.status,
                    pod,
                    task,
                    "docket answered recovery's task read with a status other than 200"
                );
                RecoveryObservation::Ambiguous
            }
            Err(why) => {
                tracing::warn!(%why, pod, task, "docket could not be reached for recovery's task read");
                RecoveryObservation::Ambiguous
            }
        })
    }

    /// The task is in docket's daemon, not in a child of the runner, so it
    /// outlives a restart: the handle is rebuilt from the journal's `pod/task`
    /// and the claim, and `follow` reads the cursor it persisted. The attempt's
    /// time starts over here, from the claim's own timeout.
    async fn resume(
        &self,
        spec: &ExecutionSpec,
        record: &AttemptJournal,
    ) -> Result<LocalRunHandle, HarnessError> {
        let (pod, task) = record
            .process_id
            .as_deref()
            .and_then(|process_id| process_id.split_once('/'))
            .filter(|(_, task)| !task.starts_with("rejected-"))
            .ok_or(HarnessError::RecoveryUnavailable)?;
        let api = probe::status(&self.client)
            .await
            .map(|status| status.api)
            .ok();
        let in_repository = self.has_repository(pod).await;
        Ok(self
            .register(RunInfo {
                pod: pod.to_owned(),
                task: task.to_owned(),
                attempt_id: record.attempt_id.as_str().to_owned(),
                workspace: record.workspace.path.clone(),
                started_at: Utc::now(),
                timeout: Duration::from_secs(spec.work.request.timeout_seconds),
                refused: None,
                api,
                in_repository,
                turn: TurnNote::of(&spec.work.request.metadata).is_some(),
                // A restart during a continuation (decision 6's "Restarts") still has to count
                // only the new hops: the baseline travels in the request's own metadata, the same
                // way `start_continuation` reads it the first time.
                baseline_usage: baseline_usage_of(&spec.work.request.metadata),
                converse: None,
                thread: TurnNote::of(&spec.work.request.metadata).map(|note| note.id),
                notice: None,
                secrets: self.run_secrets(&spec.work),
            })
            .await)
    }
}

/// Where a thread's docket task is, between the turns that continue it.
struct ThreadTask {
    pod: String,
    task: String,
    /// Where the trace and its finished hops stood when the last turn ended, so the next turn
    /// shows only its own.
    cursor: String,
    hops: usize,
}

impl ThreadTask {
    fn new(pod: &str, task: &str) -> Self {
        Self {
            pod: pod.to_owned(),
            task: task.to_owned(),
            cursor: String::new(),
            hops: 0,
        }
    }
}

/// What a thread's later turn found of its conversation.
enum Turn {
    /// The message answered the task's `turn` escalation; this is the task.
    Answered(String),
    /// There was no task to continue; a new one starts, and the sentence says why.
    Starts(String),
}

impl DocketAdapter {
    /// Where a thread's docket task is kept, beside the attempts' progress.
    fn thread_path(&self, thread: &str) -> PathBuf {
        let safe: String = thread
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        self.state_dir
            .join("docket")
            .join(format!("thread-{safe}.json"))
    }

    fn read_thread(&self, thread: &str) -> Option<ThreadTask> {
        let value: Value =
            serde_json::from_slice(&std::fs::read(self.thread_path(thread)).ok()?).ok()?;
        Some(ThreadTask {
            pod: value["pod"].as_str()?.to_owned(),
            task: value["task"].as_str()?.to_owned(),
            cursor: value["cursor"].as_str().unwrap_or_default().to_owned(),
            hops: usize::try_from(value["hops"].as_u64().unwrap_or(0)).unwrap_or(0),
        })
    }

    fn write_thread(&self, thread: &str, kept: &ThreadTask) {
        let path = self.thread_path(thread);
        let value = json!({"pod": kept.pod, "task": kept.task,
                           "cursor": kept.cursor, "hops": kept.hops});
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| {
                let partial = path.with_extension("json.partial");
                std::fs::write(&partial, value.to_string())?;
                std::fs::rename(&partial, &path)
            });
        if written.is_err() {
            tracing::warn!(
                harness = DESCRIPTOR.kind,
                "the thread's task could not be saved"
            );
        }
    }

    /// A turn ended on the conversation's `turn` escalation: where the trace stands is kept for the
    /// thread's next turn.
    fn keep_thread_position(&self, info: &RunInfo, relay: &TraceRelay) {
        let Some(thread) = &info.thread else { return };
        if let Some(mut kept) = self.read_thread(thread)
            && kept.task == info.task
        {
            kept.cursor.clone_from(&relay.cursor);
            kept.hops = relay.hops_finished;
            self.write_thread(thread, &kept);
        }
    }

    /// `POST /tasks/<pod>`.
    async fn send_task(&self, pod: &str, body: &Value) -> Result<client::Reply, HarnessError> {
        self.client
            .enqueue_task(pod, body.clone())
            .await
            .map_err(|why| {
                tracing::warn!(why, "docket could not be reached to post the task");
                HarnessError::Process
            })
    }

    /// A later turn of a thread whose team continues: the message answers the pending `turn`
    /// escalation of the thread's task. Without a task to answer (the runner never held one, the
    /// conversation ended at its cap or was closed, the task was cancelled) a new one starts and
    /// the turn says so.
    async fn answer_turn(
        &self,
        pod: &str,
        note: &TurnNote,
        attempt_id: &str,
    ) -> Result<Turn, HarnessError> {
        let starts = |why: &str| {
            Ok(Turn::Starts(format!(
                "This thread's docket task {why}, so a new conversation starts from the thread so far."
            )))
        };
        let Some(kept) = self.read_thread(&note.id).filter(|kept| kept.pod == pod) else {
            return starts("is not known to this runner");
        };
        let reply = self
            .client
            .escalations(0, Some(&kept.task))
            .await
            .map_err(|why| {
                tracing::warn!(why, "docket could not be reached to read the thread's task");
                HarnessError::Process
            })?;
        let waiting = (reply.status == 200)
            .then(|| reply.read::<EscalationList>().ok())
            .flatten()
            .and_then(|list| {
                list.escalations
                    .into_iter()
                    .rev()
                    .filter_map(|raw| serde_json::from_value::<Escalation>(raw).ok())
                    .find(|record| {
                        record.kind == TURN_KIND && matches!(record.state, EscalationState::Pending)
                    })
            });
        let Some(waiting) = waiting else {
            return starts("is no longer waiting for a message");
        };
        let body = json!({"optionId": "reply", "content": {"text": note.message}, "actor": "tack"});
        match self.client.resolve(&waiting.id, body).await {
            Ok(reply) if reply.status == 200 => {}
            // Settled between the read and the answer: the conversation ended.
            Ok(reply) if matches!(reply.status, 404 | 409) => {
                return starts("ended as the message was sent");
            }
            Ok(reply) => {
                return Err(rejected(format!(
                    "docket did not take the message: {}",
                    reply.error()
                )));
            }
            Err(why) => {
                tracing::warn!(why, "docket could not be reached to answer the turn");
                return Err(HarnessError::Process);
            }
        }
        self.write_state(attempt_id, "cursor", &kept.cursor);
        self.write_state(attempt_id, "hops", &kept.hops.to_string());
        Ok(Turn::Answered(kept.task))
    }
}

/// The `input`/`output` a continuation's own metadata carries under `continue_baseline_usage`
/// (`{"input": …, "output": …}`), when the request has one. Absent for every request that is
/// not a continuation.
fn baseline_usage_of(metadata: &Value) -> Option<(u64, u64)> {
    let usage = metadata.get("continue_baseline_usage")?;
    Some((
        usage.get("input")?.as_u64()?,
        usage.get("output")?.as_u64()?,
    ))
}

impl DocketAdapter {
    /// A request that continues a step another run's task already stopped at (decision 6, Phase
    /// 87), instead of posting a new task: `process_id` is that run's own `pod/task`, the same
    /// shape `register` keys a live run by. Needs both `retryContinues` (the route itself) and
    /// `budgetOutcome` (without it docket cannot tell a step's own budget stop from a confused
    /// model, so the work it kept is not there to continue either); missing either rejects the
    /// request before docket is asked to retry anything, the same way an unknown team does. A
    /// `409` means the retry was already sent — by this run under a fencing token that never
    /// reached completion, or by another one entirely — and the task is followed as it is, never
    /// retried a second time.
    async fn start_continuation(
        &self,
        spec: &ExecutionSpec,
        process_id: &str,
    ) -> Result<LocalRunHandle, HarnessError> {
        let request = &spec.work.request;
        let attempt_id = spec.work.lease.attempt_id.as_str().to_owned();
        let (pod, task) = process_id
            .split_once('/')
            .filter(|(pod, task)| !pod.is_empty() && !task.is_empty())
            .ok_or_else(|| {
                rejected(format!(
                    "continue_task {process_id:?} does not name a team and a task"
                ))
            })?;
        if !self.client.has_feature("retryContinues").await
            || !self.client.has_feature("budgetOutcome").await
        {
            return Err(rejected(
                "This team's docket cannot continue a step. Update docket.".to_owned(),
            ));
        }
        let reply = self.client.retry_task(pod, task).await.map_err(|why| {
            tracing::warn!(why, "docket could not be reached to retry the task");
            HarnessError::Process
        })?;
        match reply.status {
            // `200`/`201`: the retry was taken. `409`: it already had been — the task is followed
            // either way, never retried again.
            200 | 201 | 409 => {}
            401 => return Err(rejected("docket refused the runner's token".to_owned())),
            _ => {
                return Err(rejected(format!(
                    "docket refused the retry: {}",
                    reply.error()
                )));
            }
        }
        let api = probe::status(&self.client).await.map_err(rejected)?.api;
        let in_repository = self.has_repository(pod).await;
        Ok(self
            .register(RunInfo {
                pod: pod.to_owned(),
                task: task.to_owned(),
                attempt_id,
                workspace: spec.workspace.path.clone(),
                started_at: Utc::now(),
                timeout: Duration::from_secs(request.timeout_seconds),
                refused: None,
                api: Some(api),
                in_repository,
                turn: TurnNote::of(&request.metadata).is_some(),
                baseline_usage: baseline_usage_of(&request.metadata),
                converse: None,
                thread: TurnNote::of(&request.metadata).map(|note| note.id),
                notice: None,
                secrets: self.run_secrets(&spec.work),
            })
            .await)
    }
}

/// How long recovery waits, once, for a docket it cannot reach: long enough for a daemon that
/// restarts beside the runner, the same wait the runner gives a docket it starts itself.
const RECOVERY_WAIT: Duration = Duration::from_secs(30);

/// Where Tack is in docket's trace of one task: the cursor docket gave, the model the last
/// implementer hop was served by, and how many finished hops have reached the timeline.
struct TraceRelay {
    cursor: String,
    served_model: Option<String>,
    hops_finished: usize,
    /// Escalations whose events say nothing on the timeline: the conversation's `turn` records.
    quiet: HashSet<String>,
}

impl TraceRelay {
    fn new(cursor: String, hops_finished: usize) -> Self {
        Self {
            cursor,
            served_model: None,
            hops_finished,
            quiet: HashSet::new(),
        }
    }
}

impl DocketAdapter {
    /// Reads the trace past the relay's cursor and puts the task's hops and escalation events on
    /// the timeline.
    async fn relay_trace(
        &self,
        info: &RunInfo,
        relay: &mut TraceRelay,
        labels: &OptionLabels,
        progress: &mpsc::Sender<Progress>,
    ) {
        if let Ok(reply) = self.client.traces(&info.pod, &relay.cursor).await
            && reply.status == 200
            && let Ok(page) = reply.read::<TracePage>()
        {
            for line in &page.events {
                let event = match TraceEvent::parse(line) {
                    Ok(event) => event,
                    Err(why) => {
                        tracing::warn!(%why, "a trace event could not be read");
                        continue;
                    }
                };
                if event.task() != Some(info.task.as_str()) {
                    continue;
                }
                if let TraceEvent::HopFinished(hop) = &event {
                    relay.hops_finished += 1;
                    if hop.role == "implementer"
                        && let Some(model) = hop.model.as_deref().filter(|m| !m.is_empty())
                    {
                        relay.served_model = Some(model.to_owned());
                    }
                }
                let step = match &event {
                    TraceEvent::HopStarted(hop) => Some(hop_progress(false, hop)),
                    TraceEvent::HopFinished(hop) => Some(hop_progress(true, hop)),
                    _ if event
                        .escalation()
                        .is_some_and(|id| relay.quiet.contains(id)) =>
                    {
                        None
                    }
                    other => escalation_progress(other, labels),
                };
                if let Some(step) = step
                    && progress.try_send(step).is_err()
                {
                    tracing::warn!(harness = DESCRIPTOR.kind, "a progress line was dropped");
                }
            }
            if let Some(next) = page
                .next
                .as_deref()
                .filter(|n| !n.is_empty() && *n != relay.cursor)
            {
                self.write_state(&info.attempt_id, "cursor", next);
                relay.cursor = next.to_owned();
                // After the cursor: a crash between the two leaves a count too low, which only
                // costs a wait, never one too high.
                self.write_state(&info.attempt_id, "hops", &relay.hops_finished.to_string());
            }
        }
    }

    /// Reads the trace again after the task is seen ended, until every hop its record holds has
    /// shown its end on the timeline. The newest events are held back by docket, so the last
    /// hop's end can come a moment after the task's status; the wait is bounded by
    /// [`TRACE_SETTLE`], which also covers a hop whose end docket never traced.
    async fn settle_trace(
        &self,
        info: &RunInfo,
        task: &TaskView,
        relay: &mut TraceRelay,
        labels: &OptionLabels,
        progress: &mpsc::Sender<Progress>,
    ) {
        let until = tokio::time::Instant::now() + TRACE_SETTLE;
        loop {
            if relay.hops_finished >= task.hops.len() || tokio::time::Instant::now() >= until {
                return;
            }
            tokio::time::sleep(TRACE_SETTLE_POLL).await;
            self.relay_trace(info, relay, labels, progress).await;
        }
    }
}

impl DocketAdapter {
    /// What to tell a person whose task waits on `pod`, when docket says the team is paused: why
    /// and the command that resumes it, as no route of docket's does. `None` when the team runs
    /// or docket's answer cannot be read.
    async fn paused_team_line(&self, pod: &str) -> Option<String> {
        let reply = self
            .client
            .pod(pod)
            .await
            .ok()
            .filter(|r| r.status == 200)?;
        let report: wire::PodReport = reply.read().ok()?;
        if report.paused != Some(true) {
            return None;
        }
        let why = match report.paused_reason.as_deref() {
            Some("budget") => "its budget is spent",
            _ => "docket paused it",
        };
        Some(format!(
            "The team is paused ({why}), so this task waits. \
             Resume it with `docket run --pod {pod} --resume`."
        ))
    }

    /// Follows one task until it ends: trace page, inbox, record, then wait
    /// for the next poll or for the person's answer, whichever comes first.
    async fn follow(
        &self,
        info: &RunInfo,
        mut side: WaitSide,
    ) -> Result<HarnessOutcome, HarnessError> {
        if let Some(error) = &info.refused {
            let _ = side.progress.try_send(Progress {
                kind: event_kind::AGENT_REFUSED.to_owned(),
                text: error.clone(),
                usage: None,
            });
            return Ok(self
                .outcome(
                    info,
                    AttemptState::Failed,
                    json!({"code": "rejected", "message": error}),
                    None,
                    None,
                )
                .await);
        }
        if let Some(notice) = &info.notice {
            let _ = side.progress.try_send(Progress {
                kind: event_kind::AGENT_CONVERSATION_RESTARTED.to_owned(),
                text: notice.clone(),
                usage: None,
            });
        }
        // The attempt's clock bounds the agent's work; a task docket holds for a person is not
        // working, so the time between two polls that both saw it waiting is not counted.
        let mut deadline = tokio::time::Instant::now() + info.timeout;
        let mut last_poll = tokio::time::Instant::now();
        let mut was_waiting = false;
        let mut relay = TraceRelay::new(
            self.read_cursor(&info.attempt_id),
            self.read_hops_finished(&info.attempt_id),
        );
        // The escalations before this `seq` are settled, so docket is not asked for them again.
        let mut since = self.read_since(&info.attempt_id);
        // docket's ids of what was asked, and how often docket refused the answer.
        let mut asked: HashSet<String> = HashSet::new();
        let mut refused: HashMap<String, u32> = HashMap::new();
        let mut pending: Option<Pending> = None;
        // The id the held question was put to Tack under, which a refused answer suffixes.
        let mut held_as = String::new();
        // The question last put to the person, when, and whether an answer went back to docket.
        let mut last_asked: Option<(String, DateTime<Utc>, bool)> = None;
        // What docket called the options it offered, learnt from the escalations read before the
        // events that name them.
        let mut labels = OptionLabels::new();
        // Whether the person was told the team is paused, and whether this docket can say so.
        let mut pause_told = false;
        let mut pause_listed: Option<bool> = None;
        loop {
            // Whatever needs a person.
            let mut expired_in_docket = false;
            let mut settled_in_docket = false;
            let mut parked = false;
            if let Some(read) = self
                .read_escalations(info, since, &asked, pending.as_ref())
                .await
            {
                expired_in_docket = read.expired;
                settled_in_docket = read.settled;
                parked = read.parked;
                relay.quiet.extend(read.turns);
                labels.extend(read.labels);
                for (id, sentence) in read.unaskable {
                    let _ = side.progress.try_send(Progress {
                        kind: event_kind::ESCALATION_UNREADABLE.to_owned(),
                        text: sentence,
                        usage: None,
                    });
                    asked.insert(id);
                }
                for (id, until) in read.waits {
                    let _ = side.progress.try_send(Progress {
                        kind: event_kind::ESCALATION_WAITING.to_owned(),
                        text: wait_line(until, Utc::now()),
                        usage: None,
                    });
                    asked.insert(id);
                }
                if let Some((mut question, ask)) = read.next {
                    // A question asked again is a new decision: Tack keys a decision
                    // by the id, and the refused one is already resolved there.
                    if let Some(times) = refused.get(&ask.escalation) {
                        question.vendor_id = format!("{}#{times}", question.vendor_id);
                    }
                    let prompt = question.prompt.clone();
                    let vendor_id = question.vendor_id.clone();
                    if side.questions.try_send(question).is_ok() {
                        held_as = vendor_id;
                        asked.insert(ask.escalation.clone());
                        pending = Some(ask);
                        last_asked = Some((prompt, Utc::now(), false));
                    }
                }
                if read.settled_through > since {
                    since = read.settled_through;
                    self.write_state(&info.attempt_id, "since", &since.to_string());
                }
            }

            // A question put to a person that was answered in docket's own tool: the board's
            // decision for it is closed, so the next question can be asked. Only a docket that
            // stamps a new `seq` on a settled record (`settledCursor`) is read this way.
            if settled_in_docket
                && pending.is_some()
                && self.client.has_feature("settledCursor").await
                && side
                    .elsewhere
                    .try_send(AnsweredElsewhere {
                        vendor_id: held_as.clone(),
                        source: "docket".to_owned(),
                    })
                    .is_ok()
            {
                let asked_text = last_asked
                    .as_ref()
                    .map_or("a question", |(prompt, ..)| prompt);
                let _ = side.progress.try_send(Progress {
                    kind: event_kind::ESCALATION_ANSWERED_ELSEWHERE.to_owned(),
                    text: format!("answered in docket, not in Tack: {asked_text}"),
                    usage: None,
                });
                pending = None;
            }

            // The hops and escalation events since the cursor.
            self.relay_trace(info, &mut relay, &labels, &side.progress)
                .await;

            let task = match self.client.task(&info.pod, &info.task).await {
                Ok(reply) if reply.status == 200 => reply
                    .read::<TaskView>()
                    .inspect_err(|why| tracing::warn!(why, "the task could not be read"))
                    .ok(),
                Ok(reply) => {
                    tracing::warn!(status = reply.status, "the task could not be read");
                    None
                }
                Err(why) => {
                    tracing::warn!(why, "docket could not be reached");
                    None
                }
            };
            if let Some(view) = task.as_ref().filter(|view| terminal(&view.status)) {
                // docket holds back the newest second of its trace, so the last hop's end may
                // not be readable yet; it is waited for before the attempt ends.
                self.settle_trace(info, view, &mut relay, &labels, &side.progress)
                    .await;
                let model = relay.served_model.take().or_else(|| {
                    view.hops
                        .iter()
                        .rev()
                        .find(|hop| hop.role.as_deref() == Some("implementer"))
                        .and_then(|hop| hop.model.clone())
                });
                return Ok(self
                    .finish(
                        info,
                        view,
                        model,
                        &side.progress,
                        pending.as_ref().and_then(|held| held.expires_at),
                        last_asked.as_ref(),
                    )
                    .await);
            }
            // A conversation's turn is over when its task waits on the `turn` escalation: the
            // attempt ends with the turn's reply, and the task stays for the thread's next message.
            if parked
                && let Some(view) = task.as_ref().filter(|view| view.status == "waiting_input")
            {
                self.settle_trace(info, view, &mut relay, &labels, &side.progress)
                    .await;
                self.keep_thread_position(info, &relay);
                let model = relay.served_model.take();
                let mut ended = view.clone();
                ended.status = "done".to_owned();
                return Ok(self
                    .finish(
                        info,
                        &ended,
                        model,
                        &side.progress,
                        None,
                        last_asked.as_ref(),
                    )
                    .await);
            }
            // A task docket holds `pending` while its team is paused: the attempt runs and nothing
            // moves. Said once; the team is read only from a docket that lists `podPause`.
            if !pause_told && task.as_ref().is_some_and(|view| view.status == "pending") {
                if pause_listed.is_none() {
                    pause_listed = Some(self.client.has_feature(POD_PAUSE).await);
                }
                if pause_listed == Some(true)
                    && let Some(line) = self.paused_team_line(&info.pod).await
                {
                    let _ = side.progress.try_send(Progress {
                        kind: event_kind::AGENT_TEAM_PAUSED.to_owned(),
                        text: line,
                        usage: None,
                    });
                    pause_told = true;
                }
            }
            let now = tokio::time::Instant::now();
            let waiting = task
                .as_ref()
                .is_some_and(|view| waiting_on_a_person(&view.status));
            if waiting && was_waiting {
                deadline += now - last_poll;
            }
            (last_poll, was_waiting) = (now, waiting);
            // Still waiting after the escalation's own expiry, which docket's sweep has not
            // resolved, or docket itself has marked it expired: nobody answered. With no
            // `expiresAt` only an answer, docket or Stop ends the wait.
            let held_expiry = pending.as_ref().and_then(|held| held.expires_at);
            if expired_in_docket
                || (waiting && held_expiry.is_some_and(|expired| expired <= Utc::now()))
            {
                let _ = self
                    .client
                    .cancel_task(
                        &info.pod,
                        &info.task,
                        "nobody answered before the question expired",
                    )
                    .await;
                return Ok(self
                    .outcome(
                        info,
                        AttemptState::Failed,
                        json!({"code": "not_answered",
                           "message": super::not_answered(held_expiry),
                           "task": info.task}),
                        task.as_ref(),
                        relay.served_model.take(),
                    )
                    .await);
            }
            if now >= deadline {
                let _ = self
                    .client
                    .cancel_task(&info.pod, &info.task, "the attempt's time ran out")
                    .await;
                let message = super::time_ran_out(info.timeout, pending.is_some());
                return Ok(self
                    .outcome(
                        info,
                        AttemptState::Failed,
                        json!({"code": "timeout", "message": message, "task": info.task}),
                        task.as_ref(),
                        relay.served_model.take(),
                    )
                    .await);
            }

            tokio::select! {
                () = tokio::time::sleep(POLL_INTERVAL) => {}
                answer = side.answers.recv(), if pending.is_some() => {
                    if let (Some(answer), Some(ask)) = (answer, pending.take()) {
                        if answer.unanswered {
                            // Tack's own deadline passed, or the decision could not be kept:
                            // nobody answered. docket must not read that as a refusal, so
                            // nothing is posted; the task is cancelled instead.
                            let _ = self
                                .client
                                .cancel_task(
                                    &info.pod,
                                    &info.task,
                                    "nobody answered before the question expired",
                                )
                                .await;
                            return Ok(self.outcome(
                                info,
                                AttemptState::Failed,
                                json!({"code": "not_answered",
                                       "message": unanswered_text(last_asked.as_ref(), ask.expires_at),
                                       "task": info.task}),
                                task.as_ref(),
                                relay.served_model.take(),
                            ).await);
                        }
                        if let Some(problem) = self.deliver(&ask, &answer).await {
                            // The decision is resolved in Tack, so the run would wait on
                            // nothing: ask again while docket still holds the question.
                            asked.remove(&ask.escalation);
                            *refused.entry(ask.escalation.clone()).or_default() += 1;
                            let _ = side.progress.try_send(Progress {
                                kind: event_kind::AGENT_ANSWER_FAILED.to_owned(),
                                text: format!("{problem}; asking again"),
                                usage: None,
                            });
                        } else if let Some(last) = last_asked.as_mut() {
                            last.2 = true;
                        }
                    }
                }
            }
        }
    }
}

/// What docket's answer to a cancel says happened. A `409` is a task that already ended unless it
/// carries `code: "not_cancellable"` (a run that refused to stop), which nobody can call settled.
fn cancel_observation(status: u16, body: &Value) -> CancelObservation {
    match status {
        200 => CancelObservation::ProcessStopped,
        409 if body["code"] == json!("not_cancellable") => CancelObservation::Ambiguous,
        409 => CancelObservation::AlreadyTerminal,
        _ => CancelObservation::Ambiguous,
    }
}

#[cfg(test)]
mod tests;
