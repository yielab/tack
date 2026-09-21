//! The local-process harness: one lifecycle for every harness CLI that runs
//! as a child process, plus the small seam a concrete CLI fills in.
//!
//! [`LocalProcessHarness<G>`] owns everything that does not depend on which
//! CLI is running: locating the binary, the version probe, the request
//! policy every harness is held to, environment and secret resolution,
//! provider-endpoint injection, spawn, capture limits, cancellation,
//! reconciliation after a restart, log staging and the assembly of the
//! [`HarnessOutcome`].
//!
//! A harness contributes a [`HarnessDescriptor`] (data) and a
//! [`HarnessGrammar`]: how a request becomes a command line, how the
//! process's output becomes a [`RunReport`], and what it honestly supports.
//! Nothing else is per-harness, so a rule added here binds every harness at
//! once.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tack_orch::execution::{
    ActualExecution, ActualModelId, ActualModelProvider, CapabilitySupport, CapabilityValue,
    FeatureCapabilities, HarnessCapability, HarnessKind as DomainHarnessKind, Measurement,
    MeasurementSource, Usage, WorkspaceId as DomainWorkspaceId,
};
use tokio::sync::mpsc;

use crate::{
    Clock,
    client::AttemptState,
    config::ProviderConfig,
    harness::{
        AttemptJournal, CancelObservation, CancellationEvidence, DecisionAnswer, ExecutionSpec,
        HarnessAdapter, HarnessError, HarnessOutcome, HarnessProbe, LocalRunHandle,
        ModelObservationSource, Question, RecoveryObservation, StreamSignal,
        process::{
            CancelOutcome, ProcessExit, ProcessLimits, ProcessResult, ProcessSpec,
            SupervisedProcess,
        },
        redact::SecretMaterial,
    },
    provider::{ProviderEndpoint, Wire},
    secrets::SecretStore,
};

/// `request_timeout_seconds_max` in `docs/contracts/runner-v1/limits.json`.
const MAX_TIMEOUT_SECONDS: u64 = 86_400;
const PROBE_TIMEOUT: Duration = Duration::from_secs(15);
/// Reported as the model id when neither the harness nor the request named
/// one; always paired with [`ModelObservationSource::NotObserved`].
pub const UNOBSERVED_MODEL: &str = "unknown";

/// Whether a request may leave the model to the harness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelSelection {
    /// The request must name a provider and a model. The text says why,
    /// and becomes the rejection reason.
    Explicit(&'static str),
    Optional,
}

/// Everything about a harness CLI that is data rather than behaviour.
#[derive(Debug)]
pub struct HarnessDescriptor {
    /// The wire value of `requested_harness_kind`.
    pub kind: &'static str,
    /// The executable searched for on `PATH` and the well-known install
    /// locations in [`crate::harness::locate`].
    pub program: &'static str,
    /// The one model wire this CLI speaks; selects which configured
    /// provider endpoints can reach it.
    pub wire: Wire,
    pub model_selection: ModelSelection,
    /// The provider recorded when a request names none.
    pub native_provider: &'static str,
    /// Names copied from the runner's own environment into every spawn.
    /// The child's environment is otherwise exactly what the request and
    /// the provider injection put there.
    pub inherited_env: &'static [&'static str],
    /// Why a requested model id is accepted without a model list.
    pub model_passthrough: &'static str,
    /// Extra key/value notes attached to every probe report.
    pub probe_notes: &'static [(&'static str, &'static str)],
    /// Where this CLI's own credential lives, for `tack runner doctor`.
    pub credential_note: &'static str,
    /// The variable name this CLI reads a configured endpoint's credential
    /// from, when it differs from the endpoint's own `credential_env_var`
    /// (e.g. docket always reads `DOCKET_LLM_API_KEY`, never the
    /// provider's name for it). `None` keeps the provider's own name.
    pub credential_env: Option<&'static str>,
    /// Whether this CLI's own reported model comes from the endpoint's
    /// response body rather than an echo of what was configured. When
    /// true, a report's `observed_model` is recorded `harness_reported`
    /// even behind a gateway that could otherwise substitute a model —
    /// the gateway downgrade in [`LocalProcessHarness::outcome`] exists
    /// for a CLI that only ever echoes its own request.
    pub observes_served_model: bool,
}

/// What the grammar is given to build a command line.
pub struct RunContext<'a> {
    pub spec: &'a ExecutionSpec,
    /// The configured provider endpoint the request named, or `None` when
    /// the CLI runs against its own native login. The core injects the
    /// credential variable itself; the grammar only points the CLI at it.
    pub endpoint: Option<&'a ProviderEndpoint>,
    /// A directory this attempt owns, outside the workspace, for whatever
    /// state the CLI itself needs between spawn and exit (a home directory,
    /// a cache). The core computes the path and removes it once `wait` or
    /// `cancel` has finished with it; it never creates it, so a grammar
    /// naming a path under here relies on the CLI to create it.
    pub scratch: &'a Path,
}

/// The harness-specific part of a spawn.
#[derive(Debug, Default)]
pub struct Invocation {
    pub args: Vec<String>,
    /// Added on top of the inherited, requested and credential variables.
    pub env: BTreeMap<String, String>,
    /// Set only for an `ask` request: after the prompt is written, the core
    /// keeps stdin open and drives `signal`/`answer` on this CLI's stdout
    /// instead of running it to exit and closing the pipe.
    pub stdin_stays_open: bool,
}

/// What a finished process's output says happened.
#[derive(Debug)]
pub struct RunReport {
    pub succeeded: bool,
    /// Vendor evidence, reported as-is. The core adds the staged log under
    /// `artifact` when this is a JSON object.
    pub terminal_reason: serde_json::Value,
    /// The version the run itself reported, when it reports one.
    pub harness_version: Option<String>,
    /// The model id the harness itself reported, when it reports one.
    pub observed_model: Option<String>,
    pub tokens_in: Option<u64>,
    pub tokens_out: Option<u64>,
    /// `None` leaves the runner's own wall-clock measurement in place.
    pub duration_ms: Option<u64>,
    pub cost_usd: Option<f64>,
}

impl RunReport {
    /// A report carrying only a verdict and its evidence.
    pub fn verdict(succeeded: bool, terminal_reason: serde_json::Value) -> Self {
        Self {
            succeeded,
            terminal_reason,
            harness_version: None,
            observed_model: None,
            tokens_in: None,
            tokens_out: None,
            duration_ms: None,
            cost_usd: None,
        }
    }
}

/// The per-CLI seam.
pub trait HarnessGrammar: Send + Sync + 'static {
    fn descriptor(&self) -> &'static HarnessDescriptor;

    /// The per-feature support this CLI honestly provides.
    fn capabilities(&self) -> FeatureCapabilities;

    /// Builds the command line for one request, or rejects a request this
    /// CLI cannot honour — a tool it cannot restrict, a provider it does
    /// not know. Runs in `validate` and again in `start`, so the two cannot
    /// disagree.
    fn invocation(&self, run: &RunContext<'_>) -> Result<Invocation, HarnessError>;

    /// Reads a finished process. Never fails: output that cannot be read is
    /// a failed run whose `terminal_reason` says so.
    fn report(&self, run: &RunContext<'_>, result: &ProcessResult) -> RunReport;

    /// What a line of this CLI's stdout means to the core, if anything.
    /// Only ever consulted when [`Invocation::stdin_stays_open`] was set.
    /// `run` is what the grammar needs to build the handshake's requests
    /// from a reply: the prompt text is
    /// `run.spec.work.request.resolved_agent_profile.instructions`, the
    /// workspace is `run.spec.workspace.path`, and the endpoint and scratch
    /// dir are its own fields.
    fn signal(&self, _run: &RunContext<'_>, _line: &str) -> Option<StreamSignal> {
        None
    }

    /// The bytes that deliver the prompt on stdin. A CLI kept open to be
    /// asked questions usually wants it framed in its own protocol.
    fn prompt(&self, prompt: String, _stdin_stays_open: bool) -> Vec<u8> {
        prompt.into_bytes()
    }

    /// The bytes that answer `question`, written to the CLI's stdin.
    fn answer(&self, _question: &Question, _answer: &DecisionAnswer) -> Vec<u8> {
        Vec::new()
    }
}

/// Shorthand for one [`FeatureCapabilities`] entry.
pub fn capability(support: CapabilitySupport, reason: &str) -> CapabilityValue {
    CapabilityValue {
        support,
        reason: Some(reason.to_owned()),
        additional: BTreeMap::new(),
    }
}

/// The `permission_policy` entry of a capability table: how far this CLI
/// enforces the tool list and network flag a request carries. Every grammar
/// states it, so a policy one harness ignores is a declared fact rather than
/// a silent difference between harnesses.
pub fn policy_capability(
    support: CapabilitySupport,
    reason: &str,
) -> BTreeMap<String, serde_json::Value> {
    BTreeMap::from([(
        "permission_policy".to_owned(),
        serde_json::json!(capability(support, reason)),
    )])
}

/// Where the executable comes from.
#[derive(Debug, Clone)]
pub enum BinaryLocator {
    /// Searches a snapshot of the runner's own `PATH` and home directory on
    /// every call, so an uninstall after startup is caught at `validate`.
    Search {
        program: String,
        path: Option<std::ffi::OsString>,
        home: Option<PathBuf>,
    },
    /// A fixed program plus leading arguments: how tests point a grammar at
    /// the fake harness script.
    #[cfg(test)]
    Fixed {
        program: PathBuf,
        prefix_args: Vec<String>,
    },
}

impl BinaryLocator {
    fn resolve(&self) -> Result<(PathBuf, Vec<String>), String> {
        match self {
            Self::Search {
                program,
                path,
                home,
            } => super::locate::locate(program, path.as_deref(), home.as_deref())
                .map(|found| (found, Vec::new()))
                .map_err(|error| error.to_string()),
            #[cfg(test)]
            Self::Fixed {
                program,
                prefix_args,
            } => Ok((program.clone(), prefix_args.clone())),
        }
    }
}

pub(crate) struct RunningProcess {
    process: SupervisedProcess,
    record: RunRecord,
    /// The engine-facing half of an interactive run's channels, present
    /// only when [`Invocation::stdin_stays_open`] was set. Taken once by
    /// `decision_channels`, before `wait` consumes the rest of this entry.
    decision_channels: Option<(mpsc::Receiver<Question>, mpsc::Sender<DecisionAnswer>)>,
}

/// What `wait` needs back once the process itself has been consumed.
struct RunRecord {
    secrets: SecretMaterial,
    limits: ProcessLimits,
    started_at: DateTime<Utc>,
    spec: ExecutionSpec,
    endpoint: Option<ProviderEndpoint>,
    scratch: PathBuf,
    /// The process-facing half of an interactive run's channels: the ends
    /// `wait`'s interactive read loop sends a question out on and receives
    /// its answer back on. Present exactly when `decision_channels` (above,
    /// on the sibling [`RunningProcess`]) is.
    interactive: Option<(mpsc::Sender<Question>, mpsc::Receiver<DecisionAnswer>)>,
}

/// The adapter and probe for any [`HarnessGrammar`].
pub struct LocalProcessHarness<G: HarnessGrammar, C = crate::SystemClock> {
    grammar: G,
    locator: BinaryLocator,
    clock: C,
    limits: ProcessLimits,
    staging_root: PathBuf,
    secrets: SecretStore,
    providers: BTreeMap<String, ProviderConfig>,
    probe_timeout: Duration,
    /// Added to the version probe's environment only; tests steer the fake
    /// harness with it.
    probe_env: BTreeMap<String, String>,
    /// The last successfully probed version, stamped on a run whose own
    /// output reports none.
    probed_version: std::sync::Mutex<Option<String>>,
    next_handle: AtomicU64,
    pub(crate) running: tokio::sync::Mutex<BTreeMap<String, RunningProcess>>,
}

impl<G: HarnessGrammar> LocalProcessHarness<G> {
    /// Locates the CLI on this machine. A harness whose binary is absent is
    /// an `Err` naming where it was searched for, and is never registered.
    pub fn discover(
        grammar: G,
        limits: ProcessLimits,
        staging_root: PathBuf,
        secrets: SecretStore,
    ) -> Result<Self, String> {
        let (path, home) = super::locate::snapshot();
        let locator = BinaryLocator::Search {
            program: grammar.descriptor().program.to_owned(),
            path,
            home,
        };
        locator.resolve()?;
        Ok(Self::new(
            grammar,
            locator,
            crate::SystemClock,
            limits,
            staging_root,
            secrets,
        ))
    }
}

impl<G: HarnessGrammar, C: Clock> LocalProcessHarness<G, C> {
    pub(crate) fn new(
        grammar: G,
        locator: BinaryLocator,
        clock: C,
        limits: ProcessLimits,
        staging_root: PathBuf,
        secrets: SecretStore,
    ) -> Self {
        Self {
            grammar,
            locator,
            clock,
            limits,
            staging_root,
            secrets,
            providers: BTreeMap::new(),
            probe_timeout: PROBE_TIMEOUT,
            probe_env: BTreeMap::new(),
            probed_version: std::sync::Mutex::new(None),
            next_handle: AtomicU64::new(0),
            running: tokio::sync::Mutex::new(BTreeMap::new()),
        }
    }

    /// The provider endpoints a request may name. Empty means every request
    /// runs against the CLI's own native login.
    pub fn with_providers(mut self, providers: BTreeMap<String, ProviderConfig>) -> Self {
        self.providers = providers;
        self
    }

    #[cfg(test)]
    pub(crate) fn with_probe(mut self, timeout: Duration, env: BTreeMap<String, String>) -> Self {
        self.probe_timeout = timeout;
        self.probe_env = env;
        self
    }

    fn kind(&self) -> &'static str {
        self.grammar.descriptor().kind
    }

    fn reject(&self, reason: String) -> HarnessError {
        tracing::warn!(
            reason,
            harness = self.kind(),
            "request rejected before spawn"
        );
        HarnessError::Rejected { reason }
    }

    /// The rules every harness is held to before anything is spawned.
    fn check_request(&self, spec: &ExecutionSpec) -> Result<(), HarnessError> {
        let descriptor = self.grammar.descriptor();
        let request = &spec.work.request;
        let requested = request.requested_harness_kind.as_str();
        if requested != descriptor.kind {
            return Err(self.reject(format!(
                "requested harness kind {requested:?} does not match this adapter's kind {:?}",
                descriptor.kind
            )));
        }
        if let ModelSelection::Explicit(reason) = descriptor.model_selection
            && (request.requested_model_provider.is_none() || request.requested_model_id.is_none())
        {
            return Err(self.reject(reason.to_owned()));
        }
        Ok(())
    }

    fn resolve_endpoint(
        &self,
        spec: &ExecutionSpec,
    ) -> Result<Option<ProviderEndpoint>, HarnessError> {
        let provider = spec
            .work
            .request
            .requested_model_provider
            .as_ref()
            .map_or("", |provider| provider.as_str());
        crate::provider::resolve_endpoint(
            &self.providers,
            &self.secrets,
            provider,
            self.grammar.descriptor().wire,
        )
        .map_err(|error| self.reject(error.to_string()))
    }

    fn inherited_env(&self) -> BTreeMap<String, String> {
        self.grammar
            .descriptor()
            .inherited_env
            .iter()
            .filter_map(|name| Some(((*name).to_owned(), std::env::var(name).ok()?)))
            .collect()
    }

    fn run_limits(&self, spec: &ExecutionSpec) -> ProcessLimits {
        let requested = spec.work.request.timeout_seconds;
        ProcessLimits {
            max_stdout_bytes: self.limits.max_stdout_bytes,
            max_stderr_bytes: self.limits.max_stderr_bytes,
            timeout: if requested > 0 {
                Duration::from_secs(requested.min(MAX_TIMEOUT_SECONDS))
            } else {
                self.limits.timeout
            },
            termination_grace: self.limits.termination_grace,
        }
    }

    /// Where this attempt's scratch directory lives. Pure path arithmetic —
    /// nothing here touches the filesystem, so `validate` can call it too.
    fn scratch_dir(&self, spec: &ExecutionSpec) -> PathBuf {
        self.staging_root
            .join("scratch")
            .join(spec.work.lease.attempt_id.as_str())
    }

    /// Best-effort cleanup of an attempt's scratch directory. A CLI that
    /// never wrote one leaves nothing to remove; any other failure is
    /// logged by attempt id alone, never the path, and never changes the
    /// outcome that was already built.
    fn remove_scratch(&self, scratch: &Path, attempt_id: &str) {
        if let Err(error) = std::fs::remove_dir_all(scratch)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(
                attempt_id,
                harness = self.kind(),
                ?error,
                "scratch directory could not be removed"
            );
        }
    }

    /// Everything `start` spawns, built the same way `validate` checks it.
    /// The trailing `bool` is [`Invocation::stdin_stays_open`], carried back
    /// so `start` knows whether to open this run's decision channels.
    fn prepare(
        &self,
        spec: &ExecutionSpec,
    ) -> Result<
        (
            ProcessSpec,
            SecretMaterial,
            Option<ProviderEndpoint>,
            PathBuf,
            bool,
        ),
        HarnessError,
    > {
        self.check_request(spec)?;
        let (program, mut args) = self
            .locator
            .resolve()
            .map_err(|reason| self.reject(reason))?;
        let request = &spec.work.request;

        let mut secrets = SecretMaterial::new();
        let mut env = self.inherited_env();
        env.extend(super::resolve_environment(
            &self.secrets,
            request,
            &mut secrets,
        )?);

        let endpoint = self.resolve_endpoint(spec)?;
        let scratch = self.scratch_dir(spec);
        let invocation = self.grammar.invocation(&RunContext {
            spec,
            endpoint: endpoint.as_ref(),
            scratch: &scratch,
        })?;
        args.extend(invocation.args);
        env.extend(invocation.env);
        if let Some(endpoint) = &endpoint {
            let credential = endpoint.credential.expose().to_owned();
            secrets.register(credential.clone());
            let credential_var = self
                .grammar
                .descriptor()
                .credential_env
                .map_or_else(|| endpoint.credential_env_var.clone(), str::to_owned);
            env.insert(credential_var, credential);
        }

        let prompt = request.resolved_agent_profile.instructions.clone();
        secrets.register(prompt.clone());
        let stdin_stays_open = invocation.stdin_stays_open;
        let stdin = self.grammar.prompt(prompt, stdin_stays_open);

        let workspace_root = spec.workspace.path.clone();
        let working_directory = match request.repository.subdirectory.as_deref() {
            Some(subdirectory) if !subdirectory.is_empty() => workspace_root.join(subdirectory),
            _ => workspace_root.clone(),
        };
        let process_spec = ProcessSpec {
            program,
            args,
            env,
            stdin: Some(stdin),
            working_directory,
            workspace_root,
            keep_stdin_open: stdin_stays_open,
        };
        Ok((process_spec, secrets, endpoint, scratch, stdin_stays_open))
    }

    /// Runs `<program> --version`. Probing cannot fail: every way it goes
    /// wrong is the `Option<String>` error, and unrecognized output is kept
    /// under `raw_version_output` rather than reported as a version.
    async fn detect_version(&self) -> (String, Option<String>, Option<String>) {
        let program_name = self.grammar.descriptor().program;
        let failed = |reason: String| (String::new(), Some(reason), None);
        let (program, mut args) = match self.locator.resolve() {
            Ok(resolved) => resolved,
            Err(reason) => return failed(reason),
        };
        args.push("--version".to_owned());
        let mut env = self.inherited_env();
        env.extend(self.probe_env.clone());
        // A working directory for a `--version` probe that writes nothing —
        // not a test artifact, so no `tempfile` guard.
        #[allow(clippy::disallowed_methods)]
        let neutral_dir = std::env::temp_dir();
        let spec = ProcessSpec {
            program,
            args,
            env,
            stdin: None,
            working_directory: neutral_dir.clone(),
            workspace_root: neutral_dir,
            keep_stdin_open: false,
        };
        let limits = ProcessLimits::new(8192, 8192, self.probe_timeout);
        let result = match spec.spawn().await {
            Ok(process) => {
                process
                    .wait_with_capture(&limits, &SecretMaterial::new())
                    .await
            }
            Err(error) => {
                return failed(format!(
                    "{program_name} --version could not be spawned: {error}"
                ));
            }
        };
        let result = match result {
            Ok(result) => result,
            Err(error) => return failed(format!("{program_name} --version failed: {error}")),
        };
        match result.exit {
            ProcessExit::Exited(0) => {
                let output = result.stdout.text.trim();
                match parse_version(output) {
                    Some(version) => (version.to_owned(), None, None),
                    None if output.is_empty() => {
                        failed(format!("{program_name} --version produced no output"))
                    }
                    None => (
                        String::new(),
                        Some(format!(
                            "{program_name} --version output was not a recognizable version"
                        )),
                        Some(output.chars().take(200).collect()),
                    ),
                }
            }
            ProcessExit::Exited(code) => failed(format!(
                "{program_name} --version exited with status {code}"
            )),
            #[cfg(unix)]
            ProcessExit::Signaled(signal) => failed(format!(
                "{program_name} --version was terminated by signal {signal}"
            )),
            ProcessExit::TimedOut => failed(format!("{program_name} --version timed out")),
        }
    }

    async fn known_version(&self) -> String {
        if let Some(version) = self.probed_version.lock().unwrap().clone() {
            return version;
        }
        let (version, _, _) = self.detect_version().await;
        if !version.is_empty() {
            *self.probed_version.lock().unwrap() = Some(version.clone());
        }
        version
    }

    /// Whether a live pid is still the program this harness spawns, or a
    /// launcher script for it. `argv[0]` matching the resolved binary proves
    /// it directly; failing that, the attempt id appearing anywhere in the
    /// argument list proves it too — a launcher (docket) never has the real
    /// program as `argv[0]`, but always carries the attempt id it was
    /// started with. `None` when neither can be known: a bare liveness check
    /// cannot rule out a pid the OS has since given to something else.
    #[cfg(target_os = "linux")]
    fn process_is_this_harness(&self, pid: u32, attempt_id: &str) -> Option<bool> {
        let (expected, _) = self.locator.resolve().ok()?;
        let raw = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
        let mut args = raw.split(|byte| *byte == 0).filter(|part| !part.is_empty());
        let argv0 = args.next()?;
        let argv0 = Path::new(std::str::from_utf8(argv0).ok()?);
        let canonical = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if canonical(argv0) == canonical(&expected) {
            return Some(true);
        }
        Some(args.any(|arg| arg == attempt_id.as_bytes()))
    }

    #[cfg(not(target_os = "linux"))]
    fn process_is_this_harness(&self, _pid: u32, _attempt_id: &str) -> Option<bool> {
        None
    }

    async fn take_running(&self, process_id: &str) -> Result<RunningProcess, HarnessError> {
        self.running.lock().await.remove(process_id).ok_or_else(|| {
            tracing::warn!(process_id, harness = self.kind(), "handle not tracked");
            HarnessError::Process
        })
    }

    fn outcome(
        &self,
        running: &RunRecord,
        ended_at: DateTime<Utc>,
        result: &ProcessResult,
        probed_version: String,
    ) -> HarnessOutcome {
        let descriptor = self.grammar.descriptor();
        let request = &running.spec.work.request;
        let report = self.grammar.report(
            &RunContext {
                spec: &running.spec,
                endpoint: running.endpoint.as_ref(),
                scratch: &running.scratch,
            },
            result,
        );

        let mut terminal_reason = report.terminal_reason;
        if let Some(object) = terminal_reason.as_object_mut()
            && let Some(artifact) = self.stage_run_log(&running.spec, result)
        {
            object.insert("artifact".to_owned(), artifact);
        }

        let provider = request.requested_model_provider.as_ref().map_or_else(
            || descriptor.native_provider.to_owned(),
            |provider| provider.as_str().trim().to_ascii_lowercase(),
        );
        let requested_model = request.requested_model_id.as_ref().map(|id| id.as_str());
        // A configured endpoint answers after the CLI has already printed
        // the model it was asked for, so what the CLI reports is then a
        // request, not an observation — unless the CLI's own report is
        // itself the endpoint's answer (`observes_served_model`), which is
        // an observation regardless of what sits behind the endpoint.
        let (model_id, source) = match (report.observed_model, requested_model) {
            (Some(model), _) if descriptor.observes_served_model => {
                (model, ModelObservationSource::HarnessReported)
            }
            (Some(model), _)
                if crate::provider::requires_unconfirmed_model_recording(&provider) =>
            {
                (model, ModelObservationSource::RequestedNotConfirmed)
            }
            (Some(model), _) => (model, ModelObservationSource::HarnessReported),
            (None, Some(model)) => (
                model.to_owned(),
                ModelObservationSource::RequestedNotConfirmed,
            ),
            (None, None) => (
                UNOBSERVED_MODEL.to_owned(),
                ModelObservationSource::NotObserved,
            ),
        };

        let elapsed_ms = ended_at
            .signed_duration_since(running.started_at)
            .num_milliseconds()
            .max(0) as u64;
        HarnessOutcome {
            terminal_state: if report.succeeded {
                AttemptState::Succeeded
            } else {
                AttemptState::Failed
            },
            terminal_reason,
            final_checkpoint: None,
            actual_execution: ActualExecution {
                harness_kind: DomainHarnessKind::new(descriptor.kind),
                harness_version: report.harness_version.unwrap_or(probed_version),
                model_provider: ActualModelProvider::new(provider),
                model_id: ActualModelId::new(model_id),
                model_observation_source: source.as_str().to_owned(),
                capability_snapshot: self.grammar.capabilities(),
                // Overwritten by the engine, which owns workspace facts.
                workspace_id: DomainWorkspaceId::new(running.spec.workspace.id.as_str()),
                base_revision: running.spec.workspace.base_revision.clone(),
                started_at: running.started_at,
                ended_at,
                additional: BTreeMap::new(),
            },
            usage: Usage {
                tokens_in: measurement(report.tokens_in),
                tokens_out: measurement(report.tokens_out),
                duration_ms: measurement(Some(report.duration_ms.unwrap_or(elapsed_ms))),
                cost_usd: measurement(report.cost_usd),
                additional: BTreeMap::new(),
            },
        }
    }

    /// Stages the already-redacted stdout and stderr as a `log` artifact.
    /// Best-effort: a staging failure omits the artifact, never fails the
    /// attempt.
    fn stage_run_log(
        &self,
        spec: &ExecutionSpec,
        result: &ProcessResult,
    ) -> Option<serde_json::Value> {
        let workspace = &spec.workspace.path;
        let relative = PathBuf::from(".tack-runner").join(format!("{}-run.log", self.kind()));
        let absolute = workspace.join(&relative);
        std::fs::create_dir_all(absolute.parent()?).ok()?;
        let combined = format!(
            "=== stdout ===\n{}\n=== stderr ===\n{}",
            result.stdout.text, result.stderr.text
        );
        std::fs::write(&absolute, combined).ok()?;
        let stager = crate::harness::artifact::ArtifactStager::new(&self.staging_root);
        let attempt_id = spec.work.lease.attempt_id.as_str();
        match stager.stage_file(attempt_id, workspace, &relative, "log", "text/plain") {
            Ok(staged) => Some(serde_json::json!({
                "kind": staged.kind,
                "name": staged.name,
                "media_type": staged.media_type,
                "size_bytes": staged.size_bytes,
                "sha256": staged.sha256,
                "staged_path": staged.staged_path.display().to_string(),
            })),
            Err(error) => {
                tracing::warn!(?error, harness = self.kind(), "run log could not be staged");
                None
            }
        }
    }
}

/// `Measured` when there is a value, `NotMeasured` when there is none: an
/// absent number is never reported as zero.
fn measurement<T>(value: Option<T>) -> Measurement<T> {
    Measurement {
        source: if value.is_some() {
            MeasurementSource::Measured
        } else {
            MeasurementSource::NotMeasured
        },
        value,
        additional: BTreeMap::new(),
    }
}

/// Whether `token` starts with at least two dot-separated digit groups
/// (`X.Y[.Z...]`) and, past those, either ends there or continues with a
/// suffix that itself starts with a letter and holds only letters, digits,
/// `.`, `-` or `+` (`0.2.0b1`). A suffix must *start* with a letter, so
/// `999.999.999-nightly-exotic-format`, whose digit groups are followed by
/// a `-`, is still not a version.
fn digit_groups_then_letter_suffix(token: &str) -> bool {
    let bytes = token.as_bytes();
    let mut index = 0;
    let mut groups = 0;
    loop {
        let start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if index == start {
            return false;
        }
        groups += 1;
        if index < bytes.len() && bytes[index] == b'.' {
            index += 1;
            continue;
        }
        break;
    }
    if groups < 2 {
        return false;
    }
    if index == bytes.len() {
        return true;
    }
    token[index..].starts_with(|ch: char| ch.is_ascii_alphabetic())
        && token[index..]
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '+')
}

/// Finds the version in a `--version` line: the leading token when it looks
/// like a version (`2.1.223`, `3.0.0-beta.1`), otherwise a later token that
/// is a plain `X.Y[.Z]` (`codex-cli 0.149.1`) or digit groups followed by a
/// letter-led suffix (`docket 0.2.0b1`). A line with neither has no version;
/// guessing one out of it would report a version nobody printed.
pub(crate) fn parse_version(output: &str) -> Option<&str> {
    let plain = |token: &str| {
        let parts: Vec<&str> = token.split('.').collect();
        (2..=3).contains(&parts.len())
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
    };
    let mut tokens = output.split_whitespace();
    let first = tokens.next()?;
    let leading = first.starts_with(|ch: char| ch.is_ascii_digit())
        && first
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '-');
    if leading {
        return Some(first);
    }
    tokens.find(|token| plain(token) || digit_groups_then_letter_suffix(token))
}

/// The pid inside a handle this core issued (`<pid>:<sequence>`). Also reads
/// a bare pid and a kind-prefixed one, the two forms a journal written
/// before the formats were unified can still hold.
fn handle_pid(process_id: &str) -> Option<u32> {
    process_id
        .split(':')
        .find_map(|part| part.parse::<u32>().ok())
}

fn rfc3339(time: std::time::SystemTime) -> String {
    DateTime::<Utc>::from(time).to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[async_trait]
impl<G, C> HarnessProbe for LocalProcessHarness<G, C>
where
    G: HarnessGrammar,
    C: Clock + Send + Sync,
{
    fn harness_kind(&self) -> DomainHarnessKind {
        DomainHarnessKind::new(self.kind())
    }

    async fn probe(&self) -> HarnessCapability {
        let descriptor = self.grammar.descriptor();
        let probed_at = DateTime::<Utc>::from(self.clock.now());
        let (installed_version, probe_error, raw_output) = self.detect_version().await;
        if !installed_version.is_empty() {
            *self.probed_version.lock().unwrap() = Some(installed_version.clone());
        }
        let mut additional: BTreeMap<String, serde_json::Value> = descriptor
            .probe_notes
            .iter()
            .map(|(key, note)| ((*key).to_owned(), serde_json::json!(note)))
            .collect();
        if let Some(raw) = raw_output {
            additional.insert("raw_version_output".to_owned(), serde_json::json!(raw));
        }
        HarnessCapability {
            harness_kind: DomainHarnessKind::new(descriptor.kind),
            installed_version,
            probe_error,
            probed_at,
            // No harness CLI here can list its models; schedulability rests
            // on the pass-through attestation instead.
            model_combinations: Vec::new(),
            model_passthrough: Some(capability(
                CapabilitySupport::Supported,
                descriptor.model_passthrough,
            )),
            // The grammar's own honest promise, not a second, possibly
            // divergent claim — `declared_capabilities` below reuses the
            // exact same call.
            decisions: Some(self.grammar.capabilities().decisions),
            additional,
        }
    }

    fn declared_capabilities(&self) -> FeatureCapabilities {
        self.grammar.capabilities()
    }
}

#[async_trait]
impl<G, C> HarnessAdapter for LocalProcessHarness<G, C>
where
    G: HarnessGrammar,
    C: Clock + Send + Sync,
{
    /// Builds the whole spawn and discards it, so anything `start` would
    /// refuse is refused here, before the attempt is announced.
    async fn validate(&self, spec: &ExecutionSpec) -> Result<(), HarnessError> {
        self.prepare(spec).map(|_| ())
    }

    async fn start(&self, spec: &ExecutionSpec) -> Result<LocalRunHandle, HarnessError> {
        let (process_spec, secrets, endpoint, scratch, stdin_stays_open) = self.prepare(spec)?;
        let process = process_spec.spawn().await.map_err(|error| {
            tracing::warn!(?error, harness = self.kind(), "spawn failed");
            HarnessError::Process
        })?;
        // The sequence keeps two handles apart if the OS reuses a pid while
        // an earlier entry is still tracked.
        let process_id = format!(
            "{}:{}",
            process.pid(),
            self.next_handle.fetch_add(1, Ordering::SeqCst)
        );
        // One pair carries a question out to the engine; the other carries
        // its answer back in. `decision_channels` hands out the engine-facing
        // ends once; `wait`'s interactive loop drives the process-facing ones.
        let (decision_channels, interactive) = if stdin_stays_open {
            let (questions_tx, questions_rx) = mpsc::channel(1);
            let (answers_tx, answers_rx) = mpsc::channel(1);
            (
                Some((questions_rx, answers_tx)),
                Some((questions_tx, answers_rx)),
            )
        } else {
            (None, None)
        };
        self.running.lock().await.insert(
            process_id.clone(),
            RunningProcess {
                process,
                record: RunRecord {
                    secrets,
                    limits: self.run_limits(spec),
                    started_at: DateTime::<Utc>::from(self.clock.now()),
                    spec: spec.clone(),
                    endpoint,
                    scratch,
                    interactive,
                },
                decision_channels,
            },
        );
        Ok(LocalRunHandle { process_id })
    }

    async fn decision_channels(
        &self,
        handle: &LocalRunHandle,
    ) -> Option<(mpsc::Receiver<Question>, mpsc::Sender<DecisionAnswer>)> {
        self.running
            .lock()
            .await
            .get_mut(&handle.process_id)?
            .decision_channels
            .take()
    }

    /// A signal that could not be delivered is `Ambiguous` evidence, not an
    /// error: the process may or may not still be running, and the engine's
    /// recovery path is what resolves that.
    async fn cancel(&self, handle: &LocalRunHandle) -> Result<CancellationEvidence, HarnessError> {
        let running = self.take_running(&handle.process_id).await?;
        let pid = running.process.pid();
        let (observation, process_outcome) = match running
            .process
            .cancel(running.record.limits.termination_grace)
            .await
        {
            Ok(CancelOutcome::Stopped) => (CancelObservation::ProcessStopped, "stopped"),
            Ok(CancelOutcome::Killed) => (CancelObservation::ProcessStopped, "killed"),
            Err(error) => {
                tracing::warn!(?error, harness = self.kind(), "cancel signal failed");
                (CancelObservation::Ambiguous, "signal_failed")
            }
        };
        self.remove_scratch(
            &running.record.scratch,
            running.record.spec.work.lease.attempt_id.as_str(),
        );
        Ok(CancellationEvidence {
            observation,
            observed_at: crate::client::Timestamp::new(rfc3339(self.clock.now())),
            details: serde_json::Map::from_iter([
                ("pid".to_owned(), serde_json::json!(pid)),
                (
                    "process_outcome".to_owned(),
                    serde_json::json!(process_outcome),
                ),
            ]),
        })
    }

    async fn wait(&self, handle: &LocalRunHandle) -> Result<HarnessOutcome, HarnessError> {
        let RunningProcess {
            process,
            mut record,
            ..
        } = self.take_running(&handle.process_id).await?;
        let result = if let Some((questions_tx, answers_rx)) = record.interactive.take() {
            let run_context = RunContext {
                spec: &record.spec,
                endpoint: record.endpoint.as_ref(),
                scratch: &record.scratch,
            };
            process
                .wait_with_capture_and_questions(
                    &record.limits,
                    &record.secrets,
                    |line| self.grammar.signal(&run_context, line),
                    |question, answer| self.grammar.answer(question, answer),
                    questions_tx,
                    answers_rx,
                )
                .await
        } else {
            process
                .wait_with_capture(&record.limits, &record.secrets)
                .await
        }
        .map_err(|error| {
            tracing::warn!(?error, harness = self.kind(), "capture failed");
            HarnessError::Process
        })?;
        let ended_at = DateTime::<Utc>::from(self.clock.now());
        let probed_version = self.known_version().await;
        let outcome = self.outcome(&record, ended_at, &result, probed_version);
        self.remove_scratch(&record.scratch, record.spec.work.lease.attempt_id.as_str());
        Ok(outcome)
    }

    async fn reconcile(
        &self,
        journal: &AttemptJournal,
    ) -> Result<RecoveryObservation, HarnessError> {
        let Some(process_id) = journal.process_id.as_deref() else {
            return Ok(RecoveryObservation::ProcessStopped);
        };
        let Some(pid) = handle_pid(process_id) else {
            tracing::warn!(process_id, harness = self.kind(), "unrecognized handle");
            return Err(HarnessError::RecoveryUnavailable);
        };
        #[cfg(unix)]
        {
            if !crate::harness::process::process_alive(pid) {
                return Ok(RecoveryObservation::ProcessStopped);
            }
            Ok(
                match self.process_is_this_harness(pid, journal.attempt_id.as_str()) {
                    Some(true) => RecoveryObservation::ProcessRunning,
                    // Alive, but another program: the attempt's process is gone
                    // and the OS has reused its pid.
                    Some(false) => RecoveryObservation::ProcessStopped,
                    None => RecoveryObservation::Ambiguous,
                },
            )
        }
        #[cfg(not(unix))]
        {
            let _ = pid;
            Ok(RecoveryObservation::Ambiguous)
        }
    }
}

#[cfg(test)]
#[path = "local_process/tests.rs"]
mod tests;
