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

    /// The verdict is read from the exit status alone: the failure shape of
    /// `exec --json` output is unverified, so it is kept as evidence and
    /// never interpreted for that. Usage is read from the terminal
    /// `turn.completed` line when the stream has one; the served model is
    /// still not named anywhere in the stream.
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
                CapabilitySupport::Advisory,
                "input_tokens and output_tokens are real measurements, read from `exec \
                 --json`'s terminal `turn.completed` line, measured on codex-cli 0.149.1; no \
                 cost_usd field exists in the output, so cost is never reported",
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
