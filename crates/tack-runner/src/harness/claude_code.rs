//! Claude Code harness grammar.
//!
//! Implements [`crate::harness::local_process::HarnessGrammar`] for
//! `harness_kind = "claude-code"`; [`ClaudeCodeAdapter`] is
//! [`crate::harness::local_process::LocalProcessHarness<ClaudeCodeGrammar>`], which
//! carries the shared local-process lifecycle (`crate::harness::local_process`)
//! and composes the shared process/redaction/artifact infrastructure
//! (`crate::harness::{process, redact, artifact}`).
//!
//! Vendor findings — what is measured, what is a documented guess, and at what observed
//! version: `fixtures/claude_code/README.md`, next to the transcripts that prove them.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use tack_orch::execution::{
    ActualExecution, ActualModelId, ActualModelProvider, CapabilitySupport, CapabilityValue,
    FeatureCapabilities, HarnessKind as DomainHarnessKind, Measurement, MeasurementSource,
    Usage as DomainUsage, WorkspaceId as DomainWorkspaceId,
};

use crate::client::AttemptState;
use crate::config::ProviderConfig;
use crate::harness::{
    CancelObservation, ExecutionSpec, HarnessError, HarnessOutcome, ModelObservationSource,
    RecoveryObservation,
    local_process::{HarnessGrammar, LocalProcessHarness, PreparedRun},
    process::{
        CancelOutcome, ProcessError, ProcessExit, ProcessLimits, ProcessResult, ProcessSpec,
    },
    redact::SecretMaterial,
};
use crate::provider::ProviderEndpoint;
use crate::secrets::SecretStore;
// Re-exported (unused by this module's own production code) purely so
// `claude_code::tests` — a child module that relies on `use super::*` for
// everything else this file already imports — keeps seeing the frozen
// `HarnessAdapter`/`HarnessProbe` call boundary and its handle/journal
// types without a second, parallel import list.
#[cfg(test)]
pub(crate) use crate::client::Timestamp;
#[cfg(test)]
pub(crate) use crate::harness::{AttemptJournal, HarnessAdapter, HarnessProbe, LocalRunHandle};
// `reconcile`'s own liveness check moved into the shared
// `local_process::LocalProcessHarness::reconcile` (it calls `process_alive`
// generically for every grammar before ever asking `reconcile_alive`
// anything); this grammar's own production code no longer calls it
// directly, but several tests still probe real process liveness themselves.
#[cfg(all(test, unix))]
pub(crate) use crate::harness::process::process_alive;

/// The wire value for this harness, matching
/// `registry::HarnessKind::ClaudeCode.as_str()`.
const HARNESS_KIND: &str = "claude-code";

/// Provider families the installed `claude` 2.1.223 binary genuinely knows
/// about on its own — facts about the binary, never about how Tack is
/// configured. `"anthropic"` is the first-party default (no flag needed);
/// the other three are switched via environment variables the CLI itself
/// documents only indirectly (`--bare`'s help text names them collectively
/// as "3P providers"). Their exact names were confirmed by `strings` against
/// the installed binary (`ANTHROPIC_BEDROCK_BASE_URL`, `ANTHROPIC_VERTEX_*`,
/// `CLAUDE_CODE_USE_BEDROCK`, `CLAUDE_CODE_USE_VERTEX`,
/// `CLAUDE_CODE_USE_FOUNDRY` all present) — static inspection of the shipped
/// artifact, not a live provider switch (never attempted: it would need real
/// cloud credentials that would not be fabricated for this). A
/// Tack-configured provider (Vercel's gateway, Anthropic's own API used as a
/// key+endpoint rather than the native mode above) is not one of these —
/// see [`is_known_provider`], which checks both halves without listing the
/// configured one here by name.
const NATIVE_PROVIDER_FAMILIES: &[&str] = &["anthropic", "bedrock", "vertex", "foundry"];

/// Every provider family this adapter accepts: one of the harness's own
/// native families above, or the wire name of a provider
/// `crate::provider::registry` actually knows about. The configured half is
/// never copied into a second, hand-maintained list — it is asked of the
/// registry directly, so a new `Provider` module is reachable from
/// claude-code the moment it is registered, with no second edit site here.
fn is_known_provider(name: &str) -> bool {
    NATIVE_PROVIDER_FAMILIES.contains(&name)
        || crate::provider::registry()
            .iter()
            .any(|provider| provider.wire_name() == name)
}

/// The full list [`is_known_provider`] checks against, built fresh for a
/// rejection reason — never cached, since the registry half can change
/// between builds and this is only ever assembled on the one rejected path.
fn known_provider_families() -> Vec<&'static str> {
    let mut families: Vec<&'static str> = NATIVE_PROVIDER_FAMILIES.to_vec();
    families.extend(crate::provider::registry().iter().map(|p| p.wire_name()));
    families
}

/// Tool names that touch the network, matched case-insensitively against a
/// requested `permission_policy.tools` entry. Used only to reject a
/// self-contradictory request (network denied, but a network tool allowed)
/// before spawning anything.
const NETWORK_TOOLS: &[&str] = &["webfetch", "websearch"];

/// From `docs/contracts/runner-v1/limits.json`'s `request_timeout_seconds_max`
/// (frozen; not re-read from disk here since this file may not depend on
/// contract JSON parsing, but the value itself is copied verbatim).
const MAX_TIMEOUT_SECONDS: u64 = 86_400;

/// Generous but bounded stdout/stderr caps for a real coding-assistant
/// stream-json transcript. Matches the spirit of `process.rs`'s own
/// memory-bounded capture; the exact numbers are this adapter's own choice,
/// not part of the frozen contract.
const MAX_STDOUT_BYTES: usize = 16 * 1024 * 1024;
const MAX_STDERR_BYTES: usize = 4 * 1024 * 1024;

/// What to execute and any fixed leading arguments, so the same code path
/// drives either the real, absolute-resolved `claude` binary or the shared
/// fake harness (`/bin/sh <script path>`, per
/// `crate::harness::fixtures::fake_harness_command`) without a second
/// branch anywhere else in this file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessBinary {
    pub program: PathBuf,
    pub prefix_args: Vec<String>,
}

impl HarnessBinary {
    fn command_line(&self, extra_args: Vec<String>) -> (PathBuf, Vec<String>) {
        let mut args = self.prefix_args.clone();
        args.extend(extra_args);
        (self.program.clone(), args)
    }
}

/// Searches the *runner process's own* `PATH` (never an attempt-supplied
/// value), then the shared well-known install locations in
/// [`super::locate`], for an executable named `claude`. Resolved once by
/// [`ClaudeCodeAdapter::discover`]; a later uninstall is caught defensively
/// in `validate`/`start`, not by re-searching on every call.
fn discover_installed_binary() -> Result<HarnessBinary, String> {
    let program = super::locate::locate_installed("claude").map_err(|error| error.to_string())?;
    Ok(HarnessBinary {
        program,
        prefix_args: Vec::new(),
    })
}

/// Per-run state [`ClaudeCodeGrammar::prepare`] computes and
/// [`ClaudeCodeGrammar::outcome`] later consumes — everything `wait` needs
/// that only `prepare` had access to.
pub struct ClaudeCodeRunState {
    /// Needed so `outcome` can actually stage the raw run log it claims
    /// (`artifacts: Advisory`) — `prepare` is the only place these are
    /// known; `outcome` only ever sees this state.
    workspace_path: PathBuf,
    attempt_id: String,
    /// Which provider the request named, if any — `outcome`'s parser needs
    /// this to decide whether a fast `result` line's model claim can be
    /// trusted as `harness_reported` or must be downgraded to
    /// `requested_not_confirmed` (a gateway-routed run's `init` line fires
    /// before any network call reaches the gateway).
    requested_provider: Option<String>,
}

/// The Claude Code harness grammar: everything genuinely specific to the
/// `claude` CLI. Time is injected via `C: crate::Clock` on
/// [`LocalProcessHarness`] itself (never `SystemTime::now()` directly), not
/// here — this type has no clock of its own.
pub struct ClaudeCodeGrammar {
    binary: HarnessBinary,
    /// Grace period between SIGTERM and SIGKILL in `cancel`. A field (not a
    /// constant) so tests can shrink it; defaults to 5s, matching
    /// `process.rs::ProcessLimits`'s own default. Folded into
    /// `PreparedRun::limits.termination_grace` at `prepare()` time, since
    /// the shared `cancel()` in `local_process.rs` reads the grace period
    /// from the running attempt's own recorded limits, not from a
    /// grammar-specific field it has no way to reach.
    cancel_grace: Duration,
}

/// The Claude Code harness adapter/probe:
/// [`LocalProcessHarness<ClaudeCodeGrammar>`][crate::harness::local_process::LocalProcessHarness]
/// implements both [`crate::harness::HarnessAdapter`] (the frozen per-attempt
/// lifecycle) and [`crate::harness::HarnessProbe`] (capability discovery) for
/// any grammar; this alias is the name every other module (`bootstrap.rs`,
/// tests) constructs and passes around.
pub type ClaudeCodeAdapter<C = crate::SystemClock> = LocalProcessHarness<ClaudeCodeGrammar, C>;

impl ClaudeCodeAdapter<crate::SystemClock> {
    /// Discovers the installed `claude` binary via the runner process's own
    /// `PATH` and constructs an adapter around it with the real system
    /// clock. The primary, non-test constructor. Fallible (unlike
    /// [`crate::harness::codex::CodexAdapter::discover`]'s infallible,
    /// late-resolving constructor): this grammar resolves its binary
    /// eagerly, once, here — a `claude` install that cannot be found fails
    /// the whole constructor rather than being deferred to the first
    /// `validate`/`start` call.
    pub fn discover(secrets: SecretStore) -> Result<Self, String> {
        Ok(Self::with_binary(
            discover_installed_binary()?,
            crate::SystemClock,
            secrets,
        ))
    }

    /// Test-only: thin wrapper over the already-`pub`
    /// [`Self::with_binary`], named to match `codex.rs`'s
    /// identical `for_fixture` so `harness::mod::tests`'s "same fixture
    /// completes through both real adapters" acceptance proof can
    /// construct both adapters through one uniform call shape.
    #[cfg(test)]
    pub(crate) fn for_fixture(
        program: PathBuf,
        prefix_args: Vec<String>,
        secrets: SecretStore,
    ) -> Self {
        Self::with_binary(
            HarnessBinary {
                program,
                prefix_args,
            },
            crate::SystemClock,
            secrets,
        )
    }
}

impl<C: crate::Clock> ClaudeCodeAdapter<C> {
    /// Constructs an adapter around an explicit [`HarnessBinary`] and clock.
    /// Used directly by tests to point at the shared fake harness fixture
    /// (`crate::harness::fixtures::fake_harness_command`) instead of a real
    /// `claude` install.
    pub fn with_binary(binary: HarnessBinary, clock: C, secrets: SecretStore) -> Self {
        let grammar = ClaudeCodeGrammar {
            binary,
            cancel_grace: Duration::from_secs(5),
        };
        LocalProcessHarness::new(grammar, clock, secrets)
    }

    /// Overrides the SIGTERM→SIGKILL grace period used by `cancel`. Tests
    /// use a small value so a cancellation test never depends on a
    /// multi-second real sleep to pass.
    pub fn with_cancel_grace(mut self, grace: Duration) -> Self {
        self.grammar.cancel_grace = grace;
        self
    }
}

impl ClaudeCodeGrammar {
    /// Exactly `HOME` and `PATH`, read from the *runner process's own*
    /// environment (never from attempt-supplied data) — not blanket
    /// ambient-environment inheritance (which `process.rs`'s own docs flag
    /// as a rule-12 leak), but two specific, non-secret, operationally
    /// required values: `claude` needs `HOME` to find its OAuth
    /// session/config, and `PATH` if it shells out internally (observed:
    /// its Bash tool invokes a real shell). Everything else the harness
    /// needs must come through the frozen `environment` field on the
    /// request.
    fn base_environment(&self) -> BTreeMap<String, String> {
        let mut env = BTreeMap::new();
        if let Ok(home) = std::env::var("HOME") {
            env.insert("HOME".to_string(), home);
        }
        if let Ok(path) = std::env::var("PATH") {
            env.insert("PATH".to_string(), path);
        }
        env
    }

    /// Runs `<binary> --version` from a neutral, non-attempt directory (no
    /// workspace exists yet at probe time) with a bounded timeout, since a
    /// probe must never hang the caller forever on a broken installation.
    async fn detect_version_impl(&self) -> (String, Option<String>) {
        let neutral_dir = std::env::temp_dir();
        let (program, args) = self.binary.command_line(vec!["--version".to_string()]);
        let spec = ProcessSpec {
            program,
            args,
            env: self.base_environment(),
            stdin: None,
            working_directory: neutral_dir.clone(),
            workspace_root: neutral_dir,
        };
        let process = match spec.spawn().await {
            Ok(process) => process,
            Err(error) => {
                tracing::warn!(
                    ?error,
                    "claude-code adapter failed to spawn a version probe"
                );
                return (
                    String::new(),
                    Some("failed to spawn the harness binary for a version probe".to_string()),
                );
            }
        };
        let limits = ProcessLimits::new(4096, 4096, Duration::from_secs(10));
        let result = match process
            .wait_with_capture(&limits, &SecretMaterial::new())
            .await
        {
            Ok(result) => result,
            Err(error) => {
                tracing::warn!(
                    ?error,
                    "claude-code adapter's version probe failed to complete"
                );
                return (
                    String::new(),
                    Some("version probe failed while capturing output".to_string()),
                );
            }
        };
        match result.exit {
            ProcessExit::Exited(0) => parse_version_text(&result.stdout.text),
            other => (
                String::new(),
                Some(format!("version probe exited abnormally: {other:?}")),
            ),
        }
    }

    /// Best-effort process identity check for `reconcile`: does the still-
    /// alive pid's own `argv[0]` resolve to the same program this adapter
    /// would have spawned? A bare `kill(pid, 0)` liveness check alone cannot
    /// rule out pid reuse (an unrelated process started later at the same
    /// pid); this narrows that risk without claiming certainty. Linux-only
    /// (`/proc/<pid>/cmdline`); `None` (not `Some(false)`) on every other
    /// platform, or if `/proc` cannot be read, meaning "alive, but identity
    /// unverifiable" — never conflated with "confirmed a different process."
    #[cfg(target_os = "linux")]
    fn process_program_matches(&self, pid: u32) -> Option<bool> {
        let raw = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
        let mut parts = raw.split(|byte| *byte == 0).filter(|part| !part.is_empty());
        let argv0 = parts.next()?;
        let argv0_path = Path::new(std::str::from_utf8(argv0).ok()?);
        let resolved = argv0_path
            .canonicalize()
            .unwrap_or_else(|_| argv0_path.to_path_buf());
        let expected = self
            .binary
            .program
            .canonicalize()
            .unwrap_or_else(|_| self.binary.program.clone());
        Some(resolved == expected)
    }

    #[cfg(not(target_os = "linux"))]
    fn process_program_matches(&self, _pid: u32) -> Option<bool> {
        None
    }

    /// Stages the (already-scrubbed) combined stdout/stderr as a `log`
    /// artifact inside the attempt's own workspace, via
    /// [`super::artifact::ArtifactStager`] — the exact pattern
    /// `codex.rs` already proves out. `artifacts: Supported`
    /// once had no backing implementation: `wait()` never called this before.
    /// Stages under the workspace's own `.artifacts` directory, matching
    /// this adapter's live test's own choice (`ArtifactStager::new(workspace.join(".artifacts"))`)
    /// rather than a separate external staging root — `discover()` has no
    /// such root to give it. Best-effort: a staging failure only omits the
    /// `artifact` key from `terminal_reason`, never fails the attempt.
    fn stage_run_log(
        workspace_path: &Path,
        attempt_id: &str,
        stdout: &str,
        stderr: &str,
    ) -> Option<Value> {
        let relative = PathBuf::from(".tack-runner").join("claude-code-run.log");
        let absolute = workspace_path.join(&relative);
        if let Some(parent) = absolute.parent()
            && std::fs::create_dir_all(parent).is_err()
        {
            return None;
        }
        let mut combined = String::new();
        combined.push_str("=== stdout ===\n");
        combined.push_str(stdout);
        combined.push_str("\n=== stderr ===\n");
        combined.push_str(stderr);
        if std::fs::write(&absolute, combined.as_bytes()).is_err() {
            return None;
        }

        let stager = super::artifact::ArtifactStager::new(workspace_path.join(".artifacts"));
        match stager.stage_file(attempt_id, workspace_path, &relative, "log", "text/plain") {
            Ok(staged) => Some(serde_json::json!({
                "kind": staged.kind,
                "name": staged.name,
                "media_type": staged.media_type,
                "size_bytes": staged.size_bytes,
                "sha256": staged.sha256,
                "staged_path": staged.staged_path.display().to_string(),
            })),
            Err(error) => {
                tracing::warn!(?error, "claude-code wait: artifact staging failed");
                None
            }
        }
    }
}

fn parse_version_text(raw: &str) -> (String, Option<String>) {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return (
            String::new(),
            Some("version probe produced no output".to_string()),
        );
    }
    let first_token = trimmed.split_whitespace().next().unwrap_or("");
    if looks_like_a_version_token(first_token) {
        (first_token.to_string(), None)
    } else {
        // Bounded: this text came from the harness's own stdout, which this
        // code path has already decided it cannot fully trust the shape of.
        let bounded: String = trimmed.chars().take(200).collect();
        (
            bounded,
            Some("installed harness reported an unrecognized version string format".to_string()),
        )
    }
}

fn looks_like_a_version_token(token: &str) -> bool {
    if !token.starts_with(|ch: char| ch.is_ascii_digit()) {
        return false;
    }
    token
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '-')
}

/// A sentinel used whenever the actual model genuinely could not be
/// observed (malformed/absent structured output). Distinct from any real
/// model id Claude Code could report, and always paired with
/// `model_observation_source: "not_observed"` so a reader never mistakes it
/// for a harness-reported value.
const UNOBSERVED_MODEL: &str = "unknown";

struct ParsedRun {
    is_error: bool,
    terminal_reason: Value,
    harness_version: Option<String>,
    model_provider: String,
    model_id: String,
    model_observation_source: String,
    usage: DomainUsage,
}

fn not_measured_usage() -> DomainUsage {
    let not_measured = |value_is_none: bool| {
        let _ = value_is_none;
        MeasurementSource::NotMeasured
    };
    DomainUsage {
        tokens_in: Measurement {
            value: None,
            source: not_measured(true),
            additional: Default::default(),
        },
        tokens_out: Measurement {
            value: None,
            source: MeasurementSource::NotMeasured,
            additional: Default::default(),
        },
        duration_ms: Measurement {
            value: None,
            source: MeasurementSource::NotMeasured,
            additional: Default::default(),
        },
        cost_usd: Measurement {
            value: None,
            source: MeasurementSource::NotMeasured,
            additional: Default::default(),
        },
        additional: Default::default(),
    }
}

fn build_usage(result_value: &Value) -> DomainUsage {
    let tokens_in = result_value
        .pointer("/usage/input_tokens")
        .and_then(Value::as_u64);
    let tokens_out = result_value
        .pointer("/usage/output_tokens")
        .and_then(Value::as_u64);
    let duration_ms = result_value.get("duration_ms").and_then(Value::as_u64);
    let cost_usd = result_value.get("total_cost_usd").and_then(Value::as_f64);

    let source_for = |present: bool| {
        if present {
            MeasurementSource::Measured
        } else {
            MeasurementSource::NotMeasured
        }
    };

    DomainUsage {
        tokens_in: Measurement {
            value: tokens_in,
            source: source_for(tokens_in.is_some()),
            additional: Default::default(),
        },
        tokens_out: Measurement {
            value: tokens_out,
            source: source_for(tokens_out.is_some()),
            additional: Default::default(),
        },
        duration_ms: Measurement {
            value: duration_ms,
            source: source_for(duration_ms.is_some()),
            additional: Default::default(),
        },
        cost_usd: Measurement {
            value: cost_usd,
            source: source_for(cost_usd.is_some()),
            additional: Default::default(),
        },
        additional: Default::default(),
    }
}

fn bounded_prefix(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

/// Parses a terminal `{"type":"result", ...}` line (already located by
/// `parse_run_output`) into a [`ParsedRun`]. `is_error` is the sole
/// success/failure signal used — **not** `subtype`, which was directly
/// observed reporting `"success"` alongside `"is_error":true` for an
/// invalid-model API error. See the module docs.
fn parsed_from_result_line(
    result_value: &Value,
    init_model: Option<String>,
    harness_version: Option<String>,
    requested_provider: Option<&str>,
) -> ParsedRun {
    // A missing `is_error` field (never observed, but not contractually
    // guaranteed either) fails closed as an error rather than a silent
    // success.
    let is_error = result_value
        .get("is_error")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    let model_provider = requested_provider
        .map(str::to_ascii_lowercase)
        .unwrap_or_else(|| "anthropic".to_string());
    let (model_id, model_observation_source) = match init_model {
        // This line is emitted before any network call reaches whatever
        // endpoint `model_provider` names, so it states what the CLI was
        // configured to request, not necessarily what answered. Whether
        // that distinction matters is a property of the endpoint, not of
        // any one vendor — `requires_unconfirmed_model_recording` asks the
        // matching registered provider (a name matching none, including
        // every native family above, is never unconfirmed here: that
        // question only applies to a Tack-configured endpoint).
        Some(model) if crate::provider::requires_unconfirmed_model_recording(&model_provider) => (
            model,
            ModelObservationSource::RequestedNotConfirmed
                .as_str()
                .to_string(),
        ),
        Some(model) => (
            model,
            ModelObservationSource::HarnessReported.as_str().to_string(),
        ),
        None => (
            UNOBSERVED_MODEL.to_string(),
            ModelObservationSource::NotObserved.as_str().to_string(),
        ),
    };

    ParsedRun {
        is_error,
        terminal_reason: result_value.clone(),
        harness_version,
        model_provider,
        model_id,
        model_observation_source,
        usage: build_usage(result_value),
    }
}

/// Used when the process produced *some* JSON-shaped lines (so this is not
/// the "produced nothing at all, judge purely by exit code" case) but never
/// a terminal `{"type":"result"}` object — a truncated/corrupted stream, or
/// the shared fake harness's deliberately-garbage `malformed` mode. Always
/// `Failed`, and every field that cannot honestly be known is the explicit
/// unobserved sentinel, never a fabricated value.
fn malformed_outcome(result: &ProcessResult, note: &str) -> ParsedRun {
    ParsedRun {
        is_error: true,
        terminal_reason: serde_json::json!({
            "reason": "malformed_output",
            "detail": note,
            "exit": format!("{:?}", result.exit),
            "stdout_prefix": bounded_prefix(&result.stdout.text, 500),
        }),
        harness_version: None,
        model_provider: "anthropic".to_string(),
        model_id: UNOBSERVED_MODEL.to_string(),
        model_observation_source: ModelObservationSource::NotObserved.as_str().to_string(),
        usage: not_measured_usage(),
    }
}

/// Used when the process produced **no** JSON-shaped stdout at all (empty,
/// or text that never once parsed as a JSON value — e.g. the shared fake
/// harness's generic `success`/`failure` modes, which are not shaped like
/// Claude Code's real output at all by design). The only
/// honest signal left is the raw exit code, and the resulting
/// `terminal_reason` says so explicitly rather than presenting this as a
/// fully-observed result.
fn fallback_from_exit_code(result: &ProcessResult) -> ParsedRun {
    let (is_error, note): (bool, &str) = match result.exit {
        ProcessExit::Exited(0) => (
            false,
            "no structured result envelope was produced; inferred success from exit code 0",
        ),
        ProcessExit::Exited(_) => (
            true,
            "no structured result envelope was produced; inferred failure from a non-zero exit code",
        ),
        ProcessExit::TimedOut => (
            true,
            "process exceeded its timeout with no structured result envelope",
        ),
        #[cfg(unix)]
        ProcessExit::Signaled(_) => (
            true,
            "process terminated by signal with no structured result envelope",
        ),
    };
    ParsedRun {
        is_error,
        terminal_reason: serde_json::json!({
            "reason": note,
            "exit": format!("{:?}", result.exit),
            "stderr_prefix": bounded_prefix(&result.stderr.text, 500),
        }),
        harness_version: None,
        model_provider: "anthropic".to_string(),
        model_id: UNOBSERVED_MODEL.to_string(),
        model_observation_source: ModelObservationSource::NotObserved.as_str().to_string(),
        usage: not_measured_usage(),
    }
}

/// Scans every line of captured stdout for the two `stream-json` lines this
/// adapter actually needs (`system`/`init` for the session's real model and
/// `claude_code_version`, and the terminal `result` object), tolerating and
/// simply skipping any line that fails to parse — a single corrupted line
/// must never abort parsing of an otherwise-good stream.
fn parse_run_output(result: &ProcessResult, requested_provider: Option<&str>) -> ParsedRun {
    let mut init_model: Option<String> = None;
    let mut harness_version: Option<String> = None;
    let mut result_line: Option<Value> = None;
    let mut any_json_line = false;

    for line in result.stdout.text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        any_json_line = true;
        match value.get("type").and_then(Value::as_str) {
            Some("system") if value.get("subtype").and_then(Value::as_str) == Some("init") => {
                init_model = value
                    .get("model")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                harness_version = value
                    .get("claude_code_version")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
            }
            Some("result") => result_line = Some(value),
            _ => {}
        }
    }

    if let Some(result_value) = result_line {
        return parsed_from_result_line(
            &result_value,
            init_model,
            harness_version,
            requested_provider,
        );
    }

    if any_json_line {
        return malformed_outcome(
            result,
            "harness produced JSON output but no parseable terminal `result` object was found",
        );
    }

    fallback_from_exit_code(result)
}

/// The capability statement this adapter reports both from `probe` and
/// (unchanged, per attempt) as `ActualExecution.capability_snapshot`.
/// `cancel` is deliberately `Advisory`, not `Supported` — see the module
/// docs for the observed session-detachment finding that justifies it.
fn feature_capabilities() -> FeatureCapabilities {
    FeatureCapabilities {
        cancel: CapabilityValue {
            support: CapabilitySupport::Advisory,
            reason: Some(
                "The top-level `claude` process is always signalled reliably (it is always its \
                 own process-group leader). A Bash-tool-spawned subprocess was observed \
                 (via `ps`) running in its own session, distinct from that group; it is only \
                 guaranteed to be cleaned up if Claude Code exits gracefully within the SIGTERM \
                 grace period, since an escalation to SIGKILL is uncatchable and cannot reach a \
                 different session's process group."
                    .to_string(),
            ),
            additional: Default::default(),
        },
        resume: CapabilityValue {
            support: CapabilitySupport::Unsupported,
            reason: Some(
                "Headless (--print) invocation is a single ephemeral process with no daemon or \
                 reattachment interface. `--resume <session-id>` starts a *new* process that \
                 continues stored conversation history; that is a different guarantee than \
                 reattaching to this exact in-flight execution after a runner restart."
                    .to_string(),
            ),
            additional: Default::default(),
        },
        decisions: CapabilityValue {
            support: CapabilitySupport::Unsupported,
            reason: Some(
                "No observed mechanism for pausing headless execution to await an out-of-band \
                 decision through the runner protocol; permission prompts are resolved locally \
                 per --permission-mode, and the non-interactive trust dialog is documented as \
                 skipped entirely in --print mode."
                    .to_string(),
            ),
            additional: Default::default(),
        },
        // Downgraded from
        // `Supported`. Real `Write`/`Edit` tool output genuinely lands in
        // the workspace, but `wait()` used not to actually
        // staged anything — `stage_run_log` below closes that gap by
        // staging the raw, already-redacted stdout/stderr transcript, the
        // same thing the Codex adapter honestly calls
        // `Advisory` rather than `Supported`. No Claude-Code-specific
        // per-file artifact discovery (e.g. a real git diff of files it
        // changed) is implemented, so this adapter now reports the same
        // honest ceiling it does.
        artifacts: CapabilityValue {
            support: CapabilitySupport::Advisory,
            reason: Some(
                "Real Write/Edit tool output lands in the workspace, but only the raw, \
                 already-redacted stdout/stderr transcript is staged as a log artifact today \
                 (matching what the Codex adapter reports); no Claude-Code-specific \
                 per-file artifact discovery is implemented."
                    .to_string(),
            ),
            additional: Default::default(),
        },
        usage: CapabilityValue {
            support: CapabilitySupport::Advisory,
            reason: Some(
                "The harness reports token/cost totals, but an internal auxiliary model's \
                 usage is folded into `total_cost_usd` while the top-level `usage.input_tokens` \
                 / `output_tokens` fields appeared (directly observed) to reflect only the \
                 primary visible turn, so tokens_in/out may undercount true consumption \
                 relative to cost_usd."
                    .to_string(),
            ),
            additional: Default::default(),
        },
        additional: Default::default(),
    }
}

#[async_trait]
impl HarnessGrammar for ClaudeCodeGrammar {
    type RunState = ClaudeCodeRunState;

    fn harness_kind(&self) -> DomainHarnessKind {
        DomainHarnessKind::new(HARNESS_KIND)
    }

    /// `harness_kind` self-check, the provider allow-list check, and the
    /// network self-contradiction check, bundled exactly as `validate` used
    /// to run them inline — called from both `validate` and `start` so the
    /// two can never disagree about what counts as an unsupported
    /// selection.
    fn validate_selection(&self, spec: &ExecutionSpec) -> Result<(), HarnessError> {
        let request = &spec.work.request;

        if request.requested_harness_kind.as_str() != HARNESS_KIND {
            let reason = format!(
                "requested harness kind {:?} does not match this adapter's kind {HARNESS_KIND:?}",
                request.requested_harness_kind.as_str()
            );
            tracing::warn!(
                reason,
                "claude-code adapter received a spec requesting a different harness kind"
            );
            return Err(HarnessError::Rejected { reason });
        }

        if let Some(provider) = &request.requested_model_provider {
            let normalized = provider.as_str().trim().to_ascii_lowercase();
            if !is_known_provider(&normalized) {
                let known = known_provider_families();
                let reason = format!(
                    "requested model provider {:?} is not one of this adapter's known provider \
                     families {known:?}",
                    provider.as_str()
                );
                tracing::warn!(
                    reason,
                    "claude-code adapter rejected an unsupported model provider before spawn"
                );
                return Err(HarnessError::Rejected { reason });
            }
        }

        if !request.permission_policy.network {
            let requests_network_tool = request.permission_policy.tools.iter().any(|tool| {
                let lower = tool.to_ascii_lowercase();
                NETWORK_TOOLS.contains(&lower.as_str())
            });
            if requests_network_tool {
                let reason = "permission_policy denies network but names a network tool \
                               (WebFetch/WebSearch), a self-contradictory request this adapter \
                               cannot honor consistently"
                    .to_owned();
                tracing::warn!(
                    reason,
                    "claude-code adapter rejected a policy allowing a network tool while network \
                     is denied"
                );
                return Err(HarnessError::Rejected { reason });
            }
        }

        Ok(())
    }

    fn resolve_binary(&self) -> Result<(PathBuf, Vec<String>), String> {
        if !self.binary.program.exists() {
            return Err(format!(
                "resolved claude binary at {} no longer exists",
                self.binary.program.display()
            ));
        }
        Ok((self.binary.program.clone(), self.binary.prefix_args.clone()))
    }

    fn resolve_provider_endpoint(
        &self,
        spec: &ExecutionSpec,
        secrets: &SecretStore,
        providers: &BTreeMap<String, ProviderConfig>,
    ) -> Result<Option<ProviderEndpoint>, HarnessError> {
        let provider = spec
            .work
            .request
            .requested_model_provider
            .as_ref()
            .map(|provider| provider.as_str())
            .unwrap_or("");
        crate::provider::resolve_endpoint(
            providers,
            secrets,
            provider,
            crate::provider::Wire::AnthropicMessages,
        )
        .map_err(|error| {
            let reason = error.to_string();
            tracing::warn!(
                reason,
                "claude-code adapter rejected a request whose provider endpoint could not be \
                 resolved"
            );
            HarnessError::Rejected { reason }
        })
    }

    async fn prepare(
        &self,
        spec: &ExecutionSpec,
        secrets_store: &SecretStore,
        providers: &BTreeMap<String, ProviderConfig>,
    ) -> Result<PreparedRun<ClaudeCodeRunState>, HarnessError> {
        let request = &spec.work.request;
        let workspace_root = spec.workspace.path.clone();
        let working_directory = match request.repository.subdirectory.as_deref() {
            Some(subdirectory) if !subdirectory.is_empty() => workspace_root.join(subdirectory),
            _ => workspace_root.clone(),
        };

        let mut secrets = SecretMaterial::new();
        let resolved_environment =
            super::resolve_environment(secrets_store, request, &mut secrets)?;
        let mut env = self.base_environment();
        env.extend(resolved_environment);

        // A configured provider endpoint applies only when this request's
        // provider names one (e.g. a gateway) — a direct-vendor request
        // (the harness's own subscription/login mode) resolves to `None`
        // and this grammar injects nothing, so the two paths can never be
        // confused by a shared environment variable.
        if let Some(endpoint) = self.resolve_provider_endpoint(spec, secrets_store, providers)? {
            env.insert("ANTHROPIC_BASE_URL".to_string(), endpoint.base_url);
            env.insert(
                endpoint.credential_env_var,
                endpoint.credential.expose().to_string(),
            );
            // Measured against the installed CLI (2.1.260): empty,
            // unset and non-empty all produced byte-identical outgoing
            // requests, with ANTHROPIC_AUTH_TOKEN winning regardless —
            // this contradicts the vendor's own documented claim that a
            // non-empty value wins. Set empty anyway, at zero cost,
            // rather than trusted to already be absent.
            env.insert("ANTHROPIC_API_KEY".to_string(), String::new());
        }

        let tools_value = request.permission_policy.tools.join(",");

        let mut args = vec![
            "-p".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--verbose".to_string(),
            "--no-session-persistence".to_string(),
            "--permission-mode".to_string(),
            "bypassPermissions".to_string(),
            "--effort".to_string(),
            "high".to_string(),
            "--setting-sources".to_string(),
            String::new(),
            "--tools".to_string(),
            tools_value,
        ];
        if let Some(model_id) = &request.requested_model_id {
            args.push("--model".to_string());
            args.push(model_id.as_str().to_string());
        }
        if let Some(budget) = request
            .budgets
            .get("cost_usd")
            .and_then(Value::as_f64)
            .filter(|value| *value > 0.0)
        {
            args.push("--max-budget-usd".to_string());
            args.push(budget.to_string());
        }

        let prompt = request.resolved_agent_profile.instructions.clone();
        let (program, args) = self.binary.command_line(args);

        let process_spec = ProcessSpec {
            program,
            args,
            env,
            stdin: Some(prompt.into_bytes()),
            working_directory,
            workspace_root,
        };

        let timeout = Duration::from_secs(request.timeout_seconds.clamp(1, MAX_TIMEOUT_SECONDS));
        let limits = ProcessLimits {
            termination_grace: self.cancel_grace,
            ..ProcessLimits::new(MAX_STDOUT_BYTES, MAX_STDERR_BYTES, timeout)
        };
        let requested_provider = request
            .requested_model_provider
            .as_ref()
            .map(|provider| provider.as_str().to_string());

        let state = ClaudeCodeRunState {
            workspace_path: spec.workspace.path.clone(),
            attempt_id: spec.work.lease.attempt_id.as_str().to_owned(),
            requested_provider,
        };

        Ok(PreparedRun {
            process_spec,
            secrets,
            limits,
            state,
        })
    }

    fn encode_handle(&self, pid: u32) -> String {
        pid.to_string()
    }

    fn decode_handle(&self, process_id: &str) -> Option<u32> {
        process_id.parse::<u32>().ok()
    }

    fn cancel_outcome(
        &self,
        pid: u32,
        signal_result: Result<CancelOutcome, ProcessError>,
    ) -> Result<
        (
            CancelObservation,
            serde_json::Map<String, serde_json::Value>,
        ),
        HarnessError,
    > {
        let (observation, process_outcome) = match signal_result {
            Ok(CancelOutcome::Stopped) => (CancelObservation::ProcessStopped, "stopped"),
            Ok(CancelOutcome::Killed) => (CancelObservation::ProcessStopped, "killed"),
            Err(error) => {
                tracing::warn!(
                    ?error,
                    "claude-code adapter failed to deliver a cancellation signal"
                );
                (CancelObservation::Ambiguous, "signal_failed")
            }
        };
        Ok((
            observation,
            serde_json::Map::from_iter([
                ("pid".to_string(), Value::from(pid)),
                ("process_outcome".to_string(), Value::from(process_outcome)),
            ]),
        ))
    }

    fn reconcile_alive(&self, pid: u32) -> RecoveryObservation {
        match self.process_program_matches(pid) {
            Some(true) => RecoveryObservation::ProcessRunning,
            // The pid is alive, but resolves to a different program: the
            // original attempt process is confirmed gone, its pid has
            // simply been recycled by the OS to something unrelated.
            Some(false) => RecoveryObservation::ProcessStopped,
            // Alive, but identity is unverifiable on this platform
            // (non-Linux Unix, or `/proc` unreadable): a bare liveness
            // check alone is not proof this is genuinely the same
            // attempt, given pid reuse. Honest uncertainty, not a
            // confident guess either way.
            None => RecoveryObservation::Ambiguous,
        }
    }

    fn reconcile_unavailable(&self) -> Result<RecoveryObservation, HarnessError> {
        // No portable liveness primitive at all on this platform (see
        // `process.rs`'s own non-Unix cancellation fallback for the same
        // documented limitation). Reconciliation is not genuinely
        // supported here.
        Ok(RecoveryObservation::Ambiguous)
    }

    fn outcome(
        &self,
        state: ClaudeCodeRunState,
        started_at: DateTime<Utc>,
        ended_at: DateTime<Utc>,
        result: ProcessResult,
        cancelled: bool,
    ) -> HarnessOutcome {
        let parsed = parse_run_output(&result, state.requested_provider.as_deref());
        let terminal_state = if cancelled {
            AttemptState::Cancelled
        } else if parsed.is_error {
            AttemptState::Failed
        } else {
            AttemptState::Succeeded
        };

        // `artifacts: Advisory` (downgraded from an
        // unbacked `Supported` — see `feature_capabilities`) is only honest
        // if `outcome` actually stages something. Best-effort, exactly like
        // `codex.rs`'s identical `stage_run_log`: a staging
        // failure only omits the `artifact` key, never fails the attempt.
        let mut terminal_reason = parsed.terminal_reason;
        if let Some(artifact) = Self::stage_run_log(
            &state.workspace_path,
            &state.attempt_id,
            &result.stdout.text,
            &result.stderr.text,
        ) && let Some(object) = terminal_reason.as_object_mut()
        {
            object.insert("artifact".to_string(), artifact);
        }

        HarnessOutcome {
            terminal_state,
            terminal_reason,
            final_checkpoint: None,
            actual_execution: ActualExecution {
                harness_kind: DomainHarnessKind::new(HARNESS_KIND),
                harness_version: parsed.harness_version.unwrap_or_default(),
                model_provider: ActualModelProvider::new(parsed.model_provider),
                model_id: ActualModelId::new(parsed.model_id),
                model_observation_source: parsed.model_observation_source,
                capability_snapshot: self.feature_capabilities(),
                // The engine overwrites `workspace_id`/`base_revision` from
                // the real `Workspace` via `HarnessOutcome::
                // normalize_workspace_facts` after `wait` returns
                // (`engine.rs`); these are placeholders, never reported
                // onward as-is.
                workspace_id: DomainWorkspaceId::new(""),
                base_revision: String::new(),
                started_at,
                ended_at,
                additional: Default::default(),
            },
            usage: parsed.usage,
        }
    }

    async fn detect_version(
        &self,
    ) -> (String, Option<String>, BTreeMap<String, serde_json::Value>) {
        let (version, error) = self.detect_version_impl().await;
        // Unconditional, regardless of whether this particular probe
        // succeeded: the CLI has no list-models command at all, so this
        // note belongs to every probe outcome, not to a diagnostic branch
        // of the version scanner itself (which has none — see the module
        // docs' assumption on `parse_version_text`).
        let mut additional = BTreeMap::new();
        additional.insert(
            "model_discovery_note".to_string(),
            serde_json::Value::String(
                "Claude Code's CLI has no list-models command; model availability is only \
                 observable via a live, billed invocation, so this probe reports zero \
                 model_combinations rather than an unverified static alias list."
                    .to_string(),
            ),
        );
        (version, error, additional)
    }

    fn model_passthrough(&self) -> Option<CapabilityValue> {
        // A pass-through attestation: a claim about THIS grammar's
        // invocation contract (`run_arguments` appends `--model
        // <requested_model_id>` verbatim, asserted by unit test), not
        // about which models exist — the CLI validates the model itself
        // at run time and an invalid one fails the attempt with the
        // CLI's own error envelope (observed live, module docs). This is
        // what makes claude-code schedulable without inventing a model
        // list.
        Some(CapabilityValue {
            support: CapabilitySupport::Supported,
            reason: Some(
                "the adapter forwards requested_model_id verbatim via --model; the CLI \
                 validates it at run time (an invalid model returns is_error:true), so \
                 operator-specified opaque models are accepted without the probe claiming \
                 any model list"
                    .to_string(),
            ),
            additional: Default::default(),
        })
    }

    fn feature_capabilities(&self) -> FeatureCapabilities {
        feature_capabilities()
    }
}

#[cfg(test)]
#[path = "claude_code/tests.rs"]
mod tests;
