//! OpenCode harness adapter/probe (card III-D3).
//!
//! Implements the frozen [`crate::harness::HarnessAdapter`] (`engine::HarnessAdapter`,
//! unchanged by this card) and [`crate::harness::HarnessProbe`] for
//! `harness_kind = "opencode"`, composing D4's shared process/redaction/artifact
//! infrastructure (`crate::harness::{process, redact, artifact}`), the same
//! seam D1's [`crate::harness::codex::CodexAdapter`] composes.
//!
//! ## This card's advantage: `opencode` 1.18.0 is actually installed here
//!
//! Unlike D1 (no `codex` binary present), every claim below marked
//! **observed** was produced by actually invoking a real, unmodified
//! `opencode` 1.18.0 binary (`which opencode` → a linuxbrew install), always
//! inside an isolated `HOME`/`XDG_*` sandbox pointed at a throwaway temp
//! directory (never this repository, never the developer's real
//! `~/.local/share/opencode/auth.json`, which already holds unrelated live
//! configuration on this machine), and always using opencode's own
//! zero-credential `opencode/*` "zen" models — no real provider credential
//! was ever passed, read, or logged. The exact commands are reproduced in
//! `docs/agent-handoffs/part-iii/III-D3.md`. Everything else is marked
//! **assumed** and is deliberately conservative (mirroring D1's `codex.rs`
//! precedent, e.g. rejecting auto-selection) rather than guessed.
//!
//! **Observed facts this adapter's design rests on:**
//!
//! 1. `opencode --version` (or `-v`) prints a bare `X.Y.Z\n` on stdout,
//!    nothing else, exit 0. See [`is_strict_version`]/[`OpenCodeAdapter::detect_version`].
//! 2. `opencode models` prints one `<providerID>/<modelID>` line per
//!    discoverable model — the CLI's own `-m/--model` flag documents this
//!    exact `provider/model` combination syntax. With zero configured
//!    credentials, exactly six zero-cost `opencode/*` models are listed.
//!    See [`parse_model_combinations`].
//! 3. `opencode run --format json [--model <provider>/<model>]` is the
//!    non-interactive invocation. With no positional `message` argument, it
//!    reads the prompt from **stdin** — confirmed directly, not assumed —
//!    which is exactly `ProcessSpec`'s documented preference (argv is
//!    world-readable via `ps`/`/proc/<pid>/cmdline`; stdin is not). Stdout is
//!    newline-delimited JSON events (`step_start`, `text`, `step_finish`,
//!    `error`, each `{"type", "timestamp", "sessionID", ...}`); a successful
//!    `step_finish` event carries a real `part.tokens{input,output,...}` and
//!    `part.cost` — genuine harness-reported usage, not estimated. See
//!    [`parse_jsonl_events`]/[`summarize_events`].
//! 4. **A valid model id paired with the wrong provider is not rejected by
//!    opencode itself.** `opencode run --model anthropic/big-pickle ...`
//!    (a model id that is real, just under the `opencode` provider, not
//!    `anthropic`) starts a session and only fails after the fact:
//!    `{"type":"error","error":{"name":"UnknownError","data":{"message":
//!    "Unexpected server error. Check server logs for details.", ...}}}` on
//!    stdout, process exit 1. This is the direct, observed justification for
//!    this adapter's own pre-spawn pairing check in
//!    [`OpenCodeAdapter::check_pairing_supported`] — opencode provides no
//!    protection against this itself.
//! 5. `SIGTERM` cleanly stops a plain `opencode run` (single process, its own
//!    process-group leader; no descendant observed for a conversational
//!    prompt) — no orphaned processes, no partial stdout flushed. A prompt
//!    that triggers a `bash` tool call proceeded **without** any interactive
//!    approval event and without `--auto`, in the sandboxed environment this
//!    card tested against (permission behavior could differ under a
//!    different `opencode.json`; the configured attempt timeout is the
//!    universal safety net regardless).
//! 6. `opencode` needs a working `PATH` in its *own* environment (to find
//!    `bash`/`git`/etc. for its own tool calls) and a working `HOME`/`XDG_*`
//!    (to find its config/credentials/session database) — it is a Bun/Node
//!    binary, not a shell, so it does not get a shell's implicit
//!    default-`PATH`-when-unset fallback. Because `ProcessSpec::spawn`
//!    always starts a child from a **cleared** environment, this adapter
//!    must explicitly forward these from the runner process's own
//!    environment (see [`default_passthrough_env`]) or every real
//!    invocation — including the harness's own internal tool calls — would
//!    silently lose its ability to find anything.
//! 7. `opencode export <sessionID>` returns a genuine JSON document (not
//!    NDJSON; `Exporting session: ...` goes to *stderr*, stdout is pure
//!    JSON) whose `info.model.{providerID,id}` names the model **actually**
//!    used, confirmed independent of what `--model` requested. This is the
//!    one gap this adapter does **not** close (see "What this adapter does
//!    not attempt" below) — flagged for a future card, not silently ignored.
//!
//! ## What this adapter does not attempt
//!
//! - **Auto-selected models are rejected pre-spawn**, exactly like D1's
//!   `codex.rs`. `ActualExecution.model_provider`/`model_id` are non-nullable
//!   (III.1.3), and opencode's `--format json` event stream never names the
//!   provider/model it used for *any* event type observed by this card
//!   (`step_start`/`text`/`step_finish`/`error` — none carry a model field).
//!   Fact 7 above shows a real fix exists (`opencode export`), but wiring a
//!   *second* subprocess call into every successful `wait()` — with its own
//!   timeout/failure handling — is real, additional scope this card
//!   deliberately did not take on; it is not silently narrowed, and it is
//!   the recommended shape for whichever future card wants full
//!   auto-selection support. Two of three Wave 3 adapters independently
//!   landing on the same "reject auto-selection, cannot honestly confirm
//!   the actual model" conclusion is itself evidence for D5.
//! - **`opencode`'s own permission/tool policy is not translated from
//!   `PermissionPolicy`.** OpenCode has its own permission model
//!   (`opencode.json`, a `--auto` flag) that does not map cleanly onto the
//!   generic cross-harness `{tools: Vec<String>, network: bool}` shape
//!   without more evidence than this card gathered; inventing a mapping
//!   risked looking like enforcement without being enforcement. `--auto` is
//!   never passed (its own `--help` text calls it "(dangerous!)"); the
//!   attempt's own timeout is the safety net if some other `opencode.json`
//!   configuration would otherwise hang awaiting interactive approval.
//! - **No opencode-specific artifact discovery** (e.g. a git diff of files
//!   it changed). The raw, already-redacted event stream is staged as a log
//!   artifact via D4's [`ArtifactStager`] (`artifacts: advisory`).
//!
//! ## Why this contrasts with `codex.rs`: the card's distinguishing requirement
//!
//! Codex's `model_combinations` is always empty (assumption 7 in
//! `codex.rs`'s own module docs) — Codex's real model-discovery mechanism is
//! unverified, so its `validate()` can only check that *some*
//! provider/model pair was specified, never that the specific pair is real.
//! This adapter's [`parse_model_combinations`] genuinely enumerates
//! opencode's real, discovered provider→model_ids pairings (grouped by
//! provider, never flattened into one global model list), and
//! [`OpenCodeAdapter::check_pairing_supported`] validates the *exact pair*
//! against them — catching a valid model id paired with a provider that
//! never actually offers it, which a flattened list could never distinguish.
//! This is the card's distinguishing requirement, made possible only because
//! a real `opencode` binary was available to observe.

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tack_orch::execution::{
    ActualExecution, ActualModelId, ActualModelProvider, CapabilitySupport, CapabilityValue,
    FeatureCapabilities, HarnessCapability, HarnessKind, Measurement, MeasurementSource,
    ModelCombination, ModelId, ModelProvider, Usage, WorkspaceId as DomainWorkspaceId,
};

use crate::client::{AttemptState, Timestamp};
use crate::harness::{
    AttemptJournal, CancelObservation, CancellationEvidence, ExecutionSpec, HarnessAdapter,
    HarnessError, HarnessOutcome, HarnessProbe, LocalRunHandle, RecoveryObservation,
    artifact::ArtifactStager,
    process::{
        CancelOutcome, CapturedOutput, ProcessExit, ProcessLimits, ProcessSpec, SupervisedProcess,
    },
    redact::SecretMaterial,
};

const OPENCODE_HARNESS_KIND: &str = "opencode";
const OPENCODE_PROGRAM_NAME: &str = "opencode";
const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(15);
/// Reuses D1's exact convention for "this adapter cannot confirm which
/// model the harness actually used, so `ActualExecution` echoes the
/// validated request instead" — see `codex.rs`'s identical constant. Card
/// III-D5 centralized both adapters' shared literal into
/// [`crate::harness::ModelObservationSource`]; this constant is unchanged.
const MODEL_OBSERVATION_SOURCE: &str =
    crate::harness::ModelObservationSource::RequestedNotConfirmed.as_str();

/// Where to find the `opencode` executable.
#[derive(Clone)]
enum OpenCodeLocator {
    /// Searches `search_dirs` (a snapshot of `PATH`, taken once at
    /// construction) for `program_name`. Production default via
    /// [`OpenCodeAdapter::discover`].
    Search {
        program_name: String,
        search_dirs: Vec<PathBuf>,
    },
    /// A fixed program plus prefix args — how every fake-binary test in this
    /// file points the adapter at `crate::harness::fixtures::fake_harness_command`
    /// (or, for provider/model-enumeration and event-stream tests, a small
    /// inline `/bin/sh -c '...'` this card writes at test time — never a new
    /// checked-in fixture file, matching D1's identical choice to keep this
    /// directory free of concurrent sibling path collisions). Never
    /// constructed by production code (only [`OpenCodeLocator::Search`] is,
    /// via [`OpenCodeAdapter::discover`]), so this variant is
    /// `#[cfg(test)]`-only — matches `codex.rs`'s identical
    /// `CodexLocator::Fixed` gating.
    #[cfg(test)]
    Fixed {
        program: PathBuf,
        prefix_args: Vec<String>,
    },
}

impl OpenCodeLocator {
    fn resolve(&self) -> Result<(PathBuf, Vec<String>), String> {
        match self {
            #[cfg(test)]
            Self::Fixed {
                program,
                prefix_args,
            } => Ok((program.clone(), prefix_args.clone())),
            Self::Search {
                program_name,
                search_dirs,
            } => locate_in_dirs(program_name, search_dirs)
                .map(|program| (program, Vec::new()))
                .ok_or_else(|| format!("`{program_name}` was not found on PATH")),
        }
    }
}

fn system_path_dirs() -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect())
        .unwrap_or_default()
}

/// Dependency-free `PATH` search — mirrors `codex.rs`'s identical helper
/// (itself mirroring why `harness/process.rs` declares `kill(2)` via a bare
/// `extern "C"` instead of adding the `which` crate: this is one small,
/// stable, well-understood piece of logic that does not need a dependency).
fn locate_in_dirs(program_name: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    for dir in dirs {
        let candidate = dir.join(program_name);
        if !candidate.is_file() {
            continue;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            match std::fs::metadata(&candidate) {
                Ok(metadata) if metadata.permissions().mode() & 0o111 != 0 => {}
                _ => continue,
            }
        }
        return Some(candidate);
    }
    None
}

/// The environment variables forwarded verbatim from the runner process's
/// own environment into every `opencode` invocation, probe and run alike.
/// `PATH` and `HOME`/`XDG_*` are both required in practice (observed fact
/// 6, module docs): `ProcessSpec` always starts a child from a cleared
/// environment, and opencode is a Bun/Node binary with no shell-style
/// implicit fallback when either is missing. Captured once, not re-read per
/// call, so one adapter instance's behavior cannot drift mid-run if the
/// runner process's own environment changes.
fn default_passthrough_env() -> BTreeMap<String, String> {
    const FORWARDED: &[&str] = &[
        "PATH",
        "HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_CACHE_HOME",
        "XDG_STATE_HOME",
    ];
    FORWARDED
        .iter()
        .filter_map(|name| {
            std::env::var(name)
                .ok()
                .map(|value| ((*name).to_owned(), value))
        })
        .collect()
}

/// Strict `X.Y.Z` numeric-only check (observed fact 1: the real binary
/// prints exactly `"1.18.0"`, nothing else). Deliberately whole-string, not
/// substring, and deliberately rejects a two-segment `X.Y` unlike `codex.rs`'s
/// more lenient `2..=3`: every real observation of opencode's version output
/// was three segments, and being stricter here is a direct reflection of
/// that, not an arbitrary choice. The shared fixture's `unknown_version`
/// mode (`"harness-cli version 999.999.999-nightly-exotic-format"`) genuinely
/// *contains* a dot-separated numeric run but has leading words and must not
/// be reported as a clean version line.
fn is_strict_version(candidate: &str) -> bool {
    let parts: Vec<&str> = candidate.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
}

fn bounded_preview(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_owned()
    } else {
        let truncated: String = text.chars().take(max_chars).collect();
        format!("{truncated}\u{2026} (truncated)")
    }
}

fn rfc3339(time: std::time::SystemTime) -> String {
    DateTime::<Utc>::from(time).to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn measured<T>(value: T) -> Measurement<T> {
    Measurement {
        value: Some(value),
        source: MeasurementSource::Measured,
        additional: BTreeMap::new(),
    }
}

fn not_measured<T>() -> Measurement<T> {
    Measurement {
        value: None,
        source: MeasurementSource::NotMeasured,
        additional: BTreeMap::new(),
    }
}

fn describe_capture(output: &CapturedOutput) -> serde_json::Value {
    serde_json::json!({
        "truncated": output.truncated,
        "bytes_dropped": output.bytes_dropped,
        "total_bytes_seen": output.total_bytes_seen,
        // Already scrubbed by `SecretMaterial` before this text was ever
        // retained (see `process.rs::finalize_capture`); bounding it again
        // here is about payload size, not redaction.
        "text_preview": bounded_preview(&output.text, 2000),
    })
}

/// Terminal-state classification from the process exit alone. Observed fact
/// 4 (the one real error case this card actually tested — a wrong
/// provider/model pairing) exited nonzero, agreeing with the exit code; this
/// adapter trusts exit code as authoritative rather than also flipping
/// `terminal_state` on a parsed `{"type":"error"}` event, since doing the
/// latter is not backed by a case where the two ever disagreed. A parsed
/// error event still enriches `terminal_reason` for diagnosis — see
/// [`summarize_events`] — it just never overrides this classification.
fn classify_exit(exit: &ProcessExit) -> (AttemptState, &'static str, String) {
    match exit {
        ProcessExit::Exited(0) => (
            AttemptState::Succeeded,
            "completed",
            "opencode exited successfully".to_owned(),
        ),
        ProcessExit::Exited(code) => (
            AttemptState::Failed,
            "exit_code",
            format!("opencode exited with status {code}"),
        ),
        #[cfg(unix)]
        ProcessExit::Signaled(signal) => (
            AttemptState::Failed,
            "signaled",
            format!("opencode was terminated by signal {signal}"),
        ),
        ProcessExit::TimedOut => (
            AttemptState::Failed,
            "timed_out",
            "opencode exceeded its configured timeout and was killed".to_owned(),
        ),
    }
}

/// The opaque handle format this adapter hands back from `start` and expects
/// from `cancel`/`wait`/`reconcile`: `opencode:<pid>:<monotonic counter>`.
/// The counter exists only to guarantee uniqueness within one adapter
/// instance's lifetime (pids can, in principle, be reused); it carries no
/// other meaning. Mirrors `codex.rs`'s identical encoding.
fn encode_handle(pid: u32, counter: u64) -> String {
    format!("opencode:{pid}:{counter}")
}

fn parse_handle_pid(process_id: &str) -> Option<u32> {
    let mut parts = process_id.split(':');
    if parts.next()? != "opencode" {
        return None;
    }
    let pid = parts.next()?.parse::<u32>().ok()?;
    parts.next()?; // counter, required but not itself inspected
    if parts.next().is_some() {
        return None; // exactly three colon-separated parts, no more
    }
    Some(pid)
}

/// Parses `opencode models`' plain-text output (observed fact 2: one
/// `<providerID>/<modelID>` line per model). Splits each line on the
/// *first* `/` only: the left half becomes the provider (a known, explicit
/// namespace field — never a bare `provider` per III.0's vocabulary rule),
/// the right half is retained byte-for-byte as the opaque model id, even if
/// it itself contains further `/` characters. This reads the CLI's own
/// documented `--model provider/model` combination syntax; it never parses
/// or interprets the model id itself (Part III B1's rule).
///
/// A line that doesn't contain a `/`, or that would produce an empty
/// provider or model half, invalidates the *entire* batch (`Err`) rather
/// than being silently skipped: dropping a malformed line would silently
/// under-report a real combination to whatever scheduler (Wave 4's E1)
/// trusts this list to be complete. Empty output (no lines at all) is not
/// itself an error — it means zero models are currently discoverable, a
/// real, valid state, not a parse failure.
fn parse_model_combinations(stdout: &str) -> Result<Vec<ModelCombination>, String> {
    let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match line.split_once('/') {
            Some((provider, model)) if !provider.is_empty() && !model.is_empty() => {
                grouped
                    .entry(provider.to_owned())
                    .or_default()
                    .push(model.to_owned());
            }
            _ => {
                return Err(format!(
                    "unrecognized `opencode models` line (expected `provider/model`): {}",
                    bounded_preview(line, 200)
                ));
            }
        }
    }
    Ok(grouped
        .into_iter()
        .map(|(provider, model_ids)| ModelCombination {
            model_provider: ModelProvider::new(provider),
            model_ids: model_ids.into_iter().map(ModelId::new).collect(),
            discovery: "reported".to_owned(),
            additional: BTreeMap::new(),
        })
        .collect())
}

/// Best-effort parse of `opencode run --format json`'s stdout as
/// newline-delimited JSON events (observed fact 3). Returns `None` if *any*
/// non-empty line fails to parse as JSON, rather than a partial list: a
/// harness whose output does not match the expected shape (including the
/// shared fake binary's own `success`/`malformed` modes, which are not JSON
/// at all) should not have some of its lines silently trusted and others
/// silently dropped. This never affects `terminal_state` (see
/// [`classify_exit`]) — only how much enrichment (real usage numbers, a
/// diagnostic error message) `terminal_reason` can honestly carry.
fn parse_jsonl_events(stdout: &str) -> Option<Vec<serde_json::Value>> {
    let mut events = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        events.push(serde_json::from_str::<serde_json::Value>(line).ok()?);
    }
    Some(events)
}

struct EventSummary {
    tokens_in: Option<u64>,
    tokens_out: Option<u64>,
    cost_usd: Option<f64>,
    error_event: Option<serde_json::Value>,
    event_count: usize,
}

/// Extracts real usage from the *last* `step_finish` event's
/// `part.tokens.{input,output}`/`part.cost` (observed fact 3), and the
/// first `{"type":"error"}` event's full payload, if any, for diagnostic
/// enrichment (observed fact 4).
fn summarize_events(events: &[serde_json::Value]) -> EventSummary {
    let error_event = events
        .iter()
        .find(|event| event.get("type").and_then(|value| value.as_str()) == Some("error"))
        .cloned();
    let finish = events
        .iter()
        .rev()
        .find(|event| event.get("type").and_then(|value| value.as_str()) == Some("step_finish"));
    let tokens = finish.and_then(|event| event.pointer("/part/tokens"));
    EventSummary {
        tokens_in: tokens
            .and_then(|value| value.get("input"))
            .and_then(|value| value.as_u64()),
        tokens_out: tokens
            .and_then(|value| value.get("output"))
            .and_then(|value| value.as_u64()),
        cost_usd: finish
            .and_then(|event| event.pointer("/part/cost"))
            .and_then(|value| value.as_f64()),
        error_event,
        event_count: events.len(),
    }
}

/// State for one in-flight `start()` → (`cancel()` | `wait()`) pair. Not
/// `Debug`: several fields (`secrets`, the captured process handle) must
/// never be printable by accident (rule 12) — omitting the derive entirely
/// is simpler than auditing a hand-written impl every time a field is added.
struct RunningOpenCodeProcess {
    process: SupervisedProcess,
    secrets: SecretMaterial,
    limits: ProcessLimits,
    started_at: DateTime<Utc>,
    workspace_path: PathBuf,
    workspace_id: String,
    base_revision: String,
    attempt_id: String,
    harness_version: String,
    model_provider: String,
    model_id: String,
}

/// The OpenCode harness adapter/probe. Implements both
/// [`crate::harness::HarnessAdapter`] (the frozen per-attempt lifecycle) and
/// [`crate::harness::HarnessProbe`] (capability discovery).
///
/// Time is injected via `C: crate::Clock` (never `SystemTime::now()`
/// directly) so tests can assert exact `started_at`/`ended_at`/`probed_at`
/// values without a real sleep (rule 9).
pub struct OpenCodeAdapter<C = crate::SystemClock> {
    command: OpenCodeLocator,
    process_limits: ProcessLimits,
    probe_timeout: Duration,
    /// Extra environment merged into every version-detection and
    /// model-listing invocation only, on top of `passthrough_env`. Always
    /// empty in production ([`Self::discover`]); fake-binary tests use this
    /// to steer the shared fixture's `TACK_FAKE_HARNESS_MODE` during probing
    /// specifically, independent of whatever a given test's `start()` call
    /// drives via the execution request's own `environment` map.
    probe_env: BTreeMap<String, String>,
    /// `PATH`/`HOME`/`XDG_*` forwarded from the runner process's own
    /// environment — see [`default_passthrough_env`]. Captured explicitly
    /// (not read fresh from `std::env` on every call) so tests can point
    /// this at an isolated sandbox directory instead of the real one.
    passthrough_env: BTreeMap<String, String>,
    artifact_staging_root: PathBuf,
    clock: C,
    next_handle: AtomicU64,
    running: tokio::sync::Mutex<BTreeMap<String, RunningOpenCodeProcess>>,
    /// The most recently probed `(installed_version, probe_error)`, used to
    /// stamp `ActualExecution.harness_version` at `wait()` time without a
    /// redundant `--version` invocation on every single attempt. `None`
    /// until the first [`HarnessProbe::probe`] call; `start()` falls back to
    /// a one-off detection in that case rather than reporting a silently
    /// fabricated version. Mirrors `codex.rs`'s identical cache.
    last_probe: tokio::sync::Mutex<Option<(String, Option<String>)>>,
}

impl OpenCodeAdapter<crate::SystemClock> {
    /// Production constructor: resolves `opencode` from the current
    /// process's `PATH` (snapshotted once, here) rather than a hardcoded
    /// path, and forwards `PATH`/`HOME`/`XDG_*` from the same real
    /// environment (observed fact 6). `artifact_staging_root` is required
    /// explicitly, matching [`ArtifactStager::new`]'s own no-hidden-default
    /// style.
    pub fn discover(process_limits: ProcessLimits, artifact_staging_root: PathBuf) -> Self {
        Self::with_clock(
            OpenCodeLocator::Search {
                program_name: OPENCODE_PROGRAM_NAME.to_owned(),
                search_dirs: system_path_dirs(),
            },
            process_limits,
            DEFAULT_PROBE_TIMEOUT,
            BTreeMap::new(),
            default_passthrough_env(),
            artifact_staging_root,
            crate::SystemClock,
        )
    }

    /// Card III-D5, `pub(crate)` and test-only: points this adapter at an
    /// arbitrary fixture command instead of a real `opencode` binary, for
    /// the "same fixture completes through all three fake adapters"
    /// acceptance proof in `harness::mod::tests` (which needs to construct a
    /// real `OpenCodeAdapter` from outside this module). No `probe_env`
    /// override: unlike this adapter's own test module (which needs to
    /// steer the shared fake binary's single-purpose
    /// `TACK_FAKE_HARNESS_MODE`), the cross-adapter fixture script this is
    /// built for branches on its own argv, so one fixed command serves
    /// version probing, model listing and running alike.
    #[cfg(test)]
    pub(crate) fn for_fixture(
        program: PathBuf,
        prefix_args: Vec<String>,
        artifact_staging_root: PathBuf,
    ) -> Self {
        Self::with_clock(
            OpenCodeLocator::Fixed {
                program,
                prefix_args,
            },
            ProcessLimits::new(1_000_000, 1_000_000, Duration::from_secs(10)),
            Duration::from_secs(5),
            BTreeMap::new(),
            BTreeMap::new(),
            artifact_staging_root,
            crate::SystemClock,
        )
    }
}

impl<C> OpenCodeAdapter<C>
where
    C: crate::Clock,
{
    #[allow(clippy::too_many_arguments)]
    fn with_clock(
        command: OpenCodeLocator,
        process_limits: ProcessLimits,
        probe_timeout: Duration,
        probe_env: BTreeMap<String, String>,
        passthrough_env: BTreeMap<String, String>,
        artifact_staging_root: PathBuf,
        clock: C,
    ) -> Self {
        Self {
            command,
            process_limits,
            probe_timeout,
            probe_env,
            passthrough_env,
            artifact_staging_root,
            clock,
            next_handle: AtomicU64::new(0),
            running: tokio::sync::Mutex::new(BTreeMap::new()),
            last_probe: tokio::sync::Mutex::new(None),
        }
    }

    fn base_env(&self) -> BTreeMap<String, String> {
        self.passthrough_env.clone()
    }

    /// The cheap, structural half of "unsupported selection fails
    /// pre-spawn": harness-kind self-check plus "both provider and model
    /// must be present, or neither" (opencode's `--model` flag requires an
    /// explicit `provider/model` pair; there is no way to construct it from
    /// just one half). Shared by `validate` and `start` so the two can never
    /// disagree about what counts as a structurally unsupported selection.
    /// The *expensive* half — whether the specific pair is genuinely
    /// discoverable — is [`Self::check_pairing_supported`], called only from
    /// `validate`; `start` relies on the engine's guaranteed
    /// validate-before-start ordering rather than re-probing on every
    /// attempt (mirrors `codex.rs`'s identical `check_selection`/`start`
    /// split, applied to a check this card additionally has that Codex does
    /// not).
    fn check_selection(&self, spec: &ExecutionSpec) -> Result<(String, String), HarnessError> {
        if spec.work.request.requested_harness_kind.as_str() != OPENCODE_HARNESS_KIND {
            let reason = format!(
                "requested harness kind {:?} does not match this adapter's kind \
                 {OPENCODE_HARNESS_KIND:?}",
                spec.work.request.requested_harness_kind.as_str()
            );
            tracing::warn!(
                reason,
                "opencode: rejecting a spec requesting a different harness kind"
            );
            return Err(HarnessError::Rejected { reason });
        }
        match (
            &spec.work.request.requested_model_provider,
            &spec.work.request.requested_model_id,
        ) {
            (Some(provider), Some(model)) => {
                Ok((provider.as_str().to_owned(), model.as_str().to_owned()))
            }
            (None, None) => {
                let reason = "opencode cannot independently confirm which provider/model an \
                               auto-selected run actually used, so ActualExecution.model_provider/\
                               model_id (non-nullable) cannot be honestly filled; an explicit \
                               requested_model_provider and requested_model_id are both required"
                    .to_owned();
                tracing::warn!(
                    reason,
                    "opencode: rejecting an auto-selected model pre-spawn"
                );
                Err(HarnessError::Rejected { reason })
            }
            _ => {
                let reason = "opencode's --model flag requires an explicit provider/model pair; \
                               a partial selection (only one of requested_model_provider/\
                               requested_model_id) cannot be constructed into a valid --model \
                               argument"
                    .to_owned();
                tracing::warn!(
                    reason,
                    "opencode: rejecting a partial provider/model selection"
                );
                Err(HarnessError::Rejected { reason })
            }
        }
    }

    /// The expensive half: probes real capabilities and checks the exact
    /// `(provider, model)` pair against opencode's *own* discovered
    /// combinations — never against a flattened "any known model" list.
    /// This is the card's distinguishing requirement in code: a model id
    /// that is real, but only under a *different* provider, is rejected
    /// here, matching observed fact 4 (opencode itself does not protect
    /// against this).
    async fn check_pairing_supported(
        &self,
        provider: &str,
        model: &str,
    ) -> Result<(), HarnessError> {
        let capability = self.probe().await;
        if let Some(probe_reason) = &capability.probe_error {
            let reason = format!(
                "capability probe failed so the requested provider/model pairing cannot be \
                 confirmed: {probe_reason}"
            );
            tracing::warn!(
                reason,
                "opencode: rejecting execution, capability probe failed"
            );
            return Err(HarnessError::Rejected { reason });
        }
        let supported = capability.model_combinations.iter().any(|combo| {
            combo.model_provider.as_str() == provider
                && combo.model_ids.iter().any(|id| id.as_str() == model)
        });
        if supported {
            return Ok(());
        }
        let known_under_a_different_provider = capability.model_combinations.iter().any(|combo| {
            combo.model_provider.as_str() != provider
                && combo.model_ids.iter().any(|id| id.as_str() == model)
        });
        let reason = format!(
            "requested provider/model pairing {provider:?}/{model:?} is not among opencode's \
             discovered combinations{}",
            if known_under_a_different_provider {
                " (this model id is known, but only under a different provider)"
            } else {
                ""
            }
        );
        tracing::warn!(
            model_provider = provider,
            model_id = model,
            model_id_known_under_a_different_provider = known_under_a_different_provider,
            "opencode: rejecting execution, requested provider/model pairing is not among \
             opencode's discovered combinations"
        );
        Err(HarnessError::Rejected { reason })
    }

    /// Runs `opencode --version`, bounded by `self.probe_timeout`. Never
    /// returns an `Err`: every failure mode (binary missing, spawn failure,
    /// nonzero exit, timeout, unparseable output) is folded into the
    /// `Option<String>` (probe-error reason) return slot, matching
    /// `HarnessProbe::probe`'s own contract that probing itself cannot fail.
    async fn detect_version(
        &self,
    ) -> (String, Option<String>, BTreeMap<String, serde_json::Value>) {
        let (program, mut args) = match self.command.resolve() {
            Ok(resolved) => resolved,
            Err(reason) => return (String::new(), Some(reason), BTreeMap::new()),
        };
        args.push("--version".to_owned());
        let mut env = self.base_env();
        env.extend(self.probe_env.clone());

        let probe_workspace = std::env::temp_dir();
        let process_spec = ProcessSpec {
            program,
            args,
            env,
            stdin: None,
            working_directory: probe_workspace.clone(),
            workspace_root: probe_workspace,
        };

        let limits = ProcessLimits::new(8192, 8192, self.probe_timeout);
        let spawned = match process_spec.spawn().await {
            Ok(child) => child,
            Err(error) => {
                return (
                    String::new(),
                    Some(format!("opencode --version could not be spawned: {error}")),
                    BTreeMap::new(),
                );
            }
        };
        let result = match spawned
            .wait_with_capture(&limits, &SecretMaterial::new())
            .await
        {
            Ok(result) => result,
            Err(error) => {
                return (
                    String::new(),
                    Some(format!(
                        "opencode --version failed while capturing output: {error}"
                    )),
                    BTreeMap::new(),
                );
            }
        };

        match result.exit {
            ProcessExit::Exited(0) => {
                let trimmed = result.stdout.text.trim();
                if trimmed.is_empty() {
                    (
                        String::new(),
                        Some("opencode --version produced no output".to_owned()),
                        BTreeMap::new(),
                    )
                } else if is_strict_version(trimmed) {
                    (trimmed.to_owned(), None, BTreeMap::new())
                } else {
                    let mut additional = BTreeMap::new();
                    additional.insert(
                        "raw_version_output".to_owned(),
                        serde_json::json!(bounded_preview(trimmed, 200)),
                    );
                    (
                        String::new(),
                        Some(
                            "opencode --version output was not a recognizable version string"
                                .to_owned(),
                        ),
                        additional,
                    )
                }
            }
            ProcessExit::Exited(code) => (
                String::new(),
                Some(format!("opencode --version exited with status {code}")),
                BTreeMap::new(),
            ),
            #[cfg(unix)]
            ProcessExit::Signaled(signal) => (
                String::new(),
                Some(format!(
                    "opencode --version was terminated by signal {signal}"
                )),
                BTreeMap::new(),
            ),
            ProcessExit::TimedOut => (
                String::new(),
                Some("opencode --version timed out".to_owned()),
                BTreeMap::new(),
            ),
        }
    }

    /// Runs `opencode models`, bounded by `self.probe_timeout`, and parses
    /// its output via [`parse_model_combinations`].
    async fn list_model_combinations(&self) -> Result<Vec<ModelCombination>, String> {
        let (program, mut args) = self.command.resolve()?;
        args.push("models".to_owned());
        let mut env = self.base_env();
        env.extend(self.probe_env.clone());

        let probe_workspace = std::env::temp_dir();
        let process_spec = ProcessSpec {
            program,
            args,
            env,
            stdin: None,
            working_directory: probe_workspace.clone(),
            workspace_root: probe_workspace,
        };

        let limits = ProcessLimits::new(1_048_576, 65_536, self.probe_timeout);
        let spawned = process_spec
            .spawn()
            .await
            .map_err(|error| format!("opencode models could not be spawned: {error}"))?;
        let result = spawned
            .wait_with_capture(&limits, &SecretMaterial::new())
            .await
            .map_err(|error| format!("opencode models failed while capturing output: {error}"))?;

        match result.exit {
            ProcessExit::Exited(0) => parse_model_combinations(&result.stdout.text),
            ProcessExit::Exited(code) => Err(format!("opencode models exited with status {code}")),
            #[cfg(unix)]
            ProcessExit::Signaled(signal) => {
                Err(format!("opencode models was terminated by signal {signal}"))
            }
            ProcessExit::TimedOut => Err("opencode models timed out".to_owned()),
        }
    }

    /// Honest feature support (rule 7: real reasons, not guesses). Contrast
    /// with `codex.rs`'s `feature_capabilities`: `usage` is `advisory` here
    /// (not `unsupported`) because this card *did* observe real token/cost
    /// figures in opencode's own output — see [`summarize_events`].
    fn feature_capabilities(&self) -> FeatureCapabilities {
        FeatureCapabilities {
            // Card III-D5 finding 1: downgraded from `Supported`. This
            // adapter's only cancellation primitive is
            // `harness::process::SupervisedProcess::cancel` (a process-group
            // SIGTERM/SIGKILL) — the exact mechanism D2 proved (via `ps`,
            // twice, against real Claude Code) cannot reliably reach a
            // descendant a harness's own shell-tool spawns into a new OS
            // session. An adversarial check against this card's own real
            // `opencode` binary found the identical disjoint-session pattern
            // for a bash-tool subprocess (`ps`: the tool subprocess and its
            // own backgrounded child share a session/group disjoint from the
            // top-level `opencode run` process), so `Supported` was never
            // justified here either — observed fact 5 in this file's module
            // docs only tested a plain conversational run with no live tool
            // subprocess in flight at cancellation time, the easy case.
            cancel: CapabilityValue {
                support: CapabilitySupport::Advisory,
                reason: Some(
                    "the top-level opencode process is always signalled reliably (it is always \
                     its own process-group leader), but a bash-tool subprocess was observed \
                     (via `ps`) running in its own session, distinct from that group — the same \
                     pattern D2 found for Claude Code — so it is only guaranteed reached if \
                     opencode exits gracefully within the SIGTERM grace period; a SIGKILL \
                     escalation cannot reach a different session's process group"
                        .to_owned(),
                ),
                additional: BTreeMap::new(),
            },
            resume: CapabilityValue {
                support: CapabilitySupport::Unsupported,
                reason: Some(
                    "opencode exposes --continue/--session for interactive session \
                     resumption, but resuming a cancelled non-interactive run from its point \
                     of cancellation has not been observed or implemented by this adapter"
                        .to_owned(),
                ),
                additional: BTreeMap::new(),
            },
            decisions: CapabilityValue {
                support: CapabilitySupport::Unsupported,
                reason: Some(
                    "no mid-run operator-decision/question event was observed in opencode's \
                     --format json event stream; a basic tool call (bash) proceeded without any \
                     interactive approval event in the sandboxed environment this card tested \
                     against, and the runner protocol has no wired decision transport yet"
                        .to_owned(),
                ),
                additional: BTreeMap::new(),
            },
            artifacts: CapabilityValue {
                support: CapabilitySupport::Advisory,
                reason: Some(
                    "the raw captured, already-redacted event stream is staged as a log \
                     artifact; no opencode-specific artifact selection (e.g. a git diff of \
                     files it changed) is implemented"
                        .to_owned(),
                ),
                additional: BTreeMap::new(),
            },
            usage: CapabilityValue {
                support: CapabilitySupport::Advisory,
                reason: Some(
                    "token counts and dollar cost are read directly from the harness's own \
                     step_finish JSON event when the run's stdout parses as the expected \
                     newline-delimited event stream; a run whose output does not parse falls \
                     back to not_measured rather than a fabricated figure"
                        .to_owned(),
                ),
                additional: BTreeMap::new(),
            },
            additional: BTreeMap::new(),
        }
    }

    async fn take_running(&self, process_id: &str) -> Result<RunningOpenCodeProcess, HarnessError> {
        self.running.lock().await.remove(process_id).ok_or_else(|| {
            tracing::warn!(
                process_id,
                "opencode: handle not tracked by this adapter instance"
            );
            HarnessError::Process
        })
    }

    /// Stages the (already-scrubbed) combined stdout/stderr as a `log`
    /// artifact inside the attempt's own workspace, via D4's
    /// [`ArtifactStager`]. Best-effort: staging failure never fails the
    /// attempt itself. `media_type` reflects whether stdout actually parsed
    /// as the expected event stream — an honest, small extra precision over
    /// `codex.rs`'s always-`text/plain` choice, since this adapter *does*
    /// know when its output was genuinely NDJSON-shaped.
    fn stage_run_log(
        &self,
        workspace_path: &std::path::Path,
        attempt_id: &str,
        stdout: &CapturedOutput,
        stderr: &CapturedOutput,
        parsed_as_event_stream: bool,
    ) -> Option<serde_json::Value> {
        let relative = PathBuf::from(".tack-runner").join("opencode-run.log");
        let absolute = workspace_path.join(&relative);
        if let Some(parent) = absolute.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                return None;
            }
        }
        let mut combined = String::new();
        combined.push_str("=== stdout ===\n");
        combined.push_str(&stdout.text);
        combined.push_str("\n=== stderr ===\n");
        combined.push_str(&stderr.text);
        if std::fs::write(&absolute, combined.as_bytes()).is_err() {
            return None;
        }

        let media_type = if parsed_as_event_stream {
            "application/x-ndjson"
        } else {
            "text/plain"
        };
        let stager = ArtifactStager::new(&self.artifact_staging_root);
        match stager.stage_file(attempt_id, workspace_path, &relative, "log", media_type) {
            Ok(staged) => Some(serde_json::json!({
                "kind": staged.kind,
                "name": staged.name,
                "media_type": staged.media_type,
                "size_bytes": staged.size_bytes,
                "sha256": staged.sha256,
                "staged_path": staged.staged_path.display().to_string(),
            })),
            Err(error) => {
                tracing::warn!(?error, "opencode wait: artifact staging failed");
                None
            }
        }
    }
}

#[async_trait]
impl<C> HarnessProbe for OpenCodeAdapter<C>
where
    C: crate::Clock,
{
    fn harness_kind(&self) -> HarnessKind {
        HarnessKind::new(OPENCODE_HARNESS_KIND)
    }

    async fn probe(&self) -> HarnessCapability {
        let probed_at = DateTime::<Utc>::from(self.clock.now());
        let (installed_version, version_error, mut additional) = self.detect_version().await;
        *self.last_probe.lock().await = Some((installed_version.clone(), version_error.clone()));

        if let Some(reason) = version_error {
            return HarnessCapability {
                harness_kind: HarnessKind::new(OPENCODE_HARNESS_KIND),
                installed_version,
                probe_error: Some(reason),
                probed_at,
                model_combinations: Vec::new(),
                additional,
            };
        }

        match self.list_model_combinations().await {
            Ok(model_combinations) => HarnessCapability {
                harness_kind: HarnessKind::new(OPENCODE_HARNESS_KIND),
                installed_version,
                probe_error: None,
                probed_at,
                model_combinations,
                additional,
            },
            Err(reason) => {
                additional.insert("model_listing_error".to_owned(), serde_json::json!(reason));
                HarnessCapability {
                    harness_kind: HarnessKind::new(OPENCODE_HARNESS_KIND),
                    installed_version: installed_version.clone(),
                    probe_error: Some(format!(
                        "installed_version {installed_version} confirmed; provider/model \
                         enumeration failed (see additional.model_listing_error)"
                    )),
                    probed_at,
                    model_combinations: Vec::new(),
                    additional,
                }
            }
        }
    }

    fn declared_capabilities(&self) -> FeatureCapabilities {
        self.feature_capabilities()
    }
}

#[async_trait]
impl<C> HarnessAdapter for OpenCodeAdapter<C>
where
    C: crate::Clock,
{
    async fn validate(&self, spec: &ExecutionSpec) -> Result<(), HarnessError> {
        let (provider, model) = self.check_selection(spec)?;
        self.command.resolve().map_err(|reason| {
            tracing::warn!(reason, "opencode validate: binary unresolvable");
            HarnessError::Rejected { reason }
        })?;
        self.check_pairing_supported(&provider, &model).await
    }

    async fn start(&self, spec: &ExecutionSpec) -> Result<LocalRunHandle, HarnessError> {
        let (model_provider, model_id) = self.check_selection(spec)?;
        let (program, mut args) = self.command.resolve().map_err(|reason| {
            tracing::warn!(reason, "opencode start: binary unresolvable");
            HarnessError::Rejected { reason }
        })?;

        args.push("run".to_owned());
        args.push("--format".to_owned());
        args.push("json".to_owned());
        args.push("--model".to_owned());
        args.push(format!("{model_provider}/{model_id}"));

        // Observed fact 3: `opencode run` reads the message from stdin when
        // no positional message argument is given, so the prompt never
        // needs to touch argv (never visible via `ps`/`/proc/<pid>/cmdline`).
        let prompt = spec
            .work
            .request
            .resolved_agent_profile
            .instructions
            .clone();
        let mut secrets = SecretMaterial::new();
        secrets.register(prompt.clone());

        let mut env = self.base_env();
        for (key, value) in &spec.work.request.environment {
            if let Some(literal) = &value.value {
                secrets.register(literal.clone());
                env.insert(key.clone(), literal.clone());
            }
            // `secret_reference` entries are deliberately never resolved
            // here: no secret-store client exists in tack-runner yet (the
            // same, already-documented gap D4 flagged for event/artifact
            // transport, and D1 flagged identically for codex).
        }

        let timeout = if spec.work.request.timeout_seconds > 0 {
            Duration::from_secs(spec.work.request.timeout_seconds)
        } else {
            self.process_limits.timeout
        };
        let limits = ProcessLimits {
            timeout,
            ..self.process_limits.clone()
        };

        let process_spec = ProcessSpec {
            program,
            args,
            env,
            stdin: Some(prompt.into_bytes()),
            working_directory: spec.workspace.path.clone(),
            workspace_root: spec.workspace.path.clone(),
        };

        let supervised = process_spec.spawn().await.map_err(|error| {
            tracing::warn!(?error, "opencode start: spawn failed");
            HarnessError::Process
        })?;
        let pid = supervised.pid();

        let harness_version = match self.last_probe.lock().await.clone() {
            Some((version, None)) if !version.is_empty() => version,
            _ => self.detect_version().await.0,
        };

        let handle_id = encode_handle(pid, self.next_handle.fetch_add(1, Ordering::SeqCst));
        let running = RunningOpenCodeProcess {
            process: supervised,
            secrets,
            limits,
            started_at: DateTime::<Utc>::from(self.clock.now()),
            workspace_path: spec.workspace.path.clone(),
            workspace_id: spec.workspace.id.as_str().to_owned(),
            base_revision: spec.workspace.base_revision.clone(),
            attempt_id: spec.work.lease.attempt_id.as_str().to_owned(),
            harness_version,
            model_provider,
            model_id,
        };
        self.running.lock().await.insert(handle_id.clone(), running);

        Ok(LocalRunHandle {
            process_id: handle_id,
        })
    }

    async fn cancel(&self, handle: &LocalRunHandle) -> Result<CancellationEvidence, HarnessError> {
        let running = self.take_running(&handle.process_id).await?;
        let outcome = running
            .process
            .cancel(running.limits.termination_grace)
            .await
            .map_err(|error| {
                tracing::warn!(?error, "opencode cancel: signal delivery failed");
                HarnessError::Process
            })?;

        let mut details = serde_json::Map::new();
        details.insert(
            "outcome".to_owned(),
            serde_json::json!(match outcome {
                CancelOutcome::Stopped => "stopped_after_sigterm",
                CancelOutcome::Killed => "killed_after_sigkill",
            }),
        );

        Ok(CancellationEvidence {
            observation: CancelObservation::ProcessStopped,
            observed_at: Timestamp::new(rfc3339(self.clock.now())),
            details,
        })
    }

    async fn wait(&self, handle: &LocalRunHandle) -> Result<HarnessOutcome, HarnessError> {
        let running = self.take_running(&handle.process_id).await?;
        let limits = running.limits.clone();
        let secrets_for_capture = running.secrets.clone();
        let result = running
            .process
            .wait_with_capture(&limits, &secrets_for_capture)
            .await
            .map_err(|error| {
                tracing::warn!(?error, "opencode wait: capture failed");
                HarnessError::Process
            })?;

        let ended_at = DateTime::<Utc>::from(self.clock.now());
        let elapsed_ms = ended_at
            .signed_duration_since(running.started_at)
            .num_milliseconds()
            .max(0) as u64;

        let (terminal_state, code, message) = classify_exit(&result.exit);
        let parsed_events = parse_jsonl_events(&result.stdout.text);

        let mut terminal_reason = serde_json::json!({
            "code": code,
            "message": message,
            "stdout": describe_capture(&result.stdout),
            "stderr": describe_capture(&result.stderr),
        });

        let (tokens_in, tokens_out, cost_usd) = match &parsed_events {
            Some(events) => {
                let summary = summarize_events(events);
                terminal_reason["event_count"] = serde_json::json!(summary.event_count);
                if let Some(error_event) = &summary.error_event {
                    terminal_reason["harness_error_event"] = error_event.clone();
                }
                (summary.tokens_in, summary.tokens_out, summary.cost_usd)
            }
            None => {
                terminal_reason["event_stream_note"] = serde_json::json!(
                    "stdout did not parse as the expected newline-delimited JSON event stream; \
                     usage figures are not_measured and terminal_state was classified by \
                     process exit status alone"
                );
                (None, None, None)
            }
        };

        if let Some(artifact) = self.stage_run_log(
            &running.workspace_path,
            &running.attempt_id,
            &result.stdout,
            &result.stderr,
            parsed_events.is_some(),
        ) {
            terminal_reason["artifact"] = artifact;
        }

        let usage = Usage {
            tokens_in: tokens_in.map(measured).unwrap_or_else(not_measured),
            tokens_out: tokens_out.map(measured).unwrap_or_else(not_measured),
            duration_ms: measured(elapsed_ms),
            cost_usd: cost_usd.map(measured).unwrap_or_else(not_measured),
            additional: BTreeMap::new(),
        };

        let actual_execution = ActualExecution {
            harness_kind: HarnessKind::new(OPENCODE_HARNESS_KIND),
            harness_version: running.harness_version.clone(),
            model_provider: ActualModelProvider::new(running.model_provider.clone()),
            model_id: ActualModelId::new(running.model_id.clone()),
            model_observation_source: MODEL_OBSERVATION_SOURCE.to_owned(),
            capability_snapshot: self.feature_capabilities(),
            workspace_id: DomainWorkspaceId::new(running.workspace_id.clone()),
            base_revision: running.base_revision.clone(),
            started_at: running.started_at,
            ended_at,
            additional: BTreeMap::new(),
        };

        Ok(HarnessOutcome {
            terminal_state,
            terminal_reason,
            final_checkpoint: None,
            actual_execution,
            usage,
        })
    }

    async fn reconcile(
        &self,
        journal: &AttemptJournal,
    ) -> Result<RecoveryObservation, HarnessError> {
        let Some(process_id) = journal.process_id.as_deref() else {
            // Nothing was ever confirmed running for this attempt; there is
            // no process-liveness question left to answer.
            return Ok(RecoveryObservation::ProcessStopped);
        };
        let Some(pid) = parse_handle_pid(process_id) else {
            tracing::warn!(
                process_id,
                "opencode reconcile: unrecognized handle encoding"
            );
            return Err(HarnessError::RecoveryUnavailable);
        };

        #[cfg(unix)]
        {
            // "reconcile the journal only when reconciliation is genuinely
            // supported": `kill(pid, 0)` is a real liveness probe, not a
            // guess. Known limitation, documented rather than silently
            // assumed away: pid reuse means a long-dead attempt's pid could
            // in principle be reassigned to an unrelated process, producing
            // a false `ProcessRunning`. This is the *safe* direction of
            // error — `RecoveryDisposition::SafePreSpawnRequeue` only ever
            // unlocks on a confident `ProcessStopped`
            // (`tack_orch::execution::RecoveryDisposition::is_compatible_with`),
            // so a false `ProcessRunning` can only ever force a conservative
            // `needs_operator`, never a dangerous double-launch.
            if crate::harness::process::process_alive(pid) {
                Ok(RecoveryObservation::ProcessRunning)
            } else {
                Ok(RecoveryObservation::ProcessStopped)
            }
        }
        #[cfg(not(unix))]
        {
            // Non-Unix has no portable liveness primitive here, matching
            // `harness/process.rs`'s own documented non-Unix cancellation
            // fallback and `codex.rs`'s identical choice.
            let _ = pid;
            Err(HarnessError::RecoveryUnavailable)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::journal::{JournalState, WorkspaceJournal};
    use crate::client::{
        AttemptId, AttemptLease, ClaimRequestId, ClaimedWork, FencingToken, RunnerId,
        Workspace as ClientWorkspace, WorkspaceId,
    };
    use crate::harness::fixtures::fake_harness_command;
    use std::time::SystemTime;
    use tack_orch::execution::{
        AttemptSnapshot, ExecutionRequestSnapshot, HarnessKind as DomainHarnessKind,
        RequestedModelId, RequestedModelProvider,
    };

    #[derive(Clone, Copy)]
    struct FixedClock(SystemTime);

    impl crate::Clock for FixedClock {
        fn now(&self) -> SystemTime {
            self.0
        }
    }

    fn clock_at(rfc3339_at: &str) -> FixedClock {
        FixedClock(
            chrono::DateTime::parse_from_rfc3339(rfc3339_at)
                .expect("fixture timestamp")
                .into(),
        )
    }

    fn generous_limits() -> ProcessLimits {
        ProcessLimits::new(1_000_000, 1_000_000, Duration::from_secs(10))
    }

    static NEXT_DIR: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    fn temp_dir(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "tack-runner-opencode-{label}-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    /// A minimal, deterministic "fixture repo" workspace, generated fresh
    /// per test rather than checked into the tree — mirrors `codex.rs`'s
    /// identical choice, made for the identical reason: D1/D2 are
    /// concurrently adding their own sibling files to this same directory,
    /// and generating fixtures at test time sidesteps any possible
    /// path-ownership ambiguity between the three cards.
    fn deterministic_fixture_repo(label: &str) -> PathBuf {
        let root = temp_dir(label);
        std::fs::write(root.join("README.md"), b"# fixture repo\n").expect("write README");
        std::fs::write(root.join("main.rs"), b"fn main() {}\n").expect("write main.rs");
        root
    }

    fn fixed_command() -> OpenCodeLocator {
        let (program, prefix_args) = fake_harness_command();
        OpenCodeLocator::Fixed {
            program,
            prefix_args,
        }
    }

    /// A tiny inline `/bin/sh -c '...'` fixture (never a new checked-in
    /// file — see the module docs). Branches on its first *appended*
    /// argument (`$1`, since `-c script $0` consumes one slot for `$0`):
    /// `models` → three deterministic `provider/model` lines across two
    /// providers (proving grouped, non-flattened pairing); anything else
    /// (`--version`) → a clean `"1.18.0"`. This lets one adapter instance
    /// exercise `probe()`'s full version+enumeration wiring together, and
    /// specifically lets [`validate_rejects_a_valid_model_id_paired_with_the_wrong_provider`]
    /// prove the acceptance-critical case against *real* (if synthetic)
    /// multi-provider capability data rather than only a unit-tested parser.
    fn branching_fixture_command() -> OpenCodeLocator {
        let script = r#"if [ "$1" = "models" ]; then
  printf 'openai/gpt-4\nopenai/gpt-4o-mini\nanthropic/claude-3-opus\n'
else
  printf '1.18.0\n'
fi
"#;
        OpenCodeLocator::Fixed {
            program: PathBuf::from("/bin/sh"),
            prefix_args: vec![
                "-c".to_owned(),
                script.to_owned(),
                "opencode-fake".to_owned(),
            ],
        }
    }

    /// A second inline fixture whose `run`-suffixed invocation prints a
    /// real, observed-shaped JSONL event stream (adapted from this card's
    /// actual `opencode run --format json` capture — see the handoff),
    /// regardless of its own args. Only ever used to drive `start()`
    /// directly (never through the probe-based `validate()`), so it does
    /// not need to branch on its arguments the way
    /// [`branching_fixture_command`] does.
    fn jsonl_run_fixture_command(exit_code: i32) -> OpenCodeLocator {
        let script = format!(
            "printf '%s\\n' \
             '{{\"type\":\"step_start\",\"timestamp\":1000,\"sessionID\":\"ses_test\",\"part\":{{\"type\":\"step-start\"}}}}' \
             '{{\"type\":\"text\",\"timestamp\":1001,\"sessionID\":\"ses_test\",\"part\":{{\"type\":\"text\",\"text\":\"banana\"}}}}' \
             '{{\"type\":\"step_finish\",\"timestamp\":1002,\"sessionID\":\"ses_test\",\"part\":{{\"type\":\"step-finish\",\"tokens\":{{\"total\":42,\"input\":21,\"output\":3,\"reasoning\":0,\"cache\":{{\"write\":0,\"read\":18}}}},\"cost\":0.0021}}}}'; \
             exit {exit_code}"
        );
        OpenCodeLocator::Fixed {
            program: PathBuf::from("/bin/sh"),
            prefix_args: vec!["-c".to_owned(), script],
        }
    }

    /// A third inline fixture reproducing observed fact 4 exactly: a
    /// wrong-provider run that starts a session and only fails afterward
    /// with a `{"type":"error", ...}` event on stdout and a nonzero exit.
    fn jsonl_error_event_fixture_command() -> OpenCodeLocator {
        let script = r#"printf '%s\n' '{"type":"error","timestamp":2000,"sessionID":"ses_err","error":{"name":"UnknownError","data":{"message":"Unexpected server error. Check server logs for details.","ref":"err_test"}}}'; exit 1"#;
        OpenCodeLocator::Fixed {
            program: PathBuf::from("/bin/sh"),
            prefix_args: vec!["-c".to_owned(), script.to_owned()],
        }
    }

    fn adapter_with(
        command: OpenCodeLocator,
        probe_env: BTreeMap<String, String>,
    ) -> OpenCodeAdapter<FixedClock> {
        OpenCodeAdapter::with_clock(
            command,
            generous_limits(),
            Duration::from_secs(5),
            probe_env,
            BTreeMap::new(), // passthrough_env: fake tests never need a real PATH/HOME
            temp_dir("artifacts"),
            clock_at("2026-08-09T12:00:00Z"),
        )
    }

    fn adapter() -> OpenCodeAdapter<FixedClock> {
        adapter_with(fixed_command(), BTreeMap::new())
    }

    fn adapter_with_probe_env(probe_env: BTreeMap<String, String>) -> OpenCodeAdapter<FixedClock> {
        adapter_with(fixed_command(), probe_env)
    }

    fn env_map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn spec_with(
        workspace_path: PathBuf,
        model: Option<(&str, &str)>,
        extra_env: &[(&str, &str)],
    ) -> ExecutionSpec {
        let claim: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../docs/contracts/runner-v1/claim.response.json"
        ))
        .expect("claim fixture");
        let mut request: ExecutionRequestSnapshot =
            serde_json::from_value(claim["request"].clone()).expect("request fixture");
        request.requested_harness_kind = DomainHarnessKind::new(OPENCODE_HARNESS_KIND);
        request.requested_model_provider =
            model.map(|(provider, _)| RequestedModelProvider::new(provider));
        request.requested_model_id = model.map(|(_, id)| RequestedModelId::new(id));
        request.timeout_seconds = 3600;
        for (key, value) in extra_env {
            request.environment.insert(
                (*key).to_owned(),
                tack_orch::execution::EnvironmentValue {
                    value: Some((*value).to_owned()),
                    secret_reference: None,
                    additional: BTreeMap::new(),
                },
            );
        }
        let attempt: AttemptSnapshot =
            serde_json::from_value(claim["attempt"].clone()).expect("attempt fixture");

        ExecutionSpec {
            work: ClaimedWork {
                claim_request_id: ClaimRequestId::new("claim"),
                lease: AttemptLease {
                    attempt_id: AttemptId::new("attempt"),
                    runner_id: RunnerId::new("runner"),
                    fencing_token: FencingToken(1),
                    attempt_number: 1,
                    state: crate::client::AttemptState::Leased,
                    issued_at: Timestamp::new("2026-08-09T11:59:00Z"),
                    expires_at: Timestamp::new("2026-08-09T12:59:00Z"),
                },
                request,
                attempt,
            },
            workspace: ClientWorkspace {
                attempt_id: AttemptId::new("attempt"),
                id: WorkspaceId::new("ws_opencode_test"),
                path: workspace_path,
                base_revision: "revision".into(),
            },
        }
    }

    // ---- validate() / start() pre-spawn rejection --------------------

    #[tokio::test]
    async fn validate_rejects_a_mismatched_harness_kind() {
        let adapter = adapter();
        let workspace = deterministic_fixture_repo("kind-mismatch");
        let mut spec = spec_with(workspace.clone(), Some(("opencode", "big-pickle")), &[]);
        spec.work.request.requested_harness_kind = DomainHarnessKind::new("codex");

        assert!(matches!(
            adapter.validate(&spec).await,
            Err(HarnessError::Rejected { .. })
        ));
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    #[tokio::test]
    async fn validate_rejects_an_auto_selected_model_pre_spawn() {
        let adapter = adapter();
        let workspace = deterministic_fixture_repo("auto-select");
        let spec = spec_with(workspace.clone(), None, &[]);

        assert!(matches!(
            adapter.validate(&spec).await,
            Err(HarnessError::Rejected { .. })
        ));
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    #[tokio::test]
    async fn validate_rejects_a_partial_provider_only_selection() {
        let adapter = adapter();
        let workspace = deterministic_fixture_repo("partial-select");
        let mut spec = spec_with(workspace.clone(), None, &[]);
        spec.work.request.requested_model_provider = Some(RequestedModelProvider::new("openai"));
        spec.work.request.requested_model_id = None;

        assert!(matches!(
            adapter.validate(&spec).await,
            Err(HarnessError::Rejected { .. })
        ));
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    #[tokio::test]
    async fn validate_rejects_an_unresolvable_binary() {
        let empty_dir = temp_dir("empty-path");
        let adapter = OpenCodeAdapter::with_clock(
            OpenCodeLocator::Search {
                program_name: "opencode".to_owned(),
                search_dirs: vec![empty_dir.clone()],
            },
            generous_limits(),
            Duration::from_secs(1),
            BTreeMap::new(),
            BTreeMap::new(),
            temp_dir("artifacts-unresolvable"),
            clock_at("2026-08-09T12:00:00Z"),
        );
        let workspace = deterministic_fixture_repo("unresolvable");
        let spec = spec_with(workspace.clone(), Some(("opencode", "big-pickle")), &[]);

        assert!(matches!(
            adapter.validate(&spec).await,
            Err(HarnessError::Rejected { .. })
        ));
        std::fs::remove_dir_all(workspace).expect("cleanup");
        std::fs::remove_dir_all(empty_dir).expect("cleanup");
    }

    /// Acceptance (this card's distinguishing requirement): a **valid**
    /// model id paired with the **wrong** provider is rejected pre-spawn,
    /// even though `gpt-4` is genuinely a real, discoverable model — just
    /// under `openai`, never `anthropic`. Proves the check is a real pairing
    /// check against opencode's own discovered combinations, not merely "is
    /// some provider and some model present" (which is all `codex.rs` can
    /// check, since Codex's own model discovery is unverified).
    #[tokio::test]
    async fn validate_rejects_a_valid_model_id_paired_with_the_wrong_provider() {
        let adapter = adapter_with(branching_fixture_command(), BTreeMap::new());
        let workspace = deterministic_fixture_repo("wrong-provider");
        let spec = spec_with(workspace.clone(), Some(("anthropic", "gpt-4")), &[]);

        assert!(matches!(
            adapter.validate(&spec).await,
            Err(HarnessError::Rejected { .. })
        ));

        // The same model id, correctly paired, is accepted — proving the
        // rejection above was about the *pairing*, not the model id itself.
        let correctly_paired = spec_with(workspace.clone(), Some(("openai", "gpt-4")), &[]);
        adapter
            .validate(&correctly_paired)
            .await
            .expect("the same model id under its real provider must validate");

        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    #[tokio::test]
    async fn validate_rejects_a_provider_with_no_discovered_models_at_all() {
        let adapter = adapter_with(branching_fixture_command(), BTreeMap::new());
        let workspace = deterministic_fixture_repo("unknown-provider");
        let spec = spec_with(workspace.clone(), Some(("does-not-exist", "gpt-4")), &[]);

        assert!(matches!(
            adapter.validate(&spec).await,
            Err(HarnessError::Rejected { .. })
        ));
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    #[tokio::test]
    async fn validate_rejects_when_the_capability_probe_itself_fails() {
        let adapter = adapter_with_probe_env(env_map(&[("TACK_FAKE_HARNESS_MODE", "failure")]));
        let workspace = deterministic_fixture_repo("probe-fails");
        let spec = spec_with(workspace.clone(), Some(("opencode", "big-pickle")), &[]);

        assert!(matches!(
            adapter.validate(&spec).await,
            Err(HarnessError::Rejected { .. })
        ));
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    /// Acceptance: "an unsupported selection fails pre-spawn — before any
    /// process launches." Proved empirically: the spec is configured so the
    /// underlying fake process would `hang` for an hour if it were ever
    /// actually spawned. If `start()`'s pre-spawn guard were broken, this
    /// test would hang (bounded here by an explicit timeout that turns that
    /// hang into a fast, loud failure rather than a stuck CI job). Mirrors
    /// `codex.rs`'s identical proof technique.
    #[tokio::test]
    async fn unsupported_selection_fails_pre_spawn_even_when_the_process_would_otherwise_hang_forever()
     {
        let adapter = adapter();
        let workspace = deterministic_fixture_repo("pre-spawn-hang-guard");
        let spec = spec_with(
            workspace.clone(),
            None, // auto-select: rejected by check_selection before spawn
            &[
                ("TACK_FAKE_HARNESS_MODE", "hang"),
                ("TACK_FAKE_HARNESS_SLEEP_SECONDS", "3600"),
            ],
        );

        let result = tokio::time::timeout(Duration::from_secs(5), adapter.start(&spec)).await;
        assert!(
            result.is_ok(),
            "start() must reject pre-spawn, not hang waiting on a process it never launched"
        );
        assert!(matches!(
            result.unwrap(),
            Err(HarnessError::Rejected { .. })
        ));
        assert!(
            adapter.running.lock().await.is_empty(),
            "a pre-spawn rejection must never create process bookkeeping (verifier nit 4)"
        );
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    // ---- fake-binary exec-path tests ----------------------------------
    //
    // These call `start()` directly, skipping `validate()`: this adapter's
    // `validate()` performs a real capability probe (version + model
    // enumeration), which the shared fake binary's single-mode env
    // (`success`/`failure`/`malformed`) cannot shape correctly for both the
    // probe *and* the run in the same test. `start()` itself only re-checks
    // the cheap structural selection (see `check_selection`'s doc comment),
    // so this is honest, not a shortcut around anything `start()` actually
    // enforces.

    #[tokio::test]
    async fn fake_binary_success_completes_succeeded_with_normalized_output_and_a_staged_artifact()
    {
        let adapter = adapter();
        let workspace = deterministic_fixture_repo("exec-success");
        let spec = spec_with(
            workspace.clone(),
            Some(("opencode", "big-pickle")),
            &[("TACK_FAKE_HARNESS_MODE", "success")],
        );

        let handle = adapter.start(&spec).await.expect("start");
        let outcome = adapter.wait(&handle).await.expect("wait");

        assert_eq!(outcome.terminal_state, AttemptState::Succeeded);
        assert_eq!(outcome.terminal_reason["code"], "completed");
        assert!(
            outcome.terminal_reason["stdout"]["text_preview"]
                .as_str()
                .unwrap()
                .contains("fake-harness-ok")
        );
        // `"fake-harness-ok"` is not JSON, so this run's usage cannot be
        // honestly claimed as measured (rule 7: unmeasured is nullable).
        assert_eq!(
            outcome.usage.tokens_in.source,
            MeasurementSource::NotMeasured
        );
        assert!(outcome.usage.tokens_in.value.is_none());
        assert_eq!(
            outcome.usage.duration_ms.source,
            MeasurementSource::Measured
        );
        assert!(outcome.usage.duration_ms.value.is_some());
        assert_eq!(outcome.actual_execution.model_provider.as_str(), "opencode");
        assert_eq!(outcome.actual_execution.model_id.as_str(), "big-pickle");
        assert_eq!(
            outcome.actual_execution.model_observation_source,
            MODEL_OBSERVATION_SOURCE
        );

        let artifact = &outcome.terminal_reason["artifact"];
        assert_eq!(artifact["kind"], "log");
        assert_eq!(artifact["media_type"], "text/plain");
        let staged_path = artifact["staged_path"].as_str().expect("staged_path");
        let staged_bytes = std::fs::read(staged_path).expect("read staged artifact");
        assert!(String::from_utf8_lossy(&staged_bytes).contains("fake-harness-ok"));
        assert_eq!(
            artifact["sha256"].as_str().unwrap(),
            crate::harness::sha256::sha256_hex(&staged_bytes)
        );

        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    #[tokio::test]
    async fn fake_binary_failure_completes_failed_with_the_exit_code_in_terminal_reason() {
        let adapter = adapter();
        let workspace = deterministic_fixture_repo("exec-failure");
        let spec = spec_with(
            workspace.clone(),
            Some(("opencode", "big-pickle")),
            &[
                ("TACK_FAKE_HARNESS_MODE", "failure"),
                ("TACK_FAKE_HARNESS_EXIT_CODE", "17"),
            ],
        );

        let handle = adapter.start(&spec).await.expect("start");
        let outcome = adapter.wait(&handle).await.expect("wait");

        assert_eq!(outcome.terminal_state, AttemptState::Failed);
        assert_eq!(outcome.terminal_reason["code"], "exit_code");
        assert!(
            outcome.terminal_reason["message"]
                .as_str()
                .unwrap()
                .contains("17")
        );
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    /// Acceptance: malformed output. The fixture's `malformed` mode still
    /// exits 0 (this adapter trusts exit code for `terminal_state` — see
    /// `classify_exit`'s doc comment), but its stdout does not parse as the
    /// expected JSON event stream, so usage stays honestly `not_measured`
    /// and `terminal_reason` carries an explicit note rather than silently
    /// claiming measured figures it does not have. No panic either way.
    #[tokio::test]
    async fn fake_binary_malformed_output_does_not_panic_and_falls_back_to_not_measured_usage() {
        let adapter = adapter();
        let workspace = deterministic_fixture_repo("exec-malformed");
        let spec = spec_with(
            workspace.clone(),
            Some(("opencode", "big-pickle")),
            &[("TACK_FAKE_HARNESS_MODE", "malformed")],
        );

        let handle = adapter.start(&spec).await.expect("start");
        let outcome = adapter.wait(&handle).await.expect("wait");

        assert_eq!(outcome.terminal_state, AttemptState::Succeeded);
        assert!(
            outcome.terminal_reason["event_stream_note"]
                .as_str()
                .unwrap()
                .contains("did not parse")
        );
        assert_eq!(
            outcome.usage.tokens_in.source,
            MeasurementSource::NotMeasured
        );
        assert_eq!(
            outcome.usage.cost_usd.source,
            MeasurementSource::NotMeasured
        );
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    /// Acceptance: cancel kills descendants, proved through the adapter's
    /// own `start`/`cancel`, not raw `ProcessSpec` (that is D4's own test).
    #[tokio::test]
    async fn cancel_kills_the_whole_descendant_tree_via_the_adapter() {
        let adapter = adapter();
        let workspace = deterministic_fixture_repo("exec-cancel");
        let pidfile = workspace.join("grandchild.pid");
        let spec = spec_with(
            workspace.clone(),
            Some(("opencode", "big-pickle")),
            &[
                ("TACK_FAKE_HARNESS_MODE", "spawn_child"),
                (
                    "TACK_FAKE_HARNESS_PIDFILE",
                    pidfile.to_str().expect("utf8 pidfile path"),
                ),
                ("TACK_FAKE_HARNESS_SLEEP_SECONDS", "3600"),
            ],
        );

        let handle = adapter.start(&spec).await.expect("start");
        let grandchild_pid = wait_for_pidfile(&pidfile).await;
        assert!(
            crate::harness::process::process_alive(grandchild_pid),
            "grandchild must be observed running before cancellation"
        );

        let evidence = adapter.cancel(&handle).await.expect("cancel");
        assert_eq!(evidence.observation, CancelObservation::ProcessStopped);

        assert!(
            wait_until_dead(grandchild_pid, Duration::from_secs(5)).await,
            "grandchild must be gone after the adapter cancels its parent"
        );
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    /// A cancel/wait on a handle this adapter instance never produced (e.g.
    /// stale after a restart) is a typed rejection, never a silent success.
    #[tokio::test]
    async fn cancel_and_wait_on_an_untracked_handle_are_typed_rejections() {
        let adapter = adapter();
        let handle = LocalRunHandle {
            process_id: "opencode:999999:0".to_owned(),
        };
        assert!(matches!(
            adapter.cancel(&handle).await,
            Err(HarnessError::Process)
        ));
        assert!(matches!(
            adapter.wait(&handle).await,
            Err(HarnessError::Process)
        ));
    }

    // ---- real-shaped event stream (this card's usage-extraction advantage) --

    /// Drives a real, observed-shaped JSONL event stream (see
    /// [`jsonl_run_fixture_command`]) and proves usage is extracted as
    /// genuinely `Measured`, not `not_measured` — the concrete difference
    /// from `codex.rs`, which never observed a parseable usage shape.
    #[tokio::test]
    async fn wait_extracts_real_token_and_cost_usage_from_an_observed_shaped_event_stream() {
        let adapter = adapter_with(jsonl_run_fixture_command(0), BTreeMap::new());
        let workspace = deterministic_fixture_repo("real-shaped-usage");
        let spec = spec_with(workspace.clone(), Some(("opencode", "big-pickle")), &[]);

        let handle = adapter.start(&spec).await.expect("start");
        let outcome = adapter.wait(&handle).await.expect("wait");

        assert_eq!(outcome.terminal_state, AttemptState::Succeeded);
        assert_eq!(outcome.usage.tokens_in.source, MeasurementSource::Measured);
        assert_eq!(outcome.usage.tokens_in.value, Some(21));
        assert_eq!(outcome.usage.tokens_out.source, MeasurementSource::Measured);
        assert_eq!(outcome.usage.tokens_out.value, Some(3));
        assert_eq!(outcome.usage.cost_usd.source, MeasurementSource::Measured);
        assert_eq!(outcome.usage.cost_usd.value, Some(0.0021));
        assert_eq!(outcome.terminal_reason["event_count"], 3);
        assert_eq!(
            outcome.terminal_reason["artifact"]["media_type"],
            "application/x-ndjson"
        );
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    /// Reproduces observed fact 4 (the real wrong-provider error shape) and
    /// proves it enriches `terminal_reason` for diagnosis without ever
    /// flipping `terminal_state` away from what the exit code already says
    /// (`classify_exit`'s doc comment explains why exit code stays
    /// authoritative).
    #[tokio::test]
    async fn wait_embeds_a_parsed_error_event_into_terminal_reason_without_changing_exit_code_semantics()
     {
        let adapter = adapter_with(jsonl_error_event_fixture_command(), BTreeMap::new());
        let workspace = deterministic_fixture_repo("real-shaped-error");
        let spec = spec_with(workspace.clone(), Some(("anthropic", "big-pickle")), &[]);

        let handle = adapter.start(&spec).await.expect("start");
        let outcome = adapter.wait(&handle).await.expect("wait");

        assert_eq!(outcome.terminal_state, AttemptState::Failed);
        assert_eq!(outcome.terminal_reason["code"], "exit_code");
        assert_eq!(
            outcome.terminal_reason["harness_error_event"]["error"]["name"],
            "UnknownError"
        );
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    // ---- redaction (rule 12) -------------------------------------------

    /// Acceptance: arguments/environment are redacted in logs and events.
    /// Plants a canary in both the requested environment and (indirectly,
    /// since the agent profile instructions become the prompt) stdin, drives
    /// the fake harness's `echo_canary` mode so it actively echoes the
    /// canary back on stdout *and* stderr, and asserts it appears nowhere in
    /// the adapter's own output surface (`HarnessOutcome.terminal_reason`)
    /// nor in the staged log artifact.
    #[tokio::test]
    async fn secret_canaries_never_survive_into_terminal_reason_or_the_staged_artifact() {
        const CANARY_ENV: &str = "tack-test-opencode-canary-env-7d2f";
        const CANARY_STDIN: &str = "tack-test-opencode-canary-stdin-c910";
        let adapter = adapter();
        let workspace = deterministic_fixture_repo("redaction");
        let mut spec = spec_with(
            workspace.clone(),
            Some(("opencode", "big-pickle")),
            &[
                ("TACK_FAKE_HARNESS_MODE", "echo_canary"),
                ("TACK_TEST_SECRET", CANARY_ENV),
                ("TACK_FAKE_HARNESS_ECHO_ENV_KEYS", "TACK_TEST_SECRET"),
            ],
        );
        // The agent profile's instructions become the prompt piped over
        // stdin; the fake harness's `echo_canary` mode also echoes stdin
        // back, so folding a second canary into the prompt exercises that
        // path too.
        spec.work.request.resolved_agent_profile.instructions =
            format!("do the {CANARY_STDIN} thing");

        let handle = adapter.start(&spec).await.expect("start");
        let outcome = adapter.wait(&handle).await.expect("wait");

        let serialized = outcome.terminal_reason.to_string();
        assert!(
            serialized.contains("[REDACTED]"),
            "the fake harness must actually have echoed something for this test to be meaningful"
        );
        assert!(!serialized.contains(CANARY_ENV));
        assert!(!serialized.contains(CANARY_STDIN));

        let artifact_path = outcome.terminal_reason["artifact"]["staged_path"]
            .as_str()
            .expect("artifact staged");
        let staged_text = std::fs::read_to_string(artifact_path).expect("read staged artifact");
        assert!(!staged_text.contains(CANARY_ENV));
        assert!(!staged_text.contains(CANARY_STDIN));

        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    // ---- provider/model parsing (pure, no subprocess) ---------------------

    #[test]
    fn parse_model_combinations_groups_by_provider_and_preserves_pairing() {
        // The exact six lines this card observed from a real, freshly
        // installed opencode 1.18.0 with zero configured credentials (see
        // the handoff), plus two synthetic providers to prove grouping
        // across more than one provider in the same batch.
        let stdout = "opencode/big-pickle\n\
                       opencode/deepseek-v4-flash-free\n\
                       opencode/hy3-free\n\
                       opencode/mimo-v2.5-free\n\
                       opencode/nemotron-3-ultra-free\n\
                       opencode/north-mini-code-free\n\
                       openai/gpt-4\n\
                       anthropic/claude-3-opus\n";

        let combinations = parse_model_combinations(stdout).expect("parse");
        assert_eq!(
            combinations.len(),
            3,
            "grouped into exactly three providers"
        );

        let opencode = combinations
            .iter()
            .find(|combo| combo.model_provider.as_str() == "opencode")
            .expect("opencode provider present");
        assert_eq!(opencode.model_ids.len(), 6);
        assert!(
            opencode
                .model_ids
                .iter()
                .any(|id| id.as_str() == "big-pickle")
        );

        let openai = combinations
            .iter()
            .find(|combo| combo.model_provider.as_str() == "openai")
            .expect("openai provider present");
        assert_eq!(openai.model_ids.len(), 1);
        assert_eq!(openai.model_ids[0].as_str(), "gpt-4");

        // `gpt-4` never appears under `anthropic`'s list — this is exactly
        // the structural fact `check_pairing_supported` relies on.
        let anthropic = combinations
            .iter()
            .find(|combo| combo.model_provider.as_str() == "anthropic")
            .expect("anthropic provider present");
        assert!(!anthropic.model_ids.iter().any(|id| id.as_str() == "gpt-4"));
    }

    #[test]
    fn parse_model_combinations_treats_empty_output_as_zero_combinations_not_an_error() {
        assert_eq!(parse_model_combinations("").expect("parse"), Vec::new());
        assert_eq!(parse_model_combinations("\n\n").expect("parse"), Vec::new());
    }

    #[test]
    fn parse_model_combinations_rejects_a_line_not_shaped_like_provider_slash_model() {
        assert!(parse_model_combinations("opencode/big-pickle\nnot-a-valid-line\n").is_err());
        assert!(parse_model_combinations("/missing-provider").is_err());
        assert!(parse_model_combinations("missing-model/").is_err());
    }

    #[test]
    fn parse_model_combinations_keeps_a_model_id_with_extra_slashes_fully_opaque() {
        // A plausible shape for a proxying provider (e.g. an OpenRouter-style
        // `provider/vendor/model`). Only the *first* `/` is a delimiter; the
        // rest of the id is never interpreted further.
        let combinations =
            parse_model_combinations("openrouter/anthropic/claude-3-opus\n").expect("parse");
        assert_eq!(combinations.len(), 1);
        assert_eq!(combinations[0].model_provider.as_str(), "openrouter");
        assert_eq!(
            combinations[0].model_ids[0].as_str(),
            "anthropic/claude-3-opus"
        );
    }

    // ---- probe() / HarnessProbe ----------------------------------------

    #[tokio::test]
    async fn probe_reports_a_recognized_version_with_no_error() {
        let adapter = adapter_with_probe_env(env_map(&[
            ("TACK_FAKE_HARNESS_MODE", "version"),
            ("TACK_FAKE_HARNESS_VERSION", "9.9.9"),
        ]));
        let capability = adapter.probe().await;

        assert_eq!(capability.harness_kind.as_str(), OPENCODE_HARNESS_KIND);
        assert_eq!(capability.installed_version, "9.9.9");
        // The shared fixture's `version` mode's stdout is not
        // `provider/model`-shaped, so model listing fails; that is reported
        // as an explicit probe_error, never a fabricated empty-but-clean
        // capability report.
        assert!(capability.probe_error.is_some());
        assert!(capability.model_combinations.is_empty());
    }

    /// Acceptance: end-to-end wiring — version *and* model enumeration both
    /// succeed together — using [`branching_fixture_command`].
    #[tokio::test]
    async fn probe_end_to_end_reports_installed_version_and_grouped_model_combinations() {
        let adapter = adapter_with(branching_fixture_command(), BTreeMap::new());
        let capability = adapter.probe().await;

        assert_eq!(capability.installed_version, "1.18.0");
        assert_eq!(capability.probe_error, None);
        assert_eq!(capability.model_combinations.len(), 2);
        assert!(
            capability
                .model_combinations
                .iter()
                .any(
                    |combo| combo.model_provider.as_str() == "openai" && combo.model_ids.len() == 2
                )
        );
    }

    /// Acceptance: unknown version. The fixture's `unknown_version` mode
    /// exits 0 with a string that is not a clean version line; this is an
    /// explicit `probe_error`, never a fabricated clean version (rule 7).
    #[tokio::test]
    async fn probe_reports_an_unrecognized_version_string_as_an_explicit_probe_error() {
        let adapter =
            adapter_with_probe_env(env_map(&[("TACK_FAKE_HARNESS_MODE", "unknown_version")]));
        let capability = adapter.probe().await;

        assert_eq!(capability.installed_version, "");
        assert!(capability.probe_error.is_some());
        let raw = capability
            .additional
            .get("raw_version_output")
            .and_then(|value| value.as_str())
            .expect("raw output preserved for diagnosis");
        assert!(raw.contains("999.999.999"));
    }

    /// Acceptance: malformed (probe-level companion to the exec-level
    /// malformed test above).
    #[tokio::test]
    async fn probe_reports_malformed_version_output_as_an_explicit_probe_error() {
        let adapter = adapter_with_probe_env(env_map(&[("TACK_FAKE_HARNESS_MODE", "malformed")]));
        let capability = adapter.probe().await;

        assert_eq!(capability.installed_version, "");
        assert!(capability.probe_error.is_some());
    }

    #[tokio::test]
    async fn probe_reports_a_nonzero_exit_as_an_explicit_probe_error() {
        let adapter = adapter_with_probe_env(env_map(&[
            ("TACK_FAKE_HARNESS_MODE", "failure"),
            ("TACK_FAKE_HARNESS_EXIT_CODE", "3"),
        ]));
        let capability = adapter.probe().await;

        assert_eq!(capability.installed_version, "");
        assert!(capability.probe_error.unwrap().contains('3'));
    }

    #[tokio::test]
    async fn probe_reports_an_absent_binary_as_an_explicit_probe_error_never_a_fake_success() {
        let empty_dir = temp_dir("probe-empty-path");
        let adapter = OpenCodeAdapter::with_clock(
            OpenCodeLocator::Search {
                program_name: "opencode".to_owned(),
                search_dirs: vec![empty_dir.clone()],
            },
            generous_limits(),
            Duration::from_secs(1),
            BTreeMap::new(),
            BTreeMap::new(),
            temp_dir("artifacts-absent"),
            clock_at("2026-08-09T12:00:00Z"),
        );

        let capability = adapter.probe().await;
        assert_eq!(capability.installed_version, "");
        assert!(capability.probe_error.unwrap().contains("not found"));
        std::fs::remove_dir_all(empty_dir).expect("cleanup");
    }

    #[tokio::test]
    async fn probe_never_hangs_past_its_own_timeout() {
        let adapter = OpenCodeAdapter::with_clock(
            fixed_command(),
            generous_limits(),
            Duration::from_millis(50),
            env_map(&[
                ("TACK_FAKE_HARNESS_MODE", "hang"),
                ("TACK_FAKE_HARNESS_SLEEP_SECONDS", "3600"),
            ]),
            BTreeMap::new(),
            temp_dir("artifacts-hang"),
            clock_at("2026-08-09T12:00:00Z"),
        );

        let capability = tokio::time::timeout(Duration::from_secs(5), adapter.probe())
            .await
            .expect("probe must respect its own timeout rather than hanging the caller");
        assert_eq!(capability.installed_version, "");
        assert!(capability.probe_error.unwrap().contains("timed out"));
    }

    #[tokio::test]
    async fn harness_kind_matches_what_probe_itself_reports() {
        let adapter = adapter();
        let capability = adapter.probe().await;
        assert_eq!(
            HarnessProbe::harness_kind(&adapter).as_str(),
            capability.harness_kind.as_str()
        );
    }

    /// Card III-D5 finding 1, direct regression guard — mirrors `codex.rs`'s
    /// identical test. An adversarial check against this card's own real
    /// `opencode` binary found the same disjoint-session pattern D2 found
    /// for Claude Code (a bash-tool subprocess in its own session/group,
    /// distinct from the top-level process), so `Supported` was never
    /// justified here either.
    #[test]
    fn declared_cancel_capability_is_advisory_not_supported() {
        let adapter = adapter();
        let declared = HarnessProbe::declared_capabilities(&adapter);
        assert_eq!(declared.cancel.support, CapabilitySupport::Advisory);
        assert!(declared.cancel.reason.is_some());
    }

    // ---- reconcile() -----------------------------------------------------

    fn journal_with_process(process_id: Option<&str>) -> AttemptJournal {
        AttemptJournal {
            attempt_id: AttemptId::new("attempt"),
            runner_id: RunnerId::new("runner"),
            fencing_token: FencingToken(1),
            workspace: WorkspaceJournal {
                workspace_id: WorkspaceId::new("ws_opencode_test"),
                path: PathBuf::from("/tmp/does-not-matter"),
                base_revision: "revision".into(),
            },
            state: JournalState::ProcessObservedRunning,
            process_id: process_id.map(str::to_owned),
            last_event_checkpoint: None,
            pending_terminal_report: None,
        }
    }

    #[tokio::test]
    async fn reconcile_with_no_recorded_process_id_reports_stopped_without_dispatch() {
        let adapter = adapter();
        let observation = adapter
            .reconcile(&journal_with_process(None))
            .await
            .expect("reconcile");
        assert_eq!(observation, RecoveryObservation::ProcessStopped);
    }

    #[tokio::test]
    async fn reconcile_rejects_an_unrecognized_handle_encoding_as_explicitly_unavailable() {
        let adapter = adapter();
        let journal = journal_with_process(Some("not-an-opencode-handle"));
        assert!(matches!(
            adapter.reconcile(&journal).await,
            Err(HarnessError::RecoveryUnavailable)
        ));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn reconcile_observes_a_real_alive_process_as_running() {
        let adapter = adapter();
        // A real, independently-alive process this test controls directly
        // (not spawned via the adapter, but a genuine live pid either way).
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn sleep");
        let journal = journal_with_process(Some(&encode_handle(child.id(), 0)));

        let observation = adapter.reconcile(&journal).await.expect("reconcile");
        assert_eq!(observation, RecoveryObservation::ProcessRunning);

        let _ = child.kill();
        let _ = child.wait();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn reconcile_observes_a_dead_pid_as_stopped() {
        let adapter = adapter();
        let mut child = std::process::Command::new("true")
            .spawn()
            .expect("spawn true");
        let pid = child.id();
        let _ = child.wait(); // reaped: pid is now dead (short-lived `true`)

        let observation = adapter
            .reconcile(&journal_with_process(Some(&encode_handle(pid, 0))))
            .await
            .expect("reconcile");
        assert_eq!(observation, RecoveryObservation::ProcessStopped);
    }

    // ---- helpers -----------------------------------------------------

    async fn wait_for_pidfile(path: &std::path::Path) -> u32 {
        for _ in 0..200 {
            if let Ok(contents) = std::fs::read_to_string(path) {
                if let Ok(pid) = contents.trim().parse::<u32>() {
                    return pid;
                }
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("grandchild pidfile was never written: {}", path.display());
    }

    async fn wait_until_dead(pid: u32, budget: Duration) -> bool {
        let deadline = tokio::time::Instant::now() + budget;
        while tokio::time::Instant::now() < deadline {
            if !crate::harness::process::process_alive(pid) {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        !crate::harness::process::process_alive(pid)
    }

    // ---- opt-in live test ------------------------------------------------

    /// Acceptance: "an opt-in live test records version and artifact" — and,
    /// because a real `opencode` binary is this card's specific advantage
    /// over D1, this test goes further: a real non-interactive run against
    /// a real (throwaway) fixture repo, using whichever zero-cost
    /// `opencode/*` model the live probe *actually* discovers (never
    /// hardcoded), recording real token usage. Sandboxes `HOME`/`XDG_*` to a
    /// fresh temp directory so it never touches this machine's real
    /// `~/.local/share/opencode/*` state (session history, credentials) —
    /// only `PATH` is inherited from the real environment, to actually find
    /// the binary.
    ///
    /// `#[ignore]`d (a plain `cargo test` never runs this) and cleanly
    /// self-skipping at every stage if `opencode`, or a zero-credential
    /// model, is not available — never the only proof of anything the
    /// fake-binary tests above already cover independently, and never
    /// dependent on a secret (rule 8).
    #[tokio::test]
    #[ignore = "opt-in: requires a real `opencode` binary on PATH (and outbound network access \
                to opencode's own free zen models); run with `cargo test -p tack-runner --lib \
                -- --ignored opencode::tests::live_`"]
    async fn live_probe_and_a_real_free_run_record_version_and_artifact() {
        if locate_in_dirs(OPENCODE_PROGRAM_NAME, &system_path_dirs()).is_none() {
            eprintln!("skipping live opencode test: `opencode` not found on PATH");
            return;
        }

        let sandbox_home = temp_dir("live-home");
        let mut passthrough = BTreeMap::new();
        if let Ok(path) = std::env::var("PATH") {
            passthrough.insert("PATH".to_owned(), path);
        }
        passthrough.insert("HOME".to_owned(), sandbox_home.display().to_string());
        passthrough.insert(
            "XDG_CONFIG_HOME".to_owned(),
            sandbox_home.join("config").display().to_string(),
        );
        passthrough.insert(
            "XDG_DATA_HOME".to_owned(),
            sandbox_home.join("data").display().to_string(),
        );
        passthrough.insert(
            "XDG_CACHE_HOME".to_owned(),
            sandbox_home.join("cache").display().to_string(),
        );

        let adapter = OpenCodeAdapter::with_clock(
            OpenCodeLocator::Search {
                program_name: OPENCODE_PROGRAM_NAME.to_owned(),
                search_dirs: system_path_dirs(),
            },
            ProcessLimits::new(4_194_304, 1_048_576, Duration::from_secs(60)),
            Duration::from_secs(30),
            BTreeMap::new(),
            passthrough,
            temp_dir("live-artifacts"),
            crate::SystemClock,
        );

        let capability = adapter.probe().await;
        eprintln!(
            "live opencode probe: installed_version={:?} probe_error={:?} \
             model_combinations={}",
            capability.installed_version,
            capability.probe_error,
            capability.model_combinations.len()
        );
        if capability.probe_error.is_some() {
            eprintln!("skipping live run: capability probe did not succeed in this environment");
            return;
        }

        let Some(free_combo) = capability
            .model_combinations
            .iter()
            .find(|combo| combo.model_provider.as_str() == "opencode")
        else {
            eprintln!(
                "skipping live run: no zero-credential `opencode/*` model discovered (the \
                 catalog may differ in this environment)"
            );
            return;
        };
        let Some(model_id) = free_combo.model_ids.first() else {
            eprintln!("skipping live run: opencode provider reported zero models");
            return;
        };

        let workspace = deterministic_fixture_repo("live-run");
        let _ = std::process::Command::new("git")
            .arg("init")
            .arg("-q")
            .current_dir(&workspace)
            .status();

        let mut spec = spec_with(
            workspace.clone(),
            Some(("opencode", model_id.as_str())),
            &[],
        );
        spec.work.request.resolved_agent_profile.instructions =
            "Reply with exactly the single word: pineapple".to_owned();
        spec.work.request.timeout_seconds = 60;

        adapter
            .validate(&spec)
            .await
            .expect("validate a real, just-discovered zero-credential combination");
        let handle = adapter
            .start(&spec)
            .await
            .expect("start a real opencode run");
        let outcome = adapter
            .wait(&handle)
            .await
            .expect("wait for the real opencode run");

        eprintln!(
            "live opencode run: terminal_state={:?} tokens_in={:?} tokens_out={:?} \
             cost_usd={:?} harness_version={}",
            outcome.terminal_state,
            outcome.usage.tokens_in.value,
            outcome.usage.tokens_out.value,
            outcome.usage.cost_usd.value,
            outcome.actual_execution.harness_version
        );
        assert_eq!(outcome.terminal_state, AttemptState::Succeeded);
        assert_eq!(
            outcome.actual_execution.harness_version,
            capability.installed_version
        );
        let staged_path = outcome.terminal_reason["artifact"]["staged_path"]
            .as_str()
            .expect("a real run must stage a real artifact");
        assert!(
            std::fs::metadata(staged_path)
                .map(|metadata| metadata.len() > 0)
                .unwrap_or(false)
        );

        std::fs::remove_dir_all(workspace).expect("cleanup");
        std::fs::remove_dir_all(sandbox_home).expect("cleanup");
    }
}
