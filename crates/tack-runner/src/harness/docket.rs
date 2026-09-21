//! The docket CLI as a harness: its command line, how its NDJSON result
//! line is read, and what it supports. The lifecycle is
//! [`crate::harness::local_process`].
//!
//! Vendor findings — what is measured, what is a documented guess, and at
//! which version — are in `fixtures/docket/README.md`.

use std::collections::BTreeMap;

use tack_orch::execution::{CapabilitySupport, FeatureCapabilities};

use crate::harness::{
    HarnessError,
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
};

/// The variable docket's own `run` command refuses to start without, unless
/// the request's own `environment` already names it (`DOCKET_HOME` and
/// `--workspace` are supplied unconditionally by this grammar, so only the
/// endpoint is ever missing).
const BASE_URL_ENV: &str = "DOCKET_LLM_BASE_URL";

pub struct DocketGrammar;

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

impl HarnessGrammar for DocketGrammar {
    fn descriptor(&self) -> &'static HarnessDescriptor {
        &DESCRIPTOR
    }

    /// `permission_policy` and `budgets` are not applied: `capabilities`
    /// declares that rather than guessing a mapping onto docket's own
    /// per-tool policy engine.
    fn invocation(&self, run: &RunContext<'_>) -> Result<Invocation, HarnessError> {
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

        Ok(Invocation {
            args: [
                "harness",
                "run",
                "--workspace",
                &workspace_root.display().to_string(),
                "--task-file",
                "/dev/stdin",
                "--model",
                &format!("{provider}/{model_id}"),
                "--agent-id",
                run.spec.work.lease.attempt_id.as_str(),
                "--timeout",
                &request.timeout_seconds.to_string(),
            ]
            .map(str::to_owned)
            .to_vec(),
            env,
            ..Invocation::default()
        })
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
            }),
            harness_version: None,
            observed_model: (!parsed.model.served.is_empty()).then_some(parsed.model.served),
            tokens_in: Some(parsed.usage.input_tokens),
            tokens_out: Some(parsed.usage.output_tokens),
            duration_ms: None,
            cost_usd: None,
        }
    }

    fn capabilities(&self) -> FeatureCapabilities {
        FeatureCapabilities {
            cancel: capability(
                CapabilitySupport::Advisory,
                "docket stops cooperatively on SIGTERM and reports a cancelled result, but does \
                 not emit an event per child process group its own tools start, so a tool's \
                 process group that outlives the grace period cannot be confirmed stopped",
            ),
            resume: capability(
                CapabilitySupport::Unsupported,
                "harness mode is one synchronous run to completion; no reattachment interface \
                 is documented or observed",
            ),
            decisions: capability(
                CapabilitySupport::Unsupported,
                "harness mode's approval posture is fixed to non-interactive refusal: a tool \
                 call that would otherwise wait for a human is denied immediately as `blocked` \
                 rather than pausing the run to ask one",
            ),
            artifacts: capability(
                CapabilitySupport::Advisory,
                "only the captured stdout/stderr is staged as a log artifact; the result line's \
                 `blocked` detail is kept in the terminal reason, but no per-file artifact \
                 discovery is implemented",
            ),
            usage: capability(
                CapabilitySupport::Advisory,
                "the result line's usage.input_tokens/output_tokens are real measurements, but \
                 cost_usd is always null on the installed version, so cost is never reported",
            ),
            additional: policy_capability(
                CapabilitySupport::Unsupported,
                "docket applies its own per-tool policy engine; the request's tool list, \
                 network flag and budgets are not passed to it",
            ),
        }
    }
}

#[cfg(test)]
#[path = "docket/tests.rs"]
mod tests;
