//! docket as a harness: a daemon the runner talks to over HTTP, not a process
//! it spawns. [`DocketAdapter`] implements [`HarnessAdapter`] and
//! [`HarnessProbe`] directly against docket's operator API (version 5): it
//! posts the attempt as a task to the team the profile names, follows the
//! task's hops on the trace, carries docket's approvals and questions to the
//! person, and brings the finished branch into the runner's own worktree so
//! evidence, the verifier and the push read it unchanged.
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
use tack_orch::execution::{
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

use client::Client;

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

/// Progress lines the engine has not yet taken, before more are dropped.
const PROGRESS_BUFFER: usize = 256;

/// What a person reads when an attempt fails before docket ran anything.
const WRONG_WORKSPACE: &str =
    "the docket team works on a branch of your folder; choose that under Settings → Automation";

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
            CapabilitySupport::Unsupported,
            "a runner that restarts reads the task's state and reports it; it does not \
             reattach to the hop",
        ),
        decisions: capability(
            CapabilitySupport::Supported,
            "a tool call docket's policy gates, and a question a step parks on, reach the \
             person through the inbox and are answered over docket's approvals and answer routes",
        ),
        artifacts: capability(
            CapabilitySupport::Supported,
            "the task's branch is brought into the runner's worktree, so the evidence is the \
             diff of what docket committed",
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

/// What a person's answer will be posted as.
enum Pending {
    Approval {
        token: String,
    },
    Consult {
        task: String,
        /// docket's id for the question.
        question: String,
        /// The one property the answer's text fills, and whether docket requires it.
        property: String,
        required: bool,
        /// The ids of docket's own options; none when the question has none.
        options: Vec<String>,
    },
}

impl Pending {
    /// docket's id for what is asked: the approval's token or the question's id.
    fn docket_id(&self) -> &str {
        match self {
            Self::Approval { token } => token,
            Self::Consult { question, .. } => question,
        }
    }
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

    fn cursor_path(&self, attempt_id: &str) -> PathBuf {
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
        self.state_dir.join("docket").join(format!("{safe}.cursor"))
    }

    fn read_cursor(&self, attempt_id: &str) -> String {
        std::fs::read_to_string(self.cursor_path(attempt_id))
            .map(|text| text.trim().to_owned())
            .unwrap_or_default()
    }

    /// Written after the page it follows has been delivered, through a rename
    /// so a crash leaves the old cursor or the new one, never half of one.
    fn write_cursor(&self, attempt_id: &str, cursor: &str) {
        let path = self.cursor_path(attempt_id);
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| {
                let partial = path.with_extension("cursor.partial");
                std::fs::write(&partial, cursor)?;
                std::fs::rename(&partial, &path)
            });
        if written.is_err() {
            tracing::warn!(
                harness = DESCRIPTOR.kind,
                "the trace cursor could not be saved"
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

    /// `git reset --hard` of the runner's worktree onto the task's branch.
    async fn bring_in(workspace: &Path, branch: &str) -> Result<(), String> {
        if branch.is_empty() || branch.starts_with('-') {
            return Err("docket named no usable branch for the task".to_owned());
        }
        let output = tokio::process::Command::new("git")
            .arg("-C")
            .arg(workspace)
            .args(["reset", "--hard", &format!("refs/heads/{branch}")])
            .output()
            .await
            .map_err(|_| "git could not be run".to_owned())?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "the task's branch could not be brought into the worktree: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    }

    /// The commit the runner's worktree stands on, which is where the task
    /// branch is cut; the request's own base revision when git cannot say.
    async fn base_commit(spec: &ExecutionSpec) -> String {
        let head = tokio::process::Command::new("git")
            .arg("-C")
            .arg(&spec.workspace.path)
            .args(["rev-parse", "HEAD"])
            .output()
            .await
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
            .filter(|head| !head.is_empty());
        head.unwrap_or_else(|| spec.workspace.base_revision.clone())
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
    fn of(cached: Option<u64>, written: Option<u64>) -> Self {
        Self {
            read: cached,
            write: written,
        }
    }

    /// A hop's `usage`.
    fn of_hop(usage: &Value) -> Self {
        Self::of(usage["cached"].as_u64(), usage["cacheWrite"].as_u64())
    }

    /// A task's `usage`: its own `cached`/`cacheWrite` when docket sends them, else
    /// the sums of its `byModel` entries, which carry them since docket's `da556114`.
    fn of_task(usage: &Value) -> Self {
        let sum = |key: &str| {
            usage["byModel"]
                .as_object()
                .into_iter()
                .flatten()
                .filter_map(|(_, used)| used[key].as_u64())
                .reduce(|a, b| a + b)
        };
        Self::of(
            usage["cached"].as_u64().or_else(|| sum("cached")),
            usage["cacheWrite"].as_u64().or_else(|| sum("cacheWrite")),
        )
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

fn hop_role(payload: &Value) -> String {
    let role = payload["role"].as_str().unwrap_or("agent");
    let mut chars = role.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

/// The timeline line a `hop_started` or `hop_finished` record makes.
fn hop_progress(event_type: &str, payload: &Value) -> Option<Progress> {
    let who = format!(
        "{} ({})",
        hop_role(payload),
        payload["model"].as_str().unwrap_or("no model named")
    );
    match event_type {
        "hop_started" => Some(Progress {
            kind: "agent.hop_started".to_owned(),
            text: format!("{who} started"),
            usage: None,
        }),
        "hop_finished" => {
            let mut parts = vec![match payload["verdict"].as_str() {
                Some(verdict) => verdict.to_owned(),
                None if payload["ok"] == json!(false) => match payload["error"].as_str() {
                    Some(error) if !error.is_empty() => format!("failed: {error}"),
                    _ => "failed".to_owned(),
                },
                None => "done".to_owned(),
            }];
            if let Some(cycle) = payload["cycle"].as_u64().filter(|cycle| *cycle > 0) {
                parts.push(format!("cycle {cycle}"));
            }
            if let Some(gate) = payload["gate"].as_str() {
                parts.push(format!("gate {gate}"));
            }
            let mut usage = None;
            if let (Some(input), Some(output)) = (
                payload["usage"]["input"].as_u64(),
                payload["usage"]["output"].as_u64(),
            ) {
                let cache = CacheFigures::of_hop(&payload["usage"]);
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
                    duration_ms: measurement(payload["durationMs"].as_u64()),
                    cost_usd: measurement(None),
                    additional,
                });
            }
            Some(Progress {
                kind: "agent.hop_finished".to_owned(),
                text: format!("{who}: {}", parts.join(", ")),
                usage,
            })
        }
        _ => None,
    }
}

/// What the person is asked about a gated call: the tool and docket's sentence
/// naming it, when docket gave them.
fn approval_prompt(tool: Option<&str>, action: Option<&str>) -> String {
    match (tool, action) {
        (Some(tool), Some(action)) => format!("Allow {tool}? docket's policy: {action}"),
        (Some(tool), None) => format!("Allow {tool}?"),
        _ => "Allow a gated tool call?".to_owned(),
    }
}

/// The question for an approval no task holds: docket's `ApprovalView`, which
/// carries the tool and action itself.
fn approval_question(view: &Value) -> Option<Question> {
    let token = view["token"].as_str()?;
    let tool = view["tool"].as_str();
    let prompt = approval_prompt(tool, view["action"].as_str());
    Some(approval_asking(
        token,
        prompt,
        tool,
        &view["role"],
        &view["policy"],
        &Value::Null,
    ))
}

/// The question for the approval a task holds (`status: waiting_approval`,
/// `approvalToken`). Its details are the record in `GET /approvals`; without
/// one, the task's `reason` is all docket said.
fn held_approval_question(task: &Value, token: &str, record: Option<&Value>) -> Question {
    match record {
        Some(record) => {
            let tool = record["context"]["tool"].as_str();
            let prompt = approval_prompt(tool, record["action"].as_str());
            approval_asking(
                token,
                prompt,
                tool,
                &record["role"],
                &record["policy"],
                &record["options"],
            )
        }
        None => {
            let prompt = match task["reason"].as_str().filter(|reason| !reason.is_empty()) {
                Some(reason) => format!("Allow a gated tool call? docket says: {reason}"),
                None => approval_prompt(None, None),
            };
            approval_asking(
                token,
                prompt,
                None,
                &Value::Null,
                &Value::Null,
                &Value::Null,
            )
        }
    }
}

/// The three answers docket's endpoint takes (`approve_once` and `approve_task`
/// both grant), worded as the pending record words them when it is there.
fn approval_asking(
    token: &str,
    prompt: String,
    tool: Option<&str>,
    role: &Value,
    policy: &Value,
    docket_options: &Value,
) -> Question {
    let option = |id: &str, label: &str| {
        let wording = docket_options
            .as_array()
            .into_iter()
            .flatten()
            .find(|option| option["id"] == json!(id));
        let text = |key: &str| {
            wording
                .and_then(|option| option[key].as_str())
                .filter(|text| !text.is_empty())
                .map(str::to_owned)
        };
        let risks: Vec<String> = wording
            .and_then(|option| option["risks"].as_array())
            .into_iter()
            .flatten()
            .filter_map(|risk| risk.as_str().map(str::to_owned))
            .collect();
        DecisionOption {
            option_id: id.to_owned(),
            label: text("label").unwrap_or_else(|| label.to_owned()),
            description: text("description"),
            risks: Some(risks).filter(|risks| !risks.is_empty()),
            estimated_tokens: None,
        }
    };
    let mut metadata = serde_json::Map::new();
    metadata.insert("tool".to_owned(), tool.into());
    metadata.insert("role".to_owned(), role.clone());
    metadata.insert("policy".to_owned(), policy.clone());
    Question {
        vendor_id: token.to_owned(),
        kind: "approval".to_owned(),
        prompt,
        options: vec![
            option("approve_once", "Allow once"),
            option("approve_task", "Allow for this task"),
            option("deny", "Deny"),
        ],
        recommendation: None,
        metadata,
    }
}

/// The one property a parked question asks for, which its answer fills.
fn consult_property(question: &Value) -> String {
    question["requestedSchema"]["properties"]
        .as_object()
        .and_then(|properties| properties.keys().next().cloned())
        .unwrap_or_else(|| "answer".to_owned())
}

/// One of docket's options for a parked question, worded as docket words it.
fn consult_option(option: &Value) -> Option<DecisionOption> {
    let text = |key: &str| {
        option[key]
            .as_str()
            .filter(|text| !text.is_empty())
            .map(str::to_owned)
    };
    let risks: Vec<String> = option["risks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|risk| risk.as_str().map(str::to_owned))
        .collect();
    let id = option["id"].as_str()?;
    Some(DecisionOption {
        option_id: id.to_owned(),
        label: text("label").unwrap_or_else(|| id.to_owned()),
        description: text("description"),
        risks: Some(risks).filter(|risks| !risks.is_empty()),
        estimated_tokens: option["estimatedTokens"].as_u64(),
    })
}

/// docket's recommendation, when it names one of the options offered.
fn consult_recommendation(question: &Value, options: &[DecisionOption]) -> Option<Recommendation> {
    let recommendation = question.get("recommendation")?;
    let option_id = recommendation["optionId"].as_str()?;
    options
        .iter()
        .find(|option| option.option_id == option_id)?;
    let evidence: Vec<String> = recommendation["evidenceRefs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|reference| reference.as_str().map(str::to_owned))
        .collect();
    Some(Recommendation {
        option_id: option_id.to_owned(),
        rationale: recommendation["rationale"]
            .as_str()
            .filter(|text| !text.is_empty())
            .map(str::to_owned),
        evidence_refs: Some(evidence).filter(|evidence| !evidence.is_empty()),
    })
}

/// The question a task parked on (`status: waiting_input`). `question` is the
/// one docket sent: the task view's carries the `options` and `recommendation`
/// that the inbox's leaves out. With options the person chooses among them
/// (or declines); without, the person answers in text or declines.
fn consult_question(task: &str, question: &Value) -> Option<(Question, Pending)> {
    let own = |id: &str, label: &str| DecisionOption {
        option_id: id.to_owned(),
        label: label.to_owned(),
        description: None,
        risks: None,
        estimated_tokens: None,
    };
    let offered: Vec<DecisionOption> = question["options"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(consult_option)
        .collect();
    let docket_ids: Vec<String> = offered
        .iter()
        .map(|option| option.option_id.clone())
        .collect();
    let recommendation = consult_recommendation(question, &offered);
    let mut options = offered;
    if options.is_empty() {
        options.push(own("accept", "Answer"));
        options.push(own("decline", "Decline"));
    } else {
        // Whatever the person picks that is not one of docket's is a decline.
        let id = if docket_ids.iter().any(|id| id == "decline") {
            "tack_decline"
        } else {
            "decline"
        };
        options.push(own(id, "Decline"));
    }
    let property = consult_property(question);
    let required = question["requestedSchema"]["required"]
        .as_array()
        .is_some_and(|names| names.iter().any(|name| name == &json!(property)));
    let id = question["id"].as_str()?.to_owned();
    Some((
        Question {
            vendor_id: id.clone(),
            kind: "consult".to_owned(),
            prompt: question["message"]
                .as_str()
                .unwrap_or("docket asks a question")
                .to_owned(),
            options,
            recommendation,
            metadata: serde_json::Map::new(),
        },
        Pending::Consult {
            task: task.to_owned(),
            question: id,
            property,
            required,
            options: docket_ids,
        },
    ))
}

impl DocketAdapter {
    /// What this task needs from a person, if anything and not yet asked.
    async fn next_question(
        &self,
        info: &RunInfo,
        asked: &HashSet<String>,
    ) -> Option<(Question, Pending)> {
        let reply = self.client.inbox().await.ok().filter(|r| r.status == 200)?;
        for item in reply.body["needsYou"].as_array().into_iter().flatten() {
            if item["pod"] != json!(info.pod) {
                continue;
            }
            if item.get("token").is_some() && item["taskId"] == json!(info.task) {
                // An approval no task holds, which docket sends as its own item.
                if let Some(question) = approval_question(item)
                    && !asked.contains(&question.vendor_id)
                {
                    let token = question.vendor_id.clone();
                    return Some((question, Pending::Approval { token }));
                }
            } else if item["id"] != json!(info.task) {
                continue;
            } else if item["status"] == json!("waiting_approval")
                && let Some(token) = item["approvalToken"].as_str()
            {
                // An approval the task holds is the task's own item; the tool and
                // the policy's sentence are in the record `GET /approvals` lists.
                if asked.contains(token) {
                    continue;
                }
                let record = self.pending_approval(token).await;
                let question = held_approval_question(item, token, record.as_ref());
                return Some((
                    question,
                    Pending::Approval {
                        token: token.to_owned(),
                    },
                ));
            } else if item["status"] == json!("waiting_input")
                && let Some(task) = item["id"].as_str()
                && let Some(listed) = item["question"].as_object()
                && !listed
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| asked.contains(id))
            {
                // The inbox's question has no options; the task view's has them.
                let viewed = self
                    .client
                    .task(&info.pod, task)
                    .await
                    .ok()
                    .filter(|r| r.status == 200)
                    .map(|r| r.body["question"].clone())
                    .filter(|question| question["id"] == item["question"]["id"]);
                if let Some(found) =
                    consult_question(task, viewed.as_ref().unwrap_or(&item["question"]))
                {
                    return Some(found);
                }
            }
        }
        None
    }

    /// The pending record for `token` in `GET /approvals`, if docket answers
    /// and lists it.
    async fn pending_approval(&self, token: &str) -> Option<Value> {
        let reply = self
            .client
            .pending_approvals()
            .await
            .ok()
            .filter(|r| r.status == 200)?;
        reply.body["pending"]
            .as_array()?
            .iter()
            .find(|record| record["token"] == json!(token))
            .cloned()
    }

    /// Posts the person's answer where docket reads it. A `409` means docket
    /// had already resolved it, which is not a failure.
    async fn deliver(
        &self,
        info: &RunInfo,
        pending: &Pending,
        answer: &DecisionAnswer,
    ) -> Option<String> {
        let reply = match pending {
            Pending::Approval { token } => {
                let reason = answer.text.clone().unwrap_or_default();
                let body = match answer.option_id.as_deref() {
                    Some(option @ ("approve_once" | "approve_task")) => {
                        json!({"action": "grant", "channel": "tack", "option": option, "reason": reason})
                    }
                    _ => json!({"action": "deny", "channel": "tack", "reason": reason}),
                };
                self.client.approve(token, body).await
            }
            Pending::Consult {
                task,
                property,
                required,
                options,
                ..
            } => {
                let chosen = answer.option_id.as_deref();
                // docket's own options take `optionId` inside `content` (its HTTP
                // route reads only `pod`, `action`, `content` and `actor`).
                let docket_option = chosen.filter(|id| options.iter().any(|own| own == id));
                let accepts = if options.is_empty() {
                    chosen == Some("accept")
                } else {
                    docket_option.is_some()
                };
                let body = if accepts {
                    let mut content = serde_json::Map::new();
                    if let Some(id) = docket_option {
                        content.insert("optionId".to_owned(), json!(id));
                    }
                    let text = answer.text.as_deref().filter(|text| !text.is_empty());
                    if text.is_some() || *required {
                        content.insert(property.clone(), json!(text.unwrap_or_default()));
                    }
                    json!({"pod": info.pod, "action": "accept", "actor": "tack",
                           "content": content})
                } else {
                    json!({"pod": info.pod, "action": "decline", "actor": "tack"})
                };
                self.client.answer(task, body).await
            }
        };
        match reply {
            Ok(reply) if reply.status == 200 || reply.status == 409 => None,
            Ok(reply) => Some(format!("docket did not take the answer: {}", reply.error())),
            Err(why) => Some(format!("the answer could not reach docket: {why}")),
        }
    }

    fn outcome(
        &self,
        info: &RunInfo,
        state: AttemptState,
        reason: Value,
        task: &Value,
        model: Option<String>,
    ) -> HarnessOutcome {
        let usage = &task["usage"];
        let by_model: BTreeMap<String, Value> = usage["byModel"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(name, used)| {
                // `by_model` keeps the input the cache did not serve, with the cache
                // figures on top (`ModelUsage`); docket's `input` has them inside.
                let cache = CacheFigures::of_hop(used);
                let input = used["input"].as_u64().unwrap_or(0);
                let mut entry = json!({
                    "input": input
                        .saturating_sub(cache.read.unwrap_or(0))
                        .saturating_sub(cache.write.unwrap_or(0)),
                    "output": used["output"].as_u64().unwrap_or(0),
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
        CacheFigures::of_task(usage).put(&mut additional);
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
                harness_version: format!("API {}", probe::API_VERSION),
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
                tokens_in: measurement(usage["input"].as_u64()),
                tokens_out: measurement(usage["output"].as_u64()),
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
        task: &Value,
        model: Option<String>,
        progress: &mpsc::Sender<Progress>,
    ) -> HarnessOutcome {
        let status = task["status"].as_str().unwrap_or_default();
        // docket's task record says why it ended in `reason`, for a failed and a blocked task
        // alike; its hops carry their own `error`, the task has none.
        let why = task["reason"].as_str().unwrap_or_default();
        let (state, reason) = match status {
            "done" => {
                let branch = task["worktree"]["branch"].as_str().unwrap_or_default();
                match Self::bring_in(&info.workspace, branch).await {
                    Ok(()) => {
                        if let Ok(diff) = self.client.task_diff(&info.pod, &info.task).await
                            && diff.status == 200
                            && let Some(count) = diff.body["untracked"]
                                .as_array()
                                .map(Vec::len)
                                .filter(|count| *count > 0)
                        {
                            let text = if count == 1 {
                                "1 file docket did not commit is not in the evidence".to_owned()
                            } else {
                                format!(
                                    "{count} files docket did not commit are not in the evidence"
                                )
                            };
                            let _ = progress.try_send(Progress {
                                kind: "agent.untracked".to_owned(),
                                text,
                                usage: None,
                            });
                        }
                        (
                            AttemptState::Succeeded,
                            json!({"code": "done", "team": info.pod, "task": info.task,
                                   "branch": branch,
                                   "base_commit": task["worktree"]["baseCommit"]}),
                        )
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
            "blocked" => (
                AttemptState::Failed,
                json!({"code": "blocked", "message": why, "task": info.task}),
            ),
            _ => (
                AttemptState::Failed,
                json!({"code": task["failureKind"].as_str().unwrap_or("failed"),
                       "message": why, "task": info.task}),
            ),
        };
        self.outcome(info, state, reason, task, model)
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
        if repository.workspace_mode != crate::client::WorkspaceMode::LocalBranch {
            return Err(rejected(WRONG_WORKSPACE.to_owned()));
        }
        let team = Self::named_team(spec);
        // The version first: a docket that is not API 5 gets no task.
        probe::status(&self.client).await.map_err(rejected)?;
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

        // Once per team per process; docket answers 200 for one it has.
        let mut ensured = self.ensured.lock().await;
        if !ensured.contains(&pod) {
            match self.client.ensure_pod(&pod, &location).await {
                Ok(reply) if reply.status == 200 || reply.status == 201 => {
                    ensured.insert(pod.clone());
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
        drop(ensured);

        let base_ref = match request.metadata.get("rework_base").and_then(Value::as_str) {
            Some(branch) if !branch.is_empty() => branch.to_owned(),
            _ => Self::base_commit(spec).await,
        };
        let body = task::body(
            request,
            &attempt_id,
            spec.work.lease.attempt_number,
            &base_ref,
        );
        let reply = self.client.enqueue_task(&pod, body).await.map_err(|why| {
            tracing::warn!(why, "docket could not be reached to post the task");
            HarnessError::Process
        })?;
        let (task, refused) = match reply.status {
            200 | 201 => match reply.body["task"].as_str() {
                Some(id) => (id.to_owned(), None),
                None => return Err(HarnessError::Process),
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

        let process_id = format!("{pod}/{task}");
        let (questions_tx, questions_rx) = mpsc::channel(1);
        let (answers_tx, answers_rx) = mpsc::channel(1);
        let (progress_tx, progress_rx) = mpsc::channel(PROGRESS_BUFFER);
        let info = Arc::new(RunInfo {
            pod,
            task,
            attempt_id,
            workspace: spec.workspace.path.clone(),
            started_at: Utc::now(),
            timeout: Duration::from_secs(request.timeout_seconds),
            refused,
        });
        self.live.lock().await.insert(
            process_id.clone(),
            Live {
                info,
                decisions: Some((questions_rx, answers_tx)),
                progress: Some(progress_rx),
                wait: Some(WaitSide {
                    questions: questions_tx,
                    answers: answers_rx,
                    progress: progress_tx,
                }),
            },
        );
        Ok(LocalRunHandle { process_id })
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
            Ok(reply) if reply.status == 200 => {
                if terminal(reply.body["status"].as_str().unwrap_or_default()) {
                    RecoveryObservation::ProcessStopped
                } else {
                    RecoveryObservation::ProcessRunning
                }
            }
            _ => RecoveryObservation::Ambiguous,
        })
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
                kind: "agent.refused".to_owned(),
                text: error.clone(),
                usage: None,
            });
            return Ok(self.outcome(
                info,
                AttemptState::Failed,
                json!({"code": "rejected", "message": error}),
                &Value::Null,
                None,
            ));
        }
        let deadline = tokio::time::Instant::now() + info.timeout;
        let mut cursor = self.read_cursor(&info.attempt_id);
        let mut served_model: Option<String> = None;
        // docket's ids of what was asked, and how often docket refused the answer.
        let mut asked: HashSet<String> = HashSet::new();
        let mut refused: HashMap<String, u32> = HashMap::new();
        let mut pending: Option<Pending> = None;
        loop {
            // The hops since the cursor.
            if let Ok(page) = self.client.traces(&info.pod, &cursor).await
                && page.status == 200
            {
                for line in page.body["events"].as_array().into_iter().flatten() {
                    let Some(event) = line
                        .as_str()
                        .and_then(|l| serde_json::from_str::<Value>(l).ok())
                    else {
                        continue;
                    };
                    let payload = &event["payload"];
                    if payload["task"] != json!(info.task) {
                        continue;
                    }
                    let kind = event["event_type"].as_str().unwrap_or_default();
                    if kind == "hop_finished"
                        && payload["role"] == json!("implementer")
                        && let Some(model) = payload["model"].as_str().filter(|m| !m.is_empty())
                    {
                        served_model = Some(model.to_owned());
                    }
                    if let Some(step) = hop_progress(kind, payload)
                        && side.progress.try_send(step).is_err()
                    {
                        tracing::warn!(harness = DESCRIPTOR.kind, "a progress line was dropped");
                    }
                }
                if let Some(next) = page.body["next"]
                    .as_str()
                    .filter(|n| !n.is_empty() && *n != cursor)
                {
                    self.write_cursor(&info.attempt_id, next);
                    cursor = next.to_owned();
                }
            }

            // Whatever needs a person.
            if pending.is_none()
                && let Some((mut question, ask)) = self.next_question(info, &asked).await
            {
                // A question asked again is a new decision: Tack keys a decision
                // by the id, and the refused one is already resolved there.
                if let Some(times) = refused.get(ask.docket_id()) {
                    question.vendor_id = format!("{}#{times}", question.vendor_id);
                }
                if side.questions.try_send(question).is_ok() {
                    asked.insert(ask.docket_id().to_owned());
                    pending = Some(ask);
                }
            }

            let task = match self.client.task(&info.pod, &info.task).await {
                Ok(reply) if reply.status == 200 => reply.body,
                Ok(reply) => {
                    tracing::warn!(status = reply.status, "the task could not be read");
                    Value::Null
                }
                Err(why) => {
                    tracing::warn!(why, "docket could not be reached");
                    Value::Null
                }
            };
            if terminal(task["status"].as_str().unwrap_or_default()) {
                let model = served_model.or_else(|| {
                    task["hops"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .rev()
                        .find(|hop| hop["role"] == json!("implementer"))
                        .and_then(|hop| hop["model"].as_str())
                        .map(str::to_owned)
                });
                return Ok(self.finish(info, &task, model, &side.progress).await);
            }
            if tokio::time::Instant::now() >= deadline {
                let _ = self
                    .client
                    .cancel_task(&info.pod, &info.task, "the attempt's time ran out")
                    .await;
                return Ok(self.outcome(
                    info,
                    AttemptState::Failed,
                    json!({"code": "timeout", "task": info.task}),
                    &task,
                    served_model,
                ));
            }

            tokio::select! {
                () = tokio::time::sleep(POLL_INTERVAL) => {}
                answer = side.answers.recv(), if pending.is_some() => {
                    if let (Some(answer), Some(ask)) = (answer, pending.take())
                        && let Some(problem) = self.deliver(info, &ask, &answer).await
                    {
                        // The decision is resolved in Tack, so the run would wait on
                        // nothing: ask again while docket still holds the question.
                        asked.remove(ask.docket_id());
                        *refused.entry(ask.docket_id().to_owned()).or_default() += 1;
                        let _ = side.progress.try_send(Progress {
                            kind: "agent.answer_failed".to_owned(),
                            text: format!("{problem}; asking again"),
                            usage: None,
                        });
                    }
                }
            }
        }
    }
}
