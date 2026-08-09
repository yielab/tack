//! Harness process/event infrastructure and the shared adapter-registration
//! seam (card III-D4).
//!
//! ## Correcting the card's premise: `HarnessAdapter` already exists
//!
//! This card was briefed on the premise that no `HarnessAdapter` trait
//! exists in the tree yet, and that defining one here was part of the card.
//! That premise does not match the checked-out tree: C3 (Wave 2, accepted at
//! integration SHA `f931fc0`) already added
//! [`crate::client::engine::HarnessAdapter`] to `engine.rs` — five methods
//! (`validate`/`start`/`cancel`/`wait`/`reconcile`) built specifically as
//! the seam later harness cards implement, down to its own doc comment:
//! *"C3 tests use a fake adapter and later harness cards implement this
//! contract in their own files without changing engine ownership."* C3's own
//! handoff says so explicitly too: *"D1–D3 implement `HarnessAdapter` in
//! adapter-owned files."* D1/D2/D3's task lists (`validate frozen spec`,
//! `cancel process tree`, `reconcile journal only when proven supported`,
//! ...) map onto that trait's five methods almost one-to-one.
//!
//! Given that, redefining a second, competing `HarnessAdapter` trait here
//! would be actively harmful — exactly the "three agents invent three
//! incompatible interfaces" outcome this card exists to prevent, just moved
//! one level up. So this module does **not** redefine it. Instead:
//!
//! - [`crate::client::engine::HarnessAdapter`] (re-exported for convenience
//!   as [`HarnessAdapter`] at this module's root) remains *the* frozen
//!   per-attempt lifecycle interface D1/D2/D3 implement, unchanged.
//! - This module supplies the genuinely missing piece: [`HarnessProbe`],
//!   for harness discovery/capability reporting, which has no home in
//!   `engine::HarnessAdapter` at all (see below for why that is a *separate*
//!   concern, not an oversight).
//! - [`AdapterRegistry`] is the "shared registry wiring" this card owns: a
//!   struct that itself implements `engine::HarnessAdapter` by dispatching
//!   to whichever concrete adapter matches a claimed attempt's requested
//!   harness kind. `RunnerEngine<P, A, W, C>` only ever needs one concrete
//!   `A: HarnessAdapter`; `AdapterRegistry` **is** that one `A`, so a runner
//!   can serve all three harnesses through a single engine instance with
//!   zero changes to `engine.rs`.
//! - [`process`] and [`event_sink`] are the lower-level primitives
//!   [`AdapterRegistry`]'s member adapters (D1/D2/D3's own structs) compose
//!   inside their `validate`/`start`/`cancel`/`wait` implementations.
//!
//! ## Why `HarnessProbe` is not a sixth `HarnessAdapter` method
//!
//! `engine::HarnessAdapter`'s five methods all take an `ExecutionSpec` or a
//! `LocalRunHandle` — both of which only exist once a request has been
//! claimed. Capability reporting (`RunnerCapabilities.harnesses`, populated
//! at enrollment/refresh — see `tack_orch::execution::capabilities`) has to
//! run *before* any attempt exists, to tell the scheduler what this runner
//! can even do. There is nowhere on the existing trait to hang that. Rather
//! than forcing capability discovery through a method that would need to
//! fabricate an `ExecutionSpec` to call, [`HarnessProbe`] is a small,
//! separate trait for exactly that: "detect version, report capabilities,
//! independent of any specific attempt."
//!
//! ## What D5 is expected to reconcile
//!
//! This is a starting point, not a final answer — three real adapters will
//! prove or falsify it:
//!
//! 1. **`HarnessProbe` vs. a sixth `HarnessAdapter` method.** If all three
//!    real adapters end up wanting to probe capabilities as a side effect of
//!    something already spec-shaped (e.g. `validate`), folding `probe` into
//!    `engine::HarnessAdapter` may turn out simpler than keeping two traits.
//!    Keep it separate only if capability discovery genuinely needs to run
//!    (e.g. at startup, before any request exists) independent of a claimed
//!    attempt.
//! 2. **`LocalRunHandle` cannot name its own harness kind — a real interface
//!    gap, not a style choice.** `cancel`/`wait` take only `&LocalRunHandle
//!    { process_id: String }`, with no harness-kind field, so a dispatching
//!    registry has no way to route a bare handle back to the adapter that
//!    produced it. [`AdapterRegistry`] works around this by encoding the
//!    kind into the opaque `process_id` string it hands back from `start`
//!    (see `encode_handle`/`decode_handle` below) and decoding it again in
//!    `cancel`/`wait`/`reconcile`. The straightforward fix — adding a
//!    `harness_kind` field to `LocalRunHandle` — was evaluated and
//!    deliberately **not** made here: `LocalRunHandle { process_id: ... }`
//!    is constructed by literal in exactly one place this card may not
//!    touch, `crates/tack-runner/tests/crash_matrix.rs:277` (C4-owned), and
//!    any new required field breaks that construction. This is reported as
//!    the falsifying fact rule 6 asks for: D5 is the only card that can
//!    coordinate an `engine.rs` + `crash_matrix.rs` change together, and
//!    should decide whether three real adapters make the field worth adding
//!    once the workaround's actual cost is visible.
//! 3. **Kind-key type duplication.** [`AdapterRegistry`] keys directly on
//!    `tack_orch::execution::HarnessKind` (an opaque string, matching
//!    `ExecutionRequestSnapshot::requested_harness_kind`). `registry.rs`
//!    (D5-owned) separately defines its own `HarnessKind` enum
//!    (`Codex`/`ClaudeCode`/`OpenCode`/`Other(String)`), left untouched by
//!    this card since `registry.rs` is not in D4's ownership list. Whether
//!    these two types should be unified, and whether `AdapterRegistry`
//!    itself belongs in `registry.rs` instead of here, is exactly the kind
//!    of registry-shape decision D5 owns.

pub mod artifact;
pub mod codex;
pub mod event_sink;
pub mod fixtures;
pub mod process;
pub mod redact;
pub mod sha256;

use std::collections::BTreeMap;

use async_trait::async_trait;
use tack_orch::execution::{HarnessCapability, HarnessKind as DomainHarnessKind};

pub use crate::client::engine::{
    CancelObservation, CancellationEvidence, ExecutionSpec, HarnessAdapter, HarnessError,
    HarnessOutcome, LocalRunHandle,
};
pub use crate::client::{AttemptJournal, RecoveryObservation};

/// Harness discovery/capability reporting, independent of any specific
/// claimed attempt. See the module docs for why this is not a sixth
/// [`HarnessAdapter`] method.
///
/// Capability honesty (rule 7) is inherited directly from the existing,
/// already-frozen `tack_orch::execution::capabilities` types this trait
/// returns: [`HarnessCapability`] carries a nullable `probe_error` (so
/// "this harness could not be probed, because X" is representable without
/// treating probe failure as this runner's own bug), and every entry in its
/// `FeatureCapabilities` is a `CapabilityValue { support, reason }` with
/// three explicit levels (`supported` / `unsupported` / `advisory`) — never
/// a bare `bool`, and never silently omitted to mean "no". A
/// [`HarnessProbe`] implementation must fill in real reasons, not leave them
/// `None` for convenience.
#[async_trait]
pub trait HarnessProbe: Send + Sync {
    /// The kind this probe reports for. Kept as a method rather than a
    /// separate registration key so a probe cannot be registered under a
    /// kind it does not itself believe it is reporting for.
    fn harness_kind(&self) -> DomainHarnessKind;

    /// Detects the installed version and reports capabilities. Probing
    /// itself can fail (binary missing, version string unparseable, ...);
    /// that failure belongs in the returned `HarnessCapability.probe_error`,
    /// never as an `Err` — an absent/broken installation is exactly as
    /// "successful" a probe result as a healthy one, just less capable.
    async fn probe(&self) -> HarnessCapability;
}

/// Dispatches the frozen [`HarnessAdapter`] lifecycle across every
/// registered harness kind, and aggregates [`HarnessProbe`] reports. This
/// **is** the "shared registry wiring" this card owns: D5 registers each of
/// D1/D2/D3's concrete adapters here (`registry.rs`'s own `HarnessRegistry`
/// stays untouched by this card — see the module docs on what D5 should
/// reconcile about the two).
///
/// Implements [`HarnessAdapter`] itself, so `RunnerEngine::new(protocol,
/// adapter_registry, journal, workspaces)` is a complete, multi-harness
/// runner with no `engine.rs` changes: `AdapterRegistry` simply **is** the
/// engine's one concrete adapter type parameter.
#[derive(Default)]
pub struct AdapterRegistry {
    adapters: BTreeMap<String, Box<dyn HarnessAdapter>>,
    probes: BTreeMap<String, Box<dyn HarnessProbe>>,
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
        self.adapters.insert(kind.as_str().to_owned(), adapter);
        self
    }

    pub fn register_probe(&mut self, probe: Box<dyn HarnessProbe>) -> &mut Self {
        self.probes
            .insert(probe.harness_kind().as_str().to_owned(), probe);
        self
    }

    /// Harness kinds with a registered adapter, in deterministic sorted
    /// order (`BTreeMap` iteration order), never insertion order — so which
    /// card registered first can never become accidental dispatch priority.
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
            .ok_or(HarnessError::Rejected)
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
            // No process was ever confirmed running for this attempt, for
            // any harness kind: there is nothing kind-specific left to
            // check, so this is the one case that needs no dispatch at all.
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
    for pair in hex_kind.as_bytes().chunks_exact(2) {
        let pair_str = std::str::from_utf8(pair).ok()?;
        bytes.push(u8::from_str_radix(pair_str, 16).ok()?);
    }
    let kind = String::from_utf8(bytes).ok()?;
    Some((kind, inner.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::journal::{JournalState, WorkspaceJournal};
    use crate::client::{AttemptId, FencingToken, RunnerId, WorkspaceId, engine::HarnessError};
    use std::{
        path::PathBuf,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    /// A trivial trait-level fake — deliberately not the fake *binary*: this
    /// module tests dispatch/routing logic, which does not need a real
    /// subprocess.
    struct TaggedFakeAdapter {
        tag: &'static str,
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl HarnessAdapter for TaggedFakeAdapter {
        async fn validate(&self, _spec: &ExecutionSpec) -> Result<(), HarnessError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn start(&self, _spec: &ExecutionSpec) -> Result<LocalRunHandle, HarnessError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(LocalRunHandle {
                process_id: format!("{}-process", self.tag),
            })
        }

        async fn cancel(
            &self,
            handle: &LocalRunHandle,
        ) -> Result<CancellationEvidence, HarnessError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            assert_eq!(handle.process_id, format!("{}-process", self.tag));
            Ok(CancellationEvidence {
                observation: CancelObservation::ProcessStopped,
                observed_at: crate::client::Timestamp::new("2026-08-06T12:24:00Z"),
                details: serde_json::Map::from_iter([("tag".into(), serde_json::json!(self.tag))]),
            })
        }

        async fn wait(&self, handle: &LocalRunHandle) -> Result<HarnessOutcome, HarnessError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            assert_eq!(handle.process_id, format!("{}-process", self.tag));
            Ok(HarnessOutcome {
                terminal_state: crate::client::AttemptState::Succeeded,
                terminal_reason: serde_json::json!({"tag": self.tag}),
                final_checkpoint: None,
                actual_execution: actual_execution(self.tag),
                usage: usage(),
            })
        }

        async fn reconcile(
            &self,
            journal: &AttemptJournal,
        ) -> Result<RecoveryObservation, HarnessError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            assert_eq!(
                journal.process_id.as_deref(),
                Some(format!("{}-process", self.tag)).as_deref()
            );
            Ok(RecoveryObservation::ProcessStopped)
        }
    }

    fn actual_execution(tag: &str) -> tack_orch::execution::ActualExecution {
        serde_json::from_value(serde_json::json!({
            "harness_kind": tag,
            "harness_version": "1.0.0",
            "model_provider": "test-provider",
            "model_id": "test-model",
            "model_observation_source": "harness_reported",
            "capability_snapshot": {
                "cancel": {"support": "supported", "reason": null},
                "resume": {"support": "unsupported", "reason": "no resume"},
                "decisions": {"support": "supported", "reason": null},
                "artifacts": {"support": "supported", "reason": null},
                "usage": {"support": "advisory", "reason": "partial"}
            },
            "workspace_id": "ws_test",
            "base_revision": "revision",
            "started_at": "2026-08-06T12:20:00Z",
            "ended_at": "2026-08-06T12:25:00Z"
        }))
        .expect("actual execution fixture")
    }

    fn usage() -> tack_orch::execution::Usage {
        serde_json::from_value(serde_json::json!({
            "tokens_in": {"value": 1, "source": "measured"},
            "tokens_out": {"value": 2, "source": "measured"},
            "duration_ms": {"value": 3, "source": "measured"},
            "cost_usd": {"value": null, "source": "not_measured"}
        }))
        .expect("usage fixture")
    }

    fn spec_requesting(kind: &str) -> ExecutionSpec {
        let claim: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../docs/contracts/runner-v1/claim.response.json"
        ))
        .expect("claim fixture");
        let mut request: tack_orch::execution::ExecutionRequestSnapshot =
            serde_json::from_value(claim["request"].clone()).expect("request fixture");
        request.requested_harness_kind = DomainHarnessKind::new(kind);
        let attempt: tack_orch::execution::AttemptSnapshot =
            serde_json::from_value(claim["attempt"].clone()).expect("attempt fixture");
        ExecutionSpec {
            work: crate::client::ClaimedWork {
                claim_request_id: crate::client::ClaimRequestId::new("claim"),
                lease: crate::client::AttemptLease {
                    attempt_id: AttemptId::new("attempt"),
                    runner_id: RunnerId::new("runner"),
                    fencing_token: FencingToken(1),
                    attempt_number: 1,
                    state: crate::client::AttemptState::Leased,
                    issued_at: crate::client::Timestamp::new("2026-08-06T12:20:00Z"),
                    expires_at: crate::client::Timestamp::new("2026-08-06T12:21:00Z"),
                },
                request,
                attempt,
            },
            workspace: crate::client::Workspace {
                attempt_id: AttemptId::new("attempt"),
                id: WorkspaceId::new("ws_test"),
                path: PathBuf::from("/tmp/does-not-matter"),
                base_revision: "revision".into(),
            },
        }
    }

    fn journal_with_process(process_id: Option<&str>) -> AttemptJournal {
        AttemptJournal {
            attempt_id: AttemptId::new("attempt"),
            runner_id: RunnerId::new("runner"),
            fencing_token: FencingToken(1),
            workspace: WorkspaceJournal {
                workspace_id: WorkspaceId::new("ws_test"),
                path: PathBuf::from("/tmp/does-not-matter"),
                base_revision: "revision".into(),
            },
            state: JournalState::ProcessObservedRunning,
            process_id: process_id.map(str::to_owned),
            last_event_checkpoint: None,
            pending_terminal_report: None,
        }
    }

    fn registry_with_two_kinds() -> (AdapterRegistry, Arc<AtomicUsize>, Arc<AtomicUsize>) {
        let codex_calls = Arc::new(AtomicUsize::new(0));
        let opencode_calls = Arc::new(AtomicUsize::new(0));
        let mut registry = AdapterRegistry::new();
        registry.register_adapter(
            DomainHarnessKind::new("codex"),
            Box::new(TaggedFakeAdapter {
                tag: "codex",
                calls: Arc::clone(&codex_calls),
            }),
        );
        registry.register_adapter(
            DomainHarnessKind::new("opencode"),
            Box::new(TaggedFakeAdapter {
                tag: "opencode",
                calls: Arc::clone(&opencode_calls),
            }),
        );
        (registry, codex_calls, opencode_calls)
    }

    #[tokio::test]
    async fn validate_and_start_dispatch_to_the_requested_kind_only() {
        let (registry, codex_calls, opencode_calls) = registry_with_two_kinds();

        registry
            .validate(&spec_requesting("codex"))
            .await
            .expect("codex validate");
        assert_eq!(codex_calls.load(Ordering::SeqCst), 1);
        assert_eq!(opencode_calls.load(Ordering::SeqCst), 0);

        let handle = registry
            .start(&spec_requesting("opencode"))
            .await
            .expect("opencode start");
        assert_eq!(codex_calls.load(Ordering::SeqCst), 1);
        assert_eq!(opencode_calls.load(Ordering::SeqCst), 1);
        // The handle the engine sees is opaque; only this module's own
        // routing depends on its internal shape.
        assert!(handle.process_id.contains("opencode-process"));
    }

    /// Acceptance-adjacent: proves the encode/decode workaround actually
    /// routes a handle back to the *same* adapter that produced it, and
    /// never to the other registered kind — the concrete failure mode the
    /// missing `LocalRunHandle.harness_kind` field (documented above) would
    /// otherwise risk.
    #[tokio::test]
    async fn cancel_and_wait_route_the_start_generated_handle_back_to_its_own_adapter() {
        let (registry, codex_calls, opencode_calls) = registry_with_two_kinds();

        let handle = registry
            .start(&spec_requesting("opencode"))
            .await
            .expect("start");
        codex_calls.store(0, Ordering::SeqCst);
        opencode_calls.store(0, Ordering::SeqCst);

        registry.cancel(&handle).await.expect("cancel");
        assert_eq!(opencode_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            codex_calls.load(Ordering::SeqCst),
            0,
            "must never reach the other kind"
        );

        let codex_handle = registry
            .start(&spec_requesting("codex"))
            .await
            .expect("start codex");
        codex_calls.store(0, Ordering::SeqCst);
        opencode_calls.store(0, Ordering::SeqCst);
        registry.wait(&codex_handle).await.expect("wait");
        assert_eq!(codex_calls.load(Ordering::SeqCst), 1);
        assert_eq!(opencode_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn an_unregistered_kind_fails_pre_spawn_with_a_typed_rejection() {
        let (registry, _, _) = registry_with_two_kinds();

        assert!(matches!(
            registry.validate(&spec_requesting("claude-code")).await,
            Err(HarnessError::Rejected)
        ));
        assert!(matches!(
            registry.start(&spec_requesting("claude-code")).await,
            Err(HarnessError::Rejected)
        ));
    }

    #[tokio::test]
    async fn reconcile_with_no_recorded_process_id_needs_no_dispatch() {
        let registry = AdapterRegistry::new(); // no adapters registered at all
        let observation = registry
            .reconcile(&journal_with_process(None))
            .await
            .expect("reconcile with no process id");
        assert_eq!(observation, RecoveryObservation::ProcessStopped);
    }

    #[tokio::test]
    async fn reconcile_decodes_the_kind_and_routes_to_the_right_adapter() {
        let (registry, codex_calls, opencode_calls) = registry_with_two_kinds();
        let handle = registry
            .start(&spec_requesting("codex"))
            .await
            .expect("start");
        codex_calls.store(0, Ordering::SeqCst);

        let mut journal = journal_with_process(Some(&handle.process_id));
        journal.process_id = Some(handle.process_id);
        let observation = registry.reconcile(&journal).await.expect("reconcile");
        assert_eq!(observation, RecoveryObservation::ProcessStopped);
        assert_eq!(codex_calls.load(Ordering::SeqCst), 1);
        assert_eq!(opencode_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn reconcile_with_an_undecodable_process_id_is_explicitly_unavailable_not_ambiguous_success()
     {
        let (registry, _, _) = registry_with_two_kinds();
        let journal = journal_with_process(Some("not-an-encoded-handle-at-all"));

        assert!(matches!(
            registry.reconcile(&journal).await,
            Err(HarnessError::RecoveryUnavailable)
        ));
    }

    #[tokio::test]
    async fn registered_kinds_and_capabilities_are_in_deterministic_sorted_order() {
        let (registry, _, _) = registry_with_two_kinds();
        assert_eq!(registry.registered_kinds(), vec!["codex", "opencode"]);
    }

    struct FakeProbe {
        kind: &'static str,
        installed: bool,
    }

    #[async_trait]
    impl HarnessProbe for FakeProbe {
        fn harness_kind(&self) -> DomainHarnessKind {
            DomainHarnessKind::new(self.kind)
        }

        async fn probe(&self) -> HarnessCapability {
            HarnessCapability {
                harness_kind: DomainHarnessKind::new(self.kind),
                installed_version: if self.installed {
                    "1.2.3".into()
                } else {
                    String::new()
                },
                probe_error: if self.installed {
                    None
                } else {
                    Some("not found on PATH".into())
                },
                probed_at: chrono::DateTime::parse_from_rfc3339("2026-08-06T12:00:00Z")
                    .expect("fixture timestamp")
                    .into(),
                model_combinations: Vec::new(),
                additional: Default::default(),
            }
        }
    }

    #[tokio::test]
    async fn capabilities_reports_an_honest_probe_error_for_an_uninstalled_harness_never_a_fake_success()
     {
        let mut registry = AdapterRegistry::new();
        registry.register_probe(Box::new(FakeProbe {
            kind: "codex",
            installed: true,
        }));
        registry.register_probe(Box::new(FakeProbe {
            kind: "claude-code",
            installed: false,
        }));

        let reports = registry.capabilities().await;
        assert_eq!(reports.len(), 2);
        let codex = reports
            .iter()
            .find(|report| report.harness_kind.as_str() == "codex")
            .expect("codex report");
        assert_eq!(codex.probe_error, None);
        let claude = reports
            .iter()
            .find(|report| report.harness_kind.as_str() == "claude-code")
            .expect("claude-code report");
        assert_eq!(claude.probe_error.as_deref(), Some("not found on PATH"));
    }

    #[test]
    fn handle_encoding_round_trips_kinds_and_process_ids_containing_colons() {
        let encoded = encode_handle("open:code", "pid:123:extra");
        let (kind, inner) = decode_handle(&encoded).expect("decode");
        assert_eq!(kind, "open:code");
        assert_eq!(inner, "pid:123:extra");
    }

    #[test]
    fn decode_handle_rejects_input_with_no_recognizable_encoding() {
        assert_eq!(decode_handle("no-colon-here"), None);
        assert_eq!(
            decode_handle("zz:inner"),
            None,
            "non-hex prefix is rejected"
        );
    }
}
