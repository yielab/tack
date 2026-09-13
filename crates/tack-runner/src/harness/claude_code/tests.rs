use super::*;
use crate::client::{
    AttemptId, AttemptLease, ClaimRequestId, ClaimedWork, FencingToken, RunnerId, Workspace,
    WorkspaceId as ClientWorkspaceId,
};
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

/// A scratch directory that removes itself, and everything written under
/// it, when the returned guard drops — including when an assertion panics
/// first. Whatever holds a path into it must hold the guard too.
fn temp_workspace(label: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(label)
        .tempdir()
        .expect("temporary directory")
}

fn fake_binary() -> HarnessBinary {
    let (program, args) = super::super::fixtures::fake_harness_command();
    HarnessBinary {
        program,
        prefix_args: args,
    }
}

/// Takes the directory rather than making one, so the store's file cannot
/// outlive the guard that removes it. File-backed, never the platform
/// keychain, so parallel tests never see each other's entries and CI
/// needs no Secret Service.
fn test_secret_store(dir: &Path) -> crate::secrets::SecretStore {
    crate::secrets::SecretStore::file(dir.join("secrets.json"))
}

/// Returns the adapter with the scratch directory its secret store lives
/// in: drop the guard and the adapter is pointing at nothing.
fn adapter_with_fake_binary() -> (ClaudeCodeAdapter<FixedClock>, tempfile::TempDir) {
    let scratch = temp_workspace("secrets");
    let adapter =
        ClaudeCodeAdapter::with_binary(fake_binary(), clock(), test_secret_store(scratch.path()))
            .with_cancel_grace(Duration::from_millis(150));
    (adapter, scratch)
}

fn adapter_with_fake_binary_and_secrets(
    secrets: crate::secrets::SecretStore,
) -> ClaudeCodeAdapter<FixedClock> {
    ClaudeCodeAdapter::with_binary(fake_binary(), clock(), secrets)
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

fn env_entry(value: &str) -> EnvironmentValue {
    EnvironmentValue {
        value: Some(value.to_string()),
        secret_reference: None,
        additional: Default::default(),
    }
}

fn secret_reference_entry(reference: &str) -> EnvironmentValue {
    EnvironmentValue {
        value: None,
        secret_reference: Some(reference.to_string()),
        additional: Default::default(),
    }
}

// ---- validate: pre-spawn acceptance/rejection -------------------------

/// One row per spec this grammar's own `validate_selection` must accept:
/// the baseline well-formed spec, and every known provider family,
/// case-insensitively.
fn accept_cases() -> Vec<Option<&'static str>> {
    vec![
        None,
        Some("anthropic"),
        Some("BEDROCK"),
        Some("Vertex"),
        Some("foundry"),
    ]
}

#[tokio::test]
async fn validate_accepts_well_formed_specs() {
    let (adapter, _scratch) = adapter_with_fake_binary();
    for provider in accept_cases() {
        let workspace_dir = temp_workspace("validate-ok");
        let workspace = workspace_dir.path();
        let tools: &[&str] = if provider.is_none() { &["Read"] } else { &[] };
        let spec = spec_with(
            "claude-code",
            provider,
            tools,
            true,
            BTreeMap::new(),
            workspace.to_path_buf(),
        );
        assert!(
            adapter.validate(&spec).await.is_ok(),
            "provider {provider:?} should be accepted"
        );
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }
}

/// One row per pre-spawn rejection reason this grammar's own
/// `validate_selection`/`resolve_binary` can produce; each must reject
/// with a typed `HarnessError::Rejected` and leave no bookkeeping entry
/// behind. The harness-kind mismatch row also exercises shared
/// `local_process::validate` plumbing, not just this grammar.
struct RejectCase {
    name: &'static str,
    harness_kind: &'static str,
    provider: Option<&'static str>,
    tools: &'static [&'static str],
    network: bool,
    binary: Option<HarnessBinary>,
}

fn reject_cases() -> Vec<RejectCase> {
    vec![
        RejectCase {
            name: "unsupported model provider",
            harness_kind: "claude-code",
            provider: Some("openai"),
            tools: &[],
            network: true,
            binary: None,
        },
        RejectCase {
            name: "different harness kind",
            harness_kind: "codex",
            provider: None,
            tools: &[],
            network: true,
            binary: None,
        },
        RejectCase {
            name: "network tool while network denied",
            harness_kind: "claude-code",
            provider: None,
            tools: &["WebFetch"],
            network: false,
            binary: None,
        },
        RejectCase {
            name: "resolved binary no longer exists",
            harness_kind: "claude-code",
            provider: None,
            tools: &[],
            network: true,
            binary: Some(HarnessBinary {
                program: PathBuf::from("/nonexistent/definitely/not/claude"),
                prefix_args: Vec::new(),
            }),
        },
    ]
}

#[tokio::test]
async fn validate_rejects_invalid_specs() {
    for case in reject_cases() {
        let scratch = temp_workspace("secrets");
        let store = test_secret_store(scratch.path());
        let adapter =
            ClaudeCodeAdapter::with_binary(case.binary.unwrap_or_else(fake_binary), clock(), store);
        let workspace_dir = temp_workspace("validate-rejects");
        let workspace = workspace_dir.path();
        let spec = spec_with(
            case.harness_kind,
            case.provider,
            case.tools,
            case.network,
            BTreeMap::new(),
            workspace.to_path_buf(),
        );
        assert!(
            matches!(
                adapter.validate(&spec).await,
                Err(HarnessError::Rejected { .. })
            ),
            "case {:?} should be rejected",
            case.name
        );
        assert!(
            adapter.running.lock().await.is_empty(),
            "case {:?}: a pre-spawn rejection must never create process bookkeeping",
            case.name
        );
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }
}

/// Acceptance: a `secret_reference` the store cannot resolve fails at
/// `validate` with a typed reason naming only the reference, before the
/// adapter does anything else — proven here at the adapter boundary
/// (`validate` itself never touches a filesystem path outside checking
/// its own binary exists).
#[tokio::test]
async fn validate_rejects_a_missing_secret_reference_untouched() {
    let workspace_dir = temp_workspace("secret-reference-missing");
    let workspace = workspace_dir.path();
    std::fs::write(workspace.join("sentinel.txt"), b"before").expect("seed workspace");

    let state_guard = temp_workspace("secret-missing-state");
    // Deliberately not created: a failed lookup must not bring the file
    // fallback's directory into existence just by trying.
    let state_dir = state_guard.path().join("state");
    let store = crate::secrets::SecretStore::file(state_dir.join("secrets.json"));

    let adapter = adapter_with_fake_binary_and_secrets(store);
    let mut environment = BTreeMap::new();
    environment.insert(
        "SECRET_VAR".to_string(),
        secret_reference_entry("does-not-exist"),
    );
    let spec = spec_with(
        "claude-code",
        None,
        &[],
        true,
        environment,
        workspace.to_path_buf(),
    );

    let error = adapter
        .validate(&spec)
        .await
        .expect_err("a missing secret_reference must fail pre-spawn");
    assert!(
        matches!(
            &error,
            HarnessError::Rejected { reason }
                if reason.starts_with("secret_reference_unresolved:")
                    && reason.contains("does-not-exist")
        ),
        "unexpected error: {error:?}"
    );

    assert!(
        !state_dir.exists(),
        "a rejected validate must not create the secret store's state directory"
    );
    assert_eq!(
        std::fs::read_to_string(workspace.join("sentinel.txt")).expect("sentinel survives"),
        "before",
        "a rejected validate must not modify the workspace it was given"
    );
    assert_eq!(
        std::fs::read_dir(workspace)
            .expect("read workspace")
            .count(),
        1,
        "a rejected validate must not add files to the workspace it was given"
    );

    std::fs::remove_dir_all(workspace).expect("cleanup");
}

// ---- fake-binary-driven lifecycle tests ------------------------------

/// One row per exit-code-fallback classification this grammar's `outcome`
/// falls back to when no `stream-json` result envelope was produced —
/// exercised end to end through the real shared fixture (spawn, capture,
/// redact, parse), never a structured-result parse.
struct FallbackCase {
    name: &'static str,
    mode: &'static str,
    exit_code: Option<&'static str>,
    expected_states: &'static [AttemptState],
    expected_reason: Option<&'static str>,
    extra: fn(&HarnessOutcome),
}

fn fallback_cases() -> Vec<FallbackCase> {
    vec![
        FallbackCase {
            name: "success",
            mode: "success",
            exit_code: None,
            expected_states: &[AttemptState::Succeeded],
            expected_reason: Some(
                "no structured result envelope was produced; inferred success from exit code 0",
            ),
            extra: |outcome| {
                assert_eq!(outcome.actual_execution.model_id.as_str(), UNOBSERVED_MODEL);
                assert_eq!(
                    outcome.usage.tokens_in.source,
                    MeasurementSource::NotMeasured
                );
            },
        },
        FallbackCase {
            name: "failure",
            mode: "failure",
            exit_code: Some("7"),
            expected_states: &[AttemptState::Failed],
            expected_reason: Some(
                "no structured result envelope was produced; inferred failure from a non-zero \
                 exit code",
            ),
            extra: |_outcome| {},
        },
        FallbackCase {
            name: "malformed",
            mode: "malformed",
            exit_code: None,
            // The fixture's `malformed` mode exits 0, landing on the
            // success side of the fallback (same as plain `success`) —
            // any non-JSON generic output collapses into one fallback
            // rather than being distinguished from `success` by content.
            expected_states: &[AttemptState::Succeeded, AttemptState::Failed],
            expected_reason: None,
            extra: |outcome| {
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
            },
        },
    ]
}

/// Acceptance: this proves this never panics and never fabricates a
/// confident structured result, only the honest, clearly-labeled
/// exit-code fallback — see the module docs on `parse_run_output` for
/// why *any* non-JSON generic fake-binary output collapses into that one
/// fallback rather than being distinguished from `success` by content
/// alone.
#[tokio::test]
async fn fake_binary_exit_code_fallback_reports_honest_terminal_state() {
    for case in fallback_cases() {
        let (adapter, _scratch) = adapter_with_fake_binary();
        let workspace_dir = temp_workspace("fake-fallback");
        let workspace = workspace_dir.path();
        let mut environment = BTreeMap::new();
        environment.insert("TACK_FAKE_HARNESS_MODE".to_string(), env_entry(case.mode));
        if let Some(code) = case.exit_code {
            environment.insert("TACK_FAKE_HARNESS_EXIT_CODE".to_string(), env_entry(code));
        }
        let spec = spec_with(
            "claude-code",
            None,
            &[],
            true,
            environment,
            workspace.to_path_buf(),
        );

        adapter.validate(&spec).await.expect("validate");
        let handle = adapter.start(&spec).await.expect("start");
        let outcome = adapter.wait(&handle).await.expect("wait");

        assert!(
            case.expected_states.contains(&outcome.terminal_state),
            "case {:?}: unexpected terminal_state {:?}",
            case.name,
            outcome.terminal_state
        );
        if let Some(reason) = case.expected_reason {
            assert_eq!(
                outcome.terminal_reason["reason"], reason,
                "case {:?}",
                case.name
            );
        }
        (case.extra)(&outcome);
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }
}

/// `artifacts: Supported` once had no backing implementation — `wait()` never
/// called `ArtifactStager::stage_file`, only the live (billed, opt-in)
/// test's own test body did, bypassing the adapter entirely. This proves
/// the fix through the adapter's own `wait()`, via the free fake-binary
/// path, matching `codex.rs`'s identical proof shape.
#[tokio::test]
async fn fake_binary_success_stages_a_real_log_artifact() {
    let (adapter, _scratch) = adapter_with_fake_binary();
    let workspace_dir = temp_workspace("fake-success-artifact");
    let workspace = workspace_dir.path();
    let mut environment = BTreeMap::new();
    environment.insert("TACK_FAKE_HARNESS_MODE".to_string(), env_entry("success"));
    let spec = spec_with(
        "claude-code",
        None,
        &[],
        true,
        environment,
        workspace.to_path_buf(),
    );

    adapter.validate(&spec).await.expect("validate");
    let handle = adapter.start(&spec).await.expect("start");
    let outcome = adapter.wait(&handle).await.expect("wait");

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

/// A more realistic "malformed" case than generic garbage: a stream that
/// starts out perfectly valid and is then truncated before any terminal
/// `result` object arrives — e.g. a crash mid-run (fixture:
/// `fixtures/claude_code/2.1.223/truncated-mid-run.jsonl`, constructed —
/// see its `.provenance`). Pure unit test against the parser directly (no
/// process spawn needed): proves the "some JSON, but no result object"
/// branch specifically, which the generic fake binary cannot exercise.
#[test]
fn truncated_stream_with_no_result_line_is_reported_failed() {
    let stdout = include_str!("../fixtures/claude_code/2.1.223/truncated-mid-run.jsonl");
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
/// `process::tests::cancel_kills_the_whole_descendant_tree_...`.
#[tokio::test]
async fn cancel_stops_the_process_and_forgets_its_bookkeeping_entry() {
    let (adapter, _scratch) = adapter_with_fake_binary();
    let workspace_dir = temp_workspace("cancel");
    let workspace = workspace_dir.path();
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
        workspace.to_path_buf(),
    );

    adapter.validate(&spec).await.expect("validate");
    let handle = adapter.start(&spec).await.expect("start");
    assert_eq!(adapter.running.lock().await.len(), 1);

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
        adapter.running.lock().await.is_empty(),
        "cancel must remove its own bookkeeping entry"
    );
    std::fs::remove_dir_all(workspace).expect("cleanup");
}

// A cancel/wait on a handle this adapter instance never produced is now
// `harness::tests::cancel_and_wait_on_an_untracked_handle_are_typed_rejections_for_both_real_adapters`
// — `take_running`'s rejection is `local_process.rs`'s own shared
// bookkeeping, not claude-code-specific.

// ---- redaction ---------------------------------------------------------

/// Acceptance: arguments and environment are redacted in logs and
/// events; a canary appears nowhere. Plants a canary as a plain
/// environment value, drives the fake binary's `echo_canary` mode (which
/// actively echoes it back on both stdout and stderr — a worst-case
/// leaky harness), and asserts the captured/returned outcome never
/// contains it, while the `ProcessSpec` this adapter built also never
/// exposes it via `Debug` (structural half, inherited unchanged from
/// `process.rs`).
#[tokio::test]
async fn env_canary_is_redacted() {
    const CANARY: &str = "tack-d2-claude-code-canary-6f31a2";
    let (adapter, _scratch) = adapter_with_fake_binary();
    let workspace_dir = temp_workspace("canary");
    let workspace = workspace_dir.path();
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
        workspace.to_path_buf(),
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

// -----------------------------------------------------------------
// secret_reference resolution: the value reaches the spawned process,
// never a log line; a reference the store cannot resolve fails typed
// and pre-spawn, before the adapter touches anything.
// -----------------------------------------------------------------

// A *scoped* subscriber (`tracing::dispatcher::set_default`, not
// `tracing_subscriber::fmt().init()`): this test file shares a test
// binary with `git.rs`, which installs its own *global* default for the
// same reason (see its identical comment) — a second global `.init()`
// here would panic ("a global default trace dispatcher has already been
// set"). A scoped dispatcher avoids that collision and is sufficient
// here because the callsite this test exercises
// (`harness::resolve_environment`'s resolved-reference log line) is
// reached by no other test in this crate, so nothing can have cached its
// interest as "never" before this test's guard is the active dispatcher
// for the first, and only, real evaluation. The guard is held for the
// whole test (a `#[tokio::test]` with no `flavor` runs single-threaded,
// so it stays valid across every `.await` in the test body).
thread_local! {
    static SECRET_LOG_CAPTURE: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
}

struct SecretLogCapture;

impl std::io::Write for SecretLogCapture {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        SECRET_LOG_CAPTURE.with(|captured| captured.borrow_mut().extend_from_slice(buffer));
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl tracing_subscriber::fmt::MakeWriter<'_> for SecretLogCapture {
    type Writer = Self;

    fn make_writer(&self) -> Self::Writer {
        SecretLogCapture
    }
}

#[must_use = "the capture is only active while this guard is alive"]
fn install_secret_log_capture() -> tracing::dispatcher::DefaultGuard {
    SECRET_LOG_CAPTURE.with(|captured| captured.borrow_mut().clear());
    let subscriber = tracing_subscriber::fmt()
        .with_writer(SecretLogCapture)
        .with_max_level(tracing::Level::DEBUG)
        .with_ansi(false)
        .finish();
    tracing::dispatcher::set_default(&tracing::Dispatch::new(subscriber))
}

/// A shim that writes only the *byte length* of `$SECRET_VAR` to `marker`
/// — never the value itself — so a test can prove a resolved secret
/// reached the spawned process without ever holding the value.
fn secret_length_dump_binary(workspace: &Path, marker: &Path) -> HarnessBinary {
    let script = format!(
        "#!/bin/sh\nprintf '%s' \"$SECRET_VAR\" | wc -c > {}\nexit 0\n",
        marker.display()
    );
    let script_path = workspace.join("shim.sh");
    std::fs::write(&script_path, script).expect("write shim script");
    HarnessBinary {
        program: PathBuf::from("/bin/sh"),
        prefix_args: vec![script_path.display().to_string()],
    }
}

/// The byte count `secret_length_dump_binary`'s shim wrote to `marker`.
fn recorded_length(marker: &Path) -> usize {
    std::fs::read_to_string(marker)
        .expect("shim wrote the length marker")
        .trim()
        .parse()
        .expect("marker holds a byte count")
}

/// Acceptance: a live attempt with a `secret_reference` environment
/// entry reaches the spawned process with the resolved value set — the
/// shim here proves it by writing the value's *byte length* to a marker
/// file it controls, never the value itself. Captured `tracing` output
/// for the same run names the entry (positive control: asserted
/// present) and never contains the value.
#[tokio::test]
async fn secret_reference_resolves_and_only_length_reaches_the_shim() {
    let _log_capture = install_secret_log_capture();
    let workspace_dir = temp_workspace("secret-reference-length");
    let workspace = workspace_dir.path();
    let secret_value = "topsecret-canary-9f3a21";
    let store_dir = temp_workspace("secret-length-store");
    let store = crate::secrets::SecretStore::file(store_dir.path().join("secrets.json"));
    store.set("demo", secret_value).expect("seed the store");

    let marker = workspace.join("secret-length.marker");
    let binary = secret_length_dump_binary(workspace, &marker);
    let adapter = ClaudeCodeAdapter::with_binary(binary, clock(), store);
    let mut environment = BTreeMap::new();
    environment.insert("SECRET_VAR".to_string(), secret_reference_entry("demo"));
    let spec = spec_with(
        "claude-code",
        None,
        &[],
        true,
        environment,
        workspace.to_path_buf(),
    );

    adapter
        .validate(&spec)
        .await
        .expect("a resolvable secret_reference validates");
    let handle = adapter.start(&spec).await.expect("start");
    let outcome = adapter.wait(&handle).await.expect("wait");
    assert_eq!(outcome.terminal_state, AttemptState::Succeeded);
    assert_eq!(
        recorded_length(&marker),
        secret_value.len(),
        "the shim must have received the resolved value, not something else"
    );

    let captured = SECRET_LOG_CAPTURE
        .with(|captured| String::from_utf8(captured.borrow().clone()))
        .expect("utf-8");
    assert!(
        captured.contains("demo") || captured.contains("SECRET_VAR"),
        "the test is only load-bearing if resolution actually logged the entry: {captured:?}"
    );
    assert!(
        !captured.contains(secret_value),
        "the resolved secret value reached a log line: {captured}"
    );

    std::fs::remove_dir_all(workspace).expect("cleanup");
}

/// Regression guards for both capability corrections made to this file:
/// `cancel` was already `Advisory` (a correctly evidenced finding — the
/// registration-time gate in `harness::mod` relies on it staying that
/// way); `artifacts` is downgraded from an unbacked `Supported` to
/// `Advisory` (`wait()` never staged anything before this — see
/// `fake_binary_success_stages_a_real_log_artifact` above for the fix
/// itself).
#[test]
fn declared_capabilities_report_cancel_and_artifacts_advisory() {
    let (adapter, _scratch) = adapter_with_fake_binary();
    let declared = HarnessProbe::declared_capabilities(&adapter);
    assert_eq!(declared.cancel.support, CapabilitySupport::Advisory);
    assert!(declared.cancel.reason.is_some());
    assert_eq!(declared.artifacts.support, CapabilitySupport::Advisory);
    assert!(declared.artifacts.reason.is_some());
}

// ---- version parsing (pure unit tests) --------------------------------

/// One row per shape `parse_version_text` must recognize or honestly
/// flag as unrecognized — the real observed `claude --version` line, the
/// shared fixture's deliberately-unknown format, empty output, and a
/// prerelease-suffixed token.
struct VersionCase {
    name: &'static str,
    input: &'static str,
    expected_exact: Option<&'static str>,
    expected_contains: Option<&'static str>,
    expect_error: bool,
}

fn version_cases() -> Vec<VersionCase> {
    vec![
        VersionCase {
            name: "real observed version line",
            input: "2.1.223 (Claude Code)\n",
            expected_exact: Some("2.1.223"),
            expected_contains: None,
            expect_error: false,
        },
        VersionCase {
            name: "unknown fixture format",
            input: "harness-cli version 999.999.999-nightly-exotic-format\n",
            expected_exact: None,
            expected_contains: Some("harness-cli"),
            expect_error: true,
        },
        VersionCase {
            name: "empty output",
            input: "",
            expected_exact: Some(""),
            expected_contains: None,
            expect_error: true,
        },
        VersionCase {
            name: "prerelease suffix",
            input: "3.0.0-beta.1 (Claude Code)\n",
            expected_exact: Some("3.0.0-beta.1"),
            expected_contains: None,
            expect_error: false,
        },
    ]
}

#[test]
fn version_text_parsing_variants() {
    for case in version_cases() {
        let (version, error) = parse_version_text(case.input);
        assert_eq!(error.is_some(), case.expect_error, "case {:?}", case.name);
        if let Some(expected) = case.expected_exact {
            assert_eq!(version, expected, "case {:?}", case.name);
        }
        if let Some(substring) = case.expected_contains {
            assert!(
                version.contains(substring),
                "case {:?}: {version:?}",
                case.name
            );
        }
    }
}

/// The probe declares zero `model_combinations` (the CLI has no
/// list-models command) and instead attests `model_passthrough:
/// supported` — the claim the scheduler now relies on to make
/// claude-code schedulable at all. It must be Supported, carry a
/// reason, and coexist with the empty combination list rather than
/// replace its honesty note.
#[tokio::test]
async fn probe_attests_model_passthrough_instead_of_inventing_a_list() {
    let (adapter, _scratch) = adapter_with_fake_binary();
    let capability = adapter.probe().await;

    assert!(capability.model_combinations.is_empty());
    let passthrough = capability
        .model_passthrough
        .expect("claude-code probe must attest model_passthrough");
    assert_eq!(passthrough.support, CapabilitySupport::Supported);
    assert!(passthrough.reason.is_some());
    assert!(capability.additional.contains_key("model_discovery_note"));
}

/// Acceptance: fake-binary unknown version, driven through the real
/// spawn path (not only the pure-function tests above), using the
/// shared fixture's dedicated `unknown_version` mode.
#[tokio::test]
async fn probe_reports_the_fixtures_unknown_version_output_honestly() {
    let (adapter, _scratch) = adapter_with_fake_binary();
    // `detect_version` always invokes `--version` with no env override,
    // so this test instead exercises the same parsing path `probe`
    // depends on by driving the fixture in `unknown_version` mode
    // directly through `ProcessSpec`, matching exactly what
    // `detect_version` does internally.
    let workspace_dir = temp_workspace("unknown-version-probe");
    let workspace = workspace_dir.path().to_path_buf();
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
        working_directory: workspace.to_path_buf(),
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

/// One row per distinct `parse_run_output` classification claim proven
/// against a captured or constructed transcript (see
/// `fixtures/claude_code/README.md` for provenance) — the plain success
/// path, the misleading-`subtype` API-error quirk, and the
/// budget-exhaustion shape. The gateway-vs-direct differential is its
/// own test below since it compares two parses of one transcript, not
/// one parse's fields.
struct ResultCase {
    name: &'static str,
    stdout: &'static str,
    exit: ProcessExit,
    requested_provider: Option<&'static str>,
    check: fn(&ParsedRun),
}

fn result_cases() -> Vec<ResultCase> {
    vec![
        ResultCase {
            name: "real observed success transcript",
            stdout: include_str!("../fixtures/claude_code/2.1.223/success-with-usage.jsonl"),
            exit: ProcessExit::Exited(0),
            requested_provider: Some("anthropic"),
            check: |parsed| {
                assert!(!parsed.is_error);
                assert_eq!(parsed.model_id, "claude-sonnet-5");
                assert_eq!(parsed.model_observation_source, "harness_reported");
                assert_eq!(parsed.harness_version.as_deref(), Some("2.1.223"));
                assert_eq!(parsed.usage.tokens_in.value, Some(2));
                assert_eq!(parsed.usage.tokens_out.value, Some(18));
                assert_eq!(parsed.usage.tokens_in.source, MeasurementSource::Measured);
                assert_eq!(parsed.usage.cost_usd.value, Some(0.0350687));
            },
        },
        ResultCase {
            name: "is_error wins over a misleading success subtype",
            stdout: include_str!(
                "../fixtures/claude_code/2.1.223/api-error-misleading-subtype.jsonl"
            ),
            exit: ProcessExit::Exited(1),
            requested_provider: None,
            check: |parsed| {
                assert!(
                    parsed.is_error,
                    "is_error must win over a misleadingly-named subtype of \"success\""
                );
                assert_eq!(parsed.terminal_reason["subtype"], "success");
            },
        },
        ResultCase {
            name: "budget exhaustion is a distinct failed subtype",
            stdout: include_str!("../fixtures/claude_code/2.1.223/budget-exhausted.jsonl"),
            exit: ProcessExit::Exited(1),
            requested_provider: None,
            check: |parsed| {
                assert!(parsed.is_error);
                assert_eq!(parsed.terminal_reason["subtype"], "error_max_budget_usd");
                assert_eq!(parsed.usage.cost_usd.value, Some(0.013149));
            },
        },
    ]
}

#[test]
fn result_envelope_parsing_variants() {
    for case in result_cases() {
        eprintln!("result envelope case: {}", case.name);
        let result = ProcessResult {
            exit: case.exit,
            stdout: super::super::process::CapturedOutput {
                text: case.stdout.to_string(),
                truncated: false,
                bytes_dropped: 0,
                total_bytes_seen: 0,
            },
            stderr: Default::default(),
        };
        let parsed = parse_run_output(&result, case.requested_provider);
        (case.check)(&parsed);
    }
}

/// The gateway-specific half of `parsed_from_result_line`: even a
/// terminal `result` line — the case that lets a direct-provider run
/// claim `harness_reported` — must not upgrade a gateway-routed run to
/// that claim, because the init line it came from fired before any
/// network call reached the gateway.
#[test]
fn gateway_result_is_requested_not_confirmed_even_when_fast() {
    let stdout = include_str!("../fixtures/claude_code/2.1.261/gateway-routed-result.jsonl");
    let result = ProcessResult {
        exit: ProcessExit::Exited(1),
        stdout: super::super::process::CapturedOutput {
            text: stdout.to_string(),
            truncated: false,
            bytes_dropped: 0,
            total_bytes_seen: 0,
        },
        stderr: Default::default(),
    };

    let direct = parse_run_output(&result, Some("anthropic"));
    assert_eq!(
        direct.model_observation_source,
        ModelObservationSource::HarnessReported.as_str()
    );

    let gateway = parse_run_output(&result, Some(crate::config::VERCEL_AI_GATEWAY_PROVIDER));
    assert_eq!(
        gateway.model_observation_source,
        ModelObservationSource::RequestedNotConfirmed.as_str()
    );
    assert_eq!(gateway.model_id, "anthropic/claude-opus-4.6");
}

#[test]
fn a_missing_is_error_field_fails_closed_not_a_silent_success() {
    let value = serde_json::json!({"type": "result", "result": "no is_error field here"});
    let parsed = parsed_from_result_line(&value, None, None, None);
    assert!(parsed.is_error);
}

// ---- reconcile ---------------------------------------------------------

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

// A missing process id needing no liveness dispatch, an unrecognized
// handle encoding being explicitly `RecoveryUnavailable`, and a
// decodable-but-already-dead pid reporting `ProcessStopped` are all
// `local_process.rs`'s own shared `reconcile()` plumbing, entirely
// before either grammar's own `reconcile_alive`/`reconcile_unavailable`
// is ever consulted — proved once, against both real adapters, by
// `harness::tests::reconcile_reports_shared_pid_plumbing_identically_for_both_real_adapters`.
// This grammar's own identity check for a still-alive pid
// (`process_program_matches`, genuinely different from codex's
// unconditional trust) stays below, in the two Linux-only tests.

/// Deliberately drives `spawn_child` mode, not `hang`: `hang` execs into
/// `sleep`, replacing the process image so `/proc/<pid>/cmdline` becomes
/// `sleep ...` moments after spawn — behavior specific to that one fixture
/// mode, not representative of what `process_program_matches` must identify
/// against a real `claude` process, which never re-execs over its own
/// lifetime. `spawn_child`'s own process (distinct from the grandchild
/// `sleep` it backgrounds) never execs, keeping a stable, checkable
/// `/bin/sh <script>` cmdline throughout — which is what lets the bounded
/// poll below converge instead of being structurally unable to: reading
/// `/proc/<pid>/cmdline` immediately after spawn can transiently return
/// `None` (the kernel has not necessarily finished populating it yet),
/// which `reconcile` itself already reports honestly as `Ambiguous` rather
/// than guessing, so the loop exists only to make the assertion insensitive
/// to that one-time startup window, not because `reconcile` is retried in
/// production.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn reconcile_reports_running_for_a_still_running_harness() {
    let (adapter, _scratch) = adapter_with_fake_binary();
    let workspace_dir = temp_workspace("reconcile-running");
    let workspace = workspace_dir.path();
    let mut environment = BTreeMap::new();
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
        workspace.to_path_buf(),
    );
    adapter.validate(&spec).await.expect("validate");
    let handle = adapter.start(&spec).await.expect("start");
    let pid: u32 = handle.process_id.parse().expect("numeric pid");

    let journal = journal_with_process(Some(&handle.process_id));
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

    // This adapter's own `cancel` stops the process (and its backgrounded
    // grandchild, via `process.rs`'s process-group signalling) and forgets
    // the bookkeeping entry `reconcile` never touched.
    adapter.cancel(&handle).await.expect("cancel");
    assert!(!process_alive(pid));
    std::fs::remove_dir_all(workspace).expect("cleanup");
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn reconcile_reports_stopped_for_a_pid_of_an_unrelated_program() {
    // A real, currently-alive pid that this adapter did *not* spawn and
    // that does not resolve to `self.binary.program` at all: this
    // adapter's own test-runner process itself.
    let (adapter, _scratch) = adapter_with_fake_binary();
    let own_pid = std::process::id();
    let journal = journal_with_process(Some(&own_pid.to_string()));
    let observation = adapter.reconcile(&journal).await.expect("reconcile");
    assert_eq!(observation, RecoveryObservation::ProcessStopped);
}

// -----------------------------------------------------------------
// Provider endpoint injection: a configured gateway entry reaches a
// spawned process only when the request actually names it; a direct
// request must receive none of it.
// -----------------------------------------------------------------

fn enabled_gateway_providers(
    secret_name: &str,
) -> std::collections::BTreeMap<String, crate::config::ProviderConfig> {
    std::collections::BTreeMap::from([(
        crate::config::VERCEL_AI_GATEWAY_CONFIG_KEY.to_owned(),
        crate::config::ProviderConfig {
            enabled: true,
            secret: secret_name.to_owned(),
        },
    )])
}

/// A shim that records the *names* only of the environment variables it
/// was spawned with — never a value — so a test can prove a variable's
/// presence or absence without ever needing to see, let alone assert
/// on, a credential.
fn env_name_dump_binary(workspace: &Path, marker: &Path) -> HarnessBinary {
    // A single external process (`env`), no pipe to a second one: the
    // name/value split happens in `recorded_env_names` instead, purely
    // to keep this shim's own process footprint minimal under a
    // heavily parallel test run.
    let script = format!("#!/bin/sh\nenv > {}\nexit 0\n", marker.display());
    let script_path = workspace.join("dump-env-names.sh");
    std::fs::write(&script_path, script).expect("write shim script");
    HarnessBinary {
        program: PathBuf::from("/bin/sh"),
        prefix_args: vec![script_path.display().to_string()],
    }
}

/// The *names* only of the `KEY=VALUE` lines `env`'s output wrote to
/// `marker` — this helper is what actually discards every value, so no
/// caller ever inspects one, even a dummy one seeded for a test.
fn recorded_env_names(marker: &Path) -> Vec<String> {
    std::fs::read_to_string(marker)
        .expect("shim wrote the env-names marker")
        .lines()
        .filter_map(|line| line.split('=').next())
        .map(str::to_owned)
        .collect()
}

/// One row per request shape: a direct model provider (or none at all)
/// must spawn with neither `ANTHROPIC_BASE_URL` nor
/// `ANTHROPIC_AUTH_TOKEN` present, even with a gateway entry configured
/// and enabled; a request naming that configured provider must receive
/// both. Proves the two paths can never be confused by a shared
/// environment variable.
struct ProviderCase {
    name: &'static str,
    requested_provider: Option<&'static str>,
    expect_present: bool,
}

fn provider_cases() -> Vec<ProviderCase> {
    vec![
        ProviderCase {
            name: "direct model request",
            requested_provider: None,
            expect_present: false,
        },
        ProviderCase {
            name: "configured provider request",
            requested_provider: Some(crate::config::VERCEL_AI_GATEWAY_PROVIDER),
            expect_present: true,
        },
    ]
}

#[tokio::test]
async fn provider_endpoint_variables_present_only_when_configured() {
    for case in provider_cases() {
        let workspace_dir = temp_workspace("provider-guard");
        let workspace = workspace_dir.path();
        let marker = workspace.join("env-names.marker");
        let binary = env_name_dump_binary(workspace, &marker);

        let secrets_dir = temp_workspace("secrets");
        let secrets = test_secret_store(secrets_dir.path());
        secrets
            .set("demo-secret", "a-resolvable-value")
            .expect("seed store");
        let adapter = ClaudeCodeAdapter::with_binary(binary, clock(), secrets)
            .with_providers(enabled_gateway_providers("demo-secret"));

        let spec = spec_with(
            "claude-code",
            case.requested_provider,
            &[],
            true,
            BTreeMap::new(),
            workspace.to_path_buf(),
        );
        adapter.validate(&spec).await.expect("validate");
        let handle = adapter.start(&spec).await.expect("start");
        let _ = adapter.wait(&handle).await.expect("wait");

        let names = recorded_env_names(&marker);
        assert_eq!(
            names.iter().any(|name| name == "ANTHROPIC_BASE_URL"),
            case.expect_present,
            "case {:?}: {names:?}",
            case.name
        );
        assert_eq!(
            names.iter().any(|name| name == "ANTHROPIC_AUTH_TOKEN"),
            case.expect_present,
            "case {:?}: {names:?}",
            case.name
        );

        std::fs::remove_dir_all(workspace).expect("cleanup");
    }
}

// A configured-but-disabled provider rejecting pre-spawn is now
// `harness::tests::disabled_provider_rejects_both_real_adapters_before_any_process_spawns`
// — `resolve_provider_endpoint`'s discard-and-recheck plumbing lives in
// `local_process.rs`'s shared `validate`, and the actual "disabled ->
// reject" check is `provider::resolve_endpoint`'s own, called
// identically by every grammar.

// Discovery's search logic is pure and lives in `harness::locate`, with
// its own tests there (found on PATH, found only in a fallback dir, not
// found, non-executable skipped); this file no longer needs a test that
// mutates the real process `PATH` to reach the same behavior.

// Live, opt-in tests against a real installed `claude` binary live under
// `crates/tack-runner/tests/live/claude_code.rs`, not here.
