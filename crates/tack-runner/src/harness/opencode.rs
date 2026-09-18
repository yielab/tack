//! The opencode CLI as a harness: its command line, how its event stream is
//! read, and what it supports. The lifecycle is
//! [`crate::harness::local_process`].
//!
//! Vendor findings — what is measured, what is a documented guess, and at
//! which version — are in `fixtures/opencode/README.md`.

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
    min_capture_bytes: (0, 0),
    model_passthrough: "requested_model_id is forwarded verbatim via `-m tack/<model id>`; \
                        opencode validates it at run time, so no model list is claimed",
    probe_notes: &[(
        "tested_version",
        "measured against opencode 1.18.30 (ADR 0067 decision 9); an untested version still \
         probes Advisory rather than a refusal",
    )],
    credential_note: "opencode reads its provider credential from the `apiKey` field of the \
                      config this adapter injects via OPENCODE_CONFIG_CONTENT, which references \
                      the resolved endpoint's own credential_env_var by name. Tack injects that \
                      variable into the process environment; opencode's own vendor logins are \
                      out of scope (ADR 0067 decision 10).",
    credential_env: None,
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
        observed_model: None,
        tokens_in: Some(tokens_in),
        tokens_out: Some(tokens_out),
        duration_ms: None,
        cost_usd: None,
    }
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

        let tools = &request.permission_policy.tools;
        let permission = serde_json::json!({
            "webfetch": if request.permission_policy.network { "allow" } else { "deny" },
            "task": if grants(tools, "task") { "allow" } else { "deny" },
            "edit": if grants(tools, "edit") { "allow" } else { "deny" },
            "bash": if grants(tools, "bash") { "allow" } else { "deny" },
        });
        let config_content = serde_json::json!({
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

        let mut args: Vec<String> = ["run", "--pure", "--format", "json", "--title", "tack", "-m"]
            .map(str::to_owned)
            .into();
        args.push(format!("tack/{model_id}"));

        Ok(Invocation { args, env })
    }

    fn report(&self, _run: &RunContext<'_>, result: &ProcessResult) -> RunReport {
        parse_run_output(result)
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
                CapabilitySupport::Unsupported,
                "a denied permission auto-rejects rather than pausing in non-interactive mode; \
                 no question event was observed on stdout",
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
                 tool-list membership, each through opencode's own `permission` block; budgets \
                 are not passed at all",
            ),
        }
    }
}

#[cfg(test)]
#[path = "opencode/tests.rs"]
mod tests;
