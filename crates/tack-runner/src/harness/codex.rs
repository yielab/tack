//! The Codex CLI as a harness: its command line, how its exit is read, and
//! what it supports. The lifecycle is [`crate::harness::local_process`].
//!
//! Vendor findings — what is measured, what is a documented guess, and at
//! which version — are in `fixtures/codex/README.md`.

use tack_orch::execution::{Approvals, CapabilitySupport, FeatureCapabilities};

use crate::harness::{
    DecisionAnswer, DecisionOption, HarnessError, Question, StreamSignal,
    local_process::{
        HarnessDescriptor, HarnessGrammar, Invocation, LocalProcessHarness, ModelSelection,
        RunContext, RunReport, capability, policy_capability,
    },
    process::{CapturedOutput, ProcessExit, ProcessResult},
};
use crate::provider::Wire;

pub static DESCRIPTOR: HarnessDescriptor = HarnessDescriptor {
    kind: "codex",
    program: "codex",
    wire: Wire::OpenAiResponses,
    model_selection: ModelSelection::Explicit(
        "the served model is not available on codex-cli 0.149.1 (measured 2026-09-20): no \
         line of `exec --json` names which model answered, so the model that ran could not \
         be recorded; requested_model_provider and requested_model_id are both required",
    ),
    native_provider: "openai",
    inherited_env: &[],
    model_passthrough: "requested_model_id is forwarded verbatim via --model and a request \
                        without one is rejected before spawn; the Codex CLI validates the model \
                        at run time, so no model list is claimed",
    probe_notes: &[(
        "served_model",
        "measured against codex-cli 0.149.1, 2026-09-20: no line of `exec --json` names the \
         model that answered; the only model-shaped line is a `type:\"error\"` item echoing \
         the `--model` flag back, so it is not available",
    )],
    credential_note: "Codex authenticates itself (its own CLI login, or an API key it reads from \
                      its own config). Tack never reads, stores or forwards it. No host \
                      environment is forwarded into a run: only entries set on the execution \
                      request's own `environment` reach the codex process.",
    credential_env: None,
    // Measured against codex-cli 0.149.1, 2026-09-20: no line of `exec
    // --json` ever names a served model distinct from the requested one —
    // the only model-shaped line is a `type:"error"` item echoing the
    // `--model` flag back.
    observes_served_model: false,
};

/// The key an injected provider endpoint is named under in Codex's
/// `-c model_providers.<key>.*` overrides. Per invocation only: nothing
/// writes `~/.codex/config.toml`.
const PROVIDER_KEY: &str = "tack_provider";

pub struct CodexGrammar;

pub type CodexAdapter<C = crate::SystemClock> = LocalProcessHarness<CodexGrammar, C>;

/// A double-quoted TOML string, the form `-c key="value"` expects. Only
/// ever given a URL, a variable name or a display label.
fn toml_quoted(value: &str) -> String {
    format!("{value:?}")
}

/// Whether `tools` names `tool` (case-insensitively), the same convention
/// `opencode.rs`'s own `grants` uses.
fn grants(tools: &[String], tool: &str) -> bool {
    tools.iter().any(|name| name.eq_ignore_ascii_case(tool))
}

fn bounded_preview(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let truncated: String = text.chars().take(max_chars).collect();
    format!("{truncated}\u{2026} (truncated)")
}

/// Scans every stdout line for the terminal `turn.completed` line and
/// reads `usage.input_tokens`/`usage.output_tokens` off it — the only two
/// fields `RunReport` has a place for (measured on codex-cli 0.149.1,
/// 2026-09-20; `fixtures/codex/README.md`). The cached/cache-write/
/// reasoning breakdown the same object carries has no field in
/// `RunReport`, so it is not folded in or guessed at, and `cost_usd` stays
/// `None`: no such field exists in the output. A stream with no
/// `turn.completed` line leaves both `None`.
fn usage_from_output(stdout: &str) -> (Option<u64>, Option<u64>) {
    for line in stdout.lines() {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
            continue;
        };
        if event.get("type").and_then(serde_json::Value::as_str) != Some("turn.completed") {
            continue;
        }
        let tokens_in = event
            .pointer("/usage/input_tokens")
            .and_then(serde_json::Value::as_u64);
        let tokens_out = event
            .pointer("/usage/output_tokens")
            .and_then(serde_json::Value::as_u64);
        return (tokens_in, tokens_out);
    }
    (None, None)
}

fn describe_capture(output: &CapturedOutput) -> serde_json::Value {
    serde_json::json!({
        "truncated": output.truncated,
        "bytes_dropped": output.bytes_dropped,
        "total_bytes_seen": output.total_bytes_seen,
        "text_preview": bounded_preview(&output.text, 2000),
    })
}

/// `--sandbox`'s value, and the value `signal`'s `thread/start` request puts
/// under `sandbox`: a write-capable tool (`bash`, `shell`, `edit`, `write`
/// or `apply_patch`, case-insensitively) selects `workspace-write`; an empty
/// or read-only tool list selects `read-only` (measured accepted by
/// `codex app-server` too — `fixtures/codex/README.md` "Measured",
/// `app-server-thread-start.txt`).
fn sandbox_mode(tools: &[String]) -> &'static str {
    let write_capable = ["bash", "shell", "edit", "write", "apply_patch"]
        .iter()
        .any(|tool| grants(tools, tool));
    if write_capable {
        "workspace-write"
    } else {
        "read-only"
    }
}

/// Reads an `app-server` stream's verdict and usage, or `None` when the
/// stream carries neither a `turn/completed` line nor a JSON-RPC `error`
/// answering the `thread/start` (id 2) or `turn/start` (id 3) request —
/// the two shapes `exec --json` output never produces, so their absence is
/// what tells `report` to fall back to its exit-code-only path instead.
fn app_server_report(result: &ProcessResult) -> Option<RunReport> {
    let mut turn_status: Option<String> = None;
    let mut rpc_error: Option<String> = None;
    let mut tokens_in: Option<u64> = None;
    let mut tokens_out: Option<u64> = None;
    for line in result.stdout.text.lines() {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
            continue;
        };
        if matches!(
            event.get("id").and_then(serde_json::Value::as_u64),
            Some(2) | Some(3)
        ) && let Some(error) = event.get("error")
        {
            rpc_error = Some(
                error
                    .get("message")
                    .and_then(serde_json::Value::as_str)
                    .map_or_else(|| error.to_string(), str::to_owned),
            );
        }
        match event.get("method").and_then(serde_json::Value::as_str) {
            Some("thread/tokenUsage/updated") => {
                if let Some(value) = event
                    .pointer("/params/tokenUsage/total/inputTokens")
                    .and_then(serde_json::Value::as_u64)
                {
                    tokens_in = Some(value);
                }
                if let Some(value) = event
                    .pointer("/params/tokenUsage/total/outputTokens")
                    .and_then(serde_json::Value::as_u64)
                {
                    tokens_out = Some(value);
                }
            }
            Some("turn/completed") => {
                turn_status = event
                    .pointer("/params/turn/status")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned);
            }
            _ => {}
        }
    }
    if rpc_error.is_none() && turn_status.is_none() {
        return None;
    }
    let succeeded = rpc_error.is_none()
        && result.exit == ProcessExit::Exited(0)
        && turn_status.as_deref() == Some("completed");
    let (code, message) = match &rpc_error {
        Some(error) => ("app_server_error", error.clone()),
        None if succeeded => ("completed", "codex app-server turn completed".to_owned()),
        None => (
            "turn_failed",
            format!(
                "codex app-server turn finished with status {}",
                turn_status.as_deref().unwrap_or("unknown")
            ),
        ),
    };
    Some(RunReport {
        succeeded,
        terminal_reason: serde_json::json!({
            "code": code,
            "message": message,
            "stdout": describe_capture(&result.stdout),
            "stderr": describe_capture(&result.stderr),
        }),
        harness_version: None,
        observed_model: None,
        tokens_in,
        tokens_out,
        duration_ms: None,
        cost_usd: None,
    })
}

impl HarnessGrammar for CodexGrammar {
    fn descriptor(&self) -> &'static HarnessDescriptor {
        &DESCRIPTOR
    }

    /// `permission_policy.tools` maps onto `--sandbox`: a write-capable tool
    /// (`bash`, `shell`, `edit`, `write` or `apply_patch`, case-insensitively
    /// — the `grants` convention `opencode.rs` uses) selects
    /// `--sandbox workspace-write`; an empty or read-only tool list selects
    /// `--sandbox read-only`. Never `danger-full-access`, never
    /// `--dangerously-bypass-approvals-and-sandbox`. `permission_policy.approvals ==
    /// Some(Approvals::Ask)` adds no flag to `exec` — on codex-cli 0.149.1 `exec --json`
    /// never prompts under any flag measured (`exec-tool-call.jsonl`,
    /// `exec-sandbox-read-only.jsonl`, `exec-approve-for-me.jsonl`,
    /// `exec-bypass-approvals-and-sandbox.jsonl`) — it instead swaps the whole
    /// invocation for the `-c` overrides (unchanged) followed by `app-server` alone,
    /// which speaks newline-delimited JSON-RPC over stdio and does ask
    /// (`fixtures/codex/README.md` "Measured"); `signal`/`answer` drive that protocol.
    /// Any other request keeps today's `exec` line, byte for byte. `network` and
    /// `budgets` have no `exec` flag and are not passed; `capabilities` declares
    /// `permission_policy` advisory for that reason.
    fn invocation(&self, run: &RunContext<'_>) -> Result<Invocation, HarnessError> {
        let mut args = Vec::new();
        // `-c` overrides are global flags and must precede the subcommand.
        if let Some(endpoint) = run.endpoint {
            let overrides = [
                format!("model_provider={PROVIDER_KEY}"),
                format!(
                    "model_providers.{PROVIDER_KEY}.name={}",
                    toml_quoted(&endpoint.display_name)
                ),
                format!(
                    "model_providers.{PROVIDER_KEY}.base_url={}",
                    toml_quoted(&endpoint.base_url)
                ),
                format!(
                    "model_providers.{PROVIDER_KEY}.env_key={}",
                    toml_quoted(&endpoint.credential_env_var)
                ),
                format!(
                    "model_providers.{PROVIDER_KEY}.wire_api={}",
                    toml_quoted("responses")
                ),
            ];
            for value in overrides {
                args.extend(["-c".to_owned(), value]);
            }
        }
        if matches!(
            run.spec.work.request.permission_policy.approvals,
            Some(Approvals::Ask)
        ) {
            args.push("app-server".to_owned());
            return Ok(Invocation {
                args,
                stdin_stays_open: true,
                ..Invocation::default()
            });
        }
        let model = run.spec.work.request.requested_model_id.as_ref();
        let sandbox = sandbox_mode(&run.spec.work.request.permission_policy.tools);
        args.extend([
            "exec".to_owned(),
            "--json".to_owned(),
            "--sandbox".to_owned(),
            sandbox.to_owned(),
            "--model".to_owned(),
        ]);
        args.push(model.map_or_else(String::new, |model| model.as_str().to_owned()));
        Ok(Invocation {
            args,
            ..Invocation::default()
        })
    }

    /// An `app-server` run's first stdin line is the JSON-RPC `initialize`
    /// request `signal` needs a reply to before it can drive `thread/start`;
    /// any other run takes the prompt as plain text, unframed, exactly as
    /// before.
    fn prompt(&self, prompt: String, stdin_stays_open: bool) -> Vec<u8> {
        if !stdin_stays_open {
            return prompt.into_bytes();
        }
        let message = serde_json::json!({
            "id": 1,
            "method": "initialize",
            "params": {"clientInfo": {"name": "tack", "version": env!("CARGO_PKG_VERSION")}},
        });
        let mut line = message.to_string().into_bytes();
        line.push(b'\n');
        line
    }

    /// Drives the `app-server` handshake from each reply
    /// (`initialize` -> `thread/start` -> `turn/start`) and turns one
    /// `item/commandExecution/requestApproval` line into a question; a
    /// `turn/completed` line, or a JSON-RPC `error` answering `thread/start`
    /// (id 2) or `turn/start` (id 3), ends the run (measured;
    /// `fixtures/codex/README.md` "Measured"). Only ever consulted for an
    /// `ask` run.
    fn signal(&self, run: &RunContext<'_>, line: &str) -> Option<StreamSignal> {
        let value: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
        let id = value.get("id").and_then(serde_json::Value::as_u64);
        let method = value.get("method").and_then(serde_json::Value::as_str);

        if id == Some(1) && value.get("result").is_some() {
            let request = &run.spec.work.request;
            let workspace_root = &run.spec.workspace.path;
            let cwd = match request.repository.subdirectory.as_deref() {
                Some(subdirectory) if !subdirectory.is_empty() => workspace_root.join(subdirectory),
                _ => workspace_root.clone(),
            };
            let sandbox = sandbox_mode(&request.permission_policy.tools);
            let model = request
                .requested_model_id
                .as_ref()
                .map_or_else(String::new, |model| model.as_str().to_owned());
            let mut params = serde_json::Map::new();
            params.insert(
                "cwd".to_owned(),
                serde_json::json!(cwd.display().to_string()),
            );
            params.insert("approvalPolicy".to_owned(), serde_json::json!("on-request"));
            params.insert("sandbox".to_owned(), serde_json::json!(sandbox));
            params.insert("model".to_owned(), serde_json::json!(model));
            if run.endpoint.is_some() {
                params.insert("modelProvider".to_owned(), serde_json::json!(PROVIDER_KEY));
            }
            let reply = serde_json::json!({"id": 2, "method": "thread/start", "params": params});
            return Some(StreamSignal::Reply(
                serde_json::to_vec(&reply).unwrap_or_default(),
            ));
        }
        if id == Some(2) && value.get("result").is_some() {
            let thread_id = value.pointer("/result/thread/id")?.clone();
            let prompt = run
                .spec
                .work
                .request
                .resolved_agent_profile
                .instructions
                .clone();
            let reply = serde_json::json!({
                "id": 3,
                "method": "turn/start",
                "params": {
                    "threadId": thread_id,
                    "approvalPolicy": "on-request",
                    "input": [{"type": "text", "text": prompt}],
                },
            });
            return Some(StreamSignal::Reply(
                serde_json::to_vec(&reply).unwrap_or_default(),
            ));
        }
        if method == Some("item/commandExecution/requestApproval") {
            let params = value.get("params")?;
            let command = params.get("command").and_then(serde_json::Value::as_str)?;
            let reason = params.get("reason").and_then(serde_json::Value::as_str);
            let mut prompt = format!("Allow command: {command}?");
            if let Some(reason) = reason {
                prompt.push_str(&format!(" ({reason})"));
            }
            let mut metadata = serde_json::Map::new();
            metadata.insert("command".to_owned(), serde_json::json!(command));
            if let Some(cwd) = params.get("cwd") {
                metadata.insert("cwd".to_owned(), cwd.clone());
            }
            if let Some(reason) = reason {
                metadata.insert("reason".to_owned(), serde_json::json!(reason));
            }
            if let Some(item_id) = params.get("itemId") {
                metadata.insert("item_id".to_owned(), item_id.clone());
            }
            if let Some(turn_id) = params.get("turnId") {
                metadata.insert("turn_id".to_owned(), turn_id.clone());
            }
            return Some(StreamSignal::Question(Question {
                vendor_id: value.get("id")?.to_string(),
                kind: "tool_permission".to_owned(),
                prompt,
                options: vec![
                    DecisionOption {
                        option_id: "accept".to_owned(),
                        label: "Allow once".to_owned(),
                    },
                    DecisionOption {
                        option_id: "decline".to_owned(),
                        label: "Deny".to_owned(),
                    },
                ],
                metadata,
            }));
        }
        if method == Some("turn/completed") {
            return Some(StreamSignal::Finished);
        }
        if matches!(id, Some(2) | Some(3)) && value.get("error").is_some() {
            return Some(StreamSignal::Finished);
        }
        None
    }

    /// `accept` is the only option that releases the pending command;
    /// anything else — including an option this grammar never offered — is
    /// answered `decline` rather than left to hang codex's own pending
    /// request (measured; `fixtures/codex/README.md` "Measured"). The
    /// vendor id round-trips through JSON so a number stays a number.
    fn answer(&self, question: &Question, answer: &DecisionAnswer) -> Vec<u8> {
        let decision = if answer.option_id.as_deref() == Some("accept") {
            "accept"
        } else {
            "decline"
        };
        let id: serde_json::Value =
            serde_json::from_str(&question.vendor_id).unwrap_or(serde_json::Value::Null);
        let payload = serde_json::json!({"id": id, "result": {"decision": decision}});
        serde_json::to_vec(&payload).unwrap_or_default()
    }

    /// An `app-server` run's turn/completed line (and any JSON-RPC `error`
    /// answering `thread/start`/`turn/start`) decides the verdict; any other
    /// run is read the way it always was: the exit status alone, the
    /// failure shape of `exec --json` output being unverified. Usage for an
    /// `app-server` run comes from its last `thread/tokenUsage/updated`
    /// notification instead of the `turn.completed` line `exec --json`
    /// prints; the served model is still not named anywhere in either
    /// stream.
    fn report(&self, _run: &RunContext<'_>, result: &ProcessResult) -> RunReport {
        if let Some(report) = app_server_report(result) {
            return report;
        }
        let (succeeded, code, message) = match result.exit {
            ProcessExit::Exited(0) => (true, "completed", "codex exited successfully".to_owned()),
            ProcessExit::Exited(code) => (
                false,
                "exit_code",
                format!("codex exited with status {code}"),
            ),
            #[cfg(unix)]
            ProcessExit::Signaled(signal) => (
                false,
                "signaled",
                format!("codex was terminated by signal {signal}"),
            ),
            ProcessExit::TimedOut => (
                false,
                "timed_out",
                "codex exceeded its configured timeout and was killed".to_owned(),
            ),
        };
        let (tokens_in, tokens_out) = usage_from_output(&result.stdout.text);
        RunReport {
            succeeded,
            terminal_reason: serde_json::json!({
                "code": code,
                "message": message,
                "stdout": describe_capture(&result.stdout),
                "stderr": describe_capture(&result.stderr),
            }),
            harness_version: None,
            // The served model is not named anywhere in `exec --json`'s
            // stream (measured against codex-cli 0.149.1, 2026-09-20; see
            // `fixtures/codex/README.md`), so nothing is read for it here.
            observed_model: None,
            tokens_in,
            tokens_out,
            duration_ms: None,
            cost_usd: None,
        }
    }

    fn capabilities(&self) -> FeatureCapabilities {
        FeatureCapabilities {
            cancel: capability(
                CapabilitySupport::Advisory,
                "the codex process is always signalled (it leads its own process group), but a \
                 tool-spawned descendant that starts its own session is only reached if it exits \
                 within the SIGTERM grace period; SIGKILL cannot reach another session's group",
            ),
            resume: capability(
                CapabilitySupport::Unsupported,
                "codex session resume has not been observed and is not implemented",
            ),
            decisions: capability(
                CapabilitySupport::Supported,
                "measured against codex-cli 0.149.1 (`app-server-approval.txt`): over `codex \
                 app-server` with `approvalPolicy: \"on-request\"`, a command that needs to \
                 escalate beyond the sandbox pauses on one \
                 `item/commandExecution/requestApproval` request until it is answered `accept` \
                 or `decline`; a command inside the sandbox runs without asking. `codex exec`, \
                 which this adapter drives for every other request, never asks under any flag \
                 measured, so `app-server` is only ever used when the request itself asks",
            ),
            artifacts: capability(
                CapabilitySupport::Advisory,
                "only the captured stdout/stderr is staged as a log artifact; no per-file \
                 artifact discovery is implemented",
            ),
            usage: capability(
                CapabilitySupport::Advisory,
                "input_tokens and output_tokens are real measurements, read from `exec \
                 --json`'s terminal `turn.completed` line, measured on codex-cli 0.149.1; no \
                 cost_usd field exists in the output, so cost is never reported",
            ),
            additional: policy_capability(
                CapabilitySupport::Advisory,
                "the request's tool list is enforced as codex's sandbox mode (a write-capable \
                 tool grants `--sandbox workspace-write`, none grants `--sandbox read-only`; \
                 read-only's denial of a write was observed, and workspace-write's denial of a \
                 write outside the workspace was observed through `--approve-for-me`); network \
                 and budgets are not passed at all because `codex exec` 0.149.1 has no flag for \
                 either",
            ),
        }
    }
}

#[cfg(test)]
#[path = "codex/tests.rs"]
mod tests;
