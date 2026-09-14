//! The shared local-process harness lifecycle.
//!
//! [`LocalProcessHarness<G>`] owns everything about running a harness CLI as
//! a local child process that does not depend on which vendor it is:
//! process bookkeeping (one in-flight
//! [`SupervisedProcess`](crate::harness::process::SupervisedProcess) per
//! opaque [`LocalRunHandle`]), ownership of that bookkeeping on
//! `cancel`/`wait`, reconciling a recorded pid across a restart, and the
//! shared `secrets`/`providers` an attempt's environment resolves against.
//! [`HarnessGrammar`] is the seam a concrete vendor (`codex.rs`,
//! `claude_code.rs`) fills in — command line, output classification,
//! capability claims and handle encoding — without changing this trait;
//! each hook below says which vendor asymmetry it exists for.

use std::{collections::BTreeMap, path::PathBuf};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tack_orch::execution::{
    CapabilityValue, FeatureCapabilities, HarnessCapability, HarnessKind as DomainHarnessKind,
    Measurement, MeasurementSource,
};

use crate::{
    Clock,
    config::ProviderConfig,
    harness::{
        AttemptJournal, CancelObservation, CancellationEvidence, ExecutionSpec, HarnessAdapter,
        HarnessError, HarnessOutcome, HarnessProbe, LocalRunHandle, RecoveryObservation,
        process::{CancelOutcome, ProcessError, ProcessLimits, ProcessResult, ProcessSpec},
        redact::SecretMaterial,
    },
    provider::ProviderEndpoint,
    secrets::SecretStore,
};

/// Everything a [`HarnessGrammar::prepare`] call hands back to
/// [`LocalProcessHarness::start`]: what to spawn, the redaction registry
/// captured output must be scrubbed against, per-run process limits (a
/// grammar may narrow the default timeout), and grammar-specific state
/// `wait`/`cancel` will need back (`G::RunState`).
pub struct PreparedRun<S> {
    pub process_spec: ProcessSpec,
    pub secrets: SecretMaterial,
    pub limits: ProcessLimits,
    pub state: S,
}

/// The vendor-specific seam. A [`LocalProcessHarness<G>`] owns the
/// lifecycle; `G` owns one harness CLI's command line, output and
/// capability claims. Every method here exists because at least two
/// harness CLIs already disagree about it, not because a future one might.
#[async_trait]
pub trait HarnessGrammar: Send + Sync + 'static {
    /// Per-attempt state [`HarnessGrammar::prepare`] computes and
    /// [`HarnessGrammar::outcome`]/cancel later consume — e.g. the resolved
    /// model provider/id, workspace facts, a cached harness version.
    type RunState: Send + Sync + 'static;

    /// The wire value this grammar reports for
    /// `requested_harness_kind`/`ActualExecution.harness_kind`.
    fn harness_kind(&self) -> DomainHarnessKind;

    /// Per-harness pre-spawn policy beyond kind-matching (which each
    /// implementation checks itself, since the check's own error text names
    /// the harness): a requested model/provider requirement, an allow-list,
    /// a self-contradictory request. Called from both `validate` and
    /// `start`, matching each existing adapter's own discard-and-recheck
    /// discipline.
    fn validate_selection(&self, spec: &ExecutionSpec) -> Result<(), HarnessError>;

    /// Resolves the concrete program and any fixed leading arguments this
    /// grammar would spawn, or a human-readable reason it cannot. Called
    /// from `validate` (result discarded) and, at each grammar's own
    /// discretion, from `prepare`.
    fn resolve_binary(&self) -> Result<(PathBuf, Vec<String>), String>;

    /// Resolves a configured provider endpoint for this request, or `None`
    /// when the request names the harness's own direct/native provider.
    /// Owns the per-grammar `Wire` and the "which provider name does this
    /// request carry" question. Called from both `validate` (result
    /// discarded) and `prepare` (endpoint actually injected).
    fn resolve_provider_endpoint(
        &self,
        spec: &ExecutionSpec,
        secrets: &SecretStore,
        providers: &BTreeMap<String, ProviderConfig>,
    ) -> Result<Option<ProviderEndpoint>, HarnessError>;

    /// Builds everything `start` needs to spawn: command line, environment,
    /// stdin, redaction registry, per-run limits, and `RunState` for later.
    /// Async because a grammar may need to detect its own installed version
    /// while preparing a run (codex's cache-or-detect fallback).
    async fn prepare(
        &self,
        spec: &ExecutionSpec,
        secrets: &SecretStore,
        providers: &BTreeMap<String, ProviderConfig>,
    ) -> Result<PreparedRun<Self::RunState>, HarnessError>;

    /// Encodes a freshly spawned pid into this grammar's own opaque handle
    /// format (codex disambiguates pid reuse with a monotonic counter;
    /// claude-code's is a bare pid string).
    fn encode_handle(&self, pid: u32) -> String;

    /// The inverse of [`Self::encode_handle`], or `None` if `process_id` is
    /// not this grammar's own encoding.
    fn decode_handle(&self, process_id: &str) -> Option<u32>;

    /// Maps a delivered-or-failed cancellation signal to the observation and
    /// detail payload `cancel` reports. Owns both the success-path detail
    /// shape (which differs per grammar) and the failure-path policy — one
    /// grammar may treat a failed signal as `Ambiguous`, another as a typed
    /// `Err` — matching each adapter's own historical choice.
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
    >;

    /// What a still-alive recorded pid means for this grammar. A grammar
    /// with an identity check (matching `/proc/<pid>/cmdline` against its
    /// own binary) can return `Ambiguous` when it cannot verify; one with no
    /// such check reports `ProcessRunning` unconditionally.
    fn reconcile_alive(&self, pid: u32) -> RecoveryObservation;

    /// What `reconcile` reports on a platform with no liveness primitive at
    /// all (non-Unix) — the two existing grammars disagree on the *value*,
    /// not just the reasoning, so this is not folded into a default.
    fn reconcile_unavailable(&self) -> Result<RecoveryObservation, HarnessError>;

    /// Classifies a completed run into a [`HarnessOutcome`]: terminal state,
    /// reason, staged artifact, usage. An exit-code classifier and a
    /// structured-output-stream classifier share no parsing logic — this
    /// hook exists so neither is forced through the other's shape.
    /// `cancelled` is `true` exactly when `cancel` already ran for this
    /// handle, tracked generically by [`LocalProcessHarness`] itself.
    fn outcome(
        &self,
        state: Self::RunState,
        started_at: DateTime<Utc>,
        ended_at: DateTime<Utc>,
        result: ProcessResult,
        cancelled: bool,
    ) -> HarnessOutcome;

    /// Detects the installed version, honestly: every failure mode folds
    /// into the `Option<String>` slot, matching [`HarnessProbe::probe`]'s
    /// contract that probing cannot fail. The `BTreeMap` carries whatever
    /// raw diagnostic a grammar wants attached; most leave it empty.
    async fn detect_version(&self)
    -> (String, Option<String>, BTreeMap<String, serde_json::Value>);

    /// Optional side effect after a version detection completes (from a
    /// direct `probe()` call): a grammar may cache the result to stamp
    /// `harness_version` at `start()` time without a redundant spawn.
    /// Default no-op — most grammars re-derive everything from a run's own
    /// output at `wait()` time instead.
    fn after_probe(&self, _version: &str, _error: Option<&str>) {}

    /// The pass-through attestation `probe()` reports in
    /// `HarnessCapability.model_passthrough`, or `None` if this grammar does
    /// not make one.
    fn model_passthrough(&self) -> Option<CapabilityValue>;

    /// The per-feature support this grammar honestly promises, independent
    /// of any specific attempt. Reused by `wait()` to stamp
    /// `ActualExecution.capability_snapshot`, exactly like each adapter did
    /// before this existed — one source of truth per grammar, not two.
    fn feature_capabilities(&self) -> FeatureCapabilities;
}

/// One in-flight (spawned, not yet reaped) attempt process, keyed by its own
/// encoded handle in [`LocalProcessHarness::running`]. `state` is the
/// grammar's own [`HarnessGrammar::RunState`] — everything `wait`/`cancel`
/// need that only `prepare` had access to.
pub(crate) struct RunningProcess<S> {
    process: crate::harness::process::SupervisedProcess,
    secrets: SecretMaterial,
    limits: ProcessLimits,
    started_at: DateTime<Utc>,
    state: S,
}

/// The shared local-process harness lifecycle: implements both
/// [`HarnessAdapter`] and [`HarnessProbe`] for any [`HarnessGrammar`] `G`,
/// so a concrete harness module supplies only `G` plus a thin constructor.
pub struct LocalProcessHarness<G: HarnessGrammar, C = crate::SystemClock> {
    /// `pub(crate)`: a grammar-specific builder outside this module (e.g.
    /// claude-code's `with_cancel_grace`) needs its own grammar's fields
    /// after construction, and a generic accessor would exist for that one
    /// caller alone.
    pub(crate) grammar: G,
    clock: C,
    /// Resolves `secret_reference` environment entries. Shared with every
    /// other adapter the runner constructed at startup — see
    /// `crate::secrets::SecretStore`.
    secrets: SecretStore,
    /// Configured provider endpoints (`RunnerConfig::providers`), consulted
    /// only when a request's `requested_model_provider` names one. Empty by
    /// default, meaning every request spawns against the harness's own
    /// native provider.
    providers: BTreeMap<String, ProviderConfig>,
    /// `pub(crate)` rather than private: a couple of `codex`'s own tests
    /// assert directly on "no bookkeeping was created" after a pre-spawn
    /// rejection, which is simpler proved against the real field than
    /// through an added accessor that would exist for no other caller.
    pub(crate) running: tokio::sync::Mutex<BTreeMap<String, RunningProcess<G::RunState>>>,
    /// Handles `cancel` has already run for; `wait` removes and reads this
    /// back so `outcome`'s `cancelled` parameter is honest. Never cleaned up
    /// if `wait` is never subsequently called for the same handle — a
    /// small, accepted leak, matching the pre-extraction claude-code
    /// adapter's own identical design.
    cancelled: tokio::sync::Mutex<std::collections::BTreeSet<String>>,
}

impl<G: HarnessGrammar, C: Clock> LocalProcessHarness<G, C> {
    pub fn new(grammar: G, clock: C, secrets: SecretStore) -> Self {
        Self {
            grammar,
            clock,
            secrets,
            providers: BTreeMap::new(),
            running: tokio::sync::Mutex::new(BTreeMap::new()),
            cancelled: tokio::sync::Mutex::new(std::collections::BTreeSet::new()),
        }
    }

    /// Configures the provider endpoints this harness may point a spawn at.
    /// Not part of `new` itself so every existing call site (fixtures,
    /// tests) keeps constructing a harness with no configured endpoint at
    /// all, exactly today's behavior, without editing each one.
    pub fn with_providers(mut self, providers: BTreeMap<String, ProviderConfig>) -> Self {
        self.providers = providers;
        self
    }

    async fn take_running(
        &self,
        process_id: &str,
    ) -> Result<RunningProcess<G::RunState>, HarnessError> {
        self.running.lock().await.remove(process_id).ok_or_else(|| {
            tracing::warn!(
                process_id,
                harness = %self.grammar.harness_kind().as_str(),
                "handle not tracked by this adapter instance"
            );
            HarnessError::Process
        })
    }
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
        self.grammar.harness_kind()
    }

    async fn probe(&self) -> HarnessCapability {
        let probed_at = DateTime::<Utc>::from(self.clock.now());
        let (installed_version, probe_error, additional) = self.grammar.detect_version().await;
        self.grammar
            .after_probe(&installed_version, probe_error.as_deref());
        HarnessCapability {
            harness_kind: self.grammar.harness_kind(),
            installed_version,
            probe_error,
            probed_at,
            // Neither grammar implemented against this trait enumerates a
            // model list today (see each grammar's own `model_passthrough`
            // for why schedulability does not need one); a future grammar
            // that can would be the first caller to justify turning this
            // into a hook.
            model_combinations: Vec::new(),
            model_passthrough: self.grammar.model_passthrough(),
            additional,
        }
    }

    fn declared_capabilities(&self) -> FeatureCapabilities {
        self.grammar.feature_capabilities()
    }
}

#[async_trait]
impl<G, C> HarnessAdapter for LocalProcessHarness<G, C>
where
    G: HarnessGrammar,
    C: Clock + Send + Sync,
{
    async fn validate(&self, spec: &ExecutionSpec) -> Result<(), HarnessError> {
        self.grammar.validate_selection(spec)?;
        self.grammar.resolve_binary().map_err(|reason| {
            tracing::warn!(
                reason,
                harness = %self.grammar.harness_kind().as_str(),
                "validate: binary unresolvable"
            );
            HarnessError::Rejected { reason }
        })?;
        // Every `secret_reference` entry must resolve before the harness
        // process exists. This discards the resolved values — `prepare`
        // resolves again for real. The engine has already journaled and
        // announced the attempt by now, and turns a refusal here into a
        // reported failure rather than an abandoned lease.
        super::resolve_environment(
            &self.secrets,
            &spec.work.request,
            &mut SecretMaterial::new(),
        )?;
        // Same discard-and-recheck discipline, for a configured provider
        // endpoint.
        self.grammar
            .resolve_provider_endpoint(spec, &self.secrets, &self.providers)?;
        Ok(())
    }

    async fn start(&self, spec: &ExecutionSpec) -> Result<LocalRunHandle, HarnessError> {
        self.grammar.validate_selection(spec)?;
        let prepared = self
            .grammar
            .prepare(spec, &self.secrets, &self.providers)
            .await?;

        let supervised = prepared.process_spec.spawn().await.map_err(|error| {
            tracing::warn!(
                ?error,
                harness = %self.grammar.harness_kind().as_str(),
                "start: spawn failed"
            );
            HarnessError::Process
        })?;
        let pid = supervised.pid();
        let handle_id = self.grammar.encode_handle(pid);
        let started_at = DateTime::<Utc>::from(self.clock.now());

        self.running.lock().await.insert(
            handle_id.clone(),
            RunningProcess {
                process: supervised,
                secrets: prepared.secrets,
                limits: prepared.limits,
                started_at,
                state: prepared.state,
            },
        );

        Ok(LocalRunHandle {
            process_id: handle_id,
        })
    }

    async fn cancel(&self, handle: &LocalRunHandle) -> Result<CancellationEvidence, HarnessError> {
        let running = self.take_running(&handle.process_id).await?;
        self.cancelled
            .lock()
            .await
            .insert(handle.process_id.clone());
        let pid = running.process.pid();
        let signal_result = running
            .process
            .cancel(running.limits.termination_grace)
            .await;
        let (observation, details) = self.grammar.cancel_outcome(pid, signal_result)?;
        Ok(CancellationEvidence {
            observation,
            observed_at: crate::client::Timestamp::new(rfc3339(self.clock.now())),
            details,
        })
    }

    async fn wait(&self, handle: &LocalRunHandle) -> Result<HarnessOutcome, HarnessError> {
        let running = self.take_running(&handle.process_id).await?;
        let cancelled = self.cancelled.lock().await.remove(&handle.process_id);
        let result = running
            .process
            .wait_with_capture(&running.limits, &running.secrets)
            .await
            .map_err(|error| {
                tracing::warn!(
                    ?error,
                    harness = %self.grammar.harness_kind().as_str(),
                    "wait: capture failed"
                );
                HarnessError::Process
            })?;
        let ended_at = DateTime::<Utc>::from(self.clock.now());
        Ok(self.grammar.outcome(
            running.state,
            running.started_at,
            ended_at,
            result,
            cancelled,
        ))
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
        let Some(pid) = self.grammar.decode_handle(process_id) else {
            tracing::warn!(
                process_id,
                harness = %self.grammar.harness_kind().as_str(),
                "reconcile: unrecognized handle encoding"
            );
            return Err(HarnessError::RecoveryUnavailable);
        };

        #[cfg(unix)]
        {
            if crate::harness::process::process_alive(pid) {
                Ok(self.grammar.reconcile_alive(pid))
            } else {
                Ok(RecoveryObservation::ProcessStopped)
            }
        }
        #[cfg(not(unix))]
        {
            let _ = pid;
            self.grammar.reconcile_unavailable()
        }
    }
}

/// A [`tack_orch::execution::Measurement`] whose source is honestly
/// `NotMeasured` — shared because "cost is never measured yet" is the one
/// usage fact every local-process grammar agrees on, not because usage
/// itself is shared (it is not: see each grammar's own `outcome`).
pub fn not_measured<T>() -> Measurement<T> {
    Measurement {
        value: None,
        source: MeasurementSource::NotMeasured,
        additional: BTreeMap::new(),
    }
}

/// The resolve-and-reject-typed shape both existing grammars' own
/// `resolve_provider_endpoint` wrap identically around
/// `provider::resolve_endpoint` — only the provider name (required for
/// codex, optional for claude-code) and the `Wire` variant are genuinely
/// per-grammar, extracted by the caller before this runs.
pub(crate) fn resolve_provider_endpoint(
    provider: &str,
    secrets: &SecretStore,
    providers: &BTreeMap<String, ProviderConfig>,
    wire: crate::provider::Wire,
    harness_kind: &str,
) -> Result<Option<ProviderEndpoint>, HarnessError> {
    crate::provider::resolve_endpoint(providers, secrets, provider, wire).map_err(|error| {
        let reason = error.to_string();
        tracing::warn!(
            reason,
            harness = harness_kind,
            "rejecting a request whose provider endpoint could not be resolved"
        );
        HarnessError::Rejected { reason }
    })
}

/// Stages the (already-scrubbed) combined stdout/stderr as a `log`
/// artifact under `staging_root`, via [`crate::harness::artifact::ArtifactStager`]
/// — the one piece of `outcome()` genuinely identical between every
/// existing grammar; only the staging root and the log's own filename
/// differ. Best-effort: a staging failure returns `None`, never a hard
/// error — each grammar's own `outcome()` treats that as "no `artifact`
/// key," never as a failed attempt.
pub(crate) fn stage_run_log(
    staging_root: &std::path::Path,
    workspace_path: &std::path::Path,
    attempt_id: &str,
    log_filename: &str,
    stdout: &str,
    stderr: &str,
    harness_kind: &str,
) -> Option<serde_json::Value> {
    let relative = PathBuf::from(".tack-runner").join(log_filename);
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

    let stager = crate::harness::artifact::ArtifactStager::new(staging_root);
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
            tracing::warn!(
                ?error,
                harness = harness_kind,
                "wait: artifact staging failed"
            );
            None
        }
    }
}
