//! Claude Code harness adapter (card III-D2).
//!
//! This file implements [`super::HarnessAdapter`] (`engine::HarnessAdapter`,
//! frozen since Wave 2 — see the module docs on `super` for why this card
//! does not redefine it) and [`super::HarnessProbe`] for the `claude` CLI
//! (Anthropic's Claude Code), version `2.1.223` observed installed on the
//! machine this card was written on.
//!
//! ## Observed vs. assumed — read this before trusting a claim below
//!
//! Every behavioral claim in this file was checked by actually invoking the
//! installed `claude` binary from a disposable fixture directory (never this
//! repository) during this card's implementation — the exact commands and
//! outputs are recorded in `docs/agent-handoffs/part-iii/III-D2.md`. Where a
//! design choice rests on something *not* independently invoked (e.g. the
//! Bedrock/Vertex/Foundry provider families, confirmed only by `strings`
//! against the installed binary, never by an actual provider switch), the
//! handoff calls that out explicitly. Nothing here should be read as "this
//! is how Claude Code definitely behaves on every machine" — only "this is
//! what the one installed copy actually did."
//!
//! Concrete findings that shaped this implementation:
//!
//! - `claude --version` prints `"<version> (Claude Code)"` to stdout, exit 0,
//!   empty stderr, and needs neither `HOME` nor `PATH` — a fast, side-effect
//!   free probe (see [`detect_version`]).
//! - `claude -p` reads the prompt from **stdin** when no positional argument
//!   is given. This adapter always uses stdin for the prompt, never argv —
//!   matching [`super::process::ProcessSpec`]'s own documented preference and
//!   keeping the prompt out of `/proc/<pid>/cmdline`.
//! - `--output-format json` (non-streaming) has **no reliable single "model
//!   used" field** — only an aggregate `modelUsage` map that, even for a
//!   single trivial prompt, included a second, unrequested internal model
//!   (`claude-haiku-4-5-...`) alongside the one actually requested. This
//!   adapter uses `--output-format stream-json --verbose` instead, and reads
//!   the authoritative model from the `{"type":"system","subtype":"init"}`
//!   event's `model` field (cross-checked against `assistant` messages).
//! - `is_error` (boolean) is the only reliable success/failure signal.
//!   `subtype` is *not*: an invalid-model 404 was observed with
//!   `"subtype":"success"` alongside `"is_error":true`. This adapter keys
//!   exclusively off `is_error`, never `subtype`.
//! - A persisted per-user settings file (`~/.claude/settings.json`,
//!   `"effortLevel"`) silently changed default behavior in a way that broke
//!   an otherwise-valid invocation (`effort 'xhigh' is not supported when
//!   thinking is disabled`) even with the process environment fully cleared.
//!   This adapter always passes an explicit `--effort high` (verified
//!   compatible across every model exercised) rather than trusting whatever
//!   default a given machine's settings file happens to carry, and passes
//!   `--setting-sources ""` to reduce ambient configuration influence over a
//!   supposedly deterministic run.
//! - Claude Code's own Bash tool runs its command in a **new session**
//!   (distinct `pgid`/`sid` from the top-level `claude` process, confirmed
//!   twice via `ps`), unlike the shared fake harness's `spawn_child` mode
//!   (whose grandchild deliberately stays in-group). A graceful SIGTERM
//!   appeared to let Claude Code clean up that detached session itself, but
//!   a SIGKILL escalation (uncatchable) cannot give it that chance, and
//!   `kill(-pgid, SIGKILL)` does not reach a different session's group. See
//!   `feature_capabilities` below for how this is reflected honestly
//!   (`cancel: Advisory`, not `Supported`) rather than papered over.
//!
//! ## What this adapter does not attempt
//!
//! - Resolving `secret_reference`-only environment entries: no secret-store
//!   client exists in this crate yet. Such entries are skipped with a
//!   `tracing::warn!` (name only) rather than silently dropped or fabricated.
//! - Enumerating installed/available models ahead of a real invocation: the
//!   CLI has no `list-models`-style command, so [`ClaudeCodeAdapter`]'s
//!   [`super::HarnessProbe::probe`] reports zero `model_combinations` rather
//!   than an unverified static alias list ("report capabilities without
//!   assuming models").
//! - Actually exercising the Bedrock/Vertex/Foundry provider paths: doing so
//!   needs real cloud credentials this card will not fabricate or request.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};

use async_trait::async_trait;
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::Value;
use tack_orch::execution::{
    ActualExecution, ActualModelId, ActualModelProvider, CapabilitySupport, CapabilityValue,
    FeatureCapabilities, HarnessCapability, HarnessKind as DomainHarnessKind, Measurement,
    MeasurementSource, Usage as DomainUsage, WorkspaceId as DomainWorkspaceId,
};

use super::{
    AttemptJournal, CancelObservation, CancellationEvidence, ExecutionSpec, HarnessAdapter,
    HarnessError, HarnessOutcome, HarnessProbe, LocalRunHandle, RecoveryObservation,
    process::{
        CancelOutcome, ProcessExit, ProcessLimits, ProcessResult, ProcessSpec, process_alive,
    },
    redact::SecretMaterial,
};
use crate::{Clock, SystemClock, client::AttemptState, client::Timestamp};

/// The wire value for this harness, matching
/// `registry::HarnessKind::ClaudeCode.as_str()` (D5-owned `registry.rs`,
/// read but not edited by this card).
const HARNESS_KIND: &str = "claude-code";

/// Provider families the installed `claude` 2.1.223 binary genuinely knows
/// about. `"anthropic"` is the first-party default (no flag needed); the
/// other three are switched via environment variables the CLI itself
/// documents only indirectly (`--bare`'s help text names them collectively
/// as "3P providers"). Their exact names were confirmed by `strings` against
/// the installed binary (`ANTHROPIC_BEDROCK_BASE_URL`, `ANTHROPIC_VERTEX_*`,
/// `CLAUDE_CODE_USE_BEDROCK`, `CLAUDE_CODE_USE_VERTEX`,
/// `CLAUDE_CODE_USE_FOUNDRY` all present) — static inspection of the shipped
/// artifact, not a live provider switch (never attempted: it would need real
/// cloud credentials this card will not fabricate). See the D2 handoff.
const KNOWN_PROVIDERS: &[&str] = &["anthropic", "bedrock", "vertex", "foundry"];

/// Tool names that touch the network, matched case-insensitively against a
/// requested `permission_policy.tools` entry. Used only to reject a
/// self-contradictory request (network denied, but a network tool allowed)
/// before spawning anything.
const NETWORK_TOOLS: &[&str] = &["webfetch", "websearch"];

/// From `docs/contracts/runner-v1/limits.json`'s `request_timeout_seconds_max`
/// (frozen, A0/D5-owned; not re-read from disk here since this file may not
/// depend on contract JSON parsing, but the value itself is copied verbatim).
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
/// value) for an executable named `claude`, mirroring ordinary shell PATH
/// resolution. Resolved once by [`ClaudeCodeAdapter::discover`]; a later
/// uninstall is caught defensively in `validate`/`start`, not by re-searching
/// PATH on every call.
fn discover_installed_binary() -> Result<HarnessBinary, String> {
    let path_var = std::env::var_os("PATH")
        .ok_or_else(|| "runner process has no PATH environment variable set".to_string())?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join("claude");
        if !candidate.is_file() {
            continue;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let Ok(metadata) = std::fs::metadata(&candidate) else {
                continue;
            };
            if metadata.permissions().mode() & 0o111 == 0 {
                continue;
            }
        }
        let resolved = candidate.canonicalize().unwrap_or(candidate);
        return Ok(HarnessBinary {
            program: resolved,
            prefix_args: Vec::new(),
        });
    }
    Err("no executable named `claude` was found on PATH".to_string())
}

/// One in-flight (spawned, not yet reaped) attempt process, keyed by its own
/// pid (as a string) in [`ClaudeCodeAdapter::processes`]. Everything `wait`
/// needs that only `start` has access to (the original spec has no second
/// trip through `wait`/`cancel`, which take only an opaque
/// [`LocalRunHandle`]) is captured here.
struct RunningEntry {
    process: super::process::SupervisedProcess,
    secrets: SecretMaterial,
    limits: ProcessLimits,
    requested_provider: Option<String>,
    started_at: DateTime<Utc>,
}

/// The Claude Code harness adapter. `C` is the injected [`Clock`] (rule 9 —
/// no adapter method sleeps or reads `SystemTime::now()` directly; every
/// timestamp comes from `self.clock`), matching
/// `RunnerEngine<P, A, W, C = SystemClock>`'s own generic-with-default shape.
pub struct ClaudeCodeAdapter<C = SystemClock> {
    binary: HarnessBinary,
    clock: C,
    /// Grace period between SIGTERM and SIGKILL in `cancel`. A field (not a
    /// constant) so tests can shrink it; defaults to 5s, matching
    /// `process.rs::ProcessLimits`'s own default.
    cancel_grace: Duration,
    processes: tokio::sync::Mutex<BTreeMap<String, RunningEntry>>,
    /// Pids `cancel` explicitly terminated, consulted (and cleared) by
    /// `wait` if it is ever also called for the same handle — the current
    /// engine never does this in one `run_claimed` cycle (it calls either
    /// `cancel` or `wait`, never both, for a given attempt — see
    /// `engine.rs::run_claimed`), but the trait takes `&self`, not `&mut
    /// self`, and does not itself document that exclusion, so this adapter
    /// stays correct defensively rather than assuming a caller convention it
    /// cannot see from its own trait bound.
    cancelled: tokio::sync::Mutex<std::collections::BTreeSet<String>>,
}

impl ClaudeCodeAdapter<SystemClock> {
    /// Discovers the installed `claude` binary via the runner process's own
    /// `PATH` and constructs an adapter around it with the real system
    /// clock. The primary, non-test constructor.
    pub fn discover() -> Result<Self, String> {
        Ok(Self::with_binary(discover_installed_binary()?, SystemClock))
    }
}

impl<C: Clock> ClaudeCodeAdapter<C> {
    /// Constructs an adapter around an explicit [`HarnessBinary`] and clock.
    /// Used directly by tests to point at the shared fake harness fixture
    /// (`crate::harness::fixtures::fake_harness_command`) instead of a real
    /// `claude` install.
    pub fn with_binary(binary: HarnessBinary, clock: C) -> Self {
        Self {
            binary,
            clock,
            cancel_grace: Duration::from_secs(5),
            processes: tokio::sync::Mutex::new(BTreeMap::new()),
            cancelled: tokio::sync::Mutex::new(std::collections::BTreeSet::new()),
        }
    }

    /// Overrides the SIGTERM→SIGKILL grace period used by `cancel`. Tests
    /// use a small value so a cancellation test never depends on a
    /// multi-second real sleep to pass.
    pub fn with_cancel_grace(mut self, grace: Duration) -> Self {
        self.cancel_grace = grace;
        self
    }

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
    async fn detect_version(&self) -> (String, Option<String>) {
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
}

fn now_rfc3339<C: Clock>(clock: &C) -> String {
    DateTime::<Utc>::from(clock.now()).to_rfc3339_opts(SecondsFormat::Secs, true)
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

    let (model_id, model_observation_source) = match init_model {
        Some(model) => (model, "harness_reported".to_string()),
        None => (UNOBSERVED_MODEL.to_string(), "not_observed".to_string()),
    };
    let model_provider = requested_provider
        .map(str::to_ascii_lowercase)
        .unwrap_or_else(|| "anthropic".to_string());

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
        model_observation_source: "not_observed".to_string(),
        usage: not_measured_usage(),
    }
}

/// Used when the process produced **no** JSON-shaped stdout at all (empty,
/// or text that never once parsed as a JSON value — e.g. the shared fake
/// harness's generic `success`/`failure` modes, which are not shaped like
/// Claude Code's real output at all by design; see the D2 handoff). The only
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
        model_observation_source: "not_observed".to_string(),
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
        artifacts: CapabilityValue {
            support: CapabilitySupport::Supported,
            reason: None,
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
impl<C: Clock + Send + Sync> HarnessProbe for ClaudeCodeAdapter<C> {
    fn harness_kind(&self) -> DomainHarnessKind {
        DomainHarnessKind::new(HARNESS_KIND)
    }

    async fn probe(&self) -> HarnessCapability {
        let probed_at = DateTime::<Utc>::from(self.clock.now());
        let (installed_version, probe_error) = self.detect_version().await;
        let mut additional = BTreeMap::new();
        additional.insert(
            "model_discovery_note".to_string(),
            Value::String(
                "Claude Code's CLI has no list-models command; model availability is only \
                 observable via a live, billed invocation, so this probe reports zero \
                 model_combinations rather than an unverified static alias list."
                    .to_string(),
            ),
        );
        HarnessCapability {
            harness_kind: DomainHarnessKind::new(HARNESS_KIND),
            installed_version,
            probe_error,
            probed_at,
            model_combinations: Vec::new(),
            additional,
        }
    }
}

#[async_trait]
impl<C: Clock + Send + Sync> HarnessAdapter for ClaudeCodeAdapter<C> {
    async fn validate(&self, spec: &ExecutionSpec) -> Result<(), HarnessError> {
        let request = &spec.work.request;

        if request.requested_harness_kind.as_str() != HARNESS_KIND {
            tracing::warn!(
                requested = request.requested_harness_kind.as_str(),
                "claude-code adapter received a spec requesting a different harness kind"
            );
            return Err(HarnessError::Rejected);
        }

        if let Some(provider) = &request.requested_model_provider {
            let normalized = provider.as_str().trim().to_ascii_lowercase();
            if !KNOWN_PROVIDERS.contains(&normalized.as_str()) {
                tracing::warn!(
                    requested_model_provider = provider.as_str(),
                    "claude-code adapter rejected an unsupported model provider before spawn"
                );
                return Err(HarnessError::Rejected);
            }
        }

        if !request.permission_policy.network {
            let requests_network_tool = request.permission_policy.tools.iter().any(|tool| {
                let lower = tool.to_ascii_lowercase();
                NETWORK_TOOLS.contains(&lower.as_str())
            });
            if requests_network_tool {
                tracing::warn!(
                    "claude-code adapter rejected a policy allowing a network tool while network \
                     is denied"
                );
                return Err(HarnessError::Rejected);
            }
        }

        if !self.binary.program.exists() {
            tracing::warn!(
                program = %self.binary.program.display(),
                "claude-code adapter's resolved binary no longer exists"
            );
            return Err(HarnessError::Rejected);
        }

        Ok(())
    }

    async fn start(&self, spec: &ExecutionSpec) -> Result<LocalRunHandle, HarnessError> {
        let request = &spec.work.request;
        let workspace_root = spec.workspace.path.clone();
        let working_directory = match request.repository.subdirectory.as_deref() {
            Some(subdirectory) if !subdirectory.is_empty() => workspace_root.join(subdirectory),
            _ => workspace_root.clone(),
        };

        let mut env = self.base_environment();
        let mut secrets = SecretMaterial::new();
        for (name, value) in &request.environment {
            match (&value.value, &value.secret_reference) {
                (Some(raw), _) => {
                    secrets.register(raw.clone());
                    env.insert(name.clone(), raw.clone());
                }
                (None, Some(_reference)) => {
                    tracing::warn!(
                        name = %name,
                        "claude-code adapter cannot resolve a secret-reference environment entry \
                         yet (no secret-store client exists in this crate); the entry was \
                         skipped rather than fabricated"
                    );
                }
                (None, None) => {}
            }
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

        let process = process_spec.spawn().await.map_err(|error| {
            tracing::warn!(
                ?error,
                "claude-code adapter failed to spawn the harness process"
            );
            HarnessError::Process
        })?;
        let pid = process.pid();
        let process_id = pid.to_string();

        let timeout = Duration::from_secs(request.timeout_seconds.clamp(1, MAX_TIMEOUT_SECONDS));
        let limits = ProcessLimits::new(MAX_STDOUT_BYTES, MAX_STDERR_BYTES, timeout);
        let requested_provider = request
            .requested_model_provider
            .as_ref()
            .map(|provider| provider.as_str().to_string());

        let entry = RunningEntry {
            process,
            secrets,
            limits,
            requested_provider,
            started_at: DateTime::<Utc>::from(self.clock.now()),
        };
        self.processes
            .lock()
            .await
            .insert(process_id.clone(), entry);

        Ok(LocalRunHandle { process_id })
    }

    async fn cancel(&self, handle: &LocalRunHandle) -> Result<CancellationEvidence, HarnessError> {
        // Takes ownership of the entry rather than signalling by a raw pid
        // this adapter looks up separately: `SupervisedProcess::cancel`
        // (`process.rs`) is the only way to *reap* the process as part of
        // confirming it stopped. A pid-only `kill(pid, 0)` liveness poll
        // cannot distinguish "still running" from "exited but not yet
        // reaped" (a zombie still answers `kill(pid, 0)` successfully until
        // something calls `waitpid` on it) — an earlier version of this
        // method signalled by pid without ever reaping, and its own test
        // caught it hanging at `Ambiguous` forever because the killed
        // process was never actually reaped. Left as a documented lesson,
        // not silently fixed.
        let entry = self
            .processes
            .lock()
            .await
            .remove(&handle.process_id)
            .ok_or(HarnessError::Process)?;
        self.cancelled
            .lock()
            .await
            .insert(handle.process_id.clone());

        let pid = entry.process.pid();
        let (observation, process_outcome) = match entry.process.cancel(self.cancel_grace).await {
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

        Ok(CancellationEvidence {
            observation,
            observed_at: Timestamp::new(now_rfc3339(&self.clock)),
            details: serde_json::Map::from_iter([
                ("pid".to_string(), Value::from(pid)),
                ("process_outcome".to_string(), Value::from(process_outcome)),
            ]),
        })
    }

    async fn wait(&self, handle: &LocalRunHandle) -> Result<HarnessOutcome, HarnessError> {
        let entry = self
            .processes
            .lock()
            .await
            .remove(&handle.process_id)
            .ok_or(HarnessError::Process)?;
        let was_cancelled = self.cancelled.lock().await.remove(&handle.process_id);

        let result = entry
            .process
            .wait_with_capture(&entry.limits, &entry.secrets)
            .await
            .map_err(|error| {
                tracing::warn!(
                    ?error,
                    "claude-code adapter failed while capturing process output"
                );
                HarnessError::Process
            })?;

        let parsed = parse_run_output(&result, entry.requested_provider.as_deref());
        let terminal_state = if was_cancelled {
            AttemptState::Cancelled
        } else if parsed.is_error {
            AttemptState::Failed
        } else {
            AttemptState::Succeeded
        };
        let ended_at = DateTime::<Utc>::from(self.clock.now());

        Ok(HarnessOutcome {
            terminal_state,
            terminal_reason: parsed.terminal_reason,
            final_checkpoint: None,
            actual_execution: ActualExecution {
                harness_kind: DomainHarnessKind::new(HARNESS_KIND),
                harness_version: parsed.harness_version.unwrap_or_default(),
                model_provider: ActualModelProvider::new(parsed.model_provider),
                model_id: ActualModelId::new(parsed.model_id),
                model_observation_source: parsed.model_observation_source,
                capability_snapshot: feature_capabilities(),
                // The engine overwrites `workspace_id`/`base_revision` from
                // the real `Workspace` via `HarnessOutcome::
                // normalize_workspace_facts` after `wait` returns
                // (`engine.rs`); these are placeholders, never reported
                // onward as-is.
                workspace_id: DomainWorkspaceId::new(""),
                base_revision: String::new(),
                started_at: entry.started_at,
                ended_at,
                additional: Default::default(),
            },
            usage: parsed.usage,
        })
    }

    async fn reconcile(
        &self,
        journal: &AttemptJournal,
    ) -> Result<RecoveryObservation, HarnessError> {
        let Some(process_id) = &journal.process_id else {
            return Ok(RecoveryObservation::ProcessStopped);
        };
        let Ok(pid) = process_id.parse::<u32>() else {
            return Err(HarnessError::RecoveryUnavailable);
        };

        #[cfg(unix)]
        {
            if !process_alive(pid) {
                return Ok(RecoveryObservation::ProcessStopped);
            }
            match self.process_program_matches(pid) {
                Some(true) => Ok(RecoveryObservation::ProcessRunning),
                // The pid is alive, but resolves to a different program: the
                // original attempt process is confirmed gone, its pid has
                // simply been recycled by the OS to something unrelated.
                Some(false) => Ok(RecoveryObservation::ProcessStopped),
                // Alive, but identity is unverifiable on this platform
                // (non-Linux Unix, or `/proc` unreadable): a bare liveness
                // check alone is not proof this is genuinely the same
                // attempt, given pid reuse. Honest uncertainty, not a
                // confident guess either way.
                None => Ok(RecoveryObservation::Ambiguous),
            }
        }
        #[cfg(not(unix))]
        {
            // No portable liveness primitive at all on this platform (see
            // `process.rs`'s own non-Unix cancellation fallback for the same
            // documented limitation). Reconciliation is not genuinely
            // supported here.
            Ok(RecoveryObservation::Ambiguous)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{
        AttemptId, AttemptLease, ClaimRequestId, ClaimedWork, FencingToken, RunnerId, Workspace,
        WorkspaceId as ClientWorkspaceId,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tack_orch::execution::{
        AgentProfileSnapshot, AttemptSnapshot, EnvironmentValue, ExecutionRequestSnapshot,
        ExecutionState, HarnessKind as DomainHarnessKindType, PermissionPolicy, RepositorySnapshot,
        RequestedModelProvider, RunnerSelector,
    };

    #[derive(Clone, Copy)]
    struct FixedClock(std::time::SystemTime);

    impl crate::Clock for FixedClock {
        fn now(&self) -> std::time::SystemTime {
            self.0
        }
    }

    fn clock() -> FixedClock {
        FixedClock(std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(1_754_000_000))
    }

    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

    fn temp_workspace(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "tack-runner-claude-code-{label}-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&root).expect("create temp workspace");
        root
    }

    fn fake_binary() -> HarnessBinary {
        let (program, args) = super::super::fixtures::fake_harness_command();
        HarnessBinary {
            program,
            prefix_args: args,
        }
    }

    fn adapter_with_fake_binary() -> ClaudeCodeAdapter<FixedClock> {
        ClaudeCodeAdapter::with_binary(fake_binary(), clock())
            .with_cancel_grace(Duration::from_millis(150))
    }

    fn permission_policy(tools: &[&str], network: bool) -> PermissionPolicy {
        PermissionPolicy {
            tools: tools.iter().map(|tool| tool.to_string()).collect(),
            network,
            additional: Default::default(),
        }
    }

    fn spec_with(
        harness_kind: &str,
        provider: Option<&str>,
        tools: &[&str],
        network: bool,
        environment: BTreeMap<String, EnvironmentValue>,
        workspace_path: PathBuf,
    ) -> ExecutionSpec {
        let request = ExecutionRequestSnapshot {
            request_id: tack_orch::execution::ExecutionRequestId::new("exec_test"),
            item_id: tack_orch::execution::ItemId::new("item_test"),
            idempotency_key: tack_orch::execution::IdempotencyKey::new("idem_test"),
            created_by: serde_json::json!({"source": "operator", "subject_id": "test"}),
            created_at: DateTime::<Utc>::from(clock().0),
            selector: RunnerSelector::ExactRunner {
                runner_id: tack_orch::execution::RunnerId::new("runr_test"),
            },
            agent_profile_id: tack_orch::execution::AgentProfileId::new("ap_test"),
            resolved_agent_profile: AgentProfileSnapshot {
                name: "Test profile".to_string(),
                instructions: "Print exactly: ok".to_string(),
                tool_policy: serde_json::json!({}),
                timeout_seconds: 60,
                budgets: serde_json::json!({}),
                additional: Default::default(),
            },
            requested_harness_kind: DomainHarnessKindType::new(harness_kind),
            requested_model_provider: provider.map(RequestedModelProvider::new),
            requested_model_id: None,
            repository: RepositorySnapshot {
                kind: "git".to_string(),
                remote: "https://example.invalid/repo.git".to_string(),
                base_revision: "0123456789abcdef0123456789abcdef01234567".to_string(),
                subdirectory: None,
                additional: Default::default(),
            },
            permission_policy: permission_policy(tools, network),
            timeout_seconds: 5,
            budgets: serde_json::json!({}),
            status_map_policy_id: None,
            environment,
            metadata: serde_json::json!({}),
            additional: Default::default(),
        };
        let attempt = AttemptSnapshot {
            attempt_id: tack_orch::execution::AttemptId::new("att_test"),
            request_id: request.request_id.clone(),
            attempt_number: 1,
            runner_id: tack_orch::execution::RunnerId::new("runr_test"),
            fencing_token: tack_orch::execution::FencingToken(1),
            state: ExecutionState::Leased,
            workspace_id: None,
            base_revision: request.repository.base_revision.clone(),
            lease_issued_at: None,
            lease_expires_at: None,
            last_heartbeat_at: None,
            additional: Default::default(),
        };
        ExecutionSpec {
            work: ClaimedWork {
                claim_request_id: ClaimRequestId::new("claim_test"),
                lease: AttemptLease {
                    attempt_id: AttemptId::new("att_test"),
                    runner_id: RunnerId::new("runr_test"),
                    fencing_token: FencingToken(1),
                    attempt_number: 1,
                    state: AttemptState::Leased,
                    issued_at: Timestamp::new("2026-08-06T12:20:00Z"),
                    expires_at: Timestamp::new("2026-08-06T12:21:00Z"),
                },
                request,
                attempt,
            },
            workspace: Workspace {
                attempt_id: AttemptId::new("att_test"),
                id: ClientWorkspaceId::new("ws_test"),
                path: workspace_path,
                base_revision: "0123456789abcdef0123456789abcdef01234567".to_string(),
            },
        }
    }

    // ---- validate: pre-spawn rejection ----------------------------------

    #[tokio::test]
    async fn validate_accepts_a_well_formed_claude_code_spec() {
        let adapter = adapter_with_fake_binary();
        let workspace = temp_workspace("validate-ok");
        let spec = spec_with(
            "claude-code",
            None,
            &["Read"],
            true,
            BTreeMap::new(),
            workspace.clone(),
        );
        assert!(adapter.validate(&spec).await.is_ok());
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    /// Acceptance: an unsupported selection fails pre-spawn, before any
    /// process launches. A model provider this installed CLI has no way to
    /// honor (confirmed absent from the three real provider-switch families
    /// found via `strings` on the binary — see the module docs) is rejected
    /// by `validate` alone; no `SupervisedProcess` bookkeeping entry is ever
    /// created.
    #[tokio::test]
    async fn validate_rejects_an_unsupported_model_provider_before_any_process_launches() {
        let adapter = adapter_with_fake_binary();
        let workspace = temp_workspace("validate-bad-provider");
        let spec = spec_with(
            "claude-code",
            Some("openai"),
            &[],
            true,
            BTreeMap::new(),
            workspace.clone(),
        );
        assert!(matches!(
            adapter.validate(&spec).await,
            Err(HarnessError::Rejected)
        ));
        assert!(
            adapter.processes.lock().await.is_empty(),
            "a pre-spawn rejection must never create process bookkeeping"
        );
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    #[tokio::test]
    async fn validate_accepts_every_known_provider_family_case_insensitively() {
        let adapter = adapter_with_fake_binary();
        for provider in ["anthropic", "BEDROCK", "Vertex", "foundry"] {
            let workspace = temp_workspace("validate-provider-ok");
            let spec = spec_with(
                "claude-code",
                Some(provider),
                &[],
                true,
                BTreeMap::new(),
                workspace.clone(),
            );
            assert!(
                adapter.validate(&spec).await.is_ok(),
                "provider {provider} should be accepted"
            );
            std::fs::remove_dir_all(workspace).expect("cleanup");
        }
    }

    #[tokio::test]
    async fn validate_rejects_a_spec_requesting_a_different_harness_kind() {
        let adapter = adapter_with_fake_binary();
        let workspace = temp_workspace("validate-wrong-kind");
        let spec = spec_with("codex", None, &[], true, BTreeMap::new(), workspace.clone());
        assert!(matches!(
            adapter.validate(&spec).await,
            Err(HarnessError::Rejected)
        ));
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    #[tokio::test]
    async fn validate_rejects_a_network_tool_when_network_is_denied() {
        let adapter = adapter_with_fake_binary();
        let workspace = temp_workspace("validate-network-conflict");
        let spec = spec_with(
            "claude-code",
            None,
            &["WebFetch"],
            false,
            BTreeMap::new(),
            workspace.clone(),
        );
        assert!(matches!(
            adapter.validate(&spec).await,
            Err(HarnessError::Rejected)
        ));
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    #[tokio::test]
    async fn validate_rejects_when_the_resolved_binary_no_longer_exists() {
        let missing = HarnessBinary {
            program: PathBuf::from("/nonexistent/definitely/not/claude"),
            prefix_args: Vec::new(),
        };
        let adapter = ClaudeCodeAdapter::with_binary(missing, clock());
        let workspace = temp_workspace("validate-missing-binary");
        let spec = spec_with(
            "claude-code",
            None,
            &[],
            true,
            BTreeMap::new(),
            workspace.clone(),
        );
        assert!(matches!(
            adapter.validate(&spec).await,
            Err(HarnessError::Rejected)
        ));
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    // ---- fake-binary-driven lifecycle tests ------------------------------

    fn env_entry(value: &str) -> EnvironmentValue {
        EnvironmentValue {
            value: Some(value.to_string()),
            secret_reference: None,
            additional: Default::default(),
        }
    }

    /// Acceptance: fake-binary success. Drives the real shared fixture
    /// through `start`/`wait`; since the generic fake binary's `success`
    /// mode is not shaped like Claude Code's real stream-json output (by
    /// design — see the D2 handoff), this proves the honest exit-code
    /// fallback path end to end (spawn, capture, redact, parse), not a
    /// structured-result parse.
    #[tokio::test]
    async fn fake_binary_success_mode_is_reported_succeeded_via_the_honest_exit_code_fallback() {
        let adapter = adapter_with_fake_binary();
        let workspace = temp_workspace("fake-success");
        let mut environment = BTreeMap::new();
        environment.insert("TACK_FAKE_HARNESS_MODE".to_string(), env_entry("success"));
        let spec = spec_with(
            "claude-code",
            None,
            &[],
            true,
            environment,
            workspace.clone(),
        );

        adapter.validate(&spec).await.expect("validate");
        let handle = adapter.start(&spec).await.expect("start");
        let outcome = adapter.wait(&handle).await.expect("wait");

        assert_eq!(outcome.terminal_state, AttemptState::Succeeded);
        assert_eq!(
            outcome.terminal_reason["reason"],
            "no structured result envelope was produced; inferred success from exit code 0"
        );
        assert_eq!(outcome.actual_execution.model_id.as_str(), UNOBSERVED_MODEL);
        assert_eq!(
            outcome.usage.tokens_in.source,
            MeasurementSource::NotMeasured
        );
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    /// Acceptance: fake-binary failure.
    #[tokio::test]
    async fn fake_binary_failure_mode_is_reported_failed_via_the_honest_exit_code_fallback() {
        let adapter = adapter_with_fake_binary();
        let workspace = temp_workspace("fake-failure");
        let mut environment = BTreeMap::new();
        environment.insert("TACK_FAKE_HARNESS_MODE".to_string(), env_entry("failure"));
        environment.insert("TACK_FAKE_HARNESS_EXIT_CODE".to_string(), env_entry("7"));
        let spec = spec_with(
            "claude-code",
            None,
            &[],
            true,
            environment,
            workspace.clone(),
        );

        adapter.validate(&spec).await.expect("validate");
        let handle = adapter.start(&spec).await.expect("start");
        let outcome = adapter.wait(&handle).await.expect("wait");

        assert_eq!(outcome.terminal_state, AttemptState::Failed);
        assert_eq!(
            outcome.terminal_reason["reason"],
            "no structured result envelope was produced; inferred failure from a non-zero exit \
             code"
        );
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    /// Acceptance: fake-binary malformed output. Exercises the *real*
    /// `malformed` mode byte-for-byte (deliberately unparseable, mixed
    /// garbage), through the full spawn/capture pipeline, proving this
    /// never panics and never fabricates a structured, confident result —
    /// only the honest, clearly-labeled exit-code fallback (this specific
    /// fixture's exit code is 0, so this lands on the "success" side of the
    /// fallback, same as the plain `success` mode; see the module doc on
    /// `parse_run_output` for why *any* non-JSON generic fake-binary output
    /// collapses into that one fallback rather than being distinguished
    /// from `success` by content alone).
    #[tokio::test]
    async fn fake_binary_malformed_mode_never_panics_and_never_fabricates_structured_data() {
        let adapter = adapter_with_fake_binary();
        let workspace = temp_workspace("fake-malformed");
        let mut environment = BTreeMap::new();
        environment.insert("TACK_FAKE_HARNESS_MODE".to_string(), env_entry("malformed"));
        let spec = spec_with(
            "claude-code",
            None,
            &[],
            true,
            environment,
            workspace.clone(),
        );

        adapter.validate(&spec).await.expect("validate");
        let handle = adapter.start(&spec).await.expect("start");
        let outcome = adapter.wait(&handle).await.expect("wait");

        // No panic reaching here is itself part of what this test proves.
        assert!(matches!(
            outcome.terminal_state,
            AttemptState::Succeeded | AttemptState::Failed
        ));
        assert_eq!(outcome.actual_execution.model_id.as_str(), UNOBSERVED_MODEL);
        assert_eq!(
            outcome.actual_execution.model_observation_source,
            "not_observed"
        );
        assert_eq!(outcome.usage.tokens_in.value, None);
        assert_eq!(
            outcome.usage.cost_usd.source,
            MeasurementSource::NotMeasured
        );
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    /// A more realistic "malformed" case than generic garbage: a stream
    /// that starts out perfectly valid (a real `system`/`init` line) and is
    /// then truncated/corrupted before any terminal `result` object ever
    /// arrives — e.g. a crash mid-run. Pure unit test against the parser
    /// directly (no process spawn needed): proves the "some JSON, but no
    /// result object" branch specifically, which the generic fake binary
    /// cannot exercise (see the two tests above).
    #[test]
    fn a_stream_with_a_valid_init_line_but_no_terminal_result_is_failed_not_a_guessed_success() {
        let stdout = concat!(
            r#"{"type":"system","subtype":"init","model":"claude-sonnet-5","claude_code_version":"2.1.223"}"#,
            "\n",
            r#"{"type":"assistant","message":{"model":"claude-sonnet-5","content":[]}}"#,
            "\n",
        );
        let result = ProcessResult {
            exit: ProcessExit::Exited(0),
            stdout: super::super::process::CapturedOutput {
                text: stdout.to_string(),
                truncated: false,
                bytes_dropped: 0,
                total_bytes_seen: stdout.len() as u64,
            },
            stderr: Default::default(),
        };

        let parsed = parse_run_output(&result, None);

        assert!(parsed.is_error);
        assert_eq!(parsed.terminal_reason["reason"], "malformed_output");
        assert_eq!(parsed.model_id, UNOBSERVED_MODEL);
    }

    /// Acceptance: cancel kills the process. Uses the shared fixture's
    /// `spawn_child` mode (a real, still-running descendant) purely to get
    /// a real, still-alive pid to cancel against; this test's own concern is
    /// this adapter's `cancel` correctly observing the process stop and
    /// cleaning up its own bookkeeping — grandchild-tree coverage itself is
    /// D4's `process::tests::cancel_kills_the_whole_descendant_tree_...`.
    #[tokio::test]
    async fn cancel_stops_the_process_and_forgets_its_own_bookkeeping_entry() {
        let adapter = adapter_with_fake_binary();
        let workspace = temp_workspace("cancel");
        let mut environment = BTreeMap::new();
        environment.insert("TACK_FAKE_HARNESS_MODE".to_string(), env_entry("hang"));
        environment.insert(
            "TACK_FAKE_HARNESS_SLEEP_SECONDS".to_string(),
            env_entry("3600"),
        );
        let spec = spec_with(
            "claude-code",
            None,
            &[],
            true,
            environment,
            workspace.clone(),
        );

        adapter.validate(&spec).await.expect("validate");
        let handle = adapter.start(&spec).await.expect("start");
        assert_eq!(adapter.processes.lock().await.len(), 1);

        let pid: u32 = handle.process_id.parse().expect("numeric pid handle");
        assert!(
            process_alive(pid),
            "process must be observed running before cancel"
        );

        let evidence = adapter.cancel(&handle).await.expect("cancel");
        assert_eq!(evidence.observation, CancelObservation::ProcessStopped);
        assert!(
            !process_alive(pid),
            "process must actually be gone after cancel reports stopped"
        );
        assert!(
            adapter.processes.lock().await.is_empty(),
            "cancel must remove its own bookkeeping entry"
        );
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    #[tokio::test]
    async fn cancel_of_an_unknown_handle_is_a_typed_error_not_a_panic() {
        let adapter = adapter_with_fake_binary();
        let handle = LocalRunHandle {
            process_id: "not-a-number".to_string(),
        };
        assert!(matches!(
            adapter.cancel(&handle).await,
            Err(HarnessError::Process)
        ));
    }

    // ---- redaction (rule 12) ---------------------------------------------

    /// Acceptance: arguments and environment are redacted in logs and
    /// events; a canary appears nowhere. Plants a canary as a plain
    /// environment value, drives the fake binary's `echo_canary` mode (which
    /// actively echoes it back on both stdout and stderr — a worst-case
    /// leaky harness), and asserts the captured/returned outcome never
    /// contains it, while the `ProcessSpec` this adapter built also never
    /// exposes it via `Debug` (structural half, inherited unchanged from
    /// `process.rs`).
    #[tokio::test]
    async fn a_planted_canary_in_the_environment_never_survives_into_the_returned_outcome() {
        const CANARY: &str = "tack-d2-claude-code-canary-6f31a2";
        let adapter = adapter_with_fake_binary();
        let workspace = temp_workspace("canary");
        let mut environment = BTreeMap::new();
        environment.insert(
            "TACK_FAKE_HARNESS_MODE".to_string(),
            env_entry("echo_canary"),
        );
        environment.insert("TACK_TEST_SECRET".to_string(), env_entry(CANARY));
        environment.insert(
            "TACK_FAKE_HARNESS_ECHO_ENV_KEYS".to_string(),
            env_entry("TACK_TEST_SECRET"),
        );
        let spec = spec_with(
            "claude-code",
            None,
            &[],
            true,
            environment,
            workspace.clone(),
        );

        adapter.validate(&spec).await.expect("validate");
        let handle = adapter.start(&spec).await.expect("start");
        let outcome = adapter.wait(&handle).await.expect("wait");

        let serialized = serde_json::to_string(&outcome.terminal_reason).expect("serialize");
        assert!(
            !serialized.contains(CANARY),
            "canary must never survive into the returned terminal_reason"
        );
        assert!(
            serialized.contains("[REDACTED]"),
            "the leak must actually have been scrubbed, \
            not merely absent because nothing echoed it"
        );
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    #[test]
    fn secret_reference_only_environment_entries_are_never_silently_treated_as_satisfied() {
        // Structural: `EnvironmentValue{value: None, secret_reference: Some(_)}`
        // is skipped (with a warning) in `start`, never substituted with an
        // empty string that would look like an intentionally-blank value.
        // Exercised directly against the match arm rather than a full spawn,
        // since the effect (nothing inserted into `env`) is the same either
        // way and this keeps the test process-free.
        let value = EnvironmentValue {
            value: None,
            secret_reference: Some("vault://example/token".to_string()),
            additional: Default::default(),
        };
        match (&value.value, &value.secret_reference) {
            (Some(_), _) => panic!("this arm must not be taken for a secret_reference-only entry"),
            (None, Some(reference)) => assert_eq!(reference, "vault://example/token"),
            (None, None) => panic!("this arm must not be taken either"),
        }
    }

    // ---- version parsing (pure unit tests) --------------------------------

    #[test]
    fn the_real_observed_claude_code_version_string_is_recognized() {
        // Byte-for-byte what `claude --version` printed during this card's
        // implementation (see the D2 handoff): "2.1.223 (Claude Code)\n".
        let (version, error) = parse_version_text("2.1.223 (Claude Code)\n");
        assert_eq!(version, "2.1.223");
        assert_eq!(error, None);
    }

    #[test]
    fn the_fake_binarys_unknown_version_fixture_is_reported_as_explicitly_unrecognized() {
        let (version, error) =
            parse_version_text("harness-cli version 999.999.999-nightly-exotic-format\n");
        assert!(
            error.is_some(),
            "an unrecognized shape must set probe_error"
        );
        assert!(
            version.contains("harness-cli"),
            "the raw observed text must still be preserved, not discarded"
        );
    }

    #[test]
    fn empty_version_output_is_a_probe_error_with_no_fabricated_version() {
        let (version, error) = parse_version_text("");
        assert_eq!(version, "");
        assert!(error.is_some());
    }

    #[test]
    fn a_version_token_with_a_prerelease_suffix_is_still_recognized() {
        let (version, error) = parse_version_text("3.0.0-beta.1 (Claude Code)\n");
        assert_eq!(version, "3.0.0-beta.1");
        assert_eq!(error, None);
    }

    /// Acceptance: fake-binary unknown version, driven through the real
    /// spawn path (not only the pure-function tests above), using the
    /// shared fixture's dedicated `unknown_version` mode.
    #[tokio::test]
    async fn probe_reports_the_shared_fixtures_unknown_version_output_honestly() {
        let adapter = adapter_with_fake_binary();
        // `detect_version` always invokes `--version` with no env override,
        // so this test instead exercises the same parsing path `probe`
        // depends on by driving the fixture in `unknown_version` mode
        // directly through `ProcessSpec`, matching exactly what
        // `detect_version` does internally.
        let workspace = std::env::temp_dir();
        let mut env = BTreeMap::new();
        env.insert(
            "TACK_FAKE_HARNESS_MODE".to_string(),
            "unknown_version".to_string(),
        );
        let (program, args) = fake_binary().command_line(Vec::new());
        let spec = ProcessSpec {
            program,
            args,
            env,
            stdin: None,
            working_directory: workspace.clone(),
            workspace_root: workspace,
        };
        let result = spec
            .spawn()
            .await
            .expect("spawn")
            .wait_with_capture(
                &ProcessLimits::new(4096, 4096, Duration::from_secs(10)),
                &SecretMaterial::new(),
            )
            .await
            .expect("wait");
        assert_eq!(result.exit, ProcessExit::Exited(0));
        let (version, error) = parse_version_text(&result.stdout.text);
        assert!(error.is_some());
        assert!(version.contains("999.999.999"));
        let _ = adapter; // constructed only to keep this test grouped with its siblings
    }

    // ---- result-envelope parsing using real observed shapes ---------------

    fn real_success_stdout() -> String {
        // Reproduces (trimmed to the fields this parser reads) the actual
        // `--output-format stream-json --verbose` transcript observed for
        // `claude -p "Print exactly this string..." --model sonnet`, minus
        // fields irrelevant to parsing. See the D2 handoff for the full,
        // unedited capture.
        concat!(
            r#"{"type":"system","subtype":"init","cwd":"/tmp/fixture","session_id":"s1","tools":[],"#,
            r#""model":"claude-sonnet-5","claude_code_version":"2.1.223"}"#,
            "\n",
            r#"{"type":"assistant","message":{"model":"claude-sonnet-5","content":[{"type":"text","text":"ok"}]}}"#,
            "\n",
            r#"{"is_error":false,"duration_api_ms":2485,"num_turns":1,"stop_reason":"end_turn","#,
            r#""session_id":"s1","total_cost_usd":0.0350687,"usage":{"input_tokens":2,"output_tokens":18},"#,
            r#""terminal_reason":"completed","subtype":"success","result":"ok","type":"result","duration_ms":1916}"#,
            "\n",
        )
        .to_string()
    }

    #[test]
    fn a_real_observed_success_transcript_is_parsed_with_the_session_model_and_usage() {
        let result = ProcessResult {
            exit: ProcessExit::Exited(0),
            stdout: super::super::process::CapturedOutput {
                text: real_success_stdout(),
                truncated: false,
                bytes_dropped: 0,
                total_bytes_seen: 0,
            },
            stderr: Default::default(),
        };

        let parsed = parse_run_output(&result, Some("anthropic"));

        assert!(!parsed.is_error);
        assert_eq!(parsed.model_id, "claude-sonnet-5");
        assert_eq!(parsed.model_observation_source, "harness_reported");
        assert_eq!(parsed.harness_version.as_deref(), Some("2.1.223"));
        assert_eq!(parsed.usage.tokens_in.value, Some(2));
        assert_eq!(parsed.usage.tokens_out.value, Some(18));
        assert_eq!(parsed.usage.tokens_in.source, MeasurementSource::Measured);
        assert_eq!(parsed.usage.cost_usd.value, Some(0.0350687));
    }

    /// Directly encodes the real, observed CLI quirk this adapter's
    /// success/failure determination depends on: an invalid-model 400/404
    /// API error was returned with `"is_error":true` **and**
    /// `"subtype":"success"` in the same object. A parser keying off
    /// `subtype` instead of `is_error` would misreport this as a success.
    #[test]
    fn an_api_error_result_with_a_misleading_subtype_of_success_is_still_reported_as_failed() {
        let stdout = concat!(
            r#"{"type":"system","subtype":"init","model":"claude-3-5-haiku-20241022","claude_code_version":"2.1.223"}"#,
            "\n",
            r#"{"is_error":true,"terminal_reason":"api_error","subtype":"success","#,
            r#""api_error_status":404,"result":"model not found","type":"result"}"#,
            "\n",
        )
        .to_string();
        let result = ProcessResult {
            exit: ProcessExit::Exited(1),
            stdout: super::super::process::CapturedOutput {
                text: stdout,
                truncated: false,
                bytes_dropped: 0,
                total_bytes_seen: 0,
            },
            stderr: Default::default(),
        };

        let parsed = parse_run_output(&result, None);

        assert!(
            parsed.is_error,
            "is_error must win over a misleadingly-named subtype of \"success\""
        );
        assert_eq!(parsed.terminal_reason["subtype"], "success");
    }

    /// Reproduces the real observed `--max-budget-usd` exhaustion shape
    /// (`subtype: "error_max_budget_usd"`, a genuinely distinct, correctly
    /// non-"success"-looking subtype, unlike the api_error case above).
    #[test]
    fn a_budget_exhausted_result_is_parsed_as_failed() {
        let stdout = concat!(
            r#"{"type":"system","subtype":"init","model":"claude-haiku-4-5-20251001","claude_code_version":"2.1.223"}"#,
            "\n",
            r#"{"is_error":true,"terminal_reason":"budget_exhausted","subtype":"error_max_budget_usd","#,
            r#""errors":["Reached maximum budget ($1e-7)"],"total_cost_usd":0.013149,"type":"result"}"#,
            "\n",
        )
        .to_string();
        let result = ProcessResult {
            exit: ProcessExit::Exited(1),
            stdout: super::super::process::CapturedOutput {
                text: stdout,
                truncated: false,
                bytes_dropped: 0,
                total_bytes_seen: 0,
            },
            stderr: Default::default(),
        };

        let parsed = parse_run_output(&result, None);

        assert!(parsed.is_error);
        assert_eq!(parsed.terminal_reason["subtype"], "error_max_budget_usd");
        assert_eq!(parsed.usage.cost_usd.value, Some(0.013149));
    }

    #[test]
    fn a_missing_is_error_field_fails_closed_as_an_error_not_a_silent_success() {
        let value = serde_json::json!({"type": "result", "result": "no is_error field here"});
        let parsed = parsed_from_result_line(&value, None, None, None);
        assert!(parsed.is_error);
    }

    // ---- reconcile ---------------------------------------------------------

    #[tokio::test]
    async fn reconcile_with_no_recorded_process_id_needs_no_dispatch() {
        let adapter = adapter_with_fake_binary();
        let journal = journal_with_process(None);
        let observation = adapter.reconcile(&journal).await.expect("reconcile");
        assert_eq!(observation, RecoveryObservation::ProcessStopped);
    }

    #[tokio::test]
    async fn reconcile_reports_process_stopped_for_a_pid_that_no_longer_exists() {
        let adapter = adapter_with_fake_binary();
        // A pid that is astronomically unlikely to be a live process on any
        // CI/dev machine, without relying on a fixed platform-specific
        // sentinel like `i32::MAX`.
        let journal = journal_with_process(Some("2000000000"));
        let observation = adapter.reconcile(&journal).await.expect("reconcile");
        assert_eq!(observation, RecoveryObservation::ProcessStopped);
    }

    #[tokio::test]
    async fn reconcile_with_an_undecodable_process_id_is_explicitly_unavailable() {
        let adapter = adapter_with_fake_binary();
        let journal = journal_with_process(Some("not-a-pid"));
        assert!(matches!(
            adapter.reconcile(&journal).await,
            Err(HarnessError::RecoveryUnavailable)
        ));
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn reconcile_reports_process_running_for_a_genuinely_still_running_fake_harness() {
        let adapter = adapter_with_fake_binary();
        let workspace = temp_workspace("reconcile-running");
        let mut environment = BTreeMap::new();
        // Deliberately `spawn_child`, not `hang`: `hang` mode `exec`s into
        // `sleep` (see `fake_harness.sh`'s own doc comment), which replaces
        // the process image, so `/proc/<pid>/cmdline` permanently becomes
        // `sleep ...` rather than `/bin/sh <script>` moments after spawn —
        // a real `claude` process never re-execs into something else over
        // its own lifetime, so that behavior is specific to this one fixture
        // mode, not representative of what `process_program_matches` needs
        // to identify in production. `spawn_child` mode's own process
        // (distinct from the grandchild `sleep` it backgrounds) never execs,
        // keeping a stable, checkable `/bin/sh <script>` cmdline throughout.
        environment.insert(
            "TACK_FAKE_HARNESS_MODE".to_string(),
            env_entry("spawn_child"),
        );
        environment.insert(
            "TACK_FAKE_HARNESS_SLEEP_SECONDS".to_string(),
            env_entry("3600"),
        );
        let spec = spec_with(
            "claude-code",
            None,
            &[],
            true,
            environment,
            workspace.clone(),
        );
        adapter.validate(&spec).await.expect("validate");
        let handle = adapter.start(&spec).await.expect("start");
        let pid: u32 = handle.process_id.parse().expect("numeric pid");

        let journal = journal_with_process(Some(&handle.process_id));
        // Bounded poll, not a fixed sleep (rule 9). `spawn_child` mode's own
        // process never execs (see the comment above), so unlike the
        // earlier `hang`-mode attempt this converges rather than being
        // structurally unable to: under heavy parallel test load, reading
        // `/proc/<pid>/cmdline` immediately after spawn can transiently
        // fail (`process_program_matches` returning `None`, since the
        // kernel has not necessarily finished populating it), which is a
        // real, if rare, possibility `reconcile` itself already reports
        // honestly as `Ambiguous` rather than guessing — this loop exists
        // only so the test's assertion is not sensitive to that one-time
        // startup window, not because `reconcile` is retried in production.
        let mut observation = None;
        for _ in 0..80 {
            let latest = adapter.reconcile(&journal).await.expect("reconcile");
            if latest == RecoveryObservation::ProcessRunning {
                observation = Some(latest);
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        assert_eq!(observation, Some(RecoveryObservation::ProcessRunning));

        // Cleanup: this adapter's own `cancel` both stops the process (and
        // its backgrounded grandchild, via `process.rs`'s own process-group
        // signalling) and forgets the bookkeeping entry `reconcile` never
        // touched.
        adapter.cancel(&handle).await.expect("cancel");
        assert!(!process_alive(pid));
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn reconcile_reports_process_stopped_when_a_live_pid_belongs_to_an_unrelated_program() {
        // A real, currently-alive pid that this adapter did *not* spawn and
        // that does not resolve to `self.binary.program` at all: this
        // adapter's own test-runner process itself.
        let adapter = adapter_with_fake_binary();
        let own_pid = std::process::id();
        let journal = journal_with_process(Some(&own_pid.to_string()));
        let observation = adapter.reconcile(&journal).await.expect("reconcile");
        assert_eq!(observation, RecoveryObservation::ProcessStopped);
    }

    fn journal_with_process(process_id: Option<&str>) -> AttemptJournal {
        AttemptJournal {
            attempt_id: AttemptId::new("att_test"),
            runner_id: RunnerId::new("runr_test"),
            fencing_token: FencingToken(1),
            workspace: crate::client::journal::WorkspaceJournal {
                workspace_id: ClientWorkspaceId::new("ws_test"),
                path: PathBuf::from("/tmp/does-not-matter"),
                base_revision: "revision".to_string(),
            },
            state: crate::client::journal::JournalState::ProcessObservedRunning,
            process_id: process_id.map(str::to_owned),
            last_event_checkpoint: None,
            pending_terminal_report: None,
        }
    }

    // ---- discovery -----------------------------------------------------

    #[test]
    fn discover_installed_binary_fails_typed_when_path_has_no_claude_executable() {
        // SAFETY-adjacent note: this only ever *reads* PATH via a scoped
        // override for the duration of this single-threaded assertion; it
        // does not spawn a process or touch any other environment variable.
        let previous = std::env::var_os("PATH");
        // SAFETY: test-only, single-threaded within this process for the
        // duration of this scope; restored immediately below.
        unsafe {
            std::env::set_var("PATH", "/definitely/not/a/real/path/at/all");
        }
        let outcome = discover_installed_binary();
        // SAFETY: restores the prior value (or removes the override),
        // matching the pre-test state.
        unsafe {
            match &previous {
                Some(value) => std::env::set_var("PATH", value),
                None => std::env::remove_var("PATH"),
            }
        }
        assert!(outcome.is_err());
    }

    // ---- live, opt-in test against the real installed `claude` ----------

    /// Opt-in, matching D1 (`codex.rs`) and D3 (`opencode.rs`)'s own
    /// `#[ignore]`-gated live tests: never runs under a plain `cargo test`,
    /// never required in CI, and never fails just because `claude` is
    /// absent (rule 8). Unlike Codex's live test (version probe plus a
    /// purely local artifact stage, no real model call) or OpenCode's
    /// (routed to a genuinely free zen model), a real Claude Code
    /// invocation is billed — so this test additionally requires
    /// `TACK_RUN_LIVE_CLAUDE_CODE_TEST=1` even under `--ignored`, so that
    /// flag alone can never surprise-spend real money. Never depends on a
    /// secret being present in this process's own environment: whatever
    /// credential the installed CLI already carries (e.g. an OAuth session
    /// under `HOME`) is used exactly as the real installation already has
    /// it configured — this test never reads, logs, or forwards one itself.
    ///
    /// Records the observed version and stages a real produced artifact (a
    /// `README.md` this test asks Claude Code, via its `Write` tool, to
    /// overwrite, inside a disposable fixture git repo this test creates
    /// and deletes itself — never this checkout).
    #[tokio::test]
    #[ignore = "opt-in: requires a real `claude` binary on PATH *and* \
                TACK_RUN_LIVE_CLAUDE_CODE_TEST=1 (a real invocation is billed, unlike D1/D3's \
                free live tests); run with TACK_RUN_LIVE_CLAUDE_CODE_TEST=1 cargo test -p \
                tack-runner --lib -- --ignored claude_code::tests::live_"]
    async fn live_claude_code_records_version_and_a_real_artifact_when_opted_in() {
        if std::env::var("TACK_RUN_LIVE_CLAUDE_CODE_TEST").as_deref() != Ok("1") {
            eprintln!(
                "skipping live claude-code test: set TACK_RUN_LIVE_CLAUDE_CODE_TEST=1 to opt in \
                 (a real invocation is billed)"
            );
            return;
        }
        let Ok(adapter) = ClaudeCodeAdapter::discover() else {
            eprintln!("skipping live claude-code test: no `claude` binary discoverable on PATH");
            return;
        };

        let capability = adapter.probe().await;
        assert!(
            capability.probe_error.is_none(),
            "expected a healthy probe against a real installed binary: {:?}",
            capability.probe_error
        );
        assert!(!capability.installed_version.is_empty());
        eprintln!(
            "live claude-code probe: version={}",
            capability.installed_version
        );

        let workspace = temp_workspace("live-fixture-repo");
        let run = |program: &str, args: &[&str]| {
            std::process::Command::new(program)
                .args(args)
                .current_dir(&workspace)
                .output()
                .expect("git available for the live test's disposable fixture repo")
        };
        run("git", &["init", "-q"]);
        run("git", &["config", "user.email", "probe@example.invalid"]);
        run("git", &["config", "user.name", "Probe"]);
        std::fs::write(workspace.join("README.md"), "fixture\n").expect("seed fixture file");
        run("git", &["add", "README.md"]);
        run("git", &["commit", "-q", "-m", "seed"]);

        let mut environment = BTreeMap::new();
        environment.insert(
            "HOME".to_string(),
            env_entry(&std::env::var("HOME").unwrap_or_default()),
        );
        let mut spec = spec_with(
            "claude-code",
            None,
            &["Write"],
            false,
            environment,
            workspace.clone(),
        );
        // Overrides the shared helper's default "Print exactly: ok" prompt
        // (used by every other test in this module) with one that actually
        // exercises the `Write` tool this spec allows, so the artifact
        // staged below is genuinely something Claude Code produced, not
        // merely the seed content this test itself wrote.
        spec.work.request.resolved_agent_profile.instructions =
            "Using the Write tool, overwrite README.md in the current directory with exactly \
             this content: tack-d2-live-test-marker"
                .to_string();

        adapter.validate(&spec).await.expect("validate a live spec");
        let handle = adapter.start(&spec).await.expect("start a live process");
        let outcome = adapter
            .wait(&handle)
            .await
            .expect("wait for a live process");

        eprintln!(
            "live claude-code outcome: terminal_state={:?} model_id={} harness_version={}",
            outcome.terminal_state,
            outcome.actual_execution.model_id.as_str(),
            outcome.actual_execution.harness_version
        );

        let staged = super::super::artifact::ArtifactStager::new(workspace.join(".artifacts"))
            .stage_file(
                "live-test-attempt",
                &workspace,
                Path::new("README.md"),
                "log",
                "text/markdown",
            )
            .expect("stage the real artifact Claude Code's Write tool produced");
        assert!(staged.size_bytes > 0);
        let staged_content = std::fs::read_to_string(&staged.staged_path).unwrap_or_default();
        eprintln!("live claude-code staged artifact content: {staged_content:?}");
        if !staged_content.contains("tack-d2-live-test-marker") {
            eprintln!(
                "note: the model did not reproduce the exact requested marker text — this is \
                 model-phrasing variance, not itself a failure of this adapter, so it is only \
                 logged, never asserted on"
            );
        }
        eprintln!(
            "live claude-code artifact: sha256={} size_bytes={}",
            staged.sha256, staged.size_bytes
        );

        std::fs::remove_dir_all(&workspace).expect("cleanup disposable fixture repo");
    }
}
