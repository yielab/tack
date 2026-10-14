//! What a request may and may not put in a run's environment.

use std::{collections::BTreeMap, path::PathBuf, time::Duration};

use tack_orch::execution::{
    AttemptId as DomainAttemptId, AttemptSnapshot, EnvironmentValue, ExecutionRequestSnapshot,
    RunnerId as DomainRunnerId,
};
use tack_runner::{
    SecretStore,
    client::{
        AttemptId, AttemptLease, AttemptState, ClaimRequestId, ClaimedWork, FencingToken,
        HarnessAdapter, HarnessError, RunnerId, Timestamp, Workspace, WorkspaceId,
        engine::ExecutionSpec,
    },
    harness::{
        DESCRIPTORS,
        claude_code::ClaudeCodeGrammar,
        codex::CodexGrammar,
        docket::DocketGrammar,
        env_policy,
        local_process::{
            HarnessDescriptor, HarnessGrammar, Invocation, LocalProcessHarness, ModelSelection,
            RunContext, RunReport,
        },
        opencode::OpencodeGrammar,
        process::{ProcessExit, ProcessLimits, ProcessResult},
        sandbox,
    },
};

mod common;
use common::temp_dir as root;

static FAKE: HarnessDescriptor = HarnessDescriptor {
    kind: "environment-fake",
    program: concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/harness/fixtures/fake_harness.sh"
    ),
    model_selection: ModelSelection::Optional,
    inherited_env: &["PATH"],
    probe_notes: &[],
    credential_note: "",
    reports_process_groups: false,
    reports_progress: false,
    sandbox_binds: &[],
    readiness: None,
    models_docs: "",
    sign_in_docs: "",
};

/// The shared fake harness in `spawn_child` mode: it writes `marker` the
/// moment it runs, so a missing marker proves no process started.
struct MarkerGrammar {
    marker: PathBuf,
}

impl HarnessGrammar for MarkerGrammar {
    fn descriptor(&self) -> &'static HarnessDescriptor {
        &FAKE
    }

    fn capabilities(&self) -> tack_orch::execution::FeatureCapabilities {
        unreachable!("the capability table is not read by this test")
    }

    fn invocation(&self, _run: &RunContext<'_>) -> Result<Invocation, HarnessError> {
        Ok(Invocation {
            env: [
                ("TACK_FAKE_HARNESS_MODE", "spawn_child".to_owned()),
                (
                    "TACK_FAKE_HARNESS_PIDFILE",
                    self.marker.display().to_string(),
                ),
            ]
            .map(|(name, value)| (name.to_owned(), value))
            .into(),
            ..Default::default()
        })
    }

    fn report(
        &self,
        _run: &RunContext<'_>,
        _result: &ProcessResult,
    ) -> tack_runner::harness::local_process::RunReport {
        unreachable!("the attempt is cancelled, never waited on")
    }
}

fn spec(workspace: PathBuf, environment: &[(&str, EnvironmentValue)]) -> ExecutionSpec {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/contracts/runner-v1/claim.response.json"
    ))
    .expect("claim fixture");
    let mut request: ExecutionRequestSnapshot =
        serde_json::from_value(fixture["request"].clone()).expect("request snapshot");
    let mut attempt: AttemptSnapshot =
        serde_json::from_value(fixture["attempt"].clone()).expect("attempt snapshot");
    request.requested_harness_kind = tack_orch::execution::HarnessKind::new(FAKE.kind);
    request.repository.base_revision = "0123456789abcdef".into();
    request.environment = environment
        .iter()
        .map(|(name, value)| ((*name).to_owned(), value.clone()))
        .collect();
    attempt.attempt_id = DomainAttemptId::new("attempt-env");
    attempt.runner_id = DomainRunnerId::new("runner-env");
    attempt.base_revision = request.repository.base_revision.clone();
    ExecutionSpec {
        work: ClaimedWork {
            claim_request_id: ClaimRequestId::new("claim-env"),
            lease: AttemptLease {
                attempt_id: AttemptId::new("attempt-env"),
                runner_id: RunnerId::new("runner-env"),
                fencing_token: FencingToken(1),
                attempt_number: 1,
                state: AttemptState::Leased,
                issued_at: Timestamp::new("2026-08-07T12:00:00Z"),
                expires_at: Timestamp::new("2026-08-07T12:01:00Z"),
            },
            request,
            attempt,
        },
        workspace: Workspace {
            attempt_id: AttemptId::new("attempt-env"),
            id: WorkspaceId::new("ws_env"),
            path: workspace,
            base_revision: "0123456789abcdef".into(),
        },
    }
}

fn literal(value: &str) -> EnvironmentValue {
    EnvironmentValue {
        value: Some(value.to_owned()),
        secret_reference: None,
        additional: BTreeMap::new(),
    }
}

fn reference(reference: &str) -> EnvironmentValue {
    EnvironmentValue {
        value: None,
        secret_reference: Some(reference.to_owned()),
        additional: BTreeMap::new(),
    }
}

struct Fixture {
    _dir: tempfile::TempDir,
    marker: PathBuf,
    workspace: PathBuf,
    harness: LocalProcessHarness<MarkerGrammar>,
}

fn fixture(request_may_reference: &[&str]) -> Fixture {
    let dir = root("agent-boundaries");
    let marker = dir.path().join("marker");
    let workspace = dir.path().join("workspace");
    std::fs::create_dir_all(&workspace).expect("workspace");
    let secrets = SecretStore::file(dir.path().join("secrets.json"));
    secrets.set("gw", "gateway-key").expect("store a secret");
    let harness = LocalProcessHarness::discover(
        MarkerGrammar {
            marker: marker.clone(),
        },
        ProcessLimits::new(1_000_000, 1_000_000, Duration::from_secs(60)),
        dir.path().join("staging"),
        secrets,
    )
    .expect("the fake harness script is locatable")
    .with_environment(
        request_may_reference
            .iter()
            .map(|name| (*name).to_owned())
            .collect(),
        BTreeMap::new(),
    );
    Fixture {
        _dir: dir,
        marker,
        workspace,
        harness,
    }
}

async fn rejected_reason(fixture: &Fixture, environment: &[(&str, EnvironmentValue)]) -> String {
    let spec = spec(fixture.workspace.clone(), environment);
    match fixture.harness.start(&spec).await {
        Err(HarnessError::Rejected { reason }) => {
            assert!(
                !fixture.marker.exists(),
                "a rejected request must not start a process"
            );
            reason
        }
        Err(other) => panic!("expected a rejection, got {other:?}"),
        Ok(_) => panic!("expected a rejection, the request started"),
    }
}

#[test]
fn env_policy_refuses_the_names_that_steer_the_runner() {
    for name in [
        "HOME",
        "PATH",
        "GIT_SSH_COMMAND",
        "GIT_CONFIG_GLOBAL",
        "SSH_AUTH_SOCK",
        "LD_PRELOAD",
        "DYLD_INSERT_LIBRARIES",
        "PYTHONPATH",
        "PYTHONSTARTUP",
        "NODE_OPTIONS",
        "PERL5OPT",
        "RUBYOPT",
        "BASH_ENV",
        "ENV",
        "OPENAI_BASE_URL",
        "ANTHROPIC_BASE_URL",
    ] {
        assert!(env_policy::refuse(name).is_some(), "{name} must be refused");
    }
    for name in [
        "K",
        "API_KEY",
        "HOMEPAGE",
        "ENVIRONMENT",
        "NODE_ENV",
        "BASE_URL",
        "TERM",
    ] {
        assert_eq!(env_policy::refuse(name), None, "{name} must be allowed");
    }
}

#[tokio::test]
async fn a_request_naming_ld_preload_is_rejected_before_any_process_starts() {
    let fixture = fixture(&[]);
    let reason = rejected_reason(&fixture, &[("LD_PRELOAD", literal("x"))]).await;
    assert!(
        reason.starts_with("environment_name_refused: LD_PRELOAD ("),
        "{reason}"
    );
}

#[tokio::test]
async fn a_request_cannot_reference_an_env_secret() {
    let fixture = fixture(&["HOME", "gw"]);
    let reason = rejected_reason(&fixture, &[("K", reference("env:HOME"))]).await;
    assert_eq!(reason, "secret_reference_scheme_refused");
}

#[tokio::test]
async fn a_request_references_only_the_store_secrets_the_runner_lists() {
    let closed = fixture(&[]);
    let reason = rejected_reason(&closed, &[("K", reference("store:gw"))]).await;
    assert_eq!(reason, "secret_reference_not_allowed: gw");

    let open = fixture(&["gw"]);
    let spec = spec(open.workspace.clone(), &[("K", reference("store:gw"))]);
    let handle = open
        .harness
        .start(&spec)
        .await
        .expect("a listed secret starts");
    for _ in 0..200 {
        if open.marker.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let started = open.marker.exists();
    open.harness.cancel(&handle).await.expect("cancel");
    assert!(started, "the permitted request started a process");
}

static RUN_FAKE: HarnessDescriptor = HarnessDescriptor {
    kind: "claude-code",
    program: concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/harness/fixtures/fake_harness.sh"
    ),
    model_selection: ModelSelection::Optional,
    inherited_env: &["PATH"],
    probe_notes: &[],
    credential_note: "",
    reports_process_groups: false,
    reports_progress: false,
    sandbox_binds: &[".claude"],
    readiness: None,
    models_docs: "",
    sign_in_docs: "",
};

/// Runs the shared fake harness to completion in its default `success` mode.
struct RunGrammar;

impl HarnessGrammar for RunGrammar {
    fn descriptor(&self) -> &'static HarnessDescriptor {
        &RUN_FAKE
    }

    fn capabilities(&self) -> tack_orch::execution::FeatureCapabilities {
        let value = || {
            tack_runner::harness::local_process::capability(
                tack_orch::execution::CapabilitySupport::Unsupported,
                "not read by this test",
            )
        };
        tack_orch::execution::FeatureCapabilities {
            cancel: value(),
            resume: value(),
            decisions: value(),
            artifacts: value(),
            usage: value(),
            additional: BTreeMap::new(),
        }
    }

    fn invocation(&self, _run: &RunContext<'_>) -> Result<Invocation, HarnessError> {
        Ok(Invocation::default())
    }

    fn report(&self, _run: &RunContext<'_>, _result: &ProcessResult) -> RunReport {
        RunReport::verdict(true, serde_json::json!({}))
    }
}

#[tokio::test]
async fn a_run_log_never_follows_a_workspace_symlink() {
    let dir = root("run-log-symlink");
    let outside = dir.path().join("outside.txt");
    std::fs::write(&outside, "untouched").expect("outside file");
    let workspace = dir.path().join("workspace");
    std::fs::create_dir_all(workspace.join(".tack-runner")).expect("workspace");
    std::os::unix::fs::symlink(&outside, workspace.join(".tack-runner/claude-code-run.log"))
        .expect("symlink");

    let secrets = SecretStore::file(dir.path().join("secrets.json"));
    let harness = LocalProcessHarness::discover(
        RunGrammar,
        ProcessLimits::new(1_000_000, 1_000_000, Duration::from_secs(60)),
        dir.path().join("staging"),
        secrets,
    )
    .expect("the fake harness script is locatable");
    let mut spec = spec(workspace, &[]);
    spec.work.request.requested_harness_kind =
        tack_orch::execution::HarnessKind::new("claude-code");

    let handle = harness.start(&spec).await.expect("the run starts");
    let outcome = harness.wait(&handle).await.expect("the run finishes");
    assert_eq!(outcome.terminal_state, AttemptState::Succeeded);
    assert_eq!(
        std::fs::read_to_string(&outside).expect("outside file"),
        "untouched",
        "the run log wrote through the workspace symlink"
    );
    assert!(
        dir.path()
            .join("staging/runs/617474656d70742d656e76/claude-code-run.log")
            .exists(),
        "the run log is not under the state directory"
    );
    assert_eq!(outcome.terminal_reason["artifact"]["kind"], "log");
}

#[test]
fn a_subdirectory_that_escapes_the_workspace_is_rejected_before_spawn() {
    let dir = root("subdirectory-escape");
    let workspace = dir.path().join("workspace");
    std::fs::create_dir_all(&workspace).expect("workspace");
    let mut spec = spec(workspace, &[]);
    spec.work.request.repository.subdirectory = Some("../..".to_owned());
    let run = RunContext {
        spec: &spec,
        scratch: dir.path(),
        program: None,
    };
    for (kind, result) in [
        ("codex", CodexGrammar.invocation(&run)),
        ("opencode", OpencodeGrammar.invocation(&run)),
    ] {
        match result {
            Err(HarnessError::Rejected { reason }) => {
                assert_eq!(reason, "subdirectory_escapes_workspace", "{kind}")
            }
            other => panic!("{kind}: expected a rejection, got {other:?}"),
        }
    }
}

/// The shared fake harness in the given mode; the run succeeds when it exits 0.
struct ModeGrammar(Vec<(&'static str, String)>);

impl HarnessGrammar for ModeGrammar {
    fn descriptor(&self) -> &'static HarnessDescriptor {
        &RUN_FAKE
    }

    fn capabilities(&self) -> tack_orch::execution::FeatureCapabilities {
        RunGrammar.capabilities()
    }

    fn invocation(&self, _run: &RunContext<'_>) -> Result<Invocation, HarnessError> {
        Ok(Invocation {
            env: self
                .0
                .iter()
                .map(|(name, value)| ((*name).to_owned(), value.clone()))
                .collect(),
            ..Default::default()
        })
    }

    fn report(&self, _run: &RunContext<'_>, result: &ProcessResult) -> RunReport {
        RunReport::verdict(
            matches!(result.exit, ProcessExit::Exited(0)),
            serde_json::json!({}),
        )
    }
}

fn claude_spec(workspace: PathBuf) -> ExecutionSpec {
    let mut spec = spec(workspace, &[]);
    spec.work.request.requested_harness_kind =
        tack_orch::execution::HarnessKind::new("claude-code");
    spec
}

fn sandboxed_harness<G: HarnessGrammar>(
    grammar: G,
    state: &std::path::Path,
    sandbox: sandbox::Sandbox,
) -> LocalProcessHarness<G> {
    LocalProcessHarness::discover(
        grammar,
        ProcessLimits::new(1_000_000, 1_000_000, Duration::from_secs(60)),
        state.join("staging"),
        SecretStore::file(state.join("secrets.json")),
    )
    .expect("the fake harness script is locatable")
    .with_sandbox(sandbox)
}

#[test]
fn sandbox_detect_reports_none_without_bwrap_on_path() {
    let empty = root("sandbox-empty-path");
    let found = sandbox::detect(Some(empty.path().as_os_str()), None);
    assert_eq!(found.kind, sandbox::SandboxKind::None);
    assert_eq!(found.reason, "bwrap not on PATH");
    assert_eq!(sandbox::detect(None, None).kind, sandbox::SandboxKind::None);
}

#[test]
fn sandbox_binds_only_absolute_paths() {
    let dir = root("sandbox-relative");
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).expect("directory");
    let bwrap = bin.join("bwrap");
    std::fs::write(&bwrap, "#!/bin/sh\n").expect("fake bwrap");
    std::fs::set_permissions(&bwrap, std::os::unix::fs::PermissionsExt::from_mode(0o755))
        .expect("executable");
    let home = dir.path().join("home");
    let found = sandbox::detect(Some(bin.as_os_str()), Some(&home));
    let (_, argv) = found
        .wrap(
            std::path::Path::new("./harness"),
            &[],
            std::path::Path::new("./storage/workspace"),
            std::path::Path::new("./storage/scratch"),
            &[],
            sandbox::SharedFiles::default(),
        )
        .expect("wrapped");
    // bwrap resolves a relative path against the workspace it runs in, so a
    // storage directory configured relative to the runner would name nothing.
    let paths: Vec<&String> = argv
        .windows(3)
        .filter(|w| w[0] == "--bind" || w[0] == "--ro-bind")
        .flat_map(|w| [&w[1], &w[2]])
        .chain(argv.iter().skip_while(|a| *a != "--").nth(1))
        .collect();
    assert!(!paths.is_empty());
    for path in paths {
        assert!(path.starts_with('/'), "{path} is relative in {argv:?}");
    }
}

#[tokio::test]
async fn sandbox_wraps_the_spawn_in_bwrap_and_never_binds_the_state_dir() {
    let dir = root("sandbox-argv");
    let state = dir.path().join("state");
    let home = dir.path().join("home");
    let bin = dir.path().join("bin");
    let workspace = dir.path().join("workspace");
    for path in [&state, &home.join(".claude"), &bin, &workspace] {
        std::fs::create_dir_all(path).expect("directory");
    }
    std::fs::write(state.join("token"), "secret").expect("state file");
    let argv_file = dir.path().join("argv");
    let bwrap = bin.join("bwrap");
    std::fs::write(
        &bwrap,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\n",
            argv_file.display()
        ),
    )
    .expect("fake bwrap");
    std::fs::set_permissions(&bwrap, std::os::unix::fs::PermissionsExt::from_mode(0o755))
        .expect("executable");

    let found = sandbox::detect(Some(bin.as_os_str()), Some(&home));
    assert_eq!(found.kind, sandbox::SandboxKind::Bwrap);
    let harness = sandboxed_harness(RunGrammar, &state, found);
    let spec = claude_spec(workspace.clone());
    let handle = harness.start(&spec).await.expect("the run starts");
    assert_eq!(
        harness.sandbox(&handle).await.as_deref(),
        Some("bwrap"),
        "the start report names the sandbox"
    );
    harness.wait(&handle).await.expect("the run finishes");

    let argv = std::fs::read_to_string(&argv_file).expect("bwrap ran");
    let args: Vec<&str> = argv.lines().collect();
    let bound = |source: &std::path::Path| {
        let source = source.display().to_string();
        args.windows(3)
            .any(|w| w[0] == "--bind" && w[1] == source && w[2] == source)
    };
    assert!(bound(&workspace), "the workspace is bound: {args:?}");
    assert!(
        bound(&home.join(".claude")),
        "the claude-code bind: {args:?}"
    );
    assert!(
        args.iter()
            .all(|arg| !arg.contains(state.join("token").to_str().unwrap())
                && *arg != state.to_str().unwrap()),
        "the state dir is not bound: {args:?}"
    );
    assert!(args.contains(&"--die-with-parent") && args.contains(&"--unshare-pid"));
    assert_eq!(
        args.iter()
            .position(|arg| *arg == "--")
            .map(|at| args[at + 1]),
        Some(env!("CARGO_MANIFEST_DIR").to_owned() + "/src/harness/fixtures/fake_harness.sh")
            .as_deref()
    );
}

#[tokio::test]
async fn sandbox_hides_the_state_dir_from_a_real_bwrap_run() {
    let (path, _) = tack_runner::harness::locate::snapshot();
    if sandbox::detect(path.as_deref(), None).kind != sandbox::SandboxKind::Bwrap {
        if std::env::var("TACK_REQUIRE_BWRAP").as_deref() == Ok("1") {
            panic!("bwrap required by TACK_REQUIRE_BWRAP but not on PATH");
        }
        println!("skipped: bwrap is not on PATH");
        return;
    }
    let dir = root("sandbox-real");
    let state = dir.path().join("state");
    let home = dir.path().join("home");
    let workspace = dir.path().join("workspace");
    for path in [&state, &home, &workspace] {
        std::fs::create_dir_all(path).expect("directory");
    }
    std::fs::write(state.join("token"), "secret").expect("state file");
    std::fs::write(workspace.join("visible"), "ok").expect("workspace file");
    let found = sandbox::detect(path.as_deref(), Some(&home));

    let run = |env: Vec<(&'static str, String)>| {
        let (found, state, workspace) = (found.clone(), state.clone(), workspace.clone());
        async move {
            let harness = sandboxed_harness(ModeGrammar(env), &state, found);
            let handle = harness
                .start(&claude_spec(workspace))
                .await
                .expect("starts");
            harness
                .wait(&handle)
                .await
                .expect("finishes")
                .terminal_state
        }
    };
    let read = |path: &std::path::Path| {
        vec![
            ("TACK_FAKE_HARNESS_MODE", "read_relative".to_owned()),
            ("TACK_FAKE_HARNESS_READ_PATH", path.display().to_string()),
        ]
    };
    assert_eq!(
        run(read(&workspace.join("visible"))).await,
        AttemptState::Succeeded,
        "the sandboxed harness runs and sees the workspace"
    );
    assert_eq!(
        run(read(&state.join("token"))).await,
        AttemptState::Failed,
        "the state dir is not visible inside the sandbox"
    );
    let written = workspace.join("written");
    assert_eq!(
        run(vec![
            ("TACK_FAKE_HARNESS_MODE", "write_scratch_file".to_owned()),
            (
                "TACK_FAKE_HARNESS_SCRATCH_DIR",
                written.display().to_string()
            ),
        ])
        .await,
        AttemptState::Succeeded
    );
    assert!(written.join("marker").exists(), "the workspace is writable");
}

/// The shared fake under `descriptor`, and whether it can ask before it acts.
struct DescriptorGrammar(&'static HarnessDescriptor, bool);

impl HarnessGrammar for DescriptorGrammar {
    fn descriptor(&self) -> &'static HarnessDescriptor {
        self.0
    }

    fn capabilities(&self) -> tack_orch::execution::FeatureCapabilities {
        let mut capabilities = RunGrammar.capabilities();
        if self.1 {
            capabilities.decisions = tack_runner::harness::local_process::capability(
                tack_orch::execution::CapabilitySupport::Supported,
                "asks in this test",
            );
        }
        capabilities
    }

    fn invocation(&self, _run: &RunContext<'_>) -> Result<Invocation, HarnessError> {
        Ok(Invocation::default())
    }

    fn report(&self, _run: &RunContext<'_>, result: &ProcessResult) -> RunReport {
        RunReport::verdict(
            matches!(result.exit, ProcessExit::Exited(0)),
            serde_json::json!({}),
        )
    }
}

#[tokio::test]
async fn a_harness_that_does_not_start_in_bwrap_runs_unsandboxed_with_the_reason() {
    let dir = root("sandbox-per-harness");
    let (state, home, bin, workspace) = (
        dir.path().join("state"),
        dir.path().join("home"),
        dir.path().join("bin"),
        dir.path().join("workspace"),
    );
    for path in [&state, &home, &bin, &workspace] {
        std::fs::create_dir_all(path).expect("directory");
    }
    let executable = |path: &std::path::Path, text: String| {
        std::fs::write(path, text).expect("script");
        std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .expect("executable");
    };
    let breaks = bin.join("breaks-in-bwrap");
    executable(&breaks, "#!/bin/sh\necho 1.0.0\n".to_owned());
    let works = bin.join("works-in-bwrap");
    executable(&works, "#!/bin/sh\necho 1.0.0\n".to_owned());
    let log = dir.path().join("bwrap-calls");
    executable(
        &bin.join("bwrap"),
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> {log}\nwhile [ \"$1\" != -- ]; do shift; done\nshift\ncase \"$1\" in\n  *breaks-in-bwrap) echo 'execvp: No such file or directory' >&2; echo second line >&2; exit 1 ;;\nesac\nexec \"$@\"\n",
            log = log.display()
        ),
    );
    let broken: &'static HarnessDescriptor = Box::leak(Box::new(HarnessDescriptor {
        kind: "breaks-in-bwrap",
        program: Box::leak(breaks.display().to_string().into_boxed_str()),
        ..FAKE
    }));
    let working: &'static HarnessDescriptor = Box::leak(Box::new(HarnessDescriptor {
        kind: "works-in-bwrap",
        program: Box::leak(works.display().to_string().into_boxed_str()),
        ..FAKE
    }));
    let machine = sandbox::detect(Some(bin.as_os_str()), Some(&home));
    assert_eq!(machine.kind, sandbox::SandboxKind::Bwrap);
    let discover = |descriptor: &'static HarnessDescriptor| {
        LocalProcessHarness::discover(
            DescriptorGrammar(descriptor, true),
            ProcessLimits::new(1_000_000, 1_000_000, Duration::from_secs(60)),
            state.join("staging"),
            SecretStore::file(state.join("secrets.json")),
        )
        .expect("the fake program is locatable")
        .with_probed_sandbox(machine.clone())
    };
    let (broken_harness, working_harness) = (discover(broken), discover(working));

    let broken_report = tack_runner::harness::HarnessProbe::probe(&broken_harness)
        .await
        .additional;
    assert_eq!(broken_report["sandbox"], "none");
    assert_eq!(
        broken_report["sandbox_reason"],
        "breaks-in-bwrap does not start inside bwrap: execvp: No such file or directory"
    );
    let working_report = tack_runner::harness::HarnessProbe::probe(&working_harness)
        .await
        .additional;
    assert_eq!(working_report["sandbox"], "bwrap");

    let calls_before = std::fs::read_to_string(&log).expect("bwrap was probed");
    let mut spec = spec(workspace, &[]);
    spec.work.request.requested_harness_kind =
        tack_orch::execution::HarnessKind::new("breaks-in-bwrap");
    let handle = broken_harness.start(&spec).await.expect("the run starts");
    assert_eq!(
        broken_harness.sandbox(&handle).await.as_deref(),
        Some("none")
    );
    broken_harness
        .wait(&handle)
        .await
        .expect("the run finishes");
    assert_eq!(
        std::fs::read_to_string(&log).expect("log"),
        calls_before,
        "the unsandboxed spawn did not go through bwrap"
    );
}

#[tokio::test]
async fn an_agent_without_a_sandbox_that_cannot_ask_does_not_run() {
    let dir = root("unconfined-cannot-ask");
    let (state, bin, workspace) = (
        dir.path().join("state"),
        dir.path().join("bin"),
        dir.path().join("workspace"),
    );
    for path in [&state, &bin, &workspace] {
        std::fs::create_dir_all(path).expect("directory");
    }
    let program = bin.join("cannot-ask");
    std::fs::write(&program, "#!/bin/sh\necho 1.0.0\n").expect("script");
    std::fs::set_permissions(
        &program,
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .expect("executable");
    let descriptor: &'static HarnessDescriptor = Box::leak(Box::new(HarnessDescriptor {
        kind: "cannot-ask",
        program: Box::leak(program.display().to_string().into_boxed_str()),
        ..FAKE
    }));
    // No bwrap on this PATH: the runner looked and found no sandbox.
    let machine = sandbox::detect(Some(bin.as_os_str()), None);
    assert_eq!(machine.kind, sandbox::SandboxKind::None);
    let discover = |can_ask: bool| {
        LocalProcessHarness::discover(
            DescriptorGrammar(descriptor, can_ask),
            ProcessLimits::new(1_000_000, 1_000_000, Duration::from_secs(60)),
            state.join("staging"),
            SecretStore::file(state.join("secrets.json")),
        )
        .expect("the fake program is locatable")
        .with_probed_sandbox(machine.clone())
    };

    let asks = tack_runner::harness::HarnessProbe::probe(&discover(true)).await;
    assert_eq!(
        asks.probe_error, None,
        "an agent that can ask runs unsandboxed"
    );

    let silent = discover(false);
    let report = tack_runner::harness::HarnessProbe::probe(&silent).await;
    assert_eq!(report.installed_version, "1.0.0");
    let reason = report.probe_error.expect("the probe says it may not run");
    assert!(
        reason.contains("cannot ask before it acts") && reason.contains("bwrap not on PATH"),
        "{reason}"
    );
    let mut spec = spec(workspace, &[]);
    spec.work.request.requested_harness_kind = tack_orch::execution::HarnessKind::new("cannot-ask");
    match silent.start(&spec).await {
        Err(HarnessError::Rejected { reason: refused }) => assert_eq!(refused, reason),
        other => panic!("expected a refusal before spawn, got {other:?}"),
    }
}

const MODEL_CREDENTIAL_VARIABLES: [&str; 7] = [
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_API_KEY",
    "AI_GATEWAY_API_KEY",
    "OPENAI_API_KEY",
    "DOCKET_LLM_API_KEY",
    "DOCKET_LLM_BASE_URL",
];

/// Echoes the model-credential variables the shared fake harness sees.
struct EchoGrammar(&'static HarnessDescriptor);

impl HarnessGrammar for EchoGrammar {
    fn descriptor(&self) -> &'static HarnessDescriptor {
        self.0
    }

    fn capabilities(&self) -> tack_orch::execution::FeatureCapabilities {
        RunGrammar.capabilities()
    }

    fn invocation(&self, _run: &RunContext<'_>) -> Result<Invocation, HarnessError> {
        Ok(Invocation {
            env: [
                ("TACK_FAKE_HARNESS_MODE", "echo_canary".to_owned()),
                (
                    "TACK_FAKE_HARNESS_ECHO_ENV_KEYS",
                    MODEL_CREDENTIAL_VARIABLES.join(","),
                ),
            ]
            .map(|(name, value)| (name.to_owned(), value))
            .into(),
            ..Default::default()
        })
    }

    fn report(&self, _run: &RunContext<'_>, result: &ProcessResult) -> RunReport {
        RunReport::verdict(true, serde_json::json!({ "stdout": result.stdout.text }))
    }
}

/// A real descriptor's environment policy, pointed at the shared fake harness.
fn on_the_fake_harness(real: &HarnessDescriptor) -> &'static HarnessDescriptor {
    Box::leak(Box::new(HarnessDescriptor {
        kind: real.kind,
        program: FAKE.program,
        model_selection: real.model_selection,
        inherited_env: real.inherited_env,
        probe_notes: real.probe_notes,
        credential_note: real.credential_note,
        reports_process_groups: real.reports_process_groups,
        reports_progress: real.reports_progress,
        sandbox_binds: &[],
        readiness: None,
        models_docs: "",
        sign_in_docs: "",
    }))
}

/// Mutates process-wide variables: safe under `cargo nextest` (one process
/// per test), never under a bare `cargo test`.
#[tokio::test]
async fn the_runner_passes_no_model_credential_of_its_own() {
    for name in MODEL_CREDENTIAL_VARIABLES {
        unsafe { std::env::set_var(name, "runner-own-model-credential") };
    }
    for real in DESCRIPTORS {
        let dir = root("no-model-credential");
        let workspace = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace).expect("workspace");
        let harness = LocalProcessHarness::discover(
            EchoGrammar(on_the_fake_harness(real)),
            ProcessLimits::new(1_000_000, 1_000_000, Duration::from_secs(60)),
            dir.path().join("staging"),
            SecretStore::file(dir.path().join("secrets.json")),
        )
        .expect("the fake harness script is locatable");
        let mut spec = spec(workspace.clone(), &[]);
        spec.work.request.requested_harness_kind =
            tack_orch::execution::HarnessKind::new(real.kind);

        let handle = harness.start(&spec).await.expect("the run starts");
        let outcome = harness.wait(&handle).await.expect("the run finishes");
        let stdout = outcome.terminal_reason["stdout"].as_str().unwrap_or("");
        for name in MODEL_CREDENTIAL_VARIABLES {
            assert!(
                stdout.lines().any(|line| line == format!("env:{name}=")),
                "{}: {name} was not echoed empty: {stdout}",
                real.kind
            );
        }
        assert!(
            !stdout.contains("runner-own-model-credential"),
            "{}",
            real.kind
        );

        // The grammar's own part of the spawn adds none either.
        let run = RunContext {
            spec: &spec,
            scratch: dir.path(),
            program: None,
        };
        for (kind, invocation) in [
            ("codex", CodexGrammar.invocation(&run)),
            ("claude-code", ClaudeCodeGrammar.invocation(&run)),
            ("docket", DocketGrammar.invocation(&run)),
        ] {
            if let Ok(invocation) = invocation {
                for name in MODEL_CREDENTIAL_VARIABLES {
                    assert!(!invocation.env.contains_key(name), "{kind}: {name}");
                }
            }
        }
    }
    for name in MODEL_CREDENTIAL_VARIABLES {
        unsafe { std::env::remove_var(name) };
    }
}

/// opencode's `debug` commands, answered from the captured fixture with a
/// seeded key, and a harness that reports what its scratch home holds.
struct LoginGrammar(&'static HarnessDescriptor);

impl HarnessGrammar for LoginGrammar {
    fn descriptor(&self) -> &'static HarnessDescriptor {
        self.0
    }

    fn capabilities(&self) -> tack_orch::execution::FeatureCapabilities {
        RunGrammar.capabilities()
    }

    fn invocation(&self, run: &RunContext<'_>) -> Result<Invocation, HarnessError> {
        OpencodeGrammar.invocation(run)
    }

    fn report(&self, _run: &RunContext<'_>, result: &ProcessResult) -> RunReport {
        RunReport::verdict(true, serde_json::json!({ "stdout": result.stdout.text }))
    }
}

fn files_under(root: &std::path::Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(root).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(files_under(&path));
        } else {
            found.push(path);
        }
    }
    found
}

/// Mutates process-wide variables: safe under `cargo nextest` (one process
/// per test), never under a bare `cargo test`.
#[tokio::test]
async fn opencode_gets_its_login_and_providers_and_nothing_else() {
    const SEEDED_KEY: &str = "sk-seeded-key-7731";
    let dir = root("opencode-login");
    let workspace = dir.path().join("workspace");
    let home = dir.path().join("home");
    let data = home.join(".local/share/opencode");
    let bin = dir.path().join("bin");
    for path in [&workspace, &data, &bin] {
        std::fs::create_dir_all(path).expect("directory");
    }
    std::fs::write(data.join("auth.json"), "login-token-4410").expect("auth.json");
    std::fs::write(data.join("sibling.json"), "sibling-contents-9921").expect("sibling");

    let fixture = include_str!("../src/harness/fixtures/opencode/1.18.30/debug-config.json");
    let mut person: serde_json::Value = serde_json::from_str(fixture).expect("fixture config");
    person["provider"]["llamacpp"]["options"]["apiKey"] = SEEDED_KEY.into();
    person["provider"]["llamacpp"]["options"]["headers"] =
        serde_json::json!({ "x-key": "header-secret-5512" });
    std::fs::write(dir.path().join("config.json"), person.to_string()).expect("config");
    let opencode = bin.join("opencode");
    std::fs::write(
        &opencode,
        format!(
            "#!/bin/sh\ncase \"$2\" in\nconfig) cat {config};;\npaths) echo \"data {data}\";;\nesac\n",
            config = dir.path().join("config.json").display(),
            data = data.display(),
        ),
    )
    .expect("fake opencode");
    std::fs::set_permissions(
        &opencode,
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .expect("executable");
    let path = format!("{}:/usr/bin:/bin", bin.display());
    unsafe {
        std::env::set_var("PATH", &path);
        std::env::set_var("HOME", &home);
    }

    let mut spec = spec(workspace.clone(), &[]);
    spec.work.request.requested_harness_kind = tack_orch::execution::HarnessKind::new("opencode");
    spec.work.request.permission_policy.network = true;
    spec.work.request.permission_policy.approvals = None;
    spec.work.request.requested_model = Some("llamacpp/chosen".to_owned());
    let run = RunContext {
        spec: &spec,
        scratch: dir.path(),
        program: Some(&opencode),
    };
    let invocation = OpencodeGrammar.invocation(&run).expect("an invocation");
    let injected: serde_json::Value =
        serde_json::from_str(&invocation.env["OPENCODE_CONFIG_CONTENT"]).expect("injected config");
    assert_eq!(injected["provider"], person["provider"]);
    assert_eq!(injected["model"], person["model"]);
    assert_eq!(injected["small_model"], person["small_model"]);
    let mut keys: Vec<&String> = injected.as_object().expect("an object").keys().collect();
    keys.sort();
    assert_eq!(keys, ["model", "permission", "provider", "small_model"]);
    let mut permission: Vec<&String> = injected["permission"]
        .as_object()
        .expect("permissions")
        .keys()
        .collect();
    permission.sort();
    assert_eq!(permission, ["bash", "edit", "task", "webfetch"]);
    assert_ne!(injected["permission"], person["permission"]);
    assert!(invocation.args.iter().any(|argument| argument == "--pure"));
    assert!(invocation.args.join(" ").ends_with("-m llamacpp/chosen"));
    for name in MODEL_CREDENTIAL_VARIABLES {
        assert!(!invocation.env.contains_key(name), "{name}");
    }

    // No model anywhere: a refusal, not a guess.
    let mut without_model = person.clone();
    without_model
        .as_object_mut()
        .expect("object")
        .remove("model");
    std::fs::write(dir.path().join("config.json"), without_model.to_string()).expect("config");
    spec.work.request.requested_model = None;
    let run = RunContext {
        spec: &spec,
        scratch: dir.path(),
        program: Some(&opencode),
    };
    match OpencodeGrammar.invocation(&run) {
        Err(HarnessError::Rejected { reason }) => {
            assert!(reason.contains("opencode has no default model"), "{reason}")
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
    std::fs::write(dir.path().join("config.json"), person.to_string()).expect("config");
    spec.work.request.requested_model = Some("llamacpp/chosen".to_owned());

    // The run itself: what the harness can read, and what the log keeps.
    let probe = dir.path().join("probe.sh");
    // The located binary is asked for its config first, so the probe hands
    // `debug …` to the fake opencode and is the run otherwise.
    std::fs::write(
        &probe,
        format!(
            "#!/bin/sh\nif [ \"$1\" = debug ]; then exec {opencode} \"$@\"; fi\ncat > /dev/null\n\
             echo \"auth=$(cat \"$HOME/.local/share/opencode/auth.json\" 2>&1)\"\n\
             echo \"sibling=$(cat \"$HOME/.local/share/opencode/sibling.json\" 2>&1)\"\n\
             echo \"config=$OPENCODE_CONFIG_CONTENT\"\n",
            opencode = opencode.display(),
        ),
    )
    .expect("probe");
    std::fs::set_permissions(&probe, std::os::unix::fs::PermissionsExt::from_mode(0o755))
        .expect("executable");
    let probe_path: &'static str = Box::leak(probe.display().to_string().into_boxed_str());
    let descriptor: &'static HarnessDescriptor = Box::leak(Box::new(HarnessDescriptor {
        kind: "opencode",
        program: probe_path,
        model_selection: ModelSelection::Optional,
        inherited_env: &["PATH"],
        probe_notes: &[],
        credential_note: "",
        reports_process_groups: false,
        reports_progress: false,
        sandbox_binds: &[],
        readiness: None,
        models_docs: "",
        sign_in_docs: "",
    }));
    let machine = sandbox::detect(Some(std::ffi::OsStr::new(&path)), Some(&home));
    for (label, sandboxed) in [("unsandboxed", false), ("sandboxed", true)] {
        // Without bwrap the sandboxed run is refused; that refusal has its own test.
        if sandboxed && machine.kind != sandbox::SandboxKind::Bwrap {
            println!("skipped the sandboxed run: bwrap is not on PATH");
            continue;
        }
        let state = dir.path().join(label);
        let harness = LocalProcessHarness::discover(
            LoginGrammar(descriptor),
            ProcessLimits::new(1_000_000, 1_000_000, Duration::from_secs(60)),
            state.join("staging"),
            SecretStore::file(state.join("secrets.json")),
        )
        .expect("the probe is locatable");
        let harness = if sandboxed {
            harness.with_probed_sandbox(machine.clone())
        } else {
            harness
        };
        let handle = harness.start(&spec).await.expect("the run starts");
        if sandboxed && machine.kind == sandbox::SandboxKind::Bwrap {
            assert_eq!(harness.sandbox(&handle).await.as_deref(), Some("bwrap"));
        }
        let outcome = harness.wait(&handle).await.expect("the run finishes");
        let stdout = outcome.terminal_reason["stdout"].as_str().unwrap_or("");
        assert!(
            stdout.contains("auth=login-token-4410"),
            "{label}: {stdout}"
        );
        assert!(
            !stdout.contains("sibling-contents-9921") && stdout.contains("sibling=cat:"),
            "{label}: {stdout}"
        );
        for secret in [SEEDED_KEY, "header-secret-5512"] {
            assert!(!stdout.contains(secret), "{label}: {secret} in {stdout}");
            for file in files_under(&state.join("staging")) {
                let kept = std::fs::read(&file).unwrap_or_default();
                assert!(
                    !String::from_utf8_lossy(&kept).contains(secret),
                    "{label}: {secret} in {}",
                    file.display()
                );
            }
        }
        assert!(stdout.contains("[REDACTED]"), "{label}: {stdout}");
    }
}

/// docket's own invocation, run by the fake harness reading one file: a
/// file of the setup docket is handed (`DOCKET_SETUP_HOME/<name>`), or a
/// path on the host.
struct DocketSetupReader(Result<&'static str, PathBuf>);

impl HarnessGrammar for DocketSetupReader {
    fn descriptor(&self) -> &'static HarnessDescriptor {
        &RUN_FAKE
    }

    fn capabilities(&self) -> tack_orch::execution::FeatureCapabilities {
        RunGrammar.capabilities()
    }

    fn invocation(&self, run: &RunContext<'_>) -> Result<Invocation, HarnessError> {
        let mut invocation = DocketGrammar.invocation(run)?;
        let path = match &self.0 {
            Ok(name) => PathBuf::from(&invocation.env["DOCKET_SETUP_HOME"]).join(name),
            Err(host) => host.clone(),
        };
        invocation.args.clear();
        invocation.env.insert(
            "TACK_FAKE_HARNESS_MODE".to_owned(),
            "read_relative".to_owned(),
        );
        invocation.env.insert(
            "TACK_FAKE_HARNESS_READ_PATH".to_owned(),
            path.display().to_string(),
        );
        Ok(invocation)
    }

    fn report(&self, _run: &RunContext<'_>, result: &ProcessResult) -> RunReport {
        RunReport::verdict(
            matches!(result.exit, ProcessExit::Exited(0)),
            serde_json::json!({}),
        )
    }
}

/// docket's setup home holds pods, traces and an audit log beside its
/// provider catalog, model registry and credential store. Inside the
/// sandbox a docket run reads those three, bound read-only, and nothing
/// else of that home. Mutates `HOME`: safe under `cargo nextest` (one
/// process per test), never under a bare `cargo test`.
#[tokio::test]
async fn docket_reads_its_setup_and_nothing_else() {
    let dir = root("docket-setup");
    let home = dir.path().join("home");
    let setup = home.join(".docket");
    let state = dir.path().join("state");
    let workspace = dir.path().join("workspace");
    for path in [&setup.join("pods/p"), &state, &workspace] {
        std::fs::create_dir_all(path).expect("directory");
    }
    for name in [
        "docket-providers.json",
        "docket-models.json",
        "secrets.json",
        "audit.log",
        "pods/p/trace.jsonl",
    ] {
        std::fs::write(setup.join(name), "{}").expect("setup file");
    }
    unsafe { std::env::set_var("HOME", &home) };

    let mut spec = claude_spec(workspace.clone());
    spec.work.request.permission_policy.tools = vec!["*".to_owned()];
    spec.work.request.permission_policy.network = true;
    spec.work.request.permission_policy.approvals = None;
    let scratch = dir.path().join("scratch");
    let invocation = DocketGrammar
        .invocation(&RunContext {
            spec: &spec,
            scratch: &scratch,
            program: None,
        })
        .expect("an invocation");
    let handed = scratch.join("docket-setup");
    assert_eq!(
        invocation.env["DOCKET_SETUP_HOME"],
        handed.display().to_string()
    );
    assert_eq!(
        invocation.read_only_files,
        [
            "docket-providers.json",
            "docket-models.json",
            "secrets.json"
        ]
        .map(|name| (setup.join(name), handed.join(name)))
    );
    assert!(invocation.shared_files.is_empty());

    let (path, _) = tack_runner::harness::locate::snapshot();
    let found = sandbox::detect(path.as_deref(), Some(&home));
    if found.kind != sandbox::SandboxKind::Bwrap {
        if std::env::var("TACK_REQUIRE_BWRAP").as_deref() == Ok("1") {
            panic!("bwrap required by TACK_REQUIRE_BWRAP but not on PATH");
        }
        println!("skipped the sandboxed half: bwrap is not on PATH");
        return;
    }
    let (_, argv) = found
        .wrap(
            std::path::Path::new("/bin/true"),
            &[],
            &workspace,
            &scratch,
            &[],
            sandbox::SharedFiles {
                writable: &invocation.shared_files,
                read_only: &invocation.read_only_files,
            },
        )
        .expect("wrapped");
    for (source, destination) in &invocation.read_only_files {
        assert!(
            argv.windows(3).any(|bind| bind
                == [
                    "--ro-bind".to_owned(),
                    source.display().to_string(),
                    destination.display().to_string()
                ]),
            "{source:?} is not bound read-only: {argv:?}"
        );
    }

    let run = |target: Result<&'static str, PathBuf>| {
        let (found, state, spec) = (found.clone(), state.clone(), spec.clone());
        async move {
            let harness = sandboxed_harness(DocketSetupReader(target), &state, found);
            let handle = harness.start(&spec).await.expect("starts");
            harness
                .wait(&handle)
                .await
                .expect("finishes")
                .terminal_state
        }
    };
    for name in [
        "docket-providers.json",
        "docket-models.json",
        "secrets.json",
    ] {
        assert_eq!(run(Ok(name)).await, AttemptState::Succeeded, "{name}");
    }
    for name in ["audit.log", "pods/p/trace.jsonl", "secrets.json"] {
        assert_eq!(
            run(Err(setup.join(name))).await,
            AttemptState::Failed,
            "{name} is visible at its own path"
        );
    }
}
