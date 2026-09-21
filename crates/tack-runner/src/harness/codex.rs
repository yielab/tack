//! The Codex CLI as a harness: its command line, how its exit is read, and
//! what it supports. The lifecycle is [`crate::harness::local_process`].
//!
//! Vendor findings — what is measured, what is a documented guess, and at
//! which version — are in `fixtures/codex/README.md`.

use tack_orch::execution::{CapabilitySupport, FeatureCapabilities};

use crate::harness::{
    HarnessError,
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
        "codex does not report which model an auto-selected run used, so the model that ran \
         could not be recorded; requested_model_provider and requested_model_id are both \
         required",
    ),
    native_provider: "openai",
    inherited_env: &[],
    model_passthrough: "requested_model_id is forwarded verbatim via --model and a request \
                        without one is rejected before spawn; the Codex CLI validates the model \
                        at run time, so no model list is claimed",
    probe_notes: &[],
    credential_note: "Codex authenticates itself (its own CLI login, or an API key it reads from \
                      its own config). Tack never reads, stores or forwards it. No host \
                      environment is forwarded into a run: only entries set on the execution \
                      request's own `environment` reach the codex process.",
    credential_env: None,
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

fn bounded_preview(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let truncated: String = text.chars().take(max_chars).collect();
    format!("{truncated}\u{2026} (truncated)")
}

fn describe_capture(output: &CapturedOutput) -> serde_json::Value {
    serde_json::json!({
        "truncated": output.truncated,
        "bytes_dropped": output.bytes_dropped,
        "total_bytes_seen": output.total_bytes_seen,
        "text_preview": bounded_preview(&output.text, 2000),
    })
}

impl HarnessGrammar for CodexGrammar {
    fn descriptor(&self) -> &'static HarnessDescriptor {
        &DESCRIPTOR
    }

    /// `permission_policy` and `budgets` are not applied; `capabilities`
    /// declares that rather than guessing a mapping onto Codex's sandbox
    /// and approval flags, which have not been measured.
    fn invocation(&self, run: &RunContext<'_>) -> Result<Invocation, HarnessError> {
        let mut args = Vec::new();
        // `-c` overrides are global flags and must precede `exec`.
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
        let model = run.spec.work.request.requested_model_id.as_ref();
        args.extend(["exec".to_owned(), "--json".to_owned(), "--model".to_owned()]);
        args.push(model.map_or_else(String::new, |model| model.as_str().to_owned()));
        Ok(Invocation {
            args,
            ..Invocation::default()
        })
    }

    /// Read from the exit status alone. The shape of `exec --json` output
    /// is unverified, so its content is kept as evidence and never
    /// interpreted; usage and the served model stay unmeasured.
    fn report(&self, _run: &RunContext<'_>, result: &ProcessResult) -> RunReport {
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
        RunReport::verdict(
            succeeded,
            serde_json::json!({
                "code": code,
                "message": message,
                "stdout": describe_capture(&result.stdout),
                "stderr": describe_capture(&result.stderr),
            }),
        )
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
                CapabilitySupport::Unsupported,
                "codex's approval behaviour has not been observed; nothing pauses a run to \
                 await a decision",
            ),
            artifacts: capability(
                CapabilitySupport::Advisory,
                "only the captured stdout/stderr is staged as a log artifact; no per-file \
                 artifact discovery is implemented",
            ),
            usage: capability(
                CapabilitySupport::Unsupported,
                "token and cost usage have not been observed in codex output; only wall-clock \
                 duration is measured",
            ),
            additional: policy_capability(
                CapabilitySupport::Unsupported,
                "the request's tool list, network flag and budgets are not passed to codex; \
                 its sandbox and approval flags have not been measured",
            ),
        }
    }
}

#[cfg(test)]
#[path = "codex/tests.rs"]
mod tests;
