//! The opencode CLI as a harness: its command line, how its event stream is
//! read, and what it supports. The lifecycle is
//! [`crate::harness::local_process`].
//!
//! Vendor findings — what is measured, what is a documented guess, and at
//! which version — are in `fixtures/opencode/README.md`.

use std::collections::BTreeMap;

use serde_json::Value;
use tack_orch::execution::{Approvals, CapabilitySupport, FeatureCapabilities};

use crate::harness::{
    DecisionAnswer, DecisionOption, HarnessError, Question, StreamSignal,
    local_process::{
        HarnessDescriptor, HarnessGrammar, Invocation, LocalProcessHarness, ModelSelection,
        RunContext, RunReport, capability, policy_capability,
    },
    process::{ProcessExit, ProcessResult},
};
use crate::provider::Wire;

pub static DESCRIPTOR: HarnessDescriptor = HarnessDescriptor {
    kind: "opencode",
    program: "opencode",
    wire: Wire::OpenAiChatCompletions,
    model_selection: ModelSelection::Explicit(
        "opencode has no default model this adapter can rely on across installs, so a request \
         with no explicit requested_model_provider and requested_model_id is rejected before \
         spawn",
    ),
    native_provider: "opencode",
    inherited_env: &["PATH"],
    model_passthrough: "requested_model_id is forwarded verbatim via `-m tack/<model id>`; \
                        opencode validates it at run time, so no model list is claimed",
    probe_notes: &[
        (
            "tested_version",
            "measured against opencode 1.18.30 (ADR 0067 decision 9); an untested version still \
             probes Advisory rather than a refusal",
        ),
        (
            "served_model",
            "measured against opencode 1.18.30, 2026-09-20: neither the --format json event \
             stream nor opencode export ever names the served model, so it is not available",
        ),
    ],
    credential_note: "opencode reads its provider credential from the `apiKey` field of the \
                      config this adapter injects via OPENCODE_CONFIG_CONTENT, which references \
                      the resolved endpoint's own credential_env_var by name. Tack injects that \
                      variable into the process environment; opencode's own vendor logins are \
                      out of scope (ADR 0067 decision 10).",
    credential_env: None,
    // Measured against opencode 1.18.30, 2026-09-20, with the fake model
    // server answering every chunk's `model` as a value distinct from the
    // requested one: neither the `--format json` event stream
    // (`served_model.ndjson`) nor `opencode export` (`served_model_export.json`)
    // ever names it — both only ever name the requested model.
    observes_served_model: false,
};

pub struct OpencodeGrammar;

pub type OpencodeAdapter<C = crate::SystemClock> = LocalProcessHarness<OpencodeGrammar, C>;

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

/// Whether `tools` names `tool` (case-insensitively), the same convention
/// `claude_code.rs` uses for its own network-tool check.
fn grants(tools: &[String], tool: &str) -> bool {
    tools.iter().any(|name| name.eq_ignore_ascii_case(tool))
}

/// No terminal `step_finish` event at all. A process opencode itself
/// terminates on `SIGTERM` exits 143 with nothing further on stdout
/// (measured); anything else with no terminal event is evidence the
/// transcript never reached one, not that the run was cancelled.
fn cancelled_or_malformed(result: &ProcessResult) -> RunReport {
    if matches!(result.exit, ProcessExit::Exited(143)) {
        return RunReport::verdict(
            false,
            serde_json::json!({
                "reason": "cancelled",
                "detail": "no terminal step_finish event was produced before opencode exited",
            }),
        );
    }
    RunReport::verdict(
        false,
        serde_json::json!({
            "reason": "malformed_output",
            "detail": "no terminal step_finish event was produced",
            "exit": format!("{:?}", result.exit),
            "stdout_prefix": bounded_prefix(&result.stdout.text, 500),
        }),
    )
}

/// Scans every stdout line for `step_finish` (tokens are summed across
/// every one; the last `reason` decides the verdict) and `error`. A tool
/// the policy denied is opencode's own `tool_use` event and never stops
/// the run, so the exit code is never read to decide (ADR 0067 decision
/// 5) — only whether a terminal event was seen at all.
fn parse_run_output(result: &ProcessResult) -> RunReport {
    let mut last_reason = None;
    let mut tokens_in = 0u64;
    let mut tokens_out = 0u64;
    let mut saw_finish = false;
    let mut saw_error = false;

    for line in result.stdout.text.lines() {
        let Ok(event) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        match event.get("type").and_then(Value::as_str) {
            Some("step_finish") => {
                saw_finish = true;
                tokens_in += event
                    .pointer("/part/tokens/input")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                tokens_out += event
                    .pointer("/part/tokens/output")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                last_reason = event
                    .pointer("/part/reason")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
            }
            Some("error") => saw_error = true,
            _ => {}
        }
    }

    if !saw_finish {
        return cancelled_or_malformed(result);
    }
    RunReport {
        succeeded: last_reason.as_deref() == Some("stop") && !saw_error,
        terminal_reason: serde_json::json!({ "reason": last_reason }),
        harness_version: None,
        // Neither the event stream nor `opencode export` ever names the
        // served model (`fixtures/opencode/1.18.30/served_model.ndjson`,
        // `served_model_export.json`, opencode 1.18.30, 2026-09-20) — both
        // only ever name the requested model, so there is nothing to read.
        observed_model: None,
        tokens_in: Some(tokens_in),
        tokens_out: Some(tokens_out),
        duration_ms: None,
        cost_usd: None,
    }
}

/// The working directory `session/new`'s `cwd` names: the same arithmetic
/// `local_process.rs`'s `prepare` uses for the child's own working directory
/// (the workspace root, plus `request.repository.subdirectory` when it is
/// non-empty).
fn working_directory(run: &RunContext<'_>) -> std::path::PathBuf {
    let workspace_root = run.spec.workspace.path.clone();
    match run.spec.work.request.repository.subdirectory.as_deref() {
        Some(subdirectory) if !subdirectory.is_empty() => workspace_root.join(subdirectory),
        _ => workspace_root,
    }
}

fn line_bytes(value: &Value) -> Vec<u8> {
    value.to_string().into_bytes()
}

/// A `session/request_permission` request becomes a question: `vendor_id`
/// is the request's own `id` serialized back to JSON text (so a number
/// stays a number when [`OpencodeGrammar::answer`] parses it back), the
/// prompt names the pending tool call's title, and the options are `once`
/// then `reject` — `always` is never offered, so the operator's own deny
/// (the core answers an unanswered question with the last option) lands on
/// `reject`.
fn permission_question(value: &Value) -> StreamSignal {
    let vendor_id = value.get("id").cloned().unwrap_or(Value::Null).to_string();
    let tool_call = value.pointer("/params/toolCall");
    let title = tool_call
        .and_then(|call| call.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("a tool call");
    let mut metadata = serde_json::Map::new();
    if let Some(tool_call) = tool_call {
        if let Some(id) = tool_call.get("toolCallId") {
            metadata.insert("tool_call_id".to_owned(), id.clone());
        }
        if let Some(kind) = tool_call.get("kind") {
            metadata.insert("kind".to_owned(), kind.clone());
        }
        if let Some(raw_input) = tool_call.get("rawInput") {
            metadata.insert("raw_input".to_owned(), raw_input.clone());
        }
    }
    StreamSignal::Question(Question {
        vendor_id,
        kind: "tool_permission".to_owned(),
        prompt: format!("Allow {title}?"),
        options: vec![
            DecisionOption {
                option_id: "once".to_owned(),
                label: "Allow once".to_owned(),
            },
            DecisionOption {
                option_id: "reject".to_owned(),
                label: "Deny".to_owned(),
            },
        ],
        metadata,
    })
}

/// An `ask` run speaks JSON-RPC over stdout: `session/request_permission`
/// becomes a question; the id-1 and id-2 results drive the rest of the
/// handshake (`session/new`, then `session/prompt`) from the reply itself;
/// the id-3 result closes the run. Neither shape appears in a `run`
/// transcript, so a line this never recognizes falls through to
/// `parse_run_output`, unchanged.
fn parse_output(result: &ProcessResult) -> RunReport {
    for line in result.stdout.text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        let Some(id) = value
            .get("id")
            .and_then(Value::as_i64)
            .filter(|id| (1..=3).contains(id))
        else {
            continue;
        };
        if let Some(error) = value.get("error") {
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("opencode acp reported an error")
                .to_owned();
            return RunReport::verdict(
                false,
                serde_json::json!({"reason": "acp_error", "id": id, "message": message}),
            );
        }
        if id == 3
            && let Some(res) = value.get("result")
        {
            let stop_reason = res.get("stopReason").and_then(Value::as_str);
            return RunReport {
                succeeded: matches!(result.exit, ProcessExit::Exited(0))
                    && stop_reason == Some("end_turn"),
                terminal_reason: serde_json::json!({ "stopReason": stop_reason }),
                harness_version: None,
                observed_model: None,
                tokens_in: res.pointer("/usage/inputTokens").and_then(Value::as_u64),
                tokens_out: res.pointer("/usage/outputTokens").and_then(Value::as_u64),
                duration_ms: None,
                cost_usd: None,
            };
        }
    }
    parse_run_output(result)
}

impl HarnessGrammar for OpencodeGrammar {
    fn descriptor(&self) -> &'static HarnessDescriptor {
        &DESCRIPTOR
    }

    /// A `network: false` request is rejected before spawn: opencode was
    /// measured reaching a non-loopback host on every run — even `--pure`,
    /// a fresh `HOME`/`OPENCODE_CONFIG_DIR` and every `OPENCODE_DISABLE_*`
    /// variable set — so Tack cannot honour a promise this CLI itself
    /// breaks (`fixtures/opencode/README.md`).
    fn invocation(&self, run: &RunContext<'_>) -> Result<Invocation, HarnessError> {
        let request = &run.spec.work.request;
        let Some(endpoint) = run.endpoint else {
            return Err(rejected(
                "no provider endpoint resolved; opencode's own vendor logins are out of scope \
                 for this adapter"
                    .to_owned(),
            ));
        };
        if !request.permission_policy.network {
            return Err(rejected(
                "opencode reaches a non-loopback host on every run regardless of its own \
                 config, so a network: false request cannot be honoured"
                    .to_owned(),
            ));
        }

        let model_id = request
            .requested_model_id
            .as_ref()
            .map_or_else(String::new, |id| id.as_str().to_owned());
        let opencode_home = run.scratch.join("opencode-home");
        let config_dir = opencode_home.join("config");

        // `ask` swaps only the permission surface and the command line
        // measured against a real `opencode acp --pure`: every granted tool
        // is `"ask"` instead of `"allow"`, so each call pauses on a
        // `session/request_permission` request instead of running; `run`
        // (absent counts as `run`) keeps every flag and config field exactly
        // as measured before this ever existed.
        let ask = matches!(request.permission_policy.approvals, Some(Approvals::Ask));
        let granted = if ask { "ask" } else { "allow" };

        let tools = &request.permission_policy.tools;
        let permission = serde_json::json!({
            "webfetch": if request.permission_policy.network { "allow" } else { "deny" },
            "task": if grants(tools, "task") { granted } else { "deny" },
            "edit": if grants(tools, "edit") { granted } else { "deny" },
            "bash": if grants(tools, "bash") { granted } else { "deny" },
        });
        let mut config_content = serde_json::json!({
            "provider": {
                "tack": {
                    "npm": "@ai-sdk/openai-compatible",
                    "options": {
                        "baseURL": endpoint.base_url,
                        "apiKey": format!("{{env:{}}}", endpoint.credential_env_var),
                    },
                    "models": { model_id.clone(): {} },
                },
            },
            "permission": permission,
        });
        if ask {
            // ACP's `session/new` has no `-m` equivalent (measured): the
            // model is named once, at the top level of the injected config,
            // instead.
            config_content["model"] = serde_json::json!(format!("tack/{model_id}"));
        }

        let mut env = BTreeMap::new();
        env.insert("HOME".to_owned(), opencode_home.display().to_string());
        env.insert(
            "OPENCODE_CONFIG_DIR".to_owned(),
            config_dir.display().to_string(),
        );
        env.insert("OPENCODE_DISABLE_PROJECT_CONFIG".to_owned(), "1".to_owned());
        env.insert("OPENCODE_DISABLE_AUTOUPDATE".to_owned(), "1".to_owned());
        env.insert("OPENCODE_DISABLE_MODELS_FETCH".to_owned(), "1".to_owned());
        env.insert("OPENCODE_DISABLE_SHARE".to_owned(), "1".to_owned());
        env.insert(
            "OPENCODE_CONFIG_CONTENT".to_owned(),
            config_content.to_string(),
        );

        let args: Vec<String> = if ask {
            vec!["acp".to_owned(), "--pure".to_owned()]
        } else {
            let mut args: Vec<String> =
                ["run", "--pure", "--format", "json", "--title", "tack", "-m"]
                    .map(str::to_owned)
                    .into();
            args.push(format!("tack/{model_id}"));
            args
        };

        Ok(Invocation {
            args,
            env,
            stdin_stays_open: ask,
        })
    }

    fn report(&self, _run: &RunContext<'_>, result: &ProcessResult) -> RunReport {
        parse_output(result)
    }

    fn capabilities(&self) -> FeatureCapabilities {
        FeatureCapabilities {
            cancel: capability(
                CapabilitySupport::Advisory,
                "SIGTERM produces no terminal event and the bash tool's own child, in its own \
                 process group, was observed still alive afterward; the adapter reports \
                 Cancelled from exit 143 with no confirmation anything it started actually \
                 stopped",
            ),
            resume: capability(
                CapabilitySupport::Unsupported,
                "`opencode run` is one process per invocation with no reattachment interface \
                 observed; --continue/--session start a new process against stored history, not \
                 an in-flight run",
            ),
            decisions: capability(
                CapabilitySupport::Supported,
                "measured against opencode 1.18.30, 2026-09-20: over `opencode acp --pure`, a \
                 tool the request grants is set to \"ask\" in the injected permission block and \
                 each call pauses on one session/request_permission request on stdout until this \
                 adapter answers it once or reject on stdin; `opencode run` auto-rejects instead \
                 with no question ever appearing, so this transport is only used when the \
                 request's approvals is ask",
            ),
            artifacts: capability(
                CapabilitySupport::Advisory,
                "only the captured stdout/stderr is staged as a log artifact; no per-file \
                 artifact discovery reads what the write/edit tools actually touched",
            ),
            usage: capability(
                CapabilitySupport::Advisory,
                "step_finish.tokens are real per-turn measurements, summed across every step; \
                 cost is always null on the installed version for an unrecognized model, so cost \
                 is never reported",
            ),
            additional: policy_capability(
                CapabilitySupport::Advisory,
                "webfetch is gated on permission_policy.network and edit/bash/task on \
                 tool-list membership, each through opencode's own `permission` block, granted \
                 as \"allow\" or, when the request's approvals is ask, as \"ask\" (so \
                 `opencode acp --pure` pauses on session/request_permission instead of running \
                 it); budgets are not passed at all",
            ),
        }
    }

    /// An `ask` run reads ACP over stdin: the first bytes are the one
    /// `initialize` request measured against a real `opencode acp --pure`
    /// (`fixtures/opencode/1.18.30/asking_acp.txt`); the rest of the
    /// handshake is driven from the replies `signal` reads. Any other run
    /// takes the prompt as plain text on stdin, as today.
    fn prompt(&self, prompt: String, stdin_stays_open: bool) -> Vec<u8> {
        if !stdin_stays_open {
            return prompt.into_bytes();
        }
        let initialize = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": 1,
                "clientCapabilities": {"fs": {"readTextFile": false, "writeTextFile": false}},
            },
        });
        let mut line = initialize.to_string().into_bytes();
        line.push(b'\n');
        line
    }

    /// The id-1 result replies with `session/new`; the id-2 result replies
    /// with `session/prompt`, carrying the session id it names and this
    /// request's own prompt text; `session/request_permission` becomes a
    /// question; the id-3 result, or a JSON-RPC `error` on id 1-3, ends the
    /// conversation. A `session/update` notification (no numeric `id`) means
    /// nothing to the core here.
    fn signal(&self, run: &RunContext<'_>, line: &str) -> Option<StreamSignal> {
        let value: Value = serde_json::from_str(line.trim()).ok()?;
        if value.get("method").and_then(Value::as_str) == Some("session/request_permission") {
            return Some(permission_question(&value));
        }
        let id = value.get("id")?.as_i64()?;
        if value.get("error").is_some() {
            return Some(StreamSignal::Finished);
        }
        let result = value.get("result")?;
        match id {
            1 => Some(StreamSignal::Reply(line_bytes(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "session/new",
                "params": {
                    "cwd": working_directory(run).display().to_string(),
                    "mcpServers": [],
                },
            })))),
            2 => {
                let session_id = result.get("sessionId")?.as_str()?;
                let prompt = run
                    .spec
                    .work
                    .request
                    .resolved_agent_profile
                    .instructions
                    .clone();
                Some(StreamSignal::Reply(line_bytes(&serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 3,
                    "method": "session/prompt",
                    "params": {
                        "sessionId": session_id,
                        "prompt": [{"type": "text", "text": prompt}],
                    },
                }))))
            }
            3 => Some(StreamSignal::Finished),
            _ => None,
        }
    }

    /// `once` is the only answer that releases the tool call; every other
    /// answer — including one this grammar never offered — is `reject`,
    /// which `session/request_permission`'s own reply shape always accepts.
    /// `vendor_id` is parsed back to the JSON value it was serialized from,
    /// so a numeric request id is answered as a number.
    fn answer(&self, question: &Question, answer: &DecisionAnswer) -> Vec<u8> {
        let option_id = if answer.option_id.as_deref() == Some("once") {
            "once"
        } else {
            "reject"
        };
        let id: Value = serde_json::from_str(&question.vendor_id).unwrap_or(Value::Null);
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {"outcome": {"outcome": "selected", "optionId": option_id}},
        });
        serde_json::to_vec(&payload).unwrap_or_default()
    }
}

#[cfg(test)]
#[path = "opencode/tests.rs"]
mod tests;
