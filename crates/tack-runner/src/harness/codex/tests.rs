use super::*;
use crate::client::journal::{JournalState, WorkspaceJournal};
use crate::client::{
    AttemptId, AttemptLease, ClaimRequestId, ClaimedWork, FencingToken, RunnerId,
    Workspace as ClientWorkspace, WorkspaceId,
};
use crate::harness::fixtures::fake_harness_command;
use std::time::SystemTime;
use tack_orch::execution::{
    AttemptSnapshot, ExecutionRequestSnapshot, HarnessKind as DomainHarnessKind, RequestedModelId,
    RequestedModelProvider,
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

/// A scratch directory that removes itself, and everything written under
/// it, when the returned guard drops — including when an assertion panics
/// first. Whatever holds a path into it must hold the guard too.
fn temp_dir(label: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(label)
        .tempdir()
        .expect("temporary directory")
}

/// A minimal, deterministic "fixture repo" workspace: a couple of known
/// files with fixed content, created fresh per test rather than checked
/// into the tree — giving every test a real, workspace-confined,
/// reproducible directory to run the fake harness against (mirroring
/// the
/// `each_workspace_confined_process_only_ever_sees_its_own_canary_file`
/// pattern).
fn deterministic_fixture_repo(label: &str) -> tempfile::TempDir {
    let root = temp_dir(label);
    std::fs::write(root.path().join("README.md"), b"# fixture repo\n").expect("write README");
    std::fs::write(root.path().join("main.rs"), b"fn main() {}\n").expect("write main.rs");
    root
}

fn fixed_command() -> CodexLocator {
    let (program, prefix_args) = fake_harness_command();
    CodexLocator::Fixed {
        program,
        prefix_args,
    }
}

/// A fresh, hermetic file-backed store per call — never the platform
/// keychain — so parallel `#[test]` functions never see each other's
/// entries and CI needs no Secret Service.
/// Takes the directory rather than making one, so the store's file cannot
/// outlive the guard that removes it. File-backed, never the platform
/// keychain, so parallel tests never see each other's entries and CI
/// needs no Secret Service.
fn test_secret_store(dir: &std::path::Path) -> crate::secrets::SecretStore {
    crate::secrets::SecretStore::file(dir.join("secrets.json"))
}

/// Returns the adapter with the scratch directory its artifact staging
/// root and secret store both live in: drop the guard and the adapter is
/// pointing at nothing.
fn adapter_with_env(
    probe_env: BTreeMap<String, String>,
) -> (CodexAdapter<FixedClock>, tempfile::TempDir) {
    let scratch = temp_dir("artifacts");
    let adapter = CodexAdapter::with_clock(
        fixed_command(),
        generous_limits(),
        Duration::from_secs(5),
        probe_env,
        scratch.path().to_path_buf(),
        clock_at("2026-08-09T12:00:00Z"),
        test_secret_store(scratch.path()),
    );
    (adapter, scratch)
}

fn adapter() -> (CodexAdapter<FixedClock>, tempfile::TempDir) {
    adapter_with_env(BTreeMap::new())
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
        "../../../../../docs/contracts/runner-v1/claim.response.json"
    ))
    .expect("claim fixture");
    let mut request: ExecutionRequestSnapshot =
        serde_json::from_value(claim["request"].clone()).expect("request fixture");
    request.requested_harness_kind = DomainHarnessKind::new(CODEX_HARNESS_KIND);
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
            id: WorkspaceId::new("ws_codex_test"),
            path: workspace_path,
            base_revision: "revision".into(),
        },
    }
}

// ---- validate() / start() pre-spawn rejection --------------------

/// One case for [`validate_rejects_pre_spawn_selection_problems`]: builds
/// its own adapter and spec (they differ per case — a mismatched harness
/// kind, an auto-selected model, an unresolvable binary) and returns every
/// guard whose drop must outlive the call to `validate`.
struct RejectCase {
    name: &'static str,
    build: fn() -> (
        CodexAdapter<FixedClock>,
        ExecutionSpec,
        Vec<tempfile::TempDir>,
    ),
}

fn reject_cases() -> Vec<RejectCase> {
    vec![
        RejectCase {
            name: "mismatched_harness_kind",
            build: || {
                let (adapter, scratch) = adapter();
                let workspace_dir = deterministic_fixture_repo("kind-mismatch");
                let mut spec = spec_with(
                    workspace_dir.path().to_path_buf(),
                    Some(("openai", "opaque/model-alpha")),
                    &[],
                );
                spec.work.request.requested_harness_kind = DomainHarnessKind::new("claude-code");
                (adapter, spec, vec![scratch, workspace_dir])
            },
        },
        RejectCase {
            name: "auto_selected_model_pre_spawn",
            build: || {
                let (adapter, scratch) = adapter();
                let workspace_dir = deterministic_fixture_repo("auto-select");
                let spec = spec_with(workspace_dir.path().to_path_buf(), None, &[]);
                (adapter, spec, vec![scratch, workspace_dir])
            },
        },
        RejectCase {
            name: "unresolvable_binary",
            build: || {
                let empty_dir_dir = temp_dir("empty-path");
                let scratch = temp_dir("artifacts-unresolvable");
                let adapter = CodexAdapter::with_clock(
                    CodexLocator::Search {
                        // Not the real `codex` program name: the well-known
                        // fallback list includes fixed system directories
                        // (Homebrew's `/usr/local/bin`) this case cannot
                        // isolate the way it isolates `PATH`, so a name no
                        // real installer would ever use keeps it
                        // deterministic regardless of what is actually
                        // installed on the machine running it.
                        program_name: "tack-test-fixture-nonexistent-codex".to_owned(),
                        path: Some(
                            std::env::join_paths([empty_dir_dir.path()]).expect("join paths"),
                        ),
                        home: None,
                    },
                    generous_limits(),
                    Duration::from_secs(1),
                    BTreeMap::new(),
                    scratch.path().to_path_buf(),
                    clock_at("2026-08-09T12:00:00Z"),
                    test_secret_store(scratch.path()),
                );
                let workspace_dir = deterministic_fixture_repo("unresolvable");
                let spec = spec_with(
                    workspace_dir.path().to_path_buf(),
                    Some(("openai", "opaque/model-alpha")),
                    &[],
                );
                (adapter, spec, vec![empty_dir_dir, scratch, workspace_dir])
            },
        },
    ]
}

#[tokio::test]
async fn validate_rejects_pre_spawn_selection_problems() {
    for case in reject_cases() {
        let (adapter, spec, _guards) = (case.build)();
        assert!(
            matches!(
                adapter.validate(&spec).await,
                Err(HarnessError::Rejected { .. })
            ),
            "case {}",
            case.name
        );
    }
}

/// Acceptance: "an unsupported selection fails pre-spawn — validation
/// rejects it before any process is launched, not after." Proved
/// empirically, not just by code inspection: the spec is configured so
/// the underlying fake process would `hang` for an hour if it were ever
/// actually spawned. If `start()`'s pre-spawn guard were broken, this
/// test would hang (bounded here by an explicit timeout that turns that
/// hang into a fast, loud failure rather than a stuck CI job).
#[tokio::test]
async fn unsupported_selection_fails_pre_spawn_even_when_the_process_would_otherwise_hang_forever()
{
    let (adapter, _scratch) = adapter();
    let workspace_dir = deterministic_fixture_repo("pre-spawn-hang-guard");
    let spec = spec_with(
        workspace_dir.path().to_path_buf(),
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
}

// ---- fake-binary exec-path tests ----------------------------------

#[tokio::test]
async fn fake_binary_success_completes_succeeded_with_normalized_output_and_a_staged_artifact() {
    let (adapter, _scratch) = adapter();
    let workspace_dir = deterministic_fixture_repo("exec-success");
    let spec = spec_with(
        workspace_dir.path().to_path_buf(),
        Some(("openai", "opaque/model-alpha")),
        &[("TACK_FAKE_HARNESS_MODE", "success")],
    );

    adapter.validate(&spec).await.expect("validate");
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
    assert_eq!(outcome.actual_execution.model_provider.as_str(), "openai");
    assert_eq!(
        outcome.actual_execution.model_id.as_str(),
        "opaque/model-alpha"
    );
    assert_eq!(
        outcome.actual_execution.model_observation_source,
        MODEL_OBSERVATION_SOURCE
    );
    assert_eq!(
        outcome.usage.duration_ms.source,
        MeasurementSource::Measured
    );
    assert!(outcome.usage.duration_ms.value.is_some());
    assert_eq!(
        outcome.usage.tokens_in.source,
        MeasurementSource::NotMeasured
    );
    assert!(outcome.usage.tokens_in.value.is_none());

    let artifact = &outcome.terminal_reason["artifact"];
    assert_eq!(artifact["kind"], "log");
    let staged_path = artifact["staged_path"].as_str().expect("staged_path");
    let staged_bytes = std::fs::read(staged_path).expect("read staged artifact");
    assert!(String::from_utf8_lossy(&staged_bytes).contains("fake-harness-ok"));
    assert_eq!(
        artifact["sha256"].as_str().unwrap(),
        crate::harness::sha256::sha256_hex(&staged_bytes)
    );
}

/// One case for [`wait_classifies_terminal_state_from_the_exit_code_alone`]:
/// the fake harness mode to run, and the outcome that exit-code-only
/// classification (`classify_exit`) must produce for it.
struct ExecCase {
    name: &'static str,
    mode: &'static str,
    extra_env: &'static [(&'static str, &'static str)],
    expect_state: AttemptState,
    expect_code: &'static str,
    message_contains: Option<&'static str>,
    /// Robustness half of the `malformed` claim (module docs assumption
    /// (4)): the fixture's non-JSON stdout is still captured, non-empty and
    /// well-typed rather than causing a panic.
    expect_nonempty_stdout_preview: bool,
}

fn exec_classification_cases() -> Vec<ExecCase> {
    vec![
        ExecCase {
            name: "failure_exit_code",
            mode: "failure",
            extra_env: &[("TACK_FAKE_HARNESS_EXIT_CODE", "17")],
            expect_state: AttemptState::Failed,
            expect_code: "exit_code",
            message_contains: Some("17"),
            expect_nonempty_stdout_preview: false,
        },
        ExecCase {
            name: "malformed_output_still_succeeds_on_exit_zero",
            mode: "malformed",
            extra_env: &[],
            expect_state: AttemptState::Succeeded,
            expect_code: "completed",
            message_contains: None,
            expect_nonempty_stdout_preview: true,
        },
    ]
}

/// Acceptance: `classify_exit` derives `terminal_state` purely from the
/// process exit, never by inspecting output content — see module docs
/// assumption (4). The `malformed` row proves this is *robustness* (no
/// panic, a well-typed result either way), not "malformed output causes
/// failure": the fixture's `malformed` mode still exits 0.
#[tokio::test]
async fn wait_classifies_terminal_state_from_the_exit_code_alone() {
    for case in exec_classification_cases() {
        let (adapter, _scratch) = adapter();
        let workspace_dir = deterministic_fixture_repo(case.name);
        let mut env = vec![("TACK_FAKE_HARNESS_MODE", case.mode)];
        env.extend_from_slice(case.extra_env);
        let spec = spec_with(
            workspace_dir.path().to_path_buf(),
            Some(("openai", "opaque/model-alpha")),
            &env,
        );

        let handle = adapter.start(&spec).await.expect("start");
        let outcome = adapter.wait(&handle).await.expect("wait");

        assert_eq!(
            outcome.terminal_state, case.expect_state,
            "case {}",
            case.name
        );
        assert_eq!(
            outcome.terminal_reason["code"], case.expect_code,
            "case {}",
            case.name
        );
        if let Some(needle) = case.message_contains {
            assert!(
                outcome.terminal_reason["message"]
                    .as_str()
                    .unwrap()
                    .contains(needle),
                "case {}",
                case.name
            );
        }
        if case.expect_nonempty_stdout_preview {
            let preview = outcome.terminal_reason["stdout"]["text_preview"]
                .as_str()
                .expect("stdout preview is present and well-formed JSON");
            assert!(!preview.is_empty(), "case {}", case.name);
        }
    }
}

// Cancel killing the whole descendant tree through a real adapter's own
// `start`/`cancel` (not raw `ProcessSpec`, which is `process.rs`'s own
// test) is now `harness::tests::cancel_kills_the_whole_descendant_tree_via_both_real_adapters`
// — that shared-core test drives both real adapters against the same
// fake-harness fixture, so it no longer needs a codex-only copy here.

// A cancel/wait on a handle this adapter instance never produced is now
// `harness::tests::cancel_and_wait_on_an_untracked_handle_are_typed_rejections_for_both_real_adapters`
// — `take_running`'s rejection is `local_process.rs`'s own shared
// bookkeeping, not codex-specific.

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
    const CANARY_ENV: &str = "tack-test-codex-canary-env-58d1";
    let (adapter, _scratch) = adapter();
    let workspace_dir = deterministic_fixture_repo("redaction");
    let mut spec = spec_with(
        workspace_dir.path().to_path_buf(),
        Some(("openai", "opaque/model-alpha")),
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
        "do the tack-test-codex-canary-stdin-a341 thing".to_owned();
    const CANARY_STDIN: &str = "tack-test-codex-canary-stdin-a341";

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
}

// ---- probe() / HarnessProbe ----------------------------------------

/// One case for [`probe_reports_version_or_an_explicit_error_never_a_fake_success`]:
/// the fake harness's version-mode environment and what `probe()` must
/// report for it.
struct ProbeCase {
    name: &'static str,
    env: &'static [(&'static str, &'static str)],
    expect_version: &'static str,
    expect_error_contains: Option<&'static str>,
    expect_raw_contains: Option<&'static str>,
}

fn probe_cases() -> Vec<ProbeCase> {
    vec![
        ProbeCase {
            name: "recognized_version",
            env: &[
                ("TACK_FAKE_HARNESS_MODE", "version"),
                ("TACK_FAKE_HARNESS_VERSION", "9.9.9"),
            ],
            expect_version: "9.9.9",
            expect_error_contains: None,
            expect_raw_contains: None,
        },
        // Acceptance: the real `codex` CLI prefixes its version with a
        // program-name token (`codex-cli 0.149.1`) instead of printing it
        // bare. A whole-string check would misclassify this as
        // unrecognized and permanently block scheduling via
        // `HarnessProbeError`; the version must be extracted from among
        // the output's tokens instead.
        ProbeCase {
            name: "program_name_prefixed_version",
            env: &[
                ("TACK_FAKE_HARNESS_MODE", "version"),
                ("TACK_FAKE_HARNESS_VERSION", "codex-cli 0.149.1"),
            ],
            expect_version: "0.149.1",
            expect_error_contains: None,
            expect_raw_contains: None,
        },
        // The fixture's `unknown_version` mode exits 0 with a string that
        // is not a clean version line; this must be an explicit
        // `probe_error`, never a fabricated clean version (rule 7).
        ProbeCase {
            name: "unrecognized_version_string",
            env: &[("TACK_FAKE_HARNESS_MODE", "unknown_version")],
            expect_version: "",
            expect_error_contains: Some(""),
            expect_raw_contains: Some("999.999.999"),
        },
        ProbeCase {
            name: "malformed_version_output",
            env: &[("TACK_FAKE_HARNESS_MODE", "malformed")],
            expect_version: "",
            expect_error_contains: Some(""),
            expect_raw_contains: None,
        },
        ProbeCase {
            name: "nonzero_exit",
            env: &[
                ("TACK_FAKE_HARNESS_MODE", "failure"),
                ("TACK_FAKE_HARNESS_EXIT_CODE", "3"),
            ],
            expect_version: "",
            expect_error_contains: Some("3"),
            expect_raw_contains: None,
        },
    ]
}

#[tokio::test]
async fn probe_reports_version_or_an_explicit_error_never_a_fake_success() {
    for case in probe_cases() {
        let (adapter, _scratch) = adapter_with_env(env_map(case.env));
        let capability = adapter.probe().await;

        assert_eq!(
            capability.installed_version, case.expect_version,
            "case {}",
            case.name
        );
        match case.expect_error_contains {
            None => assert_eq!(capability.probe_error, None, "case {}", case.name),
            Some(needle) => assert!(
                capability
                    .probe_error
                    .as_deref()
                    .unwrap_or_default()
                    .contains(needle),
                "case {}",
                case.name
            ),
        }
        if let Some(needle) = case.expect_raw_contains {
            let raw = capability
                .additional
                .get("raw_version_output")
                .and_then(|value| value.as_str())
                .expect("raw output preserved for diagnosis");
            assert!(raw.contains(needle), "case {}", case.name);
        }
    }
}

/// Acceptance: with no enumerable models, schedulability rests on the
/// pass-through attestation alone — it must be `Supported` and carry a
/// reason, distinct from the version-parsing claim above.
#[tokio::test]
async fn probe_attests_model_passthrough_when_no_models_are_enumerable() {
    let (adapter, _scratch) = adapter_with_env(env_map(&[
        ("TACK_FAKE_HARNESS_MODE", "version"),
        ("TACK_FAKE_HARNESS_VERSION", "9.9.9"),
    ]));
    let capability = adapter.probe().await;

    assert_eq!(capability.harness_kind.as_str(), CODEX_HARNESS_KIND);
    assert!(capability.model_combinations.is_empty());
    let passthrough = capability
        .model_passthrough
        .expect("codex probe must attest model_passthrough");
    assert_eq!(passthrough.support, CapabilitySupport::Supported);
    assert!(passthrough.reason.is_some());
}

#[tokio::test]
async fn probe_reports_an_absent_binary_as_an_explicit_probe_error_never_a_fake_success() {
    let empty_dir_dir = temp_dir("probe-empty-path");
    let empty_dir = empty_dir_dir.path();
    let scratch = temp_dir("artifacts-absent");
    let adapter = CodexAdapter::with_clock(
        CodexLocator::Search {
            // Not the real `codex` program name: the well-known fallback
            // list includes fixed system directories (Homebrew's
            // `/usr/local/bin`) this test cannot isolate the way it
            // isolates `PATH`, so a name no real installer would ever
            // use keeps this test deterministic regardless of what is
            // actually installed on the machine running it.
            program_name: "tack-test-fixture-nonexistent-codex".to_owned(),
            path: Some(std::env::join_paths([empty_dir]).expect("join paths")),
            home: None,
        },
        generous_limits(),
        Duration::from_secs(1),
        BTreeMap::new(),
        scratch.path().to_path_buf(),
        clock_at("2026-08-09T12:00:00Z"),
        test_secret_store(scratch.path()),
    );

    let capability = adapter.probe().await;
    assert_eq!(capability.installed_version, "");
    assert!(capability.probe_error.unwrap().contains("not found"));
}

#[tokio::test]
async fn probe_never_hangs_past_its_own_timeout() {
    let scratch = temp_dir("artifacts-hang");
    let adapter = CodexAdapter::with_clock(
        fixed_command(),
        generous_limits(),
        Duration::from_millis(50),
        env_map(&[
            ("TACK_FAKE_HARNESS_MODE", "hang"),
            ("TACK_FAKE_HARNESS_SLEEP_SECONDS", "3600"),
        ]),
        scratch.path().to_path_buf(),
        clock_at("2026-08-09T12:00:00Z"),
        test_secret_store(scratch.path()),
    );

    let capability = tokio::time::timeout(Duration::from_secs(5), adapter.probe())
        .await
        .expect("probe must respect its own timeout rather than hanging the caller");
    assert_eq!(capability.installed_version, "");
    assert!(capability.probe_error.unwrap().contains("timed out"));
}

#[tokio::test]
async fn harness_kind_matches_what_probe_itself_reports() {
    let (adapter, _scratch) = adapter();
    let capability = adapter.probe().await;
    assert_eq!(
        HarnessProbe::harness_kind(&adapter).as_str(),
        capability.harness_kind.as_str()
    );
}

/// Direct regression guard: this adapter's only
/// cancellation primitive is `harness::process::SupervisedProcess::cancel`
/// (a process-group SIGTERM/SIGKILL), which cannot reliably
/// reach a descendant a harness's own shell-tool spawns into a new OS
/// session — and the registration-time gate
/// (`AdapterRegistry::register_probe`) refuses to register any probe
/// still claiming `Supported`. This pins the value directly, not only
/// through the registration side effect.
#[test]
fn declared_cancel_capability_is_advisory_not_supported() {
    let (adapter, _scratch) = adapter();
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
            workspace_id: WorkspaceId::new("ws_codex_test"),
            path: PathBuf::from("/tmp/does-not-matter"),
            base_revision: "revision".into(),
        },
        state: JournalState::ProcessObservedRunning,
        process_id: process_id.map(str::to_owned),
        last_event_checkpoint: None,
        pending_terminal_report: None,
    }
}

// A missing process id needing no liveness dispatch, an unrecognized
// handle encoding being explicitly `RecoveryUnavailable`, and a
// decodable-but-already-dead pid reporting `ProcessStopped` are all
// `local_process.rs`'s own shared `reconcile()` plumbing — proved once,
// against both real adapters, by
// `harness::tests::reconcile_reports_shared_pid_plumbing_identically_for_both_real_adapters`.

/// Acceptance: `reconcile_alive` trusts a live pid unconditionally (module
/// docs, asymmetry 3) — the one part of `reconcile` that is genuinely
/// codex's own, proved against a real, independently-controlled process
/// rather than a simulated liveness check.
#[cfg(unix)]
#[tokio::test]
async fn reconcile_trusts_a_live_pid_unconditionally() {
    let (adapter, _scratch) = adapter();

    let mut alive = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("spawn sleep");
    let alive_journal = journal_with_process(Some(&encode_handle(alive.id(), 0)));
    assert_eq!(
        adapter.reconcile(&alive_journal).await.expect("reconcile"),
        RecoveryObservation::ProcessRunning
    );
    let _ = alive.kill();
    let _ = alive.wait();
}

// -----------------------------------------------------------------
// Provider endpoint injection: a configured entry reaches a spawned
// process only when the request actually names it; a direct request
// must receive none of it.
// -----------------------------------------------------------------

fn enabled_gateway_providers(secret_name: &str) -> BTreeMap<String, crate::config::ProviderConfig> {
    BTreeMap::from([(
        crate::config::VERCEL_AI_GATEWAY_CONFIG_KEY.to_owned(),
        crate::config::ProviderConfig {
            enabled: true,
            secret: secret_name.to_owned(),
        },
    )])
}

/// A shim that records the *names* only of the environment variables it
/// was spawned with — never a value.
fn env_name_dump_locator(workspace: &std::path::Path, marker: &std::path::Path) -> CodexLocator {
    // A single external process (`env`), no pipe to a second one: the
    // name/value split happens in `recorded_env_names` instead, purely
    // to keep this shim's own process footprint minimal under a
    // heavily parallel test run.
    // Only the run itself records its environment. The adapter also
    // invokes this shim as `<shim> --version` for its version probe,
    // with the probe's own empty environment, and that probe can finish
    // after the run — an unconditional `env > marker` then holds
    // whichever process wrote last. `exec` is the run's subcommand; the
    // probe never passes it.
    let script = format!(
        "#!/bin/sh\nfor arg in \"$@\"; do [ \"$arg\" = exec ] && env > {}; done\nexit 0\n",
        marker.display()
    );
    let script_path = workspace.join("dump-env-names.sh");
    std::fs::write(&script_path, script).expect("write shim script");
    CodexLocator::Fixed {
        program: PathBuf::from("/bin/sh"),
        prefix_args: vec![script_path.display().to_string()],
    }
}

/// The *names* only of the `KEY=VALUE` lines `env`'s output wrote to
/// `marker` — this helper is what actually discards every value, so no
/// caller ever inspects one, even a dummy one seeded for a test.
fn recorded_env_names(marker: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(marker)
        .expect("shim wrote the env-names marker")
        .lines()
        .filter_map(|line| line.split('=').next())
        .map(str::to_owned)
        .collect()
}

/// One case for
/// [`provider_endpoint_credential_reaches_the_process_only_when_the_request_names_it`]:
/// the requested (provider, model) pair, and whether the configured
/// gateway's credential variable is expected to reach the spawned process.
struct ProviderEnvCase {
    name: &'static str,
    provider: &'static str,
    model_id: &'static str,
    expect_credential_present: bool,
}

fn provider_env_cases() -> Vec<ProviderEnvCase> {
    vec![
        ProviderEnvCase {
            name: "direct_model_request",
            provider: "openai",
            model_id: "gpt-5",
            expect_credential_present: false,
        },
        ProviderEnvCase {
            name: "configured_provider_request",
            provider: crate::config::VERCEL_AI_GATEWAY_PROVIDER,
            model_id: "openai/gpt-5.1",
            expect_credential_present: true,
        },
    ]
}

/// Acceptance: a gateway entry configured and enabled on the adapter only
/// ever reaches a spawned process when the request actually names that
/// provider — a direct-vendor request must spawn with neither the `-c
/// model_provider` flag nor the credential variable present.
#[tokio::test]
async fn provider_endpoint_credential_reaches_the_process_only_when_the_request_names_it() {
    for case in provider_env_cases() {
        let workspace_dir = deterministic_fixture_repo(case.name);
        let workspace = workspace_dir.path();
        let marker = workspace.join("env-names.marker");
        let secrets_scratch = temp_dir("secrets");
        let secrets = test_secret_store(secrets_scratch.path());
        secrets
            .set("demo-secret", "a-resolvable-value")
            .expect("seed store");
        let artifacts_scratch = temp_dir("artifacts");
        let adapter = CodexAdapter::with_clock(
            env_name_dump_locator(workspace, &marker),
            generous_limits(),
            Duration::from_secs(5),
            BTreeMap::new(),
            artifacts_scratch.path().to_path_buf(),
            clock_at("2026-08-09T12:00:00Z"),
            secrets,
        )
        .with_providers(enabled_gateway_providers("demo-secret"));

        let spec = spec_with(
            workspace.to_path_buf(),
            Some((case.provider, case.model_id)),
            &[],
        );
        adapter.validate(&spec).await.expect("validate");
        let handle = adapter.start(&spec).await.expect("start");
        let _ = adapter.wait(&handle).await.expect("wait");

        let names = recorded_env_names(&marker);
        let present = names.iter().any(|name| name == "AI_GATEWAY_API_KEY");
        assert_eq!(
            present, case.expect_credential_present,
            "case {}: {names:?}",
            case.name
        );
    }
}

// A configured-but-disabled provider rejecting pre-spawn is now
// `harness::tests::disabled_provider_rejects_both_real_adapters_before_any_process_spawns`
// — `resolve_provider_endpoint`'s discard-and-recheck plumbing lives in
// `local_process.rs`'s shared `validate`, and the actual "disabled ->
// reject" check is `provider::resolve_endpoint`'s own, called
// identically by every grammar.

// Live tests against a real `codex` binary moved to
// `crates/tack-runner/tests/live/codex.rs` (audit §5 rule 4).
