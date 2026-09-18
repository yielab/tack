//! Harnesses: the CLIs a runner can execute an attempt with.
//!
//! [`HarnessAdapter`] is the per-attempt lifecycle the engine drives and
//! [`HarnessProbe`] reports what is installed. [`local_process`] implements
//! both for any CLI that runs as a child process; `codex.rs` and
//! `claude_code.rs` each add a descriptor and a grammar. [`AdapterRegistry`]
//! dispatches on a claimed attempt's harness kind, and [`discover`] builds
//! one from whatever this machine has installed.
//!
//! Adding a harness: write its module, then add one line to [`DESCRIPTORS`]
//! and one to [`discover`]. Provider wiring and `tack runner doctor` read
//! the descriptor.

pub mod artifact;
pub mod claude_code;
pub mod codex;
pub mod docket;
pub mod event_sink;
pub mod fixtures;
pub mod local_process;
pub mod locate;
pub mod process;
pub mod redact;
pub mod sha256;
#[cfg(test)]
pub(crate) mod test_support;

use std::{collections::BTreeMap, path::Path, sync::Arc};

use async_trait::async_trait;
use tack_orch::execution::{
    CapabilitySupport, ExecutionRequestSnapshot, FeatureCapabilities, HarnessCapability,
    HarnessKind as DomainHarnessKind,
};
use thiserror::Error;

use crate::secrets::SecretStore;

pub use crate::client::engine::{
    CancelObservation, CancellationEvidence, ExecutionSpec, HarnessAdapter, HarnessError,
    HarnessOutcome, LocalRunHandle,
};
pub use crate::client::{AttemptJournal, RecoveryObservation};

/// A closed vocabulary for `ActualExecution.model_observation_source`, a
/// bare `String` on the wire. Two independently implemented adapters
/// converged on the same three meanings (`codex.rs`'s
/// `"requested_not_confirmed"`, `claude_code.rs`'s `"harness_reported"`/
/// `"not_observed"`); this enum centralizes the three literals so a future
/// adapter cannot invent an incompatible fourth string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelObservationSource {
    /// The value was read directly from the harness's own output (e.g.
    /// Claude Code's `system`/`init` event `model` field).
    HarnessReported,
    /// The value echoes the operator's request; the harness gave no
    /// independent confirmation it actually used it.
    RequestedNotConfirmed,
    /// Neither an observation nor a request value exists to report (only
    /// reachable by an adapter that permits auto-selection at all).
    NotObserved,
}

impl ModelObservationSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HarnessReported => "harness_reported",
            Self::RequestedNotConfirmed => "requested_not_confirmed",
            Self::NotObserved => "not_observed",
        }
    }
}

/// Resolves `request.environment` into concrete `NAME=value` pairs, called
/// from both `validate` (fail pre-spawn) and `start` (build the real
/// environment). A literal `value` is used as-is; a `secret_reference`
/// resolves through `store`; either way it is registered with `secrets` so
/// it is redacted if it surfaces in captured output. An unresolvable
/// reference fails typed, naming only the reference.
pub(crate) fn resolve_environment(
    store: &SecretStore,
    request: &ExecutionRequestSnapshot,
    secrets: &mut redact::SecretMaterial,
) -> Result<BTreeMap<String, String>, HarnessError> {
    let mut resolved = BTreeMap::new();
    for (name, value) in &request.environment {
        match (&value.value, &value.secret_reference) {
            (Some(literal), _) => {
                secrets.register(literal.clone());
                resolved.insert(name.clone(), literal.clone());
            }
            (None, Some(reference)) => {
                let secret = store.resolve(reference).map_err(|error| {
                    tracing::warn!(
                        name = %name,
                        reference = %reference,
                        error = %error,
                        "secret_reference could not be resolved before spawn"
                    );
                    HarnessError::Rejected {
                        reason: format!("secret_reference_unresolved: {reference}"),
                    }
                })?;
                tracing::debug!(name = %name, reference = %reference, "secret_reference resolved");
                secrets.register(secret.expose().to_owned());
                resolved.insert(name.clone(), secret.expose().to_owned());
            }
            (None, None) => {}
        }
    }
    Ok(resolved)
}

/// Harness discovery/capability reporting, independent of any specific
/// claimed attempt — needed because `engine::HarnessAdapter`'s methods all
/// take an `ExecutionSpec`/`LocalRunHandle`, which only exist once a request
/// is claimed (ADR 0066 has the full rationale).
///
/// Capability honesty is inherited from the frozen
/// `tack_orch::execution::capabilities` types this trait returns:
/// [`HarnessCapability`] carries a nullable `probe_error` (a probe failure
/// is not this runner's own bug), and every `FeatureCapabilities` entry is a
/// `CapabilityValue { support, reason }` with three explicit levels, never a
/// bare `bool`. An implementation must fill in real reasons.
#[async_trait]
pub trait HarnessProbe: Send + Sync {
    /// The kind this probe reports for. Kept as a method rather than a
    /// separate registration key so a probe cannot be registered under a
    /// kind it does not itself believe it is reporting for.
    fn harness_kind(&self) -> DomainHarnessKind;

    /// Detects the installed version and reports capabilities. A probing
    /// failure (binary missing, unparseable version, ...) belongs in the
    /// returned `HarnessCapability.probe_error`, never an `Err`.
    async fn probe(&self) -> HarnessCapability;

    /// The per-feature support this adapter honestly promises, independent
    /// of any attempt. Each real adapter reuses exactly the computation
    /// `HarnessAdapter::wait` stamps onto `ActualExecution.capability_snapshot`
    /// after a process runs, so there is one source of truth, not two that
    /// could diverge.
    ///
    /// [`AdapterRegistry::register_probe`] calls this once, at registration,
    /// and refuses a probe whose declared `cancel` support exceeds
    /// [`PROCESS_GROUP_CANCEL_CEILING`] — a lying capability is caught here,
    /// before any attempt starts.
    fn declared_capabilities(&self) -> FeatureCapabilities;
}

/// The cancellation support ceiling for any [`HarnessProbe`] built on
/// `SupervisedProcess::cancel` (a process-group SIGTERM/SIGKILL, which
/// cannot reliably reach a descendant that starts its own session) — see
/// [`HarnessProbe::declared_capabilities`]. Not a blanket rule: an adapter
/// with a genuinely different mechanism could justify a higher one.
pub const PROCESS_GROUP_CANCEL_CEILING: CapabilitySupport = CapabilitySupport::Advisory;

/// A probe rejected at registration, before it can back a claimed attempt.
/// Kept distinct from [`HarnessError`] (a per-attempt error) since this is
/// a registration-time, whole-probe rejection.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HarnessRegistrationError {
    #[error(
        "probe for harness kind {kind:?} declares cancel support {support:?}, which exceeds \
         what the shared process-group cancellation primitive can honestly promise \
         ({ceiling:?}): a descendant that starts its own session or process group is not \
         reachable by a group-wide signal"
    )]
    OverclaimedCancelSupport {
        kind: String,
        support: CapabilitySupport,
        ceiling: CapabilitySupport,
    },
}

/// Every harness this build knows, in the order `tack runner doctor` lists
/// them. Which model wire reaches which harness is read from here.
pub const DESCRIPTORS: [&local_process::HarnessDescriptor; 3] = [
    &codex::DESCRIPTOR,
    &claude_code::DESCRIPTOR,
    &docket::DESCRIPTOR,
];

pub fn descriptor(kind: &str) -> Option<&'static local_process::HarnessDescriptor> {
    DESCRIPTORS
        .into_iter()
        .find(|descriptor| descriptor.kind == kind)
}

/// A registry of the harnesses installed on this machine, and why each
/// missing one is missing. A harness that cannot be located is not
/// registered, so a request for it is a typed "no adapter is registered"
/// rather than an attempt that could never run.
pub fn discover(
    limits: &process::ProcessLimits,
    staging_root: &Path,
    secrets: &SecretStore,
    providers: &BTreeMap<String, crate::config::ProviderConfig>,
) -> (AdapterRegistry, BTreeMap<String, String>) {
    let mut found = (AdapterRegistry::new(), BTreeMap::new());
    let machine = (limits, staging_root, secrets, providers);
    install(&mut found, machine, codex::CodexGrammar);
    install(&mut found, machine, claude_code::ClaudeCodeGrammar);
    install(&mut found, machine, docket::DocketGrammar);
    found
}

type Machine<'a> = (
    &'a process::ProcessLimits,
    &'a Path,
    &'a SecretStore,
    &'a BTreeMap<String, crate::config::ProviderConfig>,
);

fn install<G: local_process::HarnessGrammar>(
    (registry, missing): &mut (AdapterRegistry, BTreeMap<String, String>),
    (limits, staging_root, secrets, providers): Machine<'_>,
    grammar: G,
) {
    let kind = grammar.descriptor().kind;
    let harness = local_process::LocalProcessHarness::discover(
        grammar,
        limits.clone(),
        staging_root.to_path_buf(),
        secrets.clone(),
    );
    match harness {
        Ok(harness) => {
            if registry
                .register(harness.with_providers(providers.clone()))
                .is_err()
            {
                tracing::warn!(harness = kind, "rejected at registration");
            }
        }
        // The reason can name a filesystem path; the log says only that.
        Err(reason) => {
            tracing::info!(harness = kind, "binary not found; not registered");
            missing.insert(kind.to_owned(), reason);
        }
    }
}

/// Dispatches the frozen [`HarnessAdapter`] lifecycle across every
/// registered harness kind, and aggregates [`HarnessProbe`] reports. Keys
/// on `tack_orch::execution::HarnessKind`.
///
/// Implements [`HarnessAdapter`] itself, so `RunnerEngine::new(...)` is a
/// complete, multi-harness runner with no `engine.rs` changes: adding a
/// harness means registering it here, never a new engine type parameter.
#[derive(Default)]
pub struct AdapterRegistry {
    adapters: BTreeMap<String, Arc<dyn HarnessAdapter>>,
    probes: BTreeMap<String, Arc<dyn HarnessProbe>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self {
            adapters: BTreeMap::new(),
            probes: BTreeMap::new(),
        }
    }

    pub fn register_adapter(
        &mut self,
        kind: DomainHarnessKind,
        adapter: Box<dyn HarnessAdapter>,
    ) -> &mut Self {
        self.adapters
            .insert(kind.as_str().to_owned(), Arc::from(adapter));
        self
    }

    /// Registers one harness as both adapter and probe, so the version a
    /// probe finds is the version its runs are stamped with.
    pub fn register<H>(&mut self, harness: H) -> Result<&mut Self, HarnessRegistrationError>
    where
        H: HarnessAdapter + HarnessProbe + 'static,
    {
        let harness = Arc::new(harness);
        self.check_declared_cancel(harness.as_ref())?;
        let kind = harness.harness_kind().as_str().to_owned();
        self.adapters.insert(kind.clone(), harness.clone());
        self.probes.insert(kind, harness);
        Ok(self)
    }

    fn check_declared_cancel(
        &self,
        probe: &dyn HarnessProbe,
    ) -> Result<(), HarnessRegistrationError> {
        let declared = probe.declared_capabilities();
        if declared.cancel.support == CapabilitySupport::Supported
            && PROCESS_GROUP_CANCEL_CEILING != CapabilitySupport::Supported
        {
            return Err(HarnessRegistrationError::OverclaimedCancelSupport {
                kind: probe.harness_kind().as_str().to_owned(),
                support: declared.cancel.support,
                ceiling: PROCESS_GROUP_CANCEL_CEILING,
            });
        }
        Ok(())
    }

    /// Registers a probe, first checking its declared capabilities against
    /// [`PROCESS_GROUP_CANCEL_CEILING`]. An overclaiming probe is rejected
    /// here and never inserted, rather than discovered wrong only once a
    /// real cancellation fails to reach a detached descendant.
    pub fn register_probe(
        &mut self,
        probe: Box<dyn HarnessProbe>,
    ) -> Result<&mut Self, HarnessRegistrationError> {
        self.check_declared_cancel(probe.as_ref())?;
        self.probes
            .insert(probe.harness_kind().as_str().to_owned(), Arc::from(probe));
        Ok(self)
    }

    /// Harness kinds with a registered adapter, in deterministic
    /// `BTreeMap` order — so registration order never becomes dispatch priority.
    pub fn registered_kinds(&self) -> Vec<String> {
        self.adapters.keys().cloned().collect()
    }

    /// Probes every registered harness. Order is the same deterministic
    /// sorted order as [`Self::registered_kinds`], so a capability snapshot
    /// diff is never noise from iteration order alone.
    pub async fn capabilities(&self) -> Vec<HarnessCapability> {
        let mut reports = Vec::with_capacity(self.probes.len());
        for probe in self.probes.values() {
            reports.push(probe.probe().await);
        }
        reports
    }

    fn resolve(&self, kind: &str) -> Result<&dyn HarnessAdapter, HarnessError> {
        self.adapters
            .get(kind)
            .map(|boxed| boxed.as_ref())
            .ok_or_else(|| HarnessError::Rejected {
                reason: format!("no adapter is registered for harness kind {kind:?}"),
            })
    }
}

#[async_trait]
impl HarnessAdapter for AdapterRegistry {
    async fn validate(&self, spec: &ExecutionSpec) -> Result<(), HarnessError> {
        let kind = spec.work.request.requested_harness_kind.as_str();
        self.resolve(kind)?.validate(spec).await
    }

    async fn start(&self, spec: &ExecutionSpec) -> Result<LocalRunHandle, HarnessError> {
        let kind = spec.work.request.requested_harness_kind.as_str();
        let handle = self.resolve(kind)?.start(spec).await?;
        Ok(LocalRunHandle {
            process_id: encode_handle(kind, &handle.process_id),
        })
    }

    async fn cancel(&self, handle: &LocalRunHandle) -> Result<CancellationEvidence, HarnessError> {
        let (kind, inner) = decode_handle(&handle.process_id).ok_or(HarnessError::Process)?;
        self.resolve(&kind)?
            .cancel(&LocalRunHandle { process_id: inner })
            .await
    }

    async fn wait(&self, handle: &LocalRunHandle) -> Result<HarnessOutcome, HarnessError> {
        let (kind, inner) = decode_handle(&handle.process_id).ok_or(HarnessError::Process)?;
        self.resolve(&kind)?
            .wait(&LocalRunHandle { process_id: inner })
            .await
    }

    async fn reconcile(
        &self,
        journal: &AttemptJournal,
    ) -> Result<RecoveryObservation, HarnessError> {
        let Some(process_id) = journal.process_id.as_deref() else {
            // No process was ever confirmed running, so there is nothing
            // kind-specific left to check or dispatch.
            return Ok(RecoveryObservation::ProcessStopped);
        };
        let (kind, inner) = decode_handle(process_id).ok_or(HarnessError::RecoveryUnavailable)?;
        let mut delegated = journal.clone();
        delegated.process_id = Some(inner);
        self.resolve(&kind)?.reconcile(&delegated).await
    }
}

/// Embeds `kind` into an opaque handle string as `<hex(kind)>:<inner>`.
/// Hex-encoding the kind (not the inner id) means the inner process id can
/// contain any bytes, including a literal `:`, without ambiguity — the
/// split only ever happens on the *first* colon, mirroring the same
/// hex-encoding-for-safe-embedding convention `journal.rs`/`workspace.rs`
/// already use for attempt ids.
fn encode_handle(kind: &str, inner: &str) -> String {
    let hex_kind: String = kind
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("{hex_kind}:{inner}")
}

fn decode_handle(process_id: &str) -> Option<(String, String)> {
    let (hex_kind, inner) = process_id.split_once(':')?;
    if hex_kind.is_empty() || hex_kind.len() % 2 != 0 {
        return None;
    }
    let mut bytes = Vec::with_capacity(hex_kind.len() / 2);
    for pair in hex_kind.as_bytes().as_chunks::<2>().0 {
        let pair_str = std::str::from_utf8(pair).ok()?;
        bytes.push(u8::from_str_radix(pair_str, 16).ok()?);
    }
    let kind = String::from_utf8(bytes).ok()?;
    Some((kind, inner.to_owned()))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
