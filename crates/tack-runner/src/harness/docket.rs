//! The docket CLI as a harness: its command line, how its NDJSON result
//! line is read, and what it supports. The lifecycle is
//! [`crate::harness::local_process`].
//!
//! Vendor findings — what is measured, what is a documented guess, and at
//! which version — are in `fixtures/docket/README.md`.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use tack_orch::execution::{Approvals, CapabilitySupport, FeatureCapabilities, PermissionPolicy};

use crate::client::engine::{Question, StreamSignal};
use crate::harness::{
    DecisionAnswer, DecisionOption, HarnessError,
    local_process::{
        HarnessDescriptor, HarnessGrammar, Invocation, LocalProcessHarness, ModelSelection,
        RunContext, RunReport, capability, policy_capability,
    },
    process::ProcessResult,
};
use crate::provider::Wire;

pub static DESCRIPTOR: HarnessDescriptor = HarnessDescriptor {
    kind: "docket",
    program: "docket",
    wire: Wire::OpenAiChatCompletions,
    model_selection: ModelSelection::Explicit(
        "docket harness mode refuses to run without --model, so a request with no explicit \
         requested_model_provider and requested_model_id is rejected before spawn",
    ),
    native_provider: "docket",
    inherited_env: &["PATH"],
    model_passthrough: "requested_model_id is forwarded verbatim after the requested provider, \
                        joined as <provider>/<model id> for --model; docket validates the \
                        result at run time, so no model list is claimed",
    probe_notes: &[],
    credential_note: "docket reads its upstream credential from DOCKET_LLM_API_KEY alone, never \
                      a provider-named variable. Tack injects the resolved provider credential \
                      under that fixed name via `credential_env`.",
    credential_env: Some("DOCKET_LLM_API_KEY"),
    observes_served_model: true,
    reports_process_groups: true,
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

/// The last line `docket harness run` prints, on success or failure. Every
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
/// docket reports a `bash` call's process group when it starts and when it
/// is gone (`fixtures/docket/contract-1.1/cancelled-process.ndjson`).
#[derive(serde::Deserialize)]
struct ProgressLine {
    event: Option<ProgressEvent>,
}

#[derive(serde::Deserialize)]
struct ProgressEvent {
    event_type: String,
    payload: ProgressPayload,
}

#[derive(serde::Deserialize)]
struct ProgressPayload {
    pgid: Option<u32>,
}

fn last_non_empty_line(text: &str) -> Option<&str> {
    text.lines().rev().find(|line| !line.trim().is_empty())
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
    if run.endpoint.is_none() && !request.environment.contains_key(BASE_URL_ENV) {
        return Err(rejected(format!(
            "no provider endpoint resolved and the request's own environment names no \
             {BASE_URL_ENV}; docket refuses to run without one"
        )));
    }

    let provider = request
        .requested_model_provider
        .as_ref()
        .map_or(String::new(), |provider| provider.as_str().to_owned());
    let model_id = request
        .requested_model_id
        .as_ref()
        .map_or(String::new(), |model| model.as_str().to_owned());
    let workspace_root = &run.spec.workspace.path;
    let docket_home = run.scratch.join("docket-home");

    let mut env = BTreeMap::new();
    env.insert("DOCKET_HOME".to_owned(), docket_home.display().to_string());
    if let Some(endpoint) = run.endpoint {
        env.insert(BASE_URL_ENV.to_owned(), endpoint.base_url.clone());
    }

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
            "harness",
            "run",
            "--workspace",
            &workspace_root.display().to_string(),
            "--task-file",
            &task_file,
            "--model",
            &format!("{provider}/{model_id}"),
        ]
        .map(str::to_owned)
        .into_iter()
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
        let Some(line) = last_non_empty_line(&result.stdout.text) else {
            return malformed(result);
        };
        let Ok(parsed) = serde_json::from_str::<ResultLine>(line.trim()) else {
            return malformed(result);
        };
        RunReport {
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
        }
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
        // Every other event names no group; only a line that mentions one
        // is worth parsing.
        if !line.contains("\"process_") {
            return None;
        }
        let event = serde_json::from_str::<ProgressLine>(line).ok()?.event?;
        let pgid = event.payload.pgid?;
        match event.event_type.as_str() {
            "process_started" => Some(StreamSignal::ProcessStarted { pgid }),
            "process_exited" => Some(StreamSignal::ProcessExited { pgid }),
            _ => None,
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
    let prompt = match (tool, call_id) {
        (Some(tool), Some(call)) => format!("Allow {tool} (call {call})?"),
        (Some(tool), None) => format!("Allow {tool}?"),
        _ => "Allow a gated tool call?".to_owned(),
    };
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
