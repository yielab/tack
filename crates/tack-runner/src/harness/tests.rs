use super::*;
use crate::client::journal::{JournalState, WorkspaceJournal};
use crate::client::{AttemptId, FencingToken, RunnerId, WorkspaceId, engine::HarnessError};
// `process::tests`'s own (`pub(crate)`) pidfile-poll helpers, reused
// below by the cross-adapter descendant-tree cancellation test rather
// than duplicated a third time.
use crate::harness::process::tests::{wait_for_pidfile, wait_until_dead};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tack_orch::execution::{CapabilityValue, RequestedModelId, RequestedModelProvider};

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

// ---- The real, reconciled adapters -------------------------------------
//
// Everything above this point tests dispatch/routing with trait-level
// fakes. These last tests are the two
// acceptance-gate proofs that need the
// *real* adapters (`codex::CodexAdapter`, `claude_code::ClaudeCodeAdapter`),
// not stand-ins: "the same fixture
// completes through both fake adapters" and "registration of both
// is order-independent." Each real adapter's own file already has
// its own exhaustive fixture-driven test suite;
// these two tests are deliberately narrow, cross-cutting proofs that
// only make sense here, where both are in scope together.

/// A scratch directory that removes itself, and everything written under
/// it, when the returned guard drops — including when an assertion panics
/// first.
fn cross_adapter_temp_dir(label: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(label)
        .tempdir()
        .expect("temporary directory")
}

/// A fresh, hermetic file-backed store per call — never the platform
/// keychain — so parallel `#[test]` functions never see each other's
/// entries and CI needs no Secret Service.
/// Takes the directory rather than making one, so the store's file cannot
/// outlive the guard that removes it. Each call gets its own file inside
/// that directory: two adapters in one test must not share a store.
fn cross_adapter_secret_store(dir: &std::path::Path) -> SecretStore {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    SecretStore::file(dir.join(format!("secrets-{n}.json")))
}

/// One deterministic fixture script, driven identically by both
/// real adapters. Never the shared `fake_harness_command()` — that
/// fixture is env-var-driven and single-purpose per spawn, which cannot
/// honestly answer a harness's own version/model-listing/run calls
/// (multiple purposes) in one adapter instance without also faking a
/// probe-only environment override this cross-cutting test has no
/// business reaching into. Branches on its own argv instead:
/// a literal `--version` token prints a clean version string; a literal
/// `models` token prints one deterministic `provider/model` line; anything
/// else (each adapter's real `run`/`exec`/`-p` invocation) prints a
/// fixed, non-JSON marker and exits 0 — which both
/// adapters' own `wait()` honestly classifies as `Succeeded` from the
/// exit code alone (Codex always does; Claude Code falls
/// back to exit-code classification when stdout does not parse as its
/// own structured output).
/// Returns the guard alongside the command: the script lives inside it,
/// so dropping it before the adapters run would delete the program they
/// are about to spawn.
fn cross_adapter_fixture_command() -> (PathBuf, Vec<String>, tempfile::TempDir) {
    let dir = cross_adapter_temp_dir("script");
    let script_path = dir.path().join("fixture.sh");
    let script = r#"#!/bin/sh
for arg in "$@"; do
  case "$arg" in
--version) echo "1.0.0"; exit 0 ;;
models) printf 'demo/model-a\n'; exit 0 ;;
  esac
done
echo "cross-adapter-fixture-complete"
exit 0
"#;
    std::fs::write(&script_path, script).expect("write cross-adapter fixture script");
    (
        PathBuf::from("/bin/sh"),
        vec![script_path.display().to_string()],
        dir,
    )
}

/// A real `CodexAdapter` driven by the shared fixture script.
fn fixture_codex_adapter(
    codex_program: &std::path::Path,
    codex_args: &[String],
    staging_root: &std::path::Path,
    secrets: &std::path::Path,
) -> crate::harness::codex::CodexAdapter {
    crate::harness::codex::CodexAdapter::for_fixture(
        codex_program.to_path_buf(),
        codex_args.to_vec(),
        staging_root.to_path_buf(),
        cross_adapter_secret_store(secrets),
    )
}

/// A real `ClaudeCodeAdapter`: the installed binary if `discover` finds
/// one, else the shared no-op fixture script — either way, a real adapter,
/// never a trait-level fake.
fn fixture_claude_code_adapter(
    secrets: &std::path::Path,
) -> crate::harness::claude_code::ClaudeCodeAdapter {
    crate::harness::claude_code::ClaudeCodeAdapter::discover(cross_adapter_secret_store(secrets))
        .unwrap_or_else(|_| {
            crate::harness::claude_code::ClaudeCodeAdapter::for_fixture(
                PathBuf::from("/bin/sh"),
                vec!["-c".to_owned(), "exit 0".to_owned()],
                cross_adapter_secret_store(secrets),
            )
        })
}

/// Dispatch itself, not only the registered-kind set, is order
/// independent. The fixture's requested model (provider "openai", id
/// "opaque/model-alpha") is unsupported by claude-code (unknown provider
/// family), so it rejects it pre-spawn regardless of registry order.
/// codex is a pass-through harness: it cannot independently verify a
/// model's identity, so it accepts any *explicit* provider/id pre-spawn
/// and defers the real check to the harness at run time — an accepted
/// dispatch, not a rejection, for any locator that resolves, which the
/// fixture command always does.
async fn assert_dispatch_is_order_independent(
    forward: &AdapterRegistry,
    backward: &AdapterRegistry,
) {
    for kind in ["codex", "claude-code"] {
        let spec = spec_requesting(kind);
        let forward_result = forward.validate(&spec).await;
        let backward_result = backward.validate(&spec).await;
        let expect_ok = kind == "codex";
        assert_eq!(
            forward_result.is_ok(),
            expect_ok,
            "kind {kind}: forward registry"
        );
        assert_eq!(
            backward_result.is_ok(),
            expect_ok,
            "kind {kind}: backward registry"
        );
    }
}

/// A fresh registry with both real adapters registered in `order` —
/// `BTreeMap`-keyed storage, so only dispatch (proven separately) could
/// possibly depend on registration order, never the registered-kind set.
fn adapter_registry_in_order(
    order: [&str; 2],
    codex_program: &std::path::Path,
    codex_args: &[String],
    staging_root: &std::path::Path,
    secrets: &std::path::Path,
) -> AdapterRegistry {
    let mut registry = AdapterRegistry::new();
    for kind in order {
        match kind {
            "codex" => registry.register_adapter(
                DomainHarnessKind::new("codex"),
                Box::new(fixture_codex_adapter(
                    codex_program,
                    codex_args,
                    staging_root,
                    secrets,
                )),
            ),
            "claude-code" => registry.register_adapter(
                DomainHarnessKind::new("claude-code"),
                Box::new(fixture_claude_code_adapter(secrets)),
            ),
            other => unreachable!("test fixture only knows codex/claude-code, got {other}"),
        };
    }
    registry
}

/// Both real adapters register as probes cleanly (registration itself
/// rejects an overclaiming probe, proven elsewhere) and both report
/// capabilities.
async fn assert_both_probes_register_and_report(
    codex_program: &std::path::Path,
    codex_args: &[String],
    staging_root: &std::path::Path,
    secrets: &std::path::Path,
) {
    let mut probe_registry = AdapterRegistry::new();
    probe_registry
        .register_probe(Box::new(fixture_codex_adapter(
            codex_program,
            codex_args,
            staging_root,
            secrets,
        )))
        .expect("codex probe registers cleanly");
    probe_registry
        .register_probe(Box::new(fixture_claude_code_adapter(secrets)))
        .expect("claude-code probe registers cleanly");

    let reports = probe_registry.capabilities().await;
    assert_eq!(reports.len(), 2, "both real probes registered");
    let mut kinds: Vec<&str> = reports.iter().map(|r| r.harness_kind.as_str()).collect();
    kinds.sort_unstable();
    assert_eq!(kinds, vec!["claude-code", "codex"]);
}

/// Builds a real `ExecutionSpec` for `kind`, reusing `claim.response.json`
/// exactly as `spec_requesting` above does, but with a real (existing)
/// workspace directory — the real adapters actually spawn a process
/// there, unlike the trait-level fakes above — and an explicit
/// provider/model pair valid for that specific adapter.
fn real_adapter_spec(
    kind: &str,
    provider: &str,
    model: &str,
    workspace_path: PathBuf,
) -> ExecutionSpec {
    real_adapter_spec_with_env(kind, provider, model, &[], workspace_path)
}

/// [`real_adapter_spec`] plus literal environment entries — for driving
/// the shared fake-harness fixture's own `TACK_FAKE_HARNESS_*` switches.
fn real_adapter_spec_with_env(
    kind: &str,
    provider: &str,
    model: &str,
    extra_env: &[(&str, &str)],
    workspace_path: PathBuf,
) -> ExecutionSpec {
    let claim: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../docs/contracts/runner-v1/claim.response.json"
    ))
    .expect("claim fixture");
    let mut request: tack_orch::execution::ExecutionRequestSnapshot =
        serde_json::from_value(claim["request"].clone()).expect("request fixture");
    request.requested_harness_kind = DomainHarnessKind::new(kind);
    request.requested_model_provider = Some(RequestedModelProvider::new(provider));
    request.requested_model_id = Some(RequestedModelId::new(model));
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
                issued_at: crate::client::Timestamp::new("2026-08-09T12:20:00Z"),
                expires_at: crate::client::Timestamp::new("2026-08-09T13:20:00Z"),
            },
            request,
            attempt,
        },
        workspace: crate::client::Workspace {
            attempt_id: AttemptId::new("attempt"),
            id: WorkspaceId::new("ws_cross_adapter"),
            path: workspace_path,
            base_revision: "revision".into(),
        },
    }
}

/// Runs `adapter` through validate/start/wait against the shared fixture
/// script and asserts it reaches `Succeeded` — the one lifecycle sequence
/// both real adapters below are proven against, identically.
async fn assert_adapter_completes_fixture(
    kind: &str,
    adapter: &dyn HarnessAdapter,
    provider: &str,
    model_id: &str,
    workspace: &std::path::Path,
) {
    let spec = real_adapter_spec(kind, provider, model_id, workspace.to_path_buf());
    adapter.validate(&spec).await.expect("validate");
    let handle = adapter.start(&spec).await.expect("start");
    let outcome = adapter.wait(&handle).await.expect("wait");
    assert_eq!(
        outcome.terminal_state,
        crate::client::AttemptState::Succeeded
    );
}

#[tokio::test]
async fn the_same_fixture_completes_through_both_real_adapters() {
    let secrets_dir = cross_adapter_temp_dir("secrets");
    let (program, args, _script_dir) = cross_adapter_fixture_command();
    let (codex, claude, _staging) = real_adapters_for(program, args, secrets_dir.path());

    let codex_workspace_dir = cross_adapter_temp_dir("codex-ws");
    assert_adapter_completes_fixture(
        "codex",
        &codex,
        "openai",
        "opaque/model-alpha",
        codex_workspace_dir.path(),
    )
    .await;

    let claude_workspace_dir = cross_adapter_temp_dir("claude-ws");
    assert_adapter_completes_fixture(
        "claude-code",
        &claude,
        "anthropic",
        "claude-fixture-model",
        claude_workspace_dir.path(),
    )
    .await;

    for workspace in [codex_workspace_dir.path(), claude_workspace_dir.path()] {
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }
}

/// Registers the two real adapters, and their probes, into two registries
/// in opposite orders: `BTreeMap`-keyed registration cannot let "who
/// registered first" become dispatch priority, proven empirically here for
/// both the registered-kind set and dispatch itself, plus each probe's
/// declared cancellation capability passing the registration-time ceiling.
#[tokio::test]
async fn registering_both_real_adapters_is_order_independent() {
    let secrets_dir = cross_adapter_temp_dir("order-secrets");
    let secrets = secrets_dir.path();
    let staging_root_dir = cross_adapter_temp_dir("order-artifacts");
    let staging_root = staging_root_dir.path();
    let (codex_program, codex_args, _codex_script_dir) = cross_adapter_fixture_command();

    let forward = adapter_registry_in_order(
        ["codex", "claude-code"],
        &codex_program,
        &codex_args,
        staging_root,
        secrets,
    );
    let backward = adapter_registry_in_order(
        ["claude-code", "codex"],
        &codex_program,
        &codex_args,
        staging_root,
        secrets,
    );
    assert_eq!(forward.registered_kinds(), backward.registered_kinds());
    assert_eq!(forward.registered_kinds(), vec!["claude-code", "codex"]);
    assert_dispatch_is_order_independent(&forward, &backward).await;

    // The registration-time gate: both real, now-reconciled probes
    // register cleanly (neither still claims `cancel: Supported`).
    assert_both_probes_register_and_report(&codex_program, &codex_args, staging_root, secrets)
        .await;
}

/// Both real adapters against one fixture command and secrets directory —
/// lifecycle mechanics belonging to `local_process.rs`, not either adapter
/// (audit §5 rule 1), each used to exist once per adapter file.
fn real_adapters_for(
    program: PathBuf,
    args: Vec<String>,
    secrets_dir: &std::path::Path,
) -> (
    crate::harness::codex::CodexAdapter,
    crate::harness::claude_code::ClaudeCodeAdapter,
    tempfile::TempDir,
) {
    let staging_dir = cross_adapter_temp_dir("codex-artifacts");
    let codex = crate::harness::codex::CodexAdapter::for_fixture(
        program.clone(),
        args.clone(),
        staging_dir.path().to_path_buf(),
        cross_adapter_secret_store(secrets_dir),
    );
    let claude = crate::harness::claude_code::ClaudeCodeAdapter::for_fixture(
        program,
        args,
        cross_adapter_secret_store(secrets_dir),
    );
    (codex, claude, staging_dir)
}

/// A disabled provider rejects at `validate`, pre-spawn, for both real
/// adapters — `provider::resolve_endpoint`'s own check, surfaced by the
/// shared `validate`. Replaces each adapter's own `*_disabled_provider_*` test.
#[tokio::test]
async fn disabled_provider_rejects_both_real_adapters_before_spawning() {
    let secrets_dir = cross_adapter_temp_dir("disabled-provider-secrets");
    let (program, args, _script_dir) = cross_adapter_fixture_command();
    let (codex, claude, _staging) = real_adapters_for(program, args, secrets_dir.path());
    let disabled_providers = BTreeMap::from([(
        crate::config::VERCEL_AI_GATEWAY_CONFIG_KEY.to_owned(),
        crate::config::ProviderConfig {
            enabled: false,
            secret: "demo-secret".to_owned(),
        },
    )]);
    let codex = codex.with_providers(disabled_providers.clone());
    let claude = claude.with_providers(disabled_providers);

    let adapters: [(&str, &dyn HarnessAdapter); 2] = [("codex", &codex), ("claude-code", &claude)];
    for (label, adapter) in adapters {
        let workspace_dir = cross_adapter_temp_dir("disabled-provider-ws");
        let spec = real_adapter_spec(
            label,
            crate::config::VERCEL_AI_GATEWAY_PROVIDER,
            "opaque/model-alpha",
            workspace_dir.path().to_path_buf(),
        );
        let error = adapter
            .validate(&spec)
            .await
            .expect_err("must reject a disabled provider pre-spawn");
        assert!(matches!(error, HarnessError::Rejected { .. }), "{label}");
    }
}

/// A cancel/wait on a handle never produced by that adapter is a typed
/// rejection, never a panic — `take_running`'s own shared bookkeeping.
#[tokio::test]
async fn cancel_and_wait_on_an_untracked_handle_are_typed_rejections() {
    let secrets_dir = cross_adapter_temp_dir("untracked-handle-secrets");
    let (program, args, _script_dir) = cross_adapter_fixture_command();
    let (codex, claude, _staging) = real_adapters_for(program, args, secrets_dir.path());
    let adapters: [&dyn HarnessAdapter; 2] = [&codex, &claude];
    let handles = [
        LocalRunHandle {
            process_id: "codex:999999:0".to_owned(),
        },
        LocalRunHandle {
            process_id: "999999".to_owned(),
        },
    ];

    for adapter in adapters {
        for handle in &handles {
            assert!(matches!(
                adapter.cancel(handle).await,
                Err(HarnessError::Process)
            ));
            assert!(matches!(
                adapter.wait(handle).await,
                Err(HarnessError::Process)
            ));
        }
    }
}

/// Cancel kills the whole descendant tree through both real adapters'
/// `start`/`cancel` — entirely shared `cancel()` plus `process.rs`'s
/// process-group signal (the primitive-level proof stays `process/tests.rs`'s).
#[tokio::test]
async fn both_adapters_route_cancel_to_kill_the_whole_descendant_tree() {
    let secrets_dir = cross_adapter_temp_dir("descendant-tree-secrets");
    let (fake_program, fake_args) = crate::harness::fixtures::fake_harness_command();
    let (codex, claude, _staging) = real_adapters_for(fake_program, fake_args, secrets_dir.path());

    let cases: [(&str, &dyn HarnessAdapter, &str, &str); 2] = [
        ("codex", &codex, "openai", "opaque/model-alpha"),
        ("claude-code", &claude, "anthropic", "claude-fixture-model"),
    ];
    for (kind, adapter, provider, model) in cases {
        let workspace_dir = cross_adapter_temp_dir("descendant-ws");
        let workspace = workspace_dir.path();
        let pidfile = workspace.join("grandchild.pid");
        let extra_env = [
            ("TACK_FAKE_HARNESS_MODE", "spawn_child"),
            (
                "TACK_FAKE_HARNESS_PIDFILE",
                pidfile.to_str().expect("utf8 pidfile path"),
            ),
            ("TACK_FAKE_HARNESS_SLEEP_SECONDS", "3600"),
        ];
        let spec =
            real_adapter_spec_with_env(kind, provider, model, &extra_env, workspace.to_path_buf());

        adapter.validate(&spec).await.expect("validate");
        let handle = adapter.start(&spec).await.expect("start");
        let grandchild_pid = wait_for_pidfile(&pidfile).await;
        assert!(
            crate::harness::process::process_alive(grandchild_pid),
            "{kind}: grandchild must be observed running before cancellation"
        );

        let evidence = adapter.cancel(&handle).await.expect("cancel");
        assert_eq!(
            evidence.observation,
            CancelObservation::ProcessStopped,
            "{kind}"
        );
        assert!(
            wait_until_dead(grandchild_pid, std::time::Duration::from_secs(5)).await,
            "{kind}: grandchild must be gone after the adapter cancels its parent"
        );
    }
}

/// Shared `reconcile()` handles three pid-independent cases identically for
/// both real adapters: no recorded process id, an undecodable handle, and
/// a decodable handle whose process already exited. A still-*alive* pid is
/// genuinely per-adapter and stays in each adapter's own file.
#[cfg(unix)]
#[tokio::test]
async fn reconcile_reports_shared_pid_plumbing_identically_for_both() {
    let secrets_dir = cross_adapter_temp_dir("reconcile-secrets");
    let (program, args, _script_dir) = cross_adapter_fixture_command();
    let (codex, claude, _staging) = real_adapters_for(program, args, secrets_dir.path());

    fn codex_handle(pid: u32) -> String {
        format!("codex:{pid}:0")
    }
    fn claude_handle(pid: u32) -> String {
        pid.to_string()
    }
    type ReconcileCase<'a> = (&'a str, &'a dyn HarnessAdapter, fn(u32) -> String);
    let cases: [ReconcileCase; 2] = [
        ("codex", &codex, codex_handle),
        ("claude-code", &claude, claude_handle),
    ];
    for (label, adapter, encode) in cases {
        // No pid at all, and an undecodable one, both need no liveness
        // dispatch — shared plumbing, before `decode_handle` even runs.
        assert_eq!(
            adapter.reconcile(&journal_with_process(None)).await,
            Ok(RecoveryObservation::ProcessStopped),
            "{label}: no recorded process id"
        );
        assert_eq!(
            adapter
                .reconcile(&journal_with_process(Some("not-a-pid-at-all")))
                .await,
            Err(HarnessError::RecoveryUnavailable),
            "{label}: undecodable process id"
        );

        // Decodable, but the process has already exited: spawn and reap a
        // real one so the pid is definitely dead, not a guessed sentinel.
        let mut dead = std::process::Command::new("true")
            .spawn()
            .expect("spawn true");
        let dead_pid = dead.id();
        let _ = dead.wait();
        assert_eq!(
            adapter
                .reconcile(&journal_with_process(Some(&encode(dead_pid))))
                .await,
            Ok(RecoveryObservation::ProcessStopped),
            "{label}: decodable but already-dead pid"
        );
    }
}
