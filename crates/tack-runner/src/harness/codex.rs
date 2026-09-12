//! Codex harness grammar.
//!
//! Implements [`crate::harness::local_process::HarnessGrammar`] for
//! `harness_kind = "codex"`; [`CodexAdapter`] is
//! [`crate::harness::local_process::LocalProcessHarness<CodexGrammar>`], which
//! carries the shared local-process lifecycle (`crate::harness::local_process`)
//! and composes the shared process/redaction/artifact infrastructure
//! (`crate::harness::{process, redact, artifact}`).
//!
//! Vendor findings — what is measured, what is a documented guess, and at what observed
//! version: `fixtures/codex/README.md`, next to the transcripts that prove them.

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
    FeatureCapabilities, HarnessKind, Measurement, MeasurementSource, Usage,
    WorkspaceId as DomainWorkspaceId,
};

use crate::client::AttemptState;
use crate::config::ProviderConfig;
use crate::harness::{
    CancelObservation, ExecutionSpec, HarnessError, HarnessOutcome, RecoveryObservation,
    artifact::ArtifactStager,
    local_process::{HarnessGrammar, LocalProcessHarness, PreparedRun, not_measured},
    process::{
        CancelOutcome, CapturedOutput, ProcessError, ProcessExit, ProcessLimits, ProcessResult,
        ProcessSpec,
    },
    redact::SecretMaterial,
};
use crate::provider::ProviderEndpoint;
use crate::secrets::SecretStore;
// Re-exported (unused by this module's own production code) purely so
// `codex::tests` — a child module that relies on `use super::*` for
// everything else this file already imports — keeps seeing the frozen
// `HarnessAdapter`/`HarnessProbe` call boundary and its handle/journal
// types without a second, parallel import list.
#[cfg(test)]
pub(crate) use crate::client::Timestamp;
#[cfg(test)]
pub(crate) use crate::harness::{AttemptJournal, HarnessAdapter, HarnessProbe, LocalRunHandle};

const CODEX_HARNESS_KIND: &str = "codex";
const CODEX_PROGRAM_NAME: &str = "codex";
const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(15);
/// Not part of any frozen vocabulary today — see the module docs' assumption
/// (5) for why this is a new, adapter-chosen value rather
/// than the fixture-exemplified `"harness_reported"`.
const MODEL_OBSERVATION_SOURCE: &str =
    crate::harness::ModelObservationSource::RequestedNotConfirmed.as_str();
/// Local key this adapter names an injected provider endpoint under, for
/// Codex's own `-c model_providers.<key>.*` overrides — an adapter-chosen
/// label, not a vendor name. Codex only ever sees it for the lifetime of
/// one invocation; nothing persists it.
const CODEX_PROVIDER_KEY: &str = "tack_provider";

/// Quotes `value` the way `-c key="value"` expects: a double-quoted TOML
/// string. Safe for the plain ASCII values this adapter ever passes (a URL,
/// an environment variable name, a display label) — never used on
/// operator- or attempt-supplied text.
fn toml_quoted(value: &str) -> String {
    format!("{value:?}")
}

/// Where to find the `codex` executable.
#[derive(Clone)]
enum CodexLocator {
    /// A snapshot of the runner process's own `PATH` and home directory,
    /// taken once at construction (see `crate::harness::locate::snapshot`)
    /// and re-searched — `PATH` first, then the shared well-known install
    /// locations — on every [`CodexLocator::resolve`] call. Production
    /// default via [`CodexAdapter::discover`].
    Search {
        program_name: String,
        path: Option<std::ffi::OsString>,
        home: Option<PathBuf>,
    },
    /// A fixed program plus prefix args — how every fake-binary test in this
    /// file points the grammar at `crate::harness::fixtures::fake_harness_command`
    /// instead of a real `codex` binary. Never constructed by production
    /// code (only [`CodexLocator::Search`] is, via [`CodexAdapter::discover`]),
    /// so this variant is `#[cfg(test)]`-only, matching the same pattern
    /// `journal.rs`'s `OwnerOnlyJournal::fail_next_update` already uses for a
    /// field that exists purely to make a test possible.
    #[cfg(test)]
    Fixed {
        program: PathBuf,
        prefix_args: Vec<String>,
    },
}

impl CodexLocator {
    fn resolve(&self) -> Result<(PathBuf, Vec<String>), String> {
        match self {
            #[cfg(test)]
            Self::Fixed {
                program,
                prefix_args,
            } => Ok((program.clone(), prefix_args.clone())),
            Self::Search {
                program_name,
                path,
                home,
            } => super::locate::locate(program_name, path.as_deref(), home.as_deref())
                .map(|program| (program, Vec::new()))
                .map_err(|error| error.to_string()),
        }
    }
}

/// Strict `X.Y[.Z]` numeric-only check against one whitespace-delimited
/// token. Deliberately whole-token, not substring: the shared fixture's
/// `unknown_version` mode (`"harness-cli version
/// 999.999.999-nightly-exotic-format"`) genuinely *contains* a dot-separated
/// numeric run, but neither that token nor any other in the line is a clean
/// version, and none must be reported as one.
fn is_strict_version(candidate: &str) -> bool {
    let parts: Vec<&str> = candidate.split('.').collect();
    (2..=3).contains(&parts.len())
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
}

/// Finds the first whitespace-delimited token in `text` that is itself a
/// strict `X.Y[.Z]` version ([`is_strict_version`]). The real binary prints
/// `codex-cli 0.149.1` — a program-name token ahead of the version, not a
/// bare version string on its own — so the check must scan tokens rather
/// than require the whole trimmed line to be one.
fn find_strict_version_token(text: &str) -> Option<&str> {
    text.split_whitespace()
        .find(|token| is_strict_version(token))
}

fn bounded_preview(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_owned()
    } else {
        let truncated: String = text.chars().take(max_chars).collect();
        format!("{truncated}\u{2026} (truncated)")
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

/// Terminal-state classification from the process exit alone — see module
/// docs, assumption (4), for why content is deliberately never consulted.
fn classify_exit(exit: &ProcessExit) -> (AttemptState, &'static str, String) {
    match exit {
        ProcessExit::Exited(0) => (
            AttemptState::Succeeded,
            "completed",
            "codex exited successfully".to_owned(),
        ),
        ProcessExit::Exited(code) => (
            AttemptState::Failed,
            "exit_code",
            format!("codex exited with status {code}"),
        ),
        #[cfg(unix)]
        ProcessExit::Signaled(signal) => (
            AttemptState::Failed,
            "signaled",
            format!("codex was terminated by signal {signal}"),
        ),
        ProcessExit::TimedOut => (
            AttemptState::Failed,
            "timed_out",
            "codex exceeded its configured timeout and was killed".to_owned(),
        ),
    }
}

/// The opaque handle format this grammar hands back from `start` and expects
/// from `cancel`/`wait`/`reconcile`: `codex:<pid>:<monotonic counter>`. The
/// counter exists only to guarantee uniqueness within one adapter instance's
/// lifetime (pids can, in principle, be reused); it carries no other meaning.
fn encode_handle(pid: u32, counter: u64) -> String {
    format!("codex:{pid}:{counter}")
}

fn parse_handle_pid(process_id: &str) -> Option<u32> {
    let mut parts = process_id.split(':');
    if parts.next()? != "codex" {
        return None;
    }
    let pid = parts.next()?.parse::<u32>().ok()?;
    parts.next()?; // counter, required but not itself inspected
    if parts.next().is_some() {
        return None; // exactly three colon-separated parts, no more
    }
    Some(pid)
}

/// Per-run state [`CodexGrammar::prepare`] computes and
/// [`CodexGrammar::outcome`] later consumes — everything `wait` needs that
/// only `prepare` had access to.
pub struct CodexRunState {
    workspace_path: PathBuf,
    workspace_id: String,
    base_revision: String,
    attempt_id: String,
    harness_version: String,
    model_provider: String,
    model_id: String,
}

/// The Codex harness grammar: everything genuinely specific to the `codex`
/// CLI. Time is injected via `C: crate::Clock` on
/// [`LocalProcessHarness`] itself (never `SystemTime::now()` directly), not
/// here — this type has no clock of its own.
pub struct CodexGrammar {
    command: CodexLocator,
    process_limits: ProcessLimits,
    probe_timeout: Duration,
    /// Extra environment merged into every version-detection invocation
    /// only. Always empty in production ([`CodexAdapter::discover`]);
    /// fake-binary tests use this to steer the shared fixture's
    /// `TACK_FAKE_HARNESS_MODE` during probing specifically, independent of
    /// whatever mode a given test's `start()`/`wait()` call drives via the
    /// execution request's own `environment` map (the exec path's env is
    /// never influenced by this field, only the probe path's is).
    probe_env: BTreeMap<String, String>,
    artifact_staging_root: PathBuf,
    next_handle: AtomicU64,
    /// The most recently probed `(installed_version, probe_error)`, used to
    /// stamp `ActualExecution.harness_version` at `start()` time without a
    /// redundant `--version` invocation on every single attempt. `None`
    /// until the first successful `probe()` call; `prepare()` falls back to
    /// a one-off detection in that case rather than reporting a silently
    /// fabricated version. A plain `std::sync::Mutex` (never held across an
    /// `.await`) rather than the async `tokio::sync::Mutex`
    /// [`LocalProcessHarness`] uses for its own state.
    last_probe: std::sync::Mutex<Option<(String, Option<String>)>>,
}

/// The Codex harness adapter/probe:
/// [`LocalProcessHarness<CodexGrammar>`][crate::harness::local_process::LocalProcessHarness]
/// implements both [`crate::harness::HarnessAdapter`] (the frozen per-attempt
/// lifecycle) and [`crate::harness::HarnessProbe`] (capability discovery) for
/// any grammar; this alias is the name every other module (`bootstrap.rs`,
/// tests) constructs and passes around.
pub type CodexAdapter<C = crate::SystemClock> = LocalProcessHarness<CodexGrammar, C>;

impl CodexAdapter<crate::SystemClock> {
    /// Production constructor: resolves `codex` from the current process's
    /// `PATH` (snapshotted once, here) rather than a hardcoded path.
    /// `artifact_staging_root` is required explicitly, matching
    /// [`ArtifactStager::new`]'s own no-hidden-default style.
    pub fn discover(
        process_limits: ProcessLimits,
        artifact_staging_root: PathBuf,
        secrets: SecretStore,
    ) -> Self {
        let (path, home) = super::locate::snapshot();
        Self::with_clock(
            CodexLocator::Search {
                program_name: CODEX_PROGRAM_NAME.to_owned(),
                path,
                home,
            },
            process_limits,
            DEFAULT_PROBE_TIMEOUT,
            BTreeMap::new(),
            artifact_staging_root,
            crate::SystemClock,
            secrets,
        )
    }

    /// `pub(crate)` and test-only: points this adapter at an
    /// arbitrary fixture command instead of a real `codex` binary, for the
    /// "same fixture completes through all three fake adapters" acceptance
    /// proof in `harness::mod::tests` (which needs to construct a real
    /// `CodexAdapter` from outside this module). Not part of the public API
    /// — `AdapterRegistry` only ever stores `Box<dyn HarnessAdapter>`, which
    /// never needs to know how a concrete adapter was constructed.
    #[cfg(test)]
    pub(crate) fn for_fixture(
        program: PathBuf,
        prefix_args: Vec<String>,
        artifact_staging_root: PathBuf,
        secrets: SecretStore,
    ) -> Self {
        Self::with_clock(
            CodexLocator::Fixed {
                program,
                prefix_args,
            },
            ProcessLimits::new(1_000_000, 1_000_000, Duration::from_secs(10)),
            Duration::from_secs(5),
            BTreeMap::new(),
            artifact_staging_root,
            crate::SystemClock,
            secrets,
        )
    }
}

impl<C: crate::Clock> CodexAdapter<C> {
    fn with_clock(
        command: CodexLocator,
        process_limits: ProcessLimits,
        probe_timeout: Duration,
        probe_env: BTreeMap<String, String>,
        artifact_staging_root: PathBuf,
        clock: C,
        secrets: SecretStore,
    ) -> Self {
        let grammar = CodexGrammar {
            command,
            process_limits,
            probe_timeout,
            probe_env,
            artifact_staging_root,
            next_handle: AtomicU64::new(0),
            last_probe: std::sync::Mutex::new(None),
        };
        LocalProcessHarness::new(grammar, clock, secrets)
    }
}

impl CodexGrammar {
    /// Runs `codex --version` (assumption (2), see module docs) with
    /// `self.probe_env` merged in, bounded by `self.probe_timeout`. Never
    /// returns an `Err`: every failure mode (binary missing, spawn failure,
    /// nonzero exit, timeout, unparseable output) is folded into the
    /// `Option<String>` (probe-error reason) return slot, matching
    /// `HarnessProbe::probe`'s own contract that probing itself cannot fail.
    async fn detect_version_impl(
        &self,
    ) -> (String, Option<String>, BTreeMap<String, serde_json::Value>) {
        let (program, mut args) = match self.command.resolve() {
            Ok(resolved) => resolved,
            Err(reason) => return (String::new(), Some(reason), BTreeMap::new()),
        };
        args.push("--version".to_owned());

        let probe_workspace = std::env::temp_dir();
        let process_spec = ProcessSpec {
            program,
            args,
            env: self.probe_env.clone(),
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
                    Some(format!("codex --version could not be spawned: {error}")),
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
                        "codex --version failed while capturing output: {error}"
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
                        Some("codex --version produced no output".to_owned()),
                        BTreeMap::new(),
                    )
                } else if let Some(version) = find_strict_version_token(trimmed) {
                    (version.to_owned(), None, BTreeMap::new())
                } else {
                    let mut additional = BTreeMap::new();
                    additional.insert(
                        "raw_version_output".to_owned(),
                        serde_json::json!(bounded_preview(trimmed, 200)),
                    );
                    (
                        String::new(),
                        Some(
                            "codex --version output was not a recognizable version string"
                                .to_owned(),
                        ),
                        additional,
                    )
                }
            }
            ProcessExit::Exited(code) => (
                String::new(),
                Some(format!("codex --version exited with status {code}")),
                BTreeMap::new(),
            ),
            #[cfg(unix)]
            ProcessExit::Signaled(signal) => (
                String::new(),
                Some(format!("codex --version was terminated by signal {signal}")),
                BTreeMap::new(),
            ),
            ProcessExit::TimedOut => (
                String::new(),
                Some("codex --version timed out".to_owned()),
                BTreeMap::new(),
            ),
        }
    }

    /// Stages the (already-scrubbed) combined stdout/stderr as a `log`
    /// artifact inside the attempt's own workspace, via
    /// [`ArtifactStager`]. Best-effort: staging failure never fails the
    /// attempt itself, matching the "auto-status propagation" best-effort
    /// pattern already established elsewhere in this codebase — it only
    /// omits the `artifact` key from `terminal_reason`.
    fn stage_run_log(
        &self,
        workspace_path: &std::path::Path,
        attempt_id: &str,
        stdout: &CapturedOutput,
        stderr: &CapturedOutput,
    ) -> Option<serde_json::Value> {
        let relative = PathBuf::from(".tack-runner").join("codex-run.log");
        let absolute = workspace_path.join(&relative);
        if let Some(parent) = absolute.parent()
            && std::fs::create_dir_all(parent).is_err()
        {
            return None;
        }
        let mut combined = String::new();
        combined.push_str("=== stdout ===\n");
        combined.push_str(&stdout.text);
        combined.push_str("\n=== stderr ===\n");
        combined.push_str(&stderr.text);
        if std::fs::write(&absolute, combined.as_bytes()).is_err() {
            return None;
        }

        let stager = ArtifactStager::new(&self.artifact_staging_root);
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
                tracing::warn!(?error, "codex wait: artifact staging failed");
                None
            }
        }
    }
}

#[async_trait]
impl HarnessGrammar for CodexGrammar {
    type RunState = CodexRunState;

    fn harness_kind(&self) -> HarnessKind {
        HarnessKind::new(CODEX_HARNESS_KIND)
    }

    /// `harness_kind` self-check plus the "no auto-selected model" rejection
    /// documented in the module docs. Called from both `validate` and
    /// `start` so the two can never disagree about what counts as an
    /// unsupported selection.
    fn validate_selection(&self, spec: &ExecutionSpec) -> Result<(), HarnessError> {
        if spec.work.request.requested_harness_kind.as_str() != CODEX_HARNESS_KIND {
            let reason = format!(
                "requested harness kind {:?} does not match this adapter's kind {CODEX_HARNESS_KIND:?}",
                spec.work.request.requested_harness_kind.as_str()
            );
            tracing::warn!(
                reason,
                "codex: rejecting a spec requesting a different harness kind"
            );
            return Err(HarnessError::Rejected { reason });
        }
        if spec.work.request.requested_model_provider.is_none()
            || spec.work.request.requested_model_id.is_none()
        {
            let reason = "codex cannot independently confirm which model an auto-selected run \
                           actually used, so ActualExecution.model_provider/model_id (non-nullable) \
                           cannot be honestly filled; an explicit requested_model_provider and \
                           requested_model_id are both required"
                .to_owned();
            tracing::warn!(reason, "codex: rejecting an auto-selected model pre-spawn");
            return Err(HarnessError::Rejected { reason });
        }
        Ok(())
    }

    fn resolve_binary(&self) -> Result<(PathBuf, Vec<String>), String> {
        self.command.resolve()
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
            .expect("validate_selection rejects a missing model provider before this point")
            .as_str();
        crate::provider::resolve_endpoint(
            providers,
            secrets,
            provider,
            crate::provider::Wire::OpenAiResponses,
        )
        .map_err(|error| {
            let reason = error.to_string();
            tracing::warn!(
                reason,
                "codex: rejecting a request whose provider endpoint could not be resolved"
            );
            HarnessError::Rejected { reason }
        })
    }

    async fn prepare(
        &self,
        spec: &ExecutionSpec,
        secrets_store: &SecretStore,
        providers: &BTreeMap<String, ProviderConfig>,
    ) -> Result<PreparedRun<CodexRunState>, HarnessError> {
        let (program, mut args) = self.command.resolve().map_err(|reason| {
            tracing::warn!(reason, "codex start: binary unresolvable");
            HarnessError::Rejected { reason }
        })?;

        let model_provider = spec
            .work
            .request
            .requested_model_provider
            .as_ref()
            .expect("validate_selection rejects a missing model provider before this point")
            .as_str()
            .to_owned();
        let model_id = spec
            .work
            .request
            .requested_model_id
            .as_ref()
            .expect("validate_selection rejects a missing model id before this point")
            .as_str()
            .to_owned();

        // A configured provider endpoint applies only when this request's
        // provider names one (e.g. a gateway) — a direct-vendor request
        // (Codex's own built-in provider) resolves to `None` and this
        // grammar injects nothing, so the two paths can never be confused
        // by a shared environment variable. `-c` overrides are per-
        // invocation only: this adapter never writes `~/.codex/config.toml`.
        // They are global flags, so they must precede the `exec` subcommand
        // pushed below.
        let endpoint = self.resolve_provider_endpoint(spec, secrets_store, providers)?;
        if let Some(endpoint) = &endpoint {
            args.push("-c".to_owned());
            args.push(format!("model_provider={CODEX_PROVIDER_KEY}"));
            args.push("-c".to_owned());
            args.push(format!(
                "model_providers.{CODEX_PROVIDER_KEY}.name={}",
                toml_quoted(&endpoint.display_name)
            ));
            args.push("-c".to_owned());
            args.push(format!(
                "model_providers.{CODEX_PROVIDER_KEY}.base_url={}",
                toml_quoted(&endpoint.base_url)
            ));
            args.push("-c".to_owned());
            args.push(format!(
                "model_providers.{CODEX_PROVIDER_KEY}.env_key={}",
                toml_quoted(&endpoint.credential_env_var)
            ));
            // Set explicitly and defensively, matching the vendor's own
            // documented shape, though non-load-bearing today — see
            // `fixtures/codex/README.md` for the measurement.
            args.push("-c".to_owned());
            args.push(format!(
                "model_providers.{CODEX_PROVIDER_KEY}.wire_api={}",
                toml_quoted("responses")
            ));
        }

        // Measured against the real binary: `exec --json --model <id>` with
        // the prompt on stdin is the actual non-interactive invocation
        // shape, not a guess — see the module docs' corrected assumption
        // (3).
        args.push("exec".to_owned());
        args.push("--json".to_owned());
        args.push("--model".to_owned());
        args.push(model_id.clone());

        let prompt = spec
            .work
            .request
            .resolved_agent_profile
            .instructions
            .clone();

        let mut secrets = SecretMaterial::new();
        secrets.register(prompt.clone());

        let mut env = super::resolve_environment(secrets_store, &spec.work.request, &mut secrets)?;
        if let Some(endpoint) = endpoint {
            env.insert(
                endpoint.credential_env_var,
                endpoint.credential.expose().to_string(),
            );
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

        let cached_probe = self.last_probe.lock().unwrap().clone();
        let harness_version = match cached_probe {
            Some((version, _)) if !version.is_empty() => version,
            _ => self.detect_version_impl().await.0,
        };

        let state = CodexRunState {
            workspace_path: spec.workspace.path.clone(),
            workspace_id: spec.workspace.id.as_str().to_owned(),
            base_revision: spec.workspace.base_revision.clone(),
            attempt_id: spec.work.lease.attempt_id.as_str().to_owned(),
            harness_version,
            model_provider,
            model_id,
        };

        Ok(PreparedRun {
            process_spec,
            secrets,
            limits,
            state,
        })
    }

    fn encode_handle(&self, pid: u32) -> String {
        encode_handle(pid, self.next_handle.fetch_add(1, Ordering::SeqCst))
    }

    fn decode_handle(&self, process_id: &str) -> Option<u32> {
        parse_handle_pid(process_id)
    }

    fn cancel_outcome(
        &self,
        _pid: u32,
        signal_result: Result<CancelOutcome, ProcessError>,
    ) -> Result<
        (
            CancelObservation,
            serde_json::Map<String, serde_json::Value>,
        ),
        HarnessError,
    > {
        match signal_result {
            Ok(outcome) => {
                let mut details = serde_json::Map::new();
                details.insert(
                    "outcome".to_owned(),
                    serde_json::json!(match outcome {
                        CancelOutcome::Stopped => "stopped_after_sigterm",
                        CancelOutcome::Killed => "killed_after_sigkill",
                    }),
                );
                Ok((CancelObservation::ProcessStopped, details))
            }
            Err(error) => {
                tracing::warn!(?error, "codex cancel: signal delivery failed");
                Err(HarnessError::Process)
            }
        }
    }

    fn reconcile_alive(&self, _pid: u32) -> RecoveryObservation {
        RecoveryObservation::ProcessRunning
    }

    fn reconcile_unavailable(&self) -> Result<RecoveryObservation, HarnessError> {
        // Reconcile the journal only when reconciliation is genuinely
        // supported: non-Unix has no portable liveness primitive here
        // (matches `harness/process.rs`'s own documented non-Unix
        // cancellation fallback), so this is honestly reported as
        // unavailable rather than guessed.
        Err(HarnessError::RecoveryUnavailable)
    }

    fn outcome(
        &self,
        state: CodexRunState,
        started_at: DateTime<Utc>,
        ended_at: DateTime<Utc>,
        result: ProcessResult,
        // codex's `classify_exit` never produces `AttemptState::Cancelled`
        // (see the module docs' assumption (4)); the shared core still
        // tracks "was `cancel()` already called on this handle" generically,
        // for claude-code's benefit, but codex has nothing to do with it.
        _cancelled: bool,
    ) -> HarnessOutcome {
        let elapsed_ms = ended_at
            .signed_duration_since(started_at)
            .num_milliseconds()
            .max(0) as u64;

        let (terminal_state, code, message) = classify_exit(&result.exit);
        let mut terminal_reason = serde_json::json!({
            "code": code,
            "message": message,
            "stdout": describe_capture(&result.stdout),
            "stderr": describe_capture(&result.stderr),
        });
        if let Some(artifact) = self.stage_run_log(
            &state.workspace_path,
            &state.attempt_id,
            &result.stdout,
            &result.stderr,
        ) {
            terminal_reason["artifact"] = artifact;
        }

        let usage = Usage {
            tokens_in: not_measured(),
            tokens_out: not_measured(),
            duration_ms: Measurement {
                value: Some(elapsed_ms),
                source: MeasurementSource::Measured,
                additional: BTreeMap::new(),
            },
            cost_usd: not_measured(),
            additional: BTreeMap::new(),
        };

        let actual_execution = ActualExecution {
            harness_kind: HarnessKind::new(CODEX_HARNESS_KIND),
            harness_version: state.harness_version,
            model_provider: ActualModelProvider::new(state.model_provider),
            model_id: ActualModelId::new(state.model_id),
            model_observation_source: MODEL_OBSERVATION_SOURCE.to_owned(),
            capability_snapshot: self.feature_capabilities(),
            workspace_id: DomainWorkspaceId::new(state.workspace_id),
            base_revision: state.base_revision,
            started_at,
            ended_at,
            additional: BTreeMap::new(),
        };

        HarnessOutcome {
            terminal_state,
            terminal_reason,
            final_checkpoint: None,
            actual_execution,
            usage,
        }
    }

    async fn detect_version(
        &self,
    ) -> (String, Option<String>, BTreeMap<String, serde_json::Value>) {
        self.detect_version_impl().await
    }

    fn after_probe(&self, version: &str, error: Option<&str>) {
        *self.last_probe.lock().unwrap() = Some((version.to_owned(), error.map(str::to_owned)));
    }

    fn model_passthrough(&self) -> Option<CapabilityValue> {
        // A pass-through attestation: a claim about THIS grammar's
        // invocation contract (`--model <requested_model_id>` is passed
        // verbatim, and a spec without an explicit model is rejected
        // pre-spawn — see the module docs), not about which models
        // exist. No model list is invented.
        Some(CapabilityValue {
            support: CapabilitySupport::Supported,
            reason: Some(
                "the adapter forwards requested_model_id verbatim via --model and rejects \
                 specs without an explicit model pre-spawn; model validity is established \
                 by the Codex CLI at run time, so operator-specified opaque models are \
                 accepted without the probe claiming any model list"
                    .to_string(),
            ),
            additional: Default::default(),
        })
    }

    /// Honest, harness-agnostic-where-possible feature support. See module
    /// docs assumption (6) for why `resume`/`decisions`/`usage` are
    /// `unsupported` rather than guessed, and why `artifacts` is `advisory`.
    fn feature_capabilities(&self) -> FeatureCapabilities {
        FeatureCapabilities {
            // Downgraded from `Supported`. This
            // adapter's only cancellation primitive is
            // `harness::process::SupervisedProcess::cancel` (a process-group
            // SIGTERM/SIGKILL), the exact same mechanism proved (via `ps`
            // against real Claude Code) cannot reliably reach a
            // descendant a harness's own shell-tool spawns into a new OS
            // session. `codex` is not installed
            // on any machine this adapter has been built against, so there
            // is no adapter-specific evidence its own tool execution stays
            // inside the process group either; claiming `Supported` on that
            // silence would be exactly the "hidden fake success" rule 7
            // forbids.
            cancel: CapabilityValue {
                support: CapabilitySupport::Advisory,
                reason: Some(
                    "the top-level codex process is always signalled reliably (it is always \
                     its own process-group leader), but a shell-tool-spawned descendant that \
                     detaches into its own OS session (observed for Claude Code; \
                     never independently verified for codex, since codex is not installed) \
                     would only be reached if it exits gracefully within the SIGTERM grace \
                     period — a SIGKILL escalation cannot reach a different session's process \
                     group"
                        .to_owned(),
                ),
                additional: BTreeMap::new(),
            },
            resume: CapabilityValue {
                support: CapabilitySupport::Unsupported,
                reason: Some(
                    "codex session/resume behavior has not been observed and is not \
                     implemented by this adapter"
                        .to_owned(),
                ),
                additional: BTreeMap::new(),
            },
            decisions: CapabilityValue {
                support: CapabilitySupport::Unsupported,
                reason: Some(
                    "the runner protocol has no wired decision transport yet, and codex's own \
                     approval/decision behavior has not been observed"
                        .to_owned(),
                ),
                additional: BTreeMap::new(),
            },
            artifacts: CapabilityValue {
                support: CapabilitySupport::Advisory,
                reason: Some(
                    "only raw captured stdout/stderr is staged as a log artifact; no \
                     codex-specific artifact discovery (e.g. a git diff) has been implemented \
                     or verified"
                        .to_owned(),
                ),
                additional: BTreeMap::new(),
            },
            usage: CapabilityValue {
                support: CapabilitySupport::Unsupported,
                reason: Some(
                    "token/cost usage has not been observed in codex output on this machine; \
                     only wall-clock duration is measured"
                        .to_owned(),
                ),
                additional: BTreeMap::new(),
            },
            additional: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
#[path = "codex/tests.rs"]
mod tests;
