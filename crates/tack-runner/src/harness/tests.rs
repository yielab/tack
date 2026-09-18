use super::*;
use crate::client::journal::{JournalState, WorkspaceJournal};
use crate::client::{AttemptId, FencingToken, RunnerId, WorkspaceId, engine::HarnessError};
// `process::tests`'s own (`pub(crate)`) pidfile-poll helpers, reused
// below by the cross-adapter descendant-tree cancellation test rather
// than duplicated a third time.
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tack_orch::execution::CapabilityValue;

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

    async fn cancel(&self, handle: &LocalRunHandle) -> Result<CancellationEvidence, HarnessError> {
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
    let claude_calls = Arc::new(AtomicUsize::new(0));
    let mut registry = AdapterRegistry::new();
    registry.register_adapter(
        DomainHarnessKind::new("codex"),
        Box::new(TaggedFakeAdapter {
            tag: "codex",
            calls: Arc::clone(&codex_calls),
        }),
    );
    registry.register_adapter(
        DomainHarnessKind::new("claude-code"),
        Box::new(TaggedFakeAdapter {
            tag: "claude-code",
            calls: Arc::clone(&claude_calls),
        }),
    );
    (registry, codex_calls, claude_calls)
}

#[tokio::test]
async fn validate_and_start_dispatch_to_the_requested_kind_only() {
    let (registry, codex_calls, claude_calls) = registry_with_two_kinds();

    registry
        .validate(&spec_requesting("codex"))
        .await
        .expect("codex validate");
    assert_eq!(codex_calls.load(Ordering::SeqCst), 1);
    assert_eq!(claude_calls.load(Ordering::SeqCst), 0);

    let handle = registry
        .start(&spec_requesting("claude-code"))
        .await
        .expect("claude-code start");
    assert_eq!(codex_calls.load(Ordering::SeqCst), 1);
    assert_eq!(claude_calls.load(Ordering::SeqCst), 1);
    // The handle the engine sees is opaque; only this module's own
    // routing depends on its internal shape.
    assert!(handle.process_id.contains("claude-code-process"));
}

/// Acceptance-adjacent: proves the encode/decode workaround actually
/// routes a handle back to the *same* adapter that produced it, and
/// never to the other registered kind — the concrete failure mode the
/// missing `LocalRunHandle.harness_kind` field (documented above) would
/// otherwise risk.
#[tokio::test]
async fn cancel_and_wait_route_the_start_handle_to_its_adapter() {
    let (registry, codex_calls, claude_calls) = registry_with_two_kinds();

    let handle = registry
        .start(&spec_requesting("claude-code"))
        .await
        .expect("start");
    codex_calls.store(0, Ordering::SeqCst);
    claude_calls.store(0, Ordering::SeqCst);

    registry.cancel(&handle).await.expect("cancel");
    assert_eq!(claude_calls.load(Ordering::SeqCst), 1);
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
    claude_calls.store(0, Ordering::SeqCst);
    registry.wait(&codex_handle).await.expect("wait");
    assert_eq!(codex_calls.load(Ordering::SeqCst), 1);
    assert_eq!(claude_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn an_unregistered_kind_fails_pre_spawn_with_a_typed_rejection() {
    let (registry, _, _) = registry_with_two_kinds();

    assert!(matches!(
        registry
            .validate(&spec_requesting("unregistered-harness"))
            .await,
        Err(HarnessError::Rejected { .. })
    ));
    assert!(matches!(
        registry
            .start(&spec_requesting("unregistered-harness"))
            .await,
        Err(HarnessError::Rejected { .. })
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
    let (registry, codex_calls, claude_calls) = registry_with_two_kinds();
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
    assert_eq!(claude_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn an_undecodable_process_id_reconciles_as_unavailable() {
    let (registry, _, _) = registry_with_two_kinds();
    let journal = journal_with_process(Some("not-an-encoded-handle-at-all"));

    assert!(matches!(
        registry.reconcile(&journal).await,
        Err(HarnessError::RecoveryUnavailable)
    ));
}

#[tokio::test]
async fn registered_kinds_and_capabilities_sort_deterministically() {
    let (registry, _, _) = registry_with_two_kinds();
    assert_eq!(registry.registered_kinds(), vec!["claude-code", "codex"]);
}

struct FakeProbe {
    kind: &'static str,
    installed: bool,
    /// Configurable so the same fake can play both an
    /// honest probe (the default every pre-existing test below uses) and
    /// a deliberately lying one, for
    /// `registering_a_probe_that_overclaims_cancel_support_is_rejected_before_any_attempt_exists`.
    cancel_support: CapabilitySupport,
}

impl FakeProbe {
    fn honest(kind: &'static str, installed: bool) -> Self {
        Self {
            kind,
            installed,
            cancel_support: CapabilitySupport::Advisory,
        }
    }
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
            // Deliberately un-attested: the fake probe stands in
            // for the runner so scheduler tests can cover the
            // "no attestation" path; the real adapters each attest
            // explicitly.
            model_passthrough: None,
            additional: Default::default(),
        }
    }

    fn declared_capabilities(&self) -> FeatureCapabilities {
        fn unsupported(reason: &str) -> CapabilityValue {
            CapabilityValue {
                support: CapabilitySupport::Unsupported,
                reason: Some(reason.to_owned()),
                additional: Default::default(),
            }
        }
        FeatureCapabilities {
            cancel: CapabilityValue {
                support: self.cancel_support,
                reason: Some("fake probe for registry-dispatch tests".to_owned()),
                additional: Default::default(),
            },
            resume: unsupported("fake"),
            decisions: unsupported("fake"),
            artifacts: unsupported("fake"),
            usage: unsupported("fake"),
            additional: Default::default(),
        }
    }
}

#[tokio::test]
async fn capabilities_reports_an_honest_probe_error_not_fake_success() {
    let mut registry = AdapterRegistry::new();
    registry
        .register_probe(Box::new(FakeProbe::honest("codex", true)))
        .expect("an honest probe registers cleanly");
    registry
        .register_probe(Box::new(FakeProbe::honest("claude-code", false)))
        .expect("an honest probe registers cleanly");

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

/// Acceptance gate: "a lying capability is caught before invocation."
/// The shared
/// cancellation primitive (`harness::process::SupervisedProcess::cancel`,
/// a process-group SIGTERM/SIGKILL) cannot reliably reach a descendant a
/// harness's own shell-tool spawns into a new OS session — checking
/// with `ps` against real Claude Code found this. A probe that
/// nonetheless claims
/// `cancel: Supported` is rejected here, at registration — never
/// silently accepted only to be discovered wrong the first time a real
/// cancellation against a live attempt fails to reach a detached
/// descendant.
#[tokio::test]
async fn registering_a_probe_overclaiming_cancel_support_is_rejected() {
    let mut registry = AdapterRegistry::new();
    let lying = FakeProbe {
        kind: "lying-harness",
        installed: true,
        cancel_support: CapabilitySupport::Supported,
    };

    let error = match registry.register_probe(Box::new(lying)) {
        Ok(_) => panic!("a probe claiming Supported cancellation must be rejected"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        HarnessRegistrationError::OverclaimedCancelSupport {
            support: CapabilitySupport::Supported,
            ceiling: CapabilitySupport::Advisory,
            ..
        }
    ));

    // Never inserted: dispatch/capability reporting never sees it.
    assert!(registry.capabilities().await.is_empty());
}

#[test]
fn handle_encoding_round_trips_kinds_and_ids_containing_colons() {
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

// ---- the real grammars, through the registry ------------------------------

/// One script stands in for both CLIs: it answers `--version` and otherwise
/// succeeds. Each harness is registered once, as adapter and probe, and an
/// attempt started through the registry is waited on through it.
#[tokio::test]
async fn a_grammar_completes_an_attempt_through_the_registry() {
    use crate::harness::test_support::{clock, scratch, script, secret_store, spec};
    let state = scratch("registry");
    let body = "[ \"$1\" = --version ] && echo 1.0.0 && exit 0\necho fixture-complete";
    let harness = |grammar| {
        local_process::LocalProcessHarness::new(
            grammar,
            script(state.path(), body),
            clock(),
            process::ProcessLimits::new(1_000_000, 1_000_000, std::time::Duration::from_secs(10)),
            state.path().join("staging"),
            secret_store(state.path()),
        )
    };
    let mut registry = AdapterRegistry::new();
    registry
        .register(harness(codex::CodexGrammar))
        .expect("codex");

    let request = spec("codex", state.path());
    registry.validate(&request).await.expect("validate");
    let handle = registry.start(&request).await.expect("start");
    let outcome = registry.wait(&handle).await.expect("wait");
    assert_eq!(
        outcome.terminal_state,
        crate::client::AttemptState::Succeeded
    );
    assert_eq!(outcome.actual_execution.harness_kind.as_str(), "codex");
    assert_eq!(outcome.actual_execution.harness_version, "1.0.0");

    let probed: Vec<_> = registry.capabilities().await;
    assert_eq!(probed[0].installed_version, "1.0.0");
    assert_eq!(registry.registered_kinds(), ["codex"]);
}

#[test]
fn every_descriptor_is_found_by_its_kind() {
    for descriptor in DESCRIPTORS {
        assert!(std::ptr::eq(
            super::descriptor(descriptor.kind).expect("found"),
            descriptor
        ));
    }
    assert!(super::descriptor("no-such-harness").is_none());
}
