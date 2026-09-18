//! Claude Code as a harness: its command line, how its `stream-json`
//! transcript is read, and what it supports. The lifecycle is
//! [`crate::harness::local_process`].
//!
//! Vendor findings — what is measured, what is a documented guess, and at
//! which version — are in `fixtures/claude_code/README.md`.

use std::collections::BTreeMap;

use serde_json::Value;
use tack_orch::execution::{CapabilitySupport, FeatureCapabilities};

use crate::harness::{
    HarnessError,
    local_process::{
        HarnessDescriptor, HarnessGrammar, Invocation, LocalProcessHarness, ModelSelection,
        RunContext, RunReport, capability, policy_capability,
    },
    process::{ProcessExit, ProcessResult},
};
use crate::provider::Wire;

pub static DESCRIPTOR: HarnessDescriptor = HarnessDescriptor {
    kind: "claude-code",
    program: "claude",
    wire: Wire::AnthropicMessages,
    model_selection: ModelSelection::Optional,
    native_provider: "anthropic",
    // `claude` finds its login session under `HOME`, and its Bash tool runs
    // a real shell that needs `PATH`. Nothing else is inherited.
    inherited_env: &["HOME", "PATH"],
    // The terminal `result` object is the last line of the transcript.
    min_capture_bytes: (16 * 1024 * 1024, 4 * 1024 * 1024),
    model_passthrough: "requested_model_id is forwarded verbatim via --model; the CLI validates \
                        it at run time (an invalid model returns is_error:true), so no model \
                        list is claimed",
    probe_notes: &[(
        "model_discovery_note",
        "Claude Code's CLI has no list-models command; model availability is only observable \
         via a live, billed invocation, so this probe reports zero model_combinations rather \
         than an unverified static alias list.",
    )],
    credential_note: "Claude Code authenticates itself: typically an OAuth session under \
                      $HOME/.claude from its own login flow, or an API key it reads from its own \
                      environment. Tack never reads, stores or forwards it. Only HOME and PATH \
                      are forwarded from the runner's environment; anything else must come \
                      through the execution request's own `environment`.",
};

/// Provider families the `claude` binary knows on its own, confirmed by
/// `strings` against 2.1.223. A Tack-configured provider is not listed
/// here: [`is_known_provider`] asks the provider registry for those.
const NATIVE_PROVIDER_FAMILIES: &[&str] = &["anthropic", "bedrock", "vertex", "foundry"];

fn is_known_provider(name: &str) -> bool {
    NATIVE_PROVIDER_FAMILIES.contains(&name)
        || crate::provider::registry()
            .iter()
            .any(|provider| provider.wire_name() == name)
}

/// Tools that reach the network, matched case-insensitively against
/// `permission_policy.tools`.
const NETWORK_TOOLS: &[&str] = &["webfetch", "websearch"];

pub struct ClaudeCodeGrammar;

pub type ClaudeCodeAdapter<C = crate::SystemClock> = LocalProcessHarness<ClaudeCodeGrammar, C>;

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

/// Reads a terminal `{"type":"result"}` object. `is_error` alone decides
/// success: `subtype` was observed reporting `"success"` beside
/// `"is_error":true` for an invalid-model API error. A missing `is_error`
/// fails closed.
fn report_from_result_line(
    result: &Value,
    init_model: Option<String>,
    harness_version: Option<String>,
) -> RunReport {
    let is_error = result
        .get("is_error")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    RunReport {
        succeeded: !is_error,
        terminal_reason: result.clone(),
        harness_version,
        observed_model: init_model,
        tokens_in: result
            .pointer("/usage/input_tokens")
            .and_then(Value::as_u64),
        tokens_out: result
            .pointer("/usage/output_tokens")
            .and_then(Value::as_u64),
        duration_ms: result.get("duration_ms").and_then(Value::as_u64),
        cost_usd: result.get("total_cost_usd").and_then(Value::as_f64),
    }
}

/// No JSON line at all: the exit status is the only signal left, and the
/// reason says the verdict was inferred rather than observed.
fn report_from_exit(result: &ProcessResult) -> RunReport {
    let (succeeded, note) = match result.exit {
        ProcessExit::Exited(0) => (
            true,
            "no structured result envelope was produced; inferred success from exit code 0",
        ),
        ProcessExit::Exited(_) => (
            false,
            "no structured result envelope was produced; inferred failure from a non-zero exit code",
        ),
        ProcessExit::TimedOut => (
            false,
            "process exceeded its timeout with no structured result envelope",
        ),
        #[cfg(unix)]
        ProcessExit::Signaled(_) => (
            false,
            "process terminated by signal with no structured result envelope",
        ),
    };
    RunReport::verdict(
        succeeded,
        serde_json::json!({
            "reason": note,
            "exit": format!("{:?}", result.exit),
            "stderr_prefix": bounded_prefix(&result.stderr.text, 500),
        }),
    )
}

/// Scans the transcript for the two lines that matter: `system`/`init`
/// (the session's model and `claude_code_version`) and the terminal
/// `result`. A line that does not parse is skipped, so one corrupted line
/// never discards an otherwise good stream. JSON lines with no `result` —
/// a truncated stream — is a failed run.
fn parse_run_output(result: &ProcessResult) -> RunReport {
    let mut init_model = None;
    let mut harness_version = None;
    let mut result_line = None;
    let mut any_json_line = false;

    for line in result.stdout.text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        any_json_line = true;
        let field = |name: &str| value.get(name).and_then(Value::as_str);
        match field("type") {
            Some("system") if field("subtype") == Some("init") => {
                init_model = field("model").map(str::to_owned);
                harness_version = field("claude_code_version").map(str::to_owned);
            }
            Some("result") => result_line = Some(value),
            _ => {}
        }
    }

    if let Some(result_line) = result_line {
        return report_from_result_line(&result_line, init_model, harness_version);
    }
    if !any_json_line {
        return report_from_exit(result);
    }
    RunReport::verdict(
        false,
        serde_json::json!({
            "reason": "malformed_output",
            "detail": "harness produced JSON output but no parseable terminal `result` object",
            "exit": format!("{:?}", result.exit),
            "stdout_prefix": bounded_prefix(&result.stdout.text, 500),
        }),
    )
}

impl HarnessGrammar for ClaudeCodeGrammar {
    fn descriptor(&self) -> &'static HarnessDescriptor {
        &DESCRIPTOR
    }

    fn invocation(&self, run: &RunContext<'_>) -> Result<Invocation, HarnessError> {
        let request = &run.spec.work.request;
        if let Some(provider) = &request.requested_model_provider
            && !is_known_provider(&provider.as_str().trim().to_ascii_lowercase())
        {
            let mut known: Vec<&str> = NATIVE_PROVIDER_FAMILIES.to_vec();
            known.extend(crate::provider::registry().iter().map(|p| p.wire_name()));
            return Err(rejected(format!(
                "requested model provider {:?} is not one of this adapter's known provider \
                 families {known:?}",
                provider.as_str()
            )));
        }
        let policy = &request.permission_policy;
        let names_network_tool = policy
            .tools
            .iter()
            .any(|tool| NETWORK_TOOLS.contains(&tool.to_ascii_lowercase().as_str()));
        if !policy.network && names_network_tool {
            return Err(rejected(
                "permission_policy denies network but names a network tool \
                 (WebFetch/WebSearch), a self-contradictory request"
                    .to_owned(),
            ));
        }

        let mut args: Vec<String> = [
            "-p",
            "--output-format",
            "stream-json",
            "--verbose",
            "--no-session-persistence",
            "--permission-mode",
            "bypassPermissions",
            "--effort",
            "high",
            "--setting-sources",
            "",
            "--tools",
        ]
        .map(str::to_owned)
        .into();
        args.push(policy.tools.join(","));
        if let Some(model) = &request.requested_model_id {
            args.extend(["--model".to_owned(), model.as_str().to_owned()]);
        }
        let budget = request.budgets.get("cost_usd").and_then(Value::as_f64);
        if let Some(budget) = budget.filter(|value| *value > 0.0) {
            args.extend(["--max-budget-usd".to_owned(), budget.to_string()]);
        }

        let mut env = BTreeMap::new();
        if let Some(endpoint) = run.endpoint {
            env.insert("ANTHROPIC_BASE_URL".to_owned(), endpoint.base_url.clone());
            // Measured against 2.1.260: empty, unset and non-empty all sent
            // byte-identical requests, `ANTHROPIC_AUTH_TOKEN` winning each
            // time, which contradicts the vendor's documented precedence.
            // Set empty anyway rather than trusted to be absent.
            env.insert("ANTHROPIC_API_KEY".to_owned(), String::new());
        }
        Ok(Invocation { args, env })
    }

    fn report(&self, _run: &RunContext<'_>, result: &ProcessResult) -> RunReport {
        parse_run_output(result)
    }

    fn capabilities(&self) -> FeatureCapabilities {
        FeatureCapabilities {
            cancel: capability(
                CapabilitySupport::Advisory,
                "The `claude` process is always signalled (it leads its own process group). A \
                 Bash-tool subprocess was observed (via `ps`) in its own session; it is only \
                 cleaned up if Claude Code exits within the SIGTERM grace period, since SIGKILL \
                 cannot reach another session's process group.",
            ),
            resume: capability(
                CapabilitySupport::Unsupported,
                "Headless (--print) invocation is one ephemeral process with no reattachment \
                 interface. `--resume <session-id>` starts a new process that continues stored \
                 history, which is not reattaching to an in-flight execution after a restart.",
            ),
            decisions: capability(
                CapabilitySupport::Unsupported,
                "No observed mechanism pauses headless execution to await an out-of-band \
                 decision; permission prompts are resolved locally per --permission-mode.",
            ),
            artifacts: capability(
                CapabilitySupport::Advisory,
                "Write/Edit output lands in the workspace, but only the redacted stdout/stderr \
                 transcript is staged as a log artifact; no per-file artifact discovery is \
                 implemented.",
            ),
            usage: capability(
                CapabilitySupport::Advisory,
                "The harness reports token and cost totals, but an auxiliary model's usage is \
                 folded into `total_cost_usd` while `usage.input_tokens`/`output_tokens` were \
                 observed to cover only the primary turn, so tokens may undercount relative to \
                 cost.",
            ),
            additional: policy_capability(
                CapabilitySupport::Advisory,
                "the tool list is enforced through --tools and cost_usd through \
                 --max-budget-usd; `network: false` only rejects a request naming WebFetch or \
                 WebSearch, and does not stop the Bash tool from reaching the network",
            ),
        }
    }
}

#[cfg(test)]
#[path = "claude_code/tests.rs"]
mod tests;
