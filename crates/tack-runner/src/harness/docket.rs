//! The docket CLI as a harness: its command line, how its NDJSON result
//! line is read, and what it supports. The lifecycle is
//! [`crate::harness::local_process`].
//!
//! Vendor findings — what is measured, what is a documented guess, and at
//! which version — are in `fixtures/docket/README.md`.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use tack_orch::execution::{Approvals, CapabilitySupport, FeatureCapabilities, PermissionPolicy};

use crate::client::engine::{Progress, Question, StreamSignal};
use crate::harness::{
    DecisionAnswer, DecisionOption, HarnessError,
    local_process::{
        HarnessDescriptor, HarnessGrammar, Invocation, LocalProcessHarness, ModelSelection,
        RunContext, RunReport, capability, policy_capability,
    },
    process::{ProcessExit, ProcessResult},
};

pub static DESCRIPTOR: HarnessDescriptor = HarnessDescriptor {
    kind: "docket",
    program: "docket",
    model_selection: ModelSelection::Explicit(
        "docket exec refuses to run without --model, so a request naming no model is \
         rejected before spawn",
    ),
    inherited_env: &["PATH"],
    probe_notes: &[],
    credential_note: "docket signs in with its own login; Tack holds no model credential.",
    reports_process_groups: true,
    reports_progress: true,
    sandbox_binds: &[],
};

/// The variable docket's own `run` command refuses to start without, unless
/// the request's own `environment` already names it (`DOCKET_HOME` and
/// `--workspace` are supplied unconditionally by this grammar, so only the
/// endpoint is ever missing).
const BASE_URL_ENV: &str = "DOCKET_LLM_BASE_URL";

mod probe;

pub use probe::{Contract, DocketFeatures};

pub struct DocketGrammar;

/// What the located binary can do, measured the first time anything asks
/// (registration reads `capabilities` at boot) and never again. Nothing
/// located is the all-false value, which is exactly the 1.0 behaviour.
fn features() -> &'static DocketFeatures {
    static FEATURES: OnceLock<DocketFeatures> = OnceLock::new();
    FEATURES.get_or_init(|| {
        crate::harness::locate::locate_installed(DESCRIPTOR.program)
            .map(|program| DocketFeatures::probe(&program))
            .unwrap_or_default()
    })
}

pub type DocketAdapter<C = crate::SystemClock> = LocalProcessHarness<DocketGrammar, C>;

fn rejected(reason: String) -> HarnessError {
    tracing::warn!(
        reason,
        harness = DESCRIPTOR.kind,
        "request rejected before spawn"
    );
    HarnessError::Rejected { reason }
}

fn bounded_prefix(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

/// The line `docket exec` prints last, on success or failure. Every
/// other stdout line is an NDJSON progress event this adapter does not read.
#[derive(serde::Deserialize)]
struct ResultLine {
    token: String,
    status: String,
    stop_reason: String,
    error: String,
    blocked: Option<serde_json::Value>,
    model: ResultModel,
    usage: ResultUsage,
    #[serde(default)]
    files: Vec<ResultFile>,
    limits: Option<ResultLimits>,
    #[serde(default)]
    task: Option<serde_json::Value>,
}

/// A path docket reports a run wrote (`op` is `write`, `edit`, `delete` or
/// `unknown`); only present on a 1.1 line.
#[derive(serde::Deserialize, serde::Serialize)]
struct ResultFile {
    path: String,
    op: String,
}

/// The ceilings docket echoes back; only present on a 1.1 line.
#[derive(serde::Deserialize)]
struct ResultLimits {
    #[serde(rename = "maxTokens")]
    max_tokens: Option<u64>,
}

#[derive(serde::Deserialize)]
struct ResultModel {
    served: String,
}

#[derive(serde::Deserialize)]
struct ResultUsage {
    input_tokens: u64,
    output_tokens: u64,
}

/// One progress line, as far as the process lifecycle is concerned: a 1.1
/// One of docket's progress events: a `bash` call's process group when it
/// starts and when it is gone
/// (`fixtures/docket/contract-1.1/cancelled-process.ndjson`), and the tool
/// calls, tool results and model calls a run makes
/// (`fixtures/docket/contract-1.1/asked-answered.ndjson`).
#[derive(serde::Deserialize)]
struct ProgressLine {
    event: Option<ProgressEvent>,
}

#[derive(serde::Deserialize)]
struct ProgressEvent {
    event_type: String,
    #[serde(default)]
    payload: serde_json::Value,
}

/// The event types [`DocketGrammar::signal`] reads past the approval and the
/// result line; a line naming none of them is never parsed.
const READ_EVENTS: [&str; 4] = [
    "\"process_",
    "\"tool_call\"",
    "\"tool_result\"",
    "\"llm_call\"",
];

/// How much of a step's text reaches the timeline.
const STEP_TEXT_CHARS: usize = 400;

/// A docket event worth a line on the attempt's timeline: a tool call and
/// what it runs, a call docket refused, a model call and what it cost.
fn step(event: &ProgressEvent) -> Option<Progress> {
    let payload = &event.payload;
    let text_of = |key: &str| payload.get(key).and_then(serde_json::Value::as_str);
    let count = |key: &str| payload.get(key).and_then(serde_json::Value::as_u64);
    let tool = text_of("tool").unwrap_or("a tool");
    let (kind, text) = match event.event_type.as_str() {
        "tool_call" => {
            // `arguments` is the call's arguments as the model sent them: a
            // JSON object in a string.
            let arguments = text_of("arguments").unwrap_or_default();
            let parsed = serde_json::from_str::<serde_json::Value>(arguments).ok();
            let command = parsed
                .as_ref()
                .and_then(|value| value.get("command"))
                .and_then(serde_json::Value::as_str);
            let text = match command {
                Some(command) => format!("{tool}: {command}"),
                None if arguments.trim().is_empty() || arguments.trim() == "{}" => tool.to_owned(),
                None => format!("{tool} {arguments}"),
            };
            ("agent.tool_call", text)
        }
        "tool_result" if text_of("decision") == Some("deny") => {
            let reason = text_of("reason").unwrap_or("no reason given");
            (
                "agent.tool_refused",
                format!("{tool} was refused: {reason}"),
            )
        }
        "llm_call" if payload.get("ok") == Some(&serde_json::Value::Bool(false)) => {
            let why = text_of("failureKind").unwrap_or("no reason given");
            ("agent.model_turn", format!("A model call failed: {why}"))
        }
        "llm_call" => {
            let turn = count("iteration")
                .map(|n| format!(" {n}"))
                .unwrap_or_default();
            let tokens = match (count("inputTokens"), count("outputTokens")) {
                (Some(input), Some(output)) => format!(": {input} tokens in, {output} out"),
                _ => String::new(),
            };
            ("agent.model_turn", format!("Model turn{turn}{tokens}"))
        }
        _ => return None,
    };
    Some(Progress {
        kind: kind.to_owned(),
        text: bounded_prefix(&text, STEP_TEXT_CHARS),
    })
}

/// No parseable result line at all: the exit status and a prefix of what
/// was printed are all that is left as evidence.
fn malformed(result: &ProcessResult) -> RunReport {
    RunReport::verdict(
        false,
        serde_json::json!({
            "code": "malformed_output",
            "detail": "docket produced no parseable terminal result line",
            "exit": format!("{:?}", result.exit),
            "stdout_prefix": bounded_prefix(&result.stdout.text, 500),
        }),
    )
}

/// docket's own tool names, the only ones a `tool:` predicate can match
/// (`core/tools.py`); the request's tool list names tools of this harness.
const DOCKET_TOOLS: &[&str] = &[
    "read", "write", "edit", "glob", "grep", "bash", "fetch", "skill", "consult",
];

/// The `kind: policy` document (`docs/contracts/config-v1/policy.schema.json`
/// in docket) that blocks every docket tool the request does not allow:
/// the tools its list leaves out, and `fetch` when `network` is false.
/// `Ok(None)` when nothing needs blocking. A request this shape cannot
/// express is refused naming the field, never narrowed.
fn policy_document(policy: &PermissionPolicy) -> Result<Option<String>, HarnessError> {
    if let Some(field) = policy.additional.keys().next() {
        return Err(rejected(format!(
            "permission_policy.{field} cannot be expressed as a docket policy document"
        )));
    }
    // Every tool, less `fetch` when the network is denied.
    if policy.allows_all_tools() {
        let mut all = policy.clone();
        all.tools = DOCKET_TOOLS
            .iter()
            .filter(|tool| policy.network || **tool != "fetch")
            .map(|tool| (*tool).to_owned())
            .collect();
        return policy_document(&all);
    }
    let allowed: Vec<String> = policy
        .tools
        .iter()
        .map(|tool| tool.to_ascii_lowercase())
        .collect();
    if let Some(unknown) = policy
        .tools
        .iter()
        .find(|tool| !DOCKET_TOOLS.contains(&tool.to_ascii_lowercase().as_str()))
    {
        return Err(rejected(format!(
            "permission_policy.tools names {unknown:?}, which is not a docket tool \
             ({}); a docket policy document can only name those",
            DOCKET_TOOLS.join(", ")
        )));
    }
    if !policy.network && allowed.iter().any(|tool| tool == "fetch") {
        return Err(rejected(
            "permission_policy.network is false but permission_policy.tools allows docket's \
             network tool `fetch`, a self-contradictory request"
                .to_owned(),
        ));
    }
    let denied: Vec<&str> = DOCKET_TOOLS
        .iter()
        .copied()
        .filter(|tool| !allowed.iter().any(|name| name == tool))
        .collect();
    if denied.is_empty() {
        return Ok(None);
    }
    let predicates: String = denied
        .iter()
        .map(|tool| format!("    - tool: {tool}\n"))
        .collect();
    Ok(Some(format!(
        "kind: policy\n\
         name: tack-permission-policy\n\
         description: The tools this Tack request does not allow.\n\
         appliesTo:\n  - \"*\"\n\
         on: toolCall\n\
         when:\n  anyOf:\n{predicates}\
         then: block\n\
         message: this tool is not allowed by the Tack request's permission_policy\n"
    )))
}

/// `permission_policy.tools` and `.network` become a `--policy` file, and
/// `budgets.tokens` a `--max-tokens`, each only when its probe is true; a
/// docket without the probe is passed nothing and `capabilities` says so.
fn invocation(features: &DocketFeatures, run: &RunContext<'_>) -> Result<Invocation, HarnessError> {
    let request = &run.spec.work.request;
    if !request.environment.contains_key(BASE_URL_ENV) {
        return Err(rejected(format!(
            "the request's own environment names no {BASE_URL_ENV}; docket refuses to run \
             without one"
        )));
    }

    let model_args: Vec<String> = request
        .requested_model
        .iter()
        .flat_map(|model| ["--model".to_owned(), model.clone()])
        .collect();
    let workspace_root = &run.spec.workspace.path;
    let docket_home = run.scratch.join("docket-home");

    let mut env = BTreeMap::new();
    env.insert("DOCKET_HOME".to_owned(), docket_home.display().to_string());

    // On 1.1 the task is a file under the scratch directory, so stdin
    // carries nothing; on 1.0 it is read from stdin, as ADR 0066 measured.
    let (contract_args, task_file) = match features.contract {
        Contract::V1_0 => (Vec::new(), "/dev/stdin".to_owned()),
        Contract::V1_1 => {
            let path = run.scratch.join("task.md");
            std::fs::write(&path, &request.resolved_agent_profile.instructions)
                .map_err(|_| rejected("the task file could not be written".to_owned()))?;
            (
                vec!["--contract".to_owned(), "1.1".to_owned()],
                path.display().to_string(),
            )
        }
    };

    // An `ask` request on a docket that offers `--answers` pauses each gated
    // call on stdin (`fixtures/docket/contract-1.1/asked-answered.ndjson`);
    // any other request keeps the refusal posture.
    let ask =
        features.answers && matches!(request.permission_policy.approvals, Some(Approvals::Ask));
    let answer_args = if ask {
        vec!["--answers".to_owned(), "stdin".to_owned()]
    } else {
        Vec::new()
    };

    // docket refuses `--recipe` together with `--agent-id` or `--max-tokens`
    // (a recipe's turns are dispatched by its pod), so a recipe run drops both.
    let recipe = features
        .recipe
        .then(|| {
            request
                .resolved_agent_profile
                .tool_policy
                .get("docket")
                .and_then(|v| v.get("recipe"))
                .and_then(serde_json::Value::as_str)
                .filter(|recipe| !recipe.is_empty())
        })
        .flatten();

    let mut limit_args = Vec::new();
    if features.token_file {
        limit_args.push("--token-file".to_owned());
        limit_args.push(run.scratch.join("token.json").display().to_string());
    }
    if features.max_tokens
        && recipe.is_none()
        && let Some(tokens) = request
            .budgets
            .get("tokens")
            .and_then(serde_json::Value::as_u64)
            .filter(|tokens| *tokens > 0)
    {
        limit_args.push("--max-tokens".to_owned());
        limit_args.push(tokens.to_string());
    }
    if features.policy
        && let Some(document) = policy_document(&request.permission_policy)?
    {
        let path = run.scratch.join("policy.yaml");
        std::fs::write(&path, document)
            .map_err(|_| rejected("the policy file could not be written".to_owned()))?;
        limit_args.push("--policy".to_owned());
        limit_args.push(path.display().to_string());
    }
    if let Some(recipe) = recipe {
        limit_args.push("--recipe".to_owned());
        limit_args.push(recipe.to_owned());
    }

    let agent_id = [
        "--agent-id".to_owned(),
        run.spec.work.lease.attempt_id.as_str().to_owned(),
    ];
    Ok(Invocation {
        args: [
            "exec",
            "--workspace",
            &workspace_root.display().to_string(),
            "--task-file",
            &task_file,
        ]
        .map(str::to_owned)
        .into_iter()
        .chain(model_args)
        .chain(if recipe.is_some() {
            Vec::new()
        } else {
            agent_id.to_vec()
        })
        .chain(["--timeout".to_owned(), request.timeout_seconds.to_string()])
        .chain(contract_args)
        .chain(answer_args)
        .chain(limit_args)
        .collect(),
        env,
        stdin_stays_open: ask,
        ..Default::default()
    })
}

impl HarnessGrammar for DocketGrammar {
    fn descriptor(&self) -> &'static HarnessDescriptor {
        &DESCRIPTOR
    }

    /// See the free `invocation`: limits and policy only where probed.
    fn invocation(&self, run: &RunContext<'_>) -> Result<Invocation, HarnessError> {
        invocation(features(), run)
    }

    fn prompt(&self, prompt: String, _stdin_stays_open: bool) -> Vec<u8> {
        match features().contract {
            Contract::V1_0 => prompt.into_bytes(),
            Contract::V1_1 => Vec::new(),
        }
    }

    fn report(&self, _run: &RunContext<'_>, result: &ProcessResult) -> RunReport {
        let mut results = result
            .stdout
            .text
            .lines()
            .filter_map(|line| serde_json::from_str::<ResultLine>(line.trim()).ok());
        let Some(parsed) = results.next() else {
            return malformed(result);
        };
        for _ in results {
            tracing::debug!(harness = DESCRIPTOR.kind, "terminal_line_ignored");
        }
        let mut report = RunReport {
            succeeded: parsed.status == "ok",
            terminal_reason: serde_json::json!({
                "code": parsed.status,
                "message": parsed.error,
                "stop_reason": parsed.stop_reason,
                "blocked": parsed.blocked,
                "token": parsed.token,
                "files": parsed
                    .files
                    .iter()
                    .filter(|file| !file.path.starts_with(".tack-runner/"))
                    .collect::<Vec<_>>(),
                "max_tokens": parsed.limits.and_then(|limits| limits.max_tokens),
                "task": parsed.task,
            }),
            harness_version: None,
            observed_model: (!parsed.model.served.is_empty()).then_some(parsed.model.served),
            tokens_in: Some(parsed.usage.input_tokens),
            tokens_out: Some(parsed.usage.output_tokens),
            duration_ms: None,
            cost_usd: None,
            usage_detail: Default::default(),
        };
        if report.succeeded && !matches!(result.exit, ProcessExit::Exited(0)) {
            report.succeeded = false;
            report.terminal_reason = serde_json::json!({
                "reason": "exit_status",
                "exit": format!("{:?}", result.exit),
                "stream": report.terminal_reason,
            });
        }
        report
    }

    fn signal(&self, _run: &RunContext<'_>, line: &str) -> Option<StreamSignal> {
        if line.contains("\"approval_requested\"") {
            return approval_question(line);
        }
        // The result line is the only one with a top-level `status` and no
        // `event`; it ends the conversation so docket's stdin can close.
        if line.contains("\"status\"") && is_result_line(line) {
            return Some(StreamSignal::Finished);
        }
        if !READ_EVENTS.iter().any(|name| line.contains(name)) {
            return None;
        }
        let event = serde_json::from_str::<ProgressLine>(line).ok()?.event?;
        let pgid = || {
            event
                .payload
                .get("pgid")
                .and_then(serde_json::Value::as_u64)
                .and_then(|pgid| u32::try_from(pgid).ok())
        };
        match event.event_type.as_str() {
            "process_started" => Some(StreamSignal::ProcessStarted { pgid: pgid()? }),
            "process_exited" => Some(StreamSignal::ProcessExited { pgid: pgid()? }),
            _ => step(&event).map(StreamSignal::Progress),
        }
    }

    /// One `AnswerLine` (`fixtures/docket/contract-1.1/`): `accept`
    /// releases the call, anything else, including an option this grammar
    /// never offered, is `decline`.
    fn answer(&self, question: &Question, answer: &DecisionAnswer) -> Vec<u8> {
        let action = if answer.option_id.as_deref() == Some("accept") {
            "accept"
        } else {
            "decline"
        };
        let payload = serde_json::json!({
            "v": "1.1.0",
            "token": question.metadata.get("token"),
            "answer": {
                "approvalToken": question.vendor_id,
                "action": action,
                "content": null,
            },
        });
        serde_json::to_vec(&payload).unwrap_or_default()
    }

    fn capabilities(&self) -> FeatureCapabilities {
        capabilities(features())
    }
}

fn is_result_line(line: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(line.trim())
        .is_ok_and(|value| value.get("status").is_some() && value.get("event").is_none())
}

/// An `approval_requested` event: the approval's own token is `payload.token`
/// and the line's own `token` is the run's, which every answer must name.
fn approval_question(line: &str) -> Option<StreamSignal> {
    let value: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    let event = value.get("event")?;
    if event.get("event_type")?.as_str()? != "approval_requested" {
        return None;
    }
    let payload = event.get("payload")?;
    let approval = payload.get("token")?.as_str()?;
    let run_token = value.get("token")?.as_str()?;
    let tool = payload.get("tool").and_then(serde_json::Value::as_str);
    let call_id = payload.get("callId").and_then(serde_json::Value::as_str);
    let action = payload.get("action").and_then(serde_json::Value::as_str);
    let rationale = payload
        .get("rationale")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|rationale| !rationale.is_empty());
    let prompt = approval_prompt(tool, call_id, action, rationale);
    let option = |option_id: &str, label: &str| DecisionOption {
        option_id: option_id.to_owned(),
        label: label.to_owned(),
        description: None,
        risks: None,
        estimated_tokens: None,
    };
    let mut metadata = serde_json::Map::new();
    metadata.insert("token".to_owned(), run_token.into());
    metadata.insert("tool".to_owned(), tool.into());
    metadata.insert("callId".to_owned(), call_id.into());
    Some(StreamSignal::Question(Question {
        vendor_id: approval.to_owned(),
        kind: "tool_permission".to_owned(),
        prompt,
        options: vec![option("accept", "Allow"), option("decline", "Deny")],
        recommendation: None,
        metadata,
    }))
}

/// What the person reads before answering: the call itself, why docket's
/// policy stopped it, and the model's own reason when it gave one.
///
/// docket's `action` is `tool call '<tool>': <reason>; call=<tool> <key>=<json> …`,
/// the render its `pre_tool_call` policies match, cut at 1000 characters. A
/// bash call's single `command` is shown as the bare command; any other call
/// shows its rendered arguments as they are. An `action` in another shape is
/// shown whole, and no `action` at all falls back to the tool and call id.
fn approval_prompt(
    tool: Option<&str>,
    call_id: Option<&str>,
    action: Option<&str>,
    rationale: Option<&str>,
) -> String {
    let asked = match (tool, action) {
        (Some(tool), Some(action)) => Some(match parse_action(tool, action) {
            Some((reason, call)) => format!("Allow {tool}{call}? docket's policy: {reason}."),
            None => format!("Allow {tool}? {action}"),
        }),
        (None, Some(action)) => Some(format!("Allow a gated tool call? {action}")),
        _ => None,
    };
    let mut prompt = asked.unwrap_or_else(|| match (tool, call_id) {
        (Some(tool), Some(call)) => format!("Allow {tool} (call {call})?"),
        (Some(tool), None) => format!("Allow {tool}?"),
        _ => "Allow a gated tool call?".to_owned(),
    });
    if let Some(rationale) = rationale {
        prompt.push_str(&format!(" The agent says: \"{rationale}\""));
    }
    prompt
}

/// `(reason, call)` out of docket's `action`, the call worded to follow the
/// tool's name: a bash `command` decoded from its JSON and quoted, any other
/// arguments as rendered, nothing for a call without arguments.
fn parse_action<'a>(tool: &str, action: &'a str) -> Option<(&'a str, String)> {
    let rest = action.strip_prefix(&format!("tool call '{tool}': "))?;
    let (reason, call) = rest.split_once("; call=")?;
    let arguments = call.strip_prefix(tool)?.trim_start();
    let call = match arguments.strip_prefix("command=") {
        Some(command) => {
            let command = serde_json::from_str::<String>(command)
                .unwrap_or_else(|_| command.trim_start_matches('"').to_owned());
            format!(" to run \"{command}\"")
        }
        None if arguments.is_empty() => String::new(),
        None => format!(" with {arguments}"),
    };
    Some((reason, call))
}

/// `base`, then `note` when the binary offers what the note describes.
/// Nothing offered is `base` unchanged, so an unprobed docket reads as it
/// always did.
fn derived(base: &str, offered: bool, note: &str) -> String {
    if offered {
        format!("{base}; {note}")
    } else {
        base.to_owned()
    }
}

fn capabilities(features: &DocketFeatures) -> FeatureCapabilities {
    let negotiated = if features.version.is_empty() {
        String::new()
    } else {
        format!(
            "; negotiated contract {} against docket {}",
            features.contract.as_str(),
            features.version
        )
    };
    FeatureCapabilities {
        cancel: if features.contract == Contract::V1_1 {
            capability(
                CapabilitySupport::Supported,
                &format!(
                    "docket stops cooperatively on SIGTERM and reports a cancelled result; this \
                     docket also reports each process group its bash calls start and end, so a \
                     cancellation stops the groups still live and says whether they went{negotiated}"
                ),
            )
        } else {
            capability(
                CapabilitySupport::Advisory,
                "docket stops cooperatively on SIGTERM and reports a cancelled result, but this \
                 docket does not report the process groups its tools start, so a tool's process \
                 group that outlives the grace period cannot be confirmed stopped",
            )
        },
        resume: capability(
            CapabilitySupport::Unsupported,
            "harness mode is one synchronous run to completion; no reattachment interface \
             is documented or observed",
        ),
        decisions: if features.answers {
            capability(
                CapabilitySupport::Supported,
                &format!(
                    "this docket accepts --answers, so a tool call its policy gates pauses the run on an approval_requested event and waits for one answer line on stdin (accept or decline), as measured in \
                     fixtures/docket/contract-1.1/asked-answered.ndjson{negotiated}"
                ),
            )
        } else {
            capability(
                CapabilitySupport::Unsupported,
                &format!(
                    "harness mode's approval posture is fixed to non-interactive refusal: a \
                     tool call that would otherwise wait for a human is denied immediately as \
                     `blocked` rather than pausing the run to ask one{negotiated}"
                ),
            )
        },
        // The result's `files` is populated by the same docket card that
        // adds `--token-file`, so that flag's probe is the one that says so.
        artifacts: if features.token_file {
            capability(
                CapabilitySupport::Supported,
                &format!(
                    "this docket's 1.1 result lists the files a run wrote, kept in the terminal \
                     reason (paths under .tack-runner/ are dropped), beside the staged \
                     stdout/stderr log{negotiated}"
                ),
            )
        } else {
            capability(
                CapabilitySupport::Unsupported,
                "this docket does not report the files a run wrote, so only the captured \
                 stdout/stderr is staged as a log",
            )
        },
        usage: capability(
            CapabilitySupport::Advisory,
            &derived(
                "the result line's usage.input_tokens/output_tokens are real measurements, but \
                 cost_usd is always null on the installed version, so cost is never reported",
                features.max_tokens,
                "this docket accepts --max-tokens, which the adapter passes from budgets.tokens",
            ),
        ),
        additional: if features.policy {
            policy_capability(
                CapabilitySupport::Supported,
                &format!(
                    "this docket accepts --policy: the request's tool list and network flag are \
                     passed as one kind: policy document blocking every docket tool they do not \
                     allow, and a request that document cannot express is refused before spawn{negotiated}"
                ),
            )
        } else {
            policy_capability(
                CapabilitySupport::Unsupported,
                &format!(
                    "this docket does not accept --policy, so the request's tool list and \
                     network flag are not passed to it{negotiated}"
                ),
            )
        },
    }
}

#[cfg(test)]
#[path = "docket/tests.rs"]
mod tests;
