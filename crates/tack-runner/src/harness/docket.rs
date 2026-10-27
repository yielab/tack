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
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use tack_protocol::event_kind;
use tack_protocol::execution::{
    ActualExecution, CapabilitySupport, FeatureCapabilities, HarnessCapability,
    HarnessKind as DomainHarnessKind, Measurement, MeasurementSource, Usage,
    WorkspaceId as DomainWorkspaceId,
};
use tokio::sync::{Mutex, mpsc};

use crate::{
    client::{AttemptState, Recommendation, Timestamp},
    config::DocketConfig,
    harness::{
        AttemptJournal, CancelObservation, CancellationEvidence, DecisionAnswer, DecisionOption,
        ExecutionSpec, HarnessAdapter, HarnessError, HarnessOutcome, HarnessProbe, LocalRunHandle,
        ModelSource, Progress, Question, RecoveryObservation,
        local_process::{HarnessDescriptor, ModelSelection, capability, policy_capability},
    },
};

mod client;
pub mod daemon;
mod probe;
mod task;
mod wire;

use client::Client;
use wire::{
    Enqueued, Escalation, EscalationEvent, EscalationList, EscalationState, HopEvent, TaskDiff,
    TaskUsage, TaskView, Tokens, TraceEvent, TracePage,
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
    readiness: None,
    models_docs: "https://github.com/yielab/docket/blob/develop/docs/CONFIGURATION.md#31-choose-which-model-each-agent-runs-on",
    sign_in_docs: "https://github.com/yielab/docket/blob/develop/docs/MODEL-GATEWAYS.md",
};

/// How often a running task's trace, inbox and record are read.
const POLL_INTERVAL: Duration = Duration::from_secs(2);

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

/// What docket offers over its API.
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
}

/// The ends of the channels `wait` drives.
struct WaitSide {
    questions: mpsc::Sender<Question>,
    answers: mpsc::Receiver<DecisionAnswer>,
    progress: mpsc::Sender<Progress>,
}

struct Live {
    info: Arc<RunInfo>,
    decisions: Option<(mpsc::Receiver<Question>, mpsc::Sender<DecisionAnswer>)>,
    progress: Option<mpsc::Receiver<Progress>>,
    wait: Option<WaitSide>,
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
        }
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
        spec.work
            .request
            .resolved_agent_profile
            .tool_policy
            .get("docket")
            .and_then(|docket| docket.get("team"))
            .and_then(Value::as_str)
            .filter(|team| !team.is_empty())
            .map(str::to_owned)
    }

    /// The pod a task goes to: the profile's team, else the project's default
    /// pod, named for the repository folder.
    fn team(spec: &ExecutionSpec) -> Result<String, HarnessError> {
        if let Some(team) = Self::named_team(spec) {
            return Ok(team);
        }
        let repository = spec
            .work
            .workspace_repository()
            .map_err(|_| rejected("the claim's repository facts disagree".to_owned()))?;
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
        self.live.lock().await.insert(
            process_id.clone(),
            Live {
                info: Arc::new(info),
                decisions: Some((questions_rx, answers_tx)),
                progress: Some(progress_rx),
                wait: Some(WaitSide {
                    questions: questions_tx,
                    answers: answers_rx,
                    progress: progress_tx,
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
            .map_err(|_| "git could not be run".to_owned())?;
        let head = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if output.status.success() && !head.is_empty() {
            Ok(head)
        } else {
            Err(format!(
                "the task's branch {branch:?} is not in the repository"
            ))
        }
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

/// The question one of docket's escalations is, and what answering it posts. The options are
/// docket's own, in its order, so the last one is the way out; a question whose only option is to
/// decline but which takes text is offered an answer in text beside it. A kind Tack does not know
/// is the sentence saying so, and nothing is asked.
fn escalation_question(record: &Escalation) -> Result<(Question, Pending), String> {
    let kind = record.kind.clone();
    if !KNOWN_KINDS.contains(&kind.as_str()) {
        return Err(format!(
            "docket asked something of kind {kind:?}, which this Tack cannot put to you; answer it in docket"
        ));
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

/// What one read of the task's escalations found.
#[derive(Default)]
struct Escalations {
    /// The first one waiting for a person that has not been asked yet.
    next: Option<(Question, Pending)>,
    /// Pending ones Tack cannot put to a person, by docket's id, and the sentence saying why.
    unaskable: Vec<(String, String)>,
    /// The label of every option of every record read.
    labels: OptionLabels,
    /// The held one has expired in docket's own record.
    expired: bool,
    /// The highest `seq` below the first record still pending: where a restarted adapter may
    /// read from without missing a question.
    settled_through: u64,
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
            if matches!(record.state, EscalationState::Pending) {
                open = true;
                if held.is_none() && found.next.is_none() && !asked.contains(&record.id) {
                    match escalation_question(&record) {
                        Ok(question) => found.next = Some(question),
                        Err(sentence) => found.unaskable.push((record.id.clone(), sentence)),
                    }
                }
            } else if !open {
                found.settled_through = found.settled_through.max(record.seq);
            }
        }
        Some(found)
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

    fn outcome(
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
                capability_snapshot: capabilities(),
                // Overwritten by the engine, which owns workspace facts.
                workspace_id: DomainWorkspaceId::new(""),
                base_revision: String::new(),
                started_at: info.started_at,
                ended_at,
                additional: BTreeMap::from([("team".to_owned(), json!(info.pod))]),
            },
            usage: Usage {
                tokens_in: measurement(usage.and_then(|usage| usage.total.input)),
                tokens_out: measurement(usage.and_then(|usage| usage.total.output)),
                duration_ms: measurement(Some(elapsed)),
                cost_usd: measurement(None),
                additional,
            },
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
    ) -> HarnessOutcome {
        // docket's task record says why it ended in `reason`, for a failed and a blocked task
        // alike; its hops carry their own `error`, the task has none.
        let why = task.reason.as_deref().unwrap_or_default();
        let failure_kind = task.failure_kind.as_deref();
        let (state, reason) = match task.status.as_str() {
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
            "blocked" => (
                AttemptState::Failed,
                json!({"code": "blocked", "message": why, "task": info.task}),
            ),
            // At L5 a tool call needed approval and docket asks nobody; the task is final until
            // someone retries it, so the attempt ends here and nothing queues it again.
            _ if failure_kind == Some("needs_human") => (
                AttemptState::Failed,
                json!({"code": "needs_human",
                       "message": if why.is_empty() { NEEDS_A_PERSON } else { why },
                       "task": info.task}),
            ),
            // The install failed before any hop; docket's reason names the command and its output.
            _ if failure_kind == Some("prepare") => (
                AttemptState::Failed,
                json!({"code": "prepare",
                       "message": format!("Could not prepare the dependencies (docket): {why}"),
                       "task": info.task}),
            ),
            _ => (
                AttemptState::Failed,
                json!({"code": failure_kind.unwrap_or("failed"),
                       "message": why, "task": info.task}),
            ),
        };
        self.outcome(info, state, reason, Some(task), model)
    }
}

#[async_trait]
impl HarnessProbe for DocketAdapter {
    fn harness_kind(&self) -> DomainHarnessKind {
        DomainHarnessKind::new(DESCRIPTOR.kind)
    }

    async fn probe(&self) -> HarnessCapability {
        probe::probe(&self.client, capabilities().decisions).await
    }

    fn declared_capabilities(&self) -> FeatureCapabilities {
        capabilities()
    }
}

#[async_trait]
impl HarnessAdapter for DocketAdapter {
    async fn validate(&self, spec: &ExecutionSpec) -> Result<(), HarnessError> {
        let repository = spec
            .work
            .workspace_repository()
            .map_err(|_| rejected("the claim's repository facts disagree".to_owned()))?;
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
        let attempt_id = spec.work.lease.attempt_id.as_str().to_owned();
        let pod = Self::team(spec)?;
        let repository = spec
            .work
            .workspace_repository()
            .map_err(|_| rejected("the claim's repository facts disagree".to_owned()))?;
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
            match self.client.ensure_pod(&pod, &location).await {
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

        let base_ref = match request.metadata.get("rework_base").and_then(Value::as_str) {
            Some(branch) if !branch.is_empty() => branch.to_owned(),
            _ => Self::base_commit(spec).await,
        };
        let api = probe::status(&self.client).await.map_err(rejected)?.api;
        let body = task::body(
            request,
            &attempt_id,
            spec.work.lease.attempt_number,
            &base_ref,
            request.autonomy(),
        );
        let reply = self.client.enqueue_task(&pod, body).await.map_err(|why| {
            tracing::warn!(why, "docket could not be reached to post the task");
            HarnessError::Process
        })?;
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
                Ok(reply) if reply.status == 200 => CancelObservation::ProcessStopped,
                Ok(reply) if reply.status == 409 => CancelObservation::AlreadyTerminal,
                _ => CancelObservation::Ambiguous,
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
        Ok(match self.client.task(pod, task).await {
            Ok(reply) if reply.status == 200 => match reply.read::<TaskView>() {
                Ok(view) if terminal(&view.status) => RecoveryObservation::ProcessStopped,
                Ok(_) => RecoveryObservation::ProcessRunning,
                Err(_) => RecoveryObservation::Ambiguous,
            },
            _ => RecoveryObservation::Ambiguous,
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
            })
            .await)
    }
}

impl DocketAdapter {
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
            return Ok(self.outcome(
                info,
                AttemptState::Failed,
                json!({"code": "rejected", "message": error}),
                None,
                None,
            ));
        }
        // The attempt's clock bounds the agent's work; a task docket holds for a person is not
        // working, so the time between two polls that both saw it waiting is not counted.
        let mut deadline = tokio::time::Instant::now() + info.timeout;
        let mut last_poll = tokio::time::Instant::now();
        let mut was_waiting = false;
        let mut cursor = self.read_cursor(&info.attempt_id);
        // The escalations before this `seq` are settled, so docket is not asked for them again.
        let mut since = self.read_since(&info.attempt_id);
        let mut served_model: Option<String> = None;
        // docket's ids of what was asked, and how often docket refused the answer.
        let mut asked: HashSet<String> = HashSet::new();
        let mut refused: HashMap<String, u32> = HashMap::new();
        let mut pending: Option<Pending> = None;
        // What docket called the options it offered, learnt from the escalations read before the
        // events that name them.
        let mut labels = OptionLabels::new();
        loop {
            // Whatever needs a person.
            let mut expired_in_docket = false;
            if let Some(read) = self
                .read_escalations(info, since, &asked, pending.as_ref())
                .await
            {
                expired_in_docket = read.expired;
                labels.extend(read.labels);
                for (id, sentence) in read.unaskable {
                    let _ = side.progress.try_send(Progress {
                        kind: event_kind::ESCALATION_UNREADABLE.to_owned(),
                        text: sentence,
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
                    if side.questions.try_send(question).is_ok() {
                        asked.insert(ask.escalation.clone());
                        pending = Some(ask);
                    }
                }
                if read.settled_through > since {
                    since = read.settled_through;
                    self.write_state(&info.attempt_id, "since", &since.to_string());
                }
            }

            // The hops and escalation events since the cursor.
            if let Ok(reply) = self.client.traces(&info.pod, &cursor).await
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
                    if let TraceEvent::HopFinished(hop) = &event
                        && hop.role == "implementer"
                        && let Some(model) = hop.model.as_deref().filter(|m| !m.is_empty())
                    {
                        served_model = Some(model.to_owned());
                    }
                    let step = match &event {
                        TraceEvent::HopStarted(hop) => Some(hop_progress(false, hop)),
                        TraceEvent::HopFinished(hop) => Some(hop_progress(true, hop)),
                        other => escalation_progress(other, &labels),
                    };
                    if let Some(step) = step
                        && side.progress.try_send(step).is_err()
                    {
                        tracing::warn!(harness = DESCRIPTOR.kind, "a progress line was dropped");
                    }
                }
                if let Some(next) = page
                    .next
                    .as_deref()
                    .filter(|n| !n.is_empty() && *n != cursor)
                {
                    self.write_state(&info.attempt_id, "cursor", next);
                    cursor = next.to_owned();
                }
            }

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
                let model = served_model.or_else(|| {
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
                    )
                    .await);
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
                return Ok(self.outcome(
                    info,
                    AttemptState::Failed,
                    json!({"code": "not_answered",
                           "message": super::not_answered(held_expiry),
                           "task": info.task}),
                    task.as_ref(),
                    served_model,
                ));
            }
            if now >= deadline {
                let _ = self
                    .client
                    .cancel_task(&info.pod, &info.task, "the attempt's time ran out")
                    .await;
                let message = super::time_ran_out(info.timeout, pending.is_some());
                return Ok(self.outcome(
                    info,
                    AttemptState::Failed,
                    json!({"code": "timeout", "message": message, "task": info.task}),
                    task.as_ref(),
                    served_model,
                ));
            }

            tokio::select! {
                () = tokio::time::sleep(POLL_INTERVAL) => {}
                answer = side.answers.recv(), if pending.is_some() => {
                    if let (Some(answer), Some(ask)) = (answer, pending.take())
                        && let Some(problem) = self.deliver(&ask, &answer).await
                    {
                        // The decision is resolved in Tack, so the run would wait on
                        // nothing: ask again while docket still holds the question.
                        asked.remove(&ask.escalation);
                        *refused.entry(ask.escalation.clone()).or_default() += 1;
                        let _ = side.progress.try_send(Progress {
                            kind: event_kind::AGENT_ANSWER_FAILED.to_owned(),
                            text: format!("{problem}; asking again"),
                            usage: None,
                        });
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
