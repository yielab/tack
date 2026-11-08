//! What a request may and may not put in a run's environment.

use std::{collections::BTreeMap, path::PathBuf, time::Duration};

use tack_protocol::execution::{
    Approvals, AttemptId as DomainAttemptId, AttemptSnapshot, EnvironmentValue,
    ExecutionRequestSnapshot, ParityCell, ParityRule, RunnerId as DomainRunnerId, validate_parity,
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

use crate::common;
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
    cancels_over_api: false,
    reports_progress: false,
    sandbox_binds: &[],
    model_endpoints: &[],
    parity: &[],
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

    fn capabilities(&self) -> tack_protocol::execution::FeatureCapabilities {
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
    request.requested_harness_kind = tack_protocol::execution::HarnessKind::new(FAKE.kind);
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
        &tack_runner::ProcessEnv::from_process(),
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
        "http_proxy",
        "no_proxy",
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
        "MY_FLAG",
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
async fn env_policy_refuses_http_proxy_from_a_request() {
    let fixture = fixture(&[]);
    let reason = rejected_reason(&fixture, &[("HTTP_PROXY", literal("x"))]).await;
    assert!(
        reason.starts_with("environment_name_refused: HTTP_PROXY ("),
        "{reason}"
    );
}

#[tokio::test]
async fn env_policy_refuses_https_proxy_from_a_request() {
    let fixture = fixture(&[]);
    let reason = rejected_reason(&fixture, &[("HTTPS_PROXY", literal("x"))]).await;
    assert!(
        reason.starts_with("environment_name_refused: HTTPS_PROXY ("),
        "{reason}"
    );
}

#[tokio::test]
async fn env_policy_refuses_all_proxy_from_a_request() {
    let fixture = fixture(&[]);
    let reason = rejected_reason(&fixture, &[("ALL_PROXY", literal("x"))]).await;
    assert!(
        reason.starts_with("environment_name_refused: ALL_PROXY ("),
        "{reason}"
    );
}

#[tokio::test]
async fn env_policy_refuses_no_proxy_from_a_request() {
    let fixture = fixture(&[]);
    let reason = rejected_reason(&fixture, &[("NO_PROXY", literal("x"))]).await;
    assert!(
        reason.starts_with("environment_name_refused: NO_PROXY ("),
        "{reason}"
    );
}

#[tokio::test]
async fn env_policy_refuses_node_path_from_a_request() {
    let fixture = fixture(&[]);
    let reason = rejected_reason(&fixture, &[("NODE_PATH", literal("x"))]).await;
    assert!(
        reason.starts_with("environment_name_refused: NODE_PATH ("),
        "{reason}"
    );
}

#[tokio::test]
async fn env_policy_refuses_java_tool_options_from_a_request() {
    let fixture = fixture(&[]);
    let reason = rejected_reason(&fixture, &[("JAVA_TOOL_OPTIONS", literal("x"))]).await;
    assert!(
        reason.starts_with("environment_name_refused: JAVA_TOOL_OPTIONS ("),
        "{reason}"
    );
}

#[tokio::test]
async fn env_policy_refuses_claude_config_dir_from_a_request() {
    let fixture = fixture(&[]);
    let reason = rejected_reason(&fixture, &[("CLAUDE_CONFIG_DIR", literal("x"))]).await;
    assert!(
        reason.starts_with("environment_name_refused: CLAUDE_CONFIG_DIR ("),
        "{reason}"
    );
}

#[tokio::test]
async fn env_policy_refuses_codex_home_from_a_request() {
    let fixture = fixture(&[]);
    let reason = rejected_reason(&fixture, &[("CODEX_HOME", literal("x"))]).await;
    assert!(
        reason.starts_with("environment_name_refused: CODEX_HOME ("),
        "{reason}"
    );
}

#[tokio::test]
async fn env_policy_refuses_opencode_config_from_a_request() {
    let fixture = fixture(&[]);
    let reason = rejected_reason(&fixture, &[("OPENCODE_CONFIG", literal("x"))]).await;
    assert!(
        reason.starts_with("environment_name_refused: OPENCODE_CONFIG ("),
        "{reason}"
    );
}

#[tokio::test]
async fn env_policy_refuses_opencode_config_dir_from_a_request() {
    let fixture = fixture(&[]);
    let reason = rejected_reason(&fixture, &[("OPENCODE_CONFIG_DIR", literal("x"))]).await;
    assert!(
        reason.starts_with("environment_name_refused: OPENCODE_CONFIG_DIR ("),
        "{reason}"
    );
}

#[tokio::test]
async fn env_policy_refuses_xdg_base_directories_from_a_request() {
    let fixture = fixture(&[]);
    let reason = rejected_reason(&fixture, &[("XDG_CONFIG_HOME", literal("x"))]).await;
    assert!(
        reason.starts_with("environment_name_refused: XDG_CONFIG_HOME ("),
        "{reason}"
    );
}

#[tokio::test]
async fn env_policy_refuses_npm_config_settings_from_a_request() {
    let fixture = fixture(&[]);
    let reason = rejected_reason(&fixture, &[("npm_config_cache", literal("x"))]).await;
    assert!(
        reason.starts_with("environment_name_refused: npm_config_cache ("),
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
    cancels_over_api: false,
    reports_progress: false,
    sandbox_binds: &[sandbox::ConfigDir {
        name: ".claude",
        read_only: &["settings.json"],
        login: &[".credentials.json"],
    }],
    model_endpoints: &[],
    parity: &[],
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

    fn capabilities(&self) -> tack_protocol::execution::FeatureCapabilities {
        let value = || {
            tack_runner::harness::local_process::capability(
                tack_protocol::execution::CapabilitySupport::Unsupported,
                "not read by this test",
            )
        };
        tack_protocol::execution::FeatureCapabilities {
            proposals: None,
            activity: None,
            converse: None,
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
        &tack_runner::ProcessEnv::from_process(),
    )
    .expect("the fake harness script is locatable");
    let mut spec = spec(workspace, &[]);
    spec.work.request.requested_harness_kind =
        tack_protocol::execution::HarnessKind::new("claude-code");

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
        host: &tack_runner::ProcessEnv::default(),
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

    fn capabilities(&self) -> tack_protocol::execution::FeatureCapabilities {
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
        tack_protocol::execution::HarnessKind::new("claude-code");
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
        &tack_runner::ProcessEnv::from_process(),
    )
    .expect("the fake harness script is locatable")
    .with_probed_sandbox(sandbox)
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
    for name in ["settings.json", ".credentials.json", "history.jsonl"] {
        std::fs::write(home.join(".claude").join(name), "{}").expect("config file");
    }
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
    let bound_by = |flag: &str, source: &std::path::Path| {
        let source = source.display().to_string();
        args.windows(3)
            .any(|w| w[0] == flag && w[1] == source && w[2] == source)
    };
    let bound = |source: &std::path::Path| bound_by("--bind", source);
    assert!(bound(&workspace), "the workspace is bound: {args:?}");
    assert!(
        bound_by("--ro-bind", &home.join(".claude/settings.json")),
        "the settings are read-only: {args:?}"
    );
    assert!(
        bound(&home.join(".claude/.credentials.json")),
        "the login is shared as it is: {args:?}"
    );
    for not_bound in [".claude", ".claude/history.jsonl"] {
        let path = home.join(not_bound).display().to_string();
        assert!(
            args.iter().all(|arg| *arg != path),
            "{not_bound} is the run's own, not the person's: {args:?}"
        );
    }
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

/// `program args…` wrapped by the real `bwrap` as a harness would be, or `None` when this
/// machine has no `bwrap` (unless `TACK_REQUIRE_BWRAP=1`, which makes that a failure).
fn real_bwrap_command(
    dir: &std::path::Path,
    program: &std::path::Path,
    args: &[String],
) -> Option<std::process::Command> {
    let path = tack_runner::ProcessEnv::from_process().search_path();
    let found = sandbox::detect(path.as_deref(), Some(&dir.join("home")));
    if found.kind != sandbox::SandboxKind::Bwrap {
        if std::env::var("TACK_REQUIRE_BWRAP").as_deref() == Ok("1") {
            panic!("bwrap required by TACK_REQUIRE_BWRAP but not on PATH");
        }
        println!("skipped: bwrap is not on PATH");
        return None;
    }
    let (workspace, scratch) = (dir.join("workspace"), dir.join("scratch"));
    for path in [&workspace, &scratch] {
        std::fs::create_dir_all(path).expect("directory");
    }
    let (bwrap, argv) = found
        .wrap(
            program,
            args,
            &workspace,
            &scratch,
            &[],
            sandbox::SharedFiles::default(),
        )
        .expect("wrapped");
    let mut command = std::process::Command::new(bwrap);
    command.args(argv).current_dir(&workspace);
    Some(command)
}

#[test]
fn sandbox_shares_neither_ipc_nor_hostname_namespace_and_holds_no_capabilities() {
    let dir = root("sandbox-namespaces");
    let script = "readlink /proc/self/ns/ipc /proc/self/ns/uts; \
                  grep -E '^Cap(Eff|Prm|Bnd):' /proc/self/status";
    let Some(mut command) = real_bwrap_command(
        dir.path(),
        std::path::Path::new("/bin/sh"),
        &["-c".to_owned(), script.to_owned()],
    ) else {
        return;
    };
    let output = command.output().expect("bwrap runs");
    assert!(
        output.status.success(),
        "the probe ran: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let inside = String::from_utf8_lossy(&output.stdout).into_owned();
    let mut lines = inside.lines();
    for namespace in ["ipc", "uts"] {
        let host = std::fs::read_link(format!("/proc/self/ns/{namespace}")).expect("host ns");
        assert_ne!(
            lines.next().expect("namespace line"),
            host.display().to_string(),
            "the sandbox shares the host's {namespace} namespace"
        );
    }
    for line in lines {
        let (name, value) = line.split_once(':').expect("capability line");
        assert_eq!(value.trim(), "0000000000000000", "{name} is not empty");
    }
}

#[test]
fn sandbox_still_starts_git_node_and_python() {
    let dir = root("sandbox-tools");
    // System directories first: the sandbox shows `/usr` and `/bin`, so a tool found there
    // brings its libraries; one from a version manager is bound by itself.
    let search: Vec<std::path::PathBuf> = ["/usr/bin", "/bin"]
        .map(std::path::PathBuf::from)
        .into_iter()
        .chain(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ))
        .collect();
    for tool in ["git", "node", "python3"] {
        let Some(program) = search
            .iter()
            .map(|directory| directory.join(tool))
            .find(|candidate| candidate.is_file())
        else {
            println!("skipped: {tool} is not installed");
            continue;
        };
        let Some(mut command) = real_bwrap_command(dir.path(), &program, &["--version".to_owned()])
        else {
            return;
        };
        let output = command.output().expect("bwrap runs");
        assert!(
            output.status.success(),
            "{tool} does not start in the sandbox: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[tokio::test]
async fn sandbox_hides_the_state_dir_from_a_real_bwrap_run() {
    let path = tack_runner::ProcessEnv::from_process().search_path();
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

/// What a CLI that is handed its own config directory does there, run for real in `bwrap`: the
/// script tries to rewrite the person's config, then does what a CLI does while it works.
const CONFIG_SCRIPT: &str = r#"#!/bin/sh
dir="$HOME/$TACK_TEST_CONFIG_DIR"
if [ "$TACK_TEST_CASE" = attack ]; then
  ( echo '{"hooks":"evil"}' > "$dir/$TACK_TEST_CONFIG_FILE" ) 2>/dev/null && { echo "rewrote the config" >&2; exit 1; }
  ( rm -f "$dir/$TACK_TEST_CONFIG_FILE" ) 2>/dev/null
  ( mv "$dir/$TACK_TEST_CONFIG_FILE" "$dir/moved" ) 2>/dev/null
  grep -q original "$dir/$TACK_TEST_CONFIG_FILE" || { echo "the config is gone or changed" >&2; exit 1; }
  [ ! -e "$dir/history.jsonl" ] || { echo "the person's history is visible" >&2; exit 1; }
else
  grep -q original "$dir/$TACK_TEST_CONFIG_FILE" || { echo "the config is not readable" >&2; exit 1; }
  mkdir -p "$dir/sessions" && echo turn > "$dir/sessions/one.jsonl" || { echo "no session" >&2; exit 1; }
  echo "turn two" >> "$dir/sessions/one.jsonl" || { echo "session not kept" >&2; exit 1; }
  echo refreshed > "$dir/$TACK_TEST_LOGIN_FILE" || { echo "the login is not refreshed" >&2; exit 1; }
fi
"#;

/// The config script under a descriptor's real binds.
struct ConfigGrammar {
    descriptor: &'static HarnessDescriptor,
    env: Vec<(&'static str, String)>,
}

impl HarnessGrammar for ConfigGrammar {
    fn descriptor(&self) -> &'static HarnessDescriptor {
        self.descriptor
    }

    fn capabilities(&self) -> tack_protocol::execution::FeatureCapabilities {
        RunGrammar.capabilities()
    }

    fn invocation(&self, _run: &RunContext<'_>) -> Result<Invocation, HarnessError> {
        Ok(Invocation {
            env: self
                .env
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

/// `(real descriptor, config file the CLI reads, login file)` for each CLI whose own directory is
/// bound into the sandbox.
fn config_dirs() -> [(&'static HarnessDescriptor, &'static str, &'static str); 2] {
    [
        (
            &tack_runner::harness::claude_code::DESCRIPTOR,
            "settings.json",
            ".credentials.json",
        ),
        (
            &tack_runner::harness::codex::DESCRIPTOR,
            "config.toml",
            "auth.json",
        ),
    ]
}

/// Runs the config script as `descriptor`'s CLI in a real `bwrap` against a home that holds a
/// config, a login and a history, and returns that home for the caller to read; `None` when
/// this machine has no `bwrap` (unless `TACK_REQUIRE_BWRAP=1`, which makes that a failure).
async fn run_config_script(
    name: &str,
    descriptor: &'static HarnessDescriptor,
    config_file: &str,
    login_file: &str,
    case: &str,
) -> Option<(AttemptState, PathBuf, tempfile::TempDir)> {
    let path = tack_runner::ProcessEnv::from_process().search_path();
    if sandbox::detect(path.as_deref(), None).kind != sandbox::SandboxKind::Bwrap {
        if std::env::var("TACK_REQUIRE_BWRAP").as_deref() == Ok("1") {
            panic!("bwrap required by TACK_REQUIRE_BWRAP but not on PATH");
        }
        println!("skipped: bwrap is not on PATH");
        return None;
    }
    let dir = root(&format!("config-{name}-{case}"));
    let (state, home, workspace) = (
        dir.path().join("state"),
        dir.path().join("home"),
        dir.path().join("workspace"),
    );
    let config_dir = home.join(format!(
        ".{}",
        descriptor.kind.replace("claude-code", "claude")
    ));
    for path in [&state, &config_dir, &workspace] {
        std::fs::create_dir_all(path).expect("directory");
    }
    std::fs::write(config_dir.join(config_file), "original config").expect("config");
    std::fs::write(config_dir.join(login_file), "original login").expect("login");
    std::fs::write(config_dir.join("history.jsonl"), "private").expect("history");
    let script = dir.path().join("cli");
    std::fs::write(&script, CONFIG_SCRIPT).expect("script");
    std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755))
        .expect("executable");
    let descriptor: &'static HarnessDescriptor = Box::leak(Box::new(HarnessDescriptor {
        program: Box::leak(script.display().to_string().into_boxed_str()),
        ..*descriptor
    }));
    let grammar = ConfigGrammar {
        descriptor,
        env: vec![
            ("HOME", home.display().to_string()),
            (
                "TACK_TEST_CONFIG_DIR",
                config_dir
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            ),
            ("TACK_TEST_CONFIG_FILE", config_file.to_owned()),
            ("TACK_TEST_LOGIN_FILE", login_file.to_owned()),
            ("TACK_TEST_CASE", case.to_owned()),
        ],
    };
    let found = sandbox::detect(path.as_deref(), Some(&home));
    let harness = sandboxed_harness(grammar, &state, found);
    let mut spec = spec(workspace, &[]);
    spec.work.request.requested_harness_kind =
        tack_protocol::execution::HarnessKind::new(descriptor.kind);
    let handle = harness.start(&spec).await.expect("starts");
    let outcome = harness.wait(&handle).await.expect("finishes");
    // The guard goes back with the home, so the caller reads it before it is deleted.
    let home = dir.path().join("home");
    Some((outcome.terminal_state, home, dir))
}

#[tokio::test]
async fn bind_the_agents_config_cannot_be_rewritten_from_inside_the_sandbox() {
    for (descriptor, config_file, login_file) in config_dirs() {
        let Some((state, home, _guard)) =
            run_config_script("attack", descriptor, config_file, login_file, "attack").await
        else {
            return;
        };
        let kind = descriptor.kind;
        assert_eq!(
            state,
            AttemptState::Succeeded,
            "{kind}: the script saw the config intact"
        );
        let config_dir = home.join(format!(".{}", kind.replace("claude-code", "claude")));
        assert_eq!(
            std::fs::read_to_string(config_dir.join(config_file)).expect("config"),
            "original config",
            "{kind}: the person's config is as it was"
        );
        assert!(
            !config_dir.join("moved").exists(),
            "{kind}: the config was not moved away"
        );
    }
}

#[tokio::test]
async fn bind_the_agents_own_state_and_refreshed_login_still_work() {
    for (descriptor, config_file, login_file) in config_dirs() {
        let Some((state, home, _guard)) =
            run_config_script("twin", descriptor, config_file, login_file, "twin").await
        else {
            return;
        };
        let kind = descriptor.kind;
        assert_eq!(
            state,
            AttemptState::Succeeded,
            "{kind}: the CLI starts and keeps its session"
        );
        let config_dir = home.join(format!(".{}", kind.replace("claude-code", "claude")));
        assert_eq!(
            std::fs::read_to_string(config_dir.join(login_file)).expect("login"),
            "refreshed\n",
            "{kind}: a token refreshed in place reaches the person's login"
        );
        assert!(
            !config_dir.join("sessions").exists(),
            "{kind}: the session lives in the run's scratch home, not the person's directory"
        );
        assert_eq!(
            std::fs::read_to_string(config_dir.join(config_file)).expect("config"),
            "original config"
        );
    }
}

/// The person's home with an `mcp_servers` table in their Codex config and a login in each CLI's
/// directory, as the runner sees it.
fn home_with_integrations() -> (tempfile::TempDir, tack_runner::ProcessEnv) {
    let dir = root("integrations");
    let home = dir.path().join("home");
    for name in [".codex", ".claude"] {
        std::fs::create_dir_all(home.join(name)).expect("directory");
    }
    std::fs::write(
        home.join(".codex/config.toml"),
        "model = \"o3\"\n[mcp_servers.tracker]\ncommand = \"canary-mcp-command\"\nargs = [\"--token\", \"canary-mcp-arg\"]\n\
         [mcp_servers.\"docs.site\"]\nurl = \"https://canary-mcp-url.invalid/mcp\"\n",
    )
    .expect("codex config");
    std::fs::write(
        home.join(".codex/auth.json"),
        r#"{"OPENAI_API_KEY":null,"tokens":{"access_token":"canary-codex-access","refresh_token":"canary-codex-refresh"}}"#,
    )
    .expect("codex login");
    std::fs::write(
        home.join(".claude/.credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"canary-claude-access","refreshToken":"canary-claude-refresh","subscriptionType":"max"}}"#,
    )
    .expect("claude login");
    let env = tack_runner::ProcessEnv::from_pairs([("HOME", home)]);
    (dir, env)
}

fn invocation_of<G: HarnessGrammar>(
    grammar: &G,
    kind: &str,
    host: &tack_runner::ProcessEnv,
    ask: bool,
) -> Invocation {
    let dir = root("invocation");
    let mut spec = spec(dir.path().to_path_buf(), &[]);
    spec.work.request.requested_harness_kind = tack_protocol::execution::HarnessKind::new(kind);
    spec.work.request.permission_policy.approvals =
        ask.then_some(tack_protocol::execution::Approvals::Ask);
    grammar
        .invocation(&RunContext {
            spec: &spec,
            scratch: dir.path(),
            program: None,
            host,
        })
        .expect("invocation")
}

#[test]
fn mcp_servers_in_the_users_codex_config_are_switched_off_by_name() {
    let (_home, host) = home_with_integrations();
    for ask in [false, true] {
        let invocation = invocation_of(&CodexGrammar, "codex", &host, ask);
        let at = invocation
            .args
            .iter()
            .position(|arg| arg == "-c")
            .expect("an override");
        assert_eq!(
            invocation.args[at + 1],
            r#"mcp_servers={"docs.site"={enabled=false},"tracker"={enabled=false}}"#,
            "ask={ask}: {:?}",
            invocation.args
        );
        let everything = format!("{:?} {:?}", invocation.args, invocation.env);
        for canary in ["canary-mcp-command", "canary-mcp-arg", "canary-mcp-url"] {
            assert!(
                !everything.contains(canary),
                "ask={ask}: {canary} reached Codex"
            );
        }
    }
}

#[test]
fn mcp_override_is_an_empty_table_when_the_config_names_no_server() {
    let invocation = invocation_of(
        &CodexGrammar,
        "codex",
        &tack_runner::ProcessEnv::default(),
        false,
    );
    assert!(
        invocation
            .args
            .windows(2)
            .any(|pair| pair == ["-c", "mcp_servers={}"]),
        "{:?}",
        invocation.args
    );
    // Claude Code and opencode already carry theirs.
    let claude = invocation_of(
        &ClaudeCodeGrammar,
        "claude-code",
        &tack_runner::ProcessEnv::default(),
        false,
    );
    assert!(claude.args.contains(&"--strict-mcp-config".to_owned()));
}

#[test]
fn redact_the_login_of_both_clis_reads_redacted() {
    use tack_runner::harness::redact::SecretMaterial;
    let (_home, host) = home_with_integrations();
    for (kind, secrets, canaries) in [
        (
            "claude-code",
            invocation_of(&ClaudeCodeGrammar, "claude-code", &host, false).secrets,
            ["canary-claude-access", "canary-claude-refresh"],
        ),
        (
            "codex",
            invocation_of(&CodexGrammar, "codex", &host, false).secrets,
            ["canary-codex-access", "canary-codex-refresh"],
        ),
    ] {
        let mut material = SecretMaterial::new();
        material.register_all(secrets);
        for canary in canaries {
            assert_eq!(
                material.scrub(&format!("token={canary}")),
                "token=[REDACTED]",
                "{kind}: {canary}"
            );
        }
        assert_eq!(
            material.scrub("plan: max"),
            "plan: max",
            "{kind}: a plan name is not a secret"
        );
    }
}

/// The shared fake under `descriptor`, and whether it can ask before it acts.
struct DescriptorGrammar(&'static HarnessDescriptor, bool);

impl HarnessGrammar for DescriptorGrammar {
    fn descriptor(&self) -> &'static HarnessDescriptor {
        self.0
    }

    fn capabilities(&self) -> tack_protocol::execution::FeatureCapabilities {
        let mut capabilities = RunGrammar.capabilities();
        if self.1 {
            capabilities.decisions = tack_runner::harness::local_process::capability(
                tack_protocol::execution::CapabilitySupport::Supported,
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
            &tack_runner::ProcessEnv::from_process(),
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
        tack_protocol::execution::HarnessKind::new("breaks-in-bwrap");
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
            &tack_runner::ProcessEnv::from_process(),
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
    spec.work.request.requested_harness_kind =
        tack_protocol::execution::HarnessKind::new("cannot-ask");
    match silent.start(&spec).await {
        Err(HarnessError::Rejected { reason: refused }) => assert_eq!(refused, reason),
        other => panic!("expected a refusal before spawn, got {other:?}"),
    }
}

/// The command line and environment a grammar built for a spawn.
type Built = (Vec<String>, BTreeMap<String, String>);

/// A real grammar under the shared shim program, remembering the command line and environment it
/// built for the last spawn.
struct Recording<G> {
    grammar: G,
    descriptor: &'static HarnessDescriptor,
    seen: std::sync::Arc<std::sync::Mutex<Built>>,
}

impl<G: HarnessGrammar> HarnessGrammar for Recording<G> {
    fn descriptor(&self) -> &'static HarnessDescriptor {
        self.descriptor
    }

    fn capabilities(&self) -> tack_protocol::execution::FeatureCapabilities {
        self.grammar.capabilities()
    }

    fn invocation(&self, run: &RunContext<'_>) -> Result<Invocation, HarnessError> {
        let invocation = self.grammar.invocation(run)?;
        *self.seen.lock().unwrap() = (invocation.args.clone(), invocation.env.clone());
        Ok(invocation)
    }

    fn prompt(&self, prompt: String, stdin_stays_open: bool) -> Vec<u8> {
        self.grammar.prompt(prompt, stdin_stays_open)
    }

    fn report(&self, run: &RunContext<'_>, result: &ProcessResult) -> RunReport {
        self.grammar.report(run, result)
    }
}

/// A real agent's grammar under the shim program, on a machine whose sandbox is `bwrap` or none
/// and whose runner requires one or not, with a request for `approvals` and what the grammar built.
struct Shim<G: HarnessGrammar> {
    harness: LocalProcessHarness<Recording<G>>,
    spec: ExecutionSpec,
    seen: std::sync::Arc<std::sync::Mutex<Built>>,
    _dir: tempfile::TempDir,
}

fn shim<G: HarnessGrammar>(
    grammar: G,
    kind: &'static str,
    approvals: Option<Approvals>,
    with_bwrap: bool,
    required: bool,
) -> Shim<G> {
    let dir = root("approvals-without-a-sandbox");
    let (bin, home, workspace) = (
        dir.path().join("bin"),
        dir.path().join("home"),
        dir.path().join("workspace"),
    );
    for path in [&bin, &home, &workspace] {
        std::fs::create_dir_all(path).expect("directory");
    }
    let executable = |path: &std::path::Path, text: &str| {
        std::fs::write(path, text).expect("script");
        std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .expect("executable");
    };
    // Stands in for every agent's binary; `debug config` and `debug paths` are all opencode asks.
    let program = bin.join(kind);
    executable(
        &program,
        "#!/bin/sh\nif [ \"$2\" = config ]; then echo '{\"model\":\"p/m\"}'; elif [ \"$2\" = paths ]; then echo 'data /nonexistent'; else echo 1.0.0; fi\n",
    );
    if with_bwrap {
        executable(&bin.join("bwrap"), "#!/bin/sh\nexit 0\n");
    }
    let descriptor: &'static HarnessDescriptor = Box::leak(Box::new(HarnessDescriptor {
        program: Box::leak(program.display().to_string().into_boxed_str()),
        ..*on_the_fake_harness(
            DESCRIPTORS
                .iter()
                .find(|real| real.kind == kind)
                .expect("a registered local agent"),
        )
    }));
    let machine = sandbox::detect(Some(bin.as_os_str()), Some(&home)).requiring(required);
    assert_eq!(
        machine.kind,
        if with_bwrap {
            sandbox::SandboxKind::Bwrap
        } else {
            sandbox::SandboxKind::None
        }
    );
    let env = tack_runner::ProcessEnv::from_pairs([
        ("HOME", home.as_os_str()),
        ("PATH", bin.as_os_str()),
    ]);
    let seen = Default::default();
    let harness = LocalProcessHarness::discover(
        Recording {
            grammar,
            descriptor,
            seen: std::sync::Arc::clone(&seen),
        },
        ProcessLimits::new(1_000_000, 1_000_000, Duration::from_secs(60)),
        dir.path().join("staging"),
        SecretStore::file(dir.path().join("secrets.json")),
        &env,
    )
    .expect("the shim is locatable")
    .with_probed_sandbox(machine);
    let mut spec = spec(workspace, &[]);
    spec.work.request.requested_harness_kind = tack_protocol::execution::HarnessKind::new(kind);
    spec.work.request.permission_policy.network = true;
    spec.work.request.permission_policy.approvals = approvals;
    Shim {
        harness,
        spec,
        seen,
        _dir: dir,
    }
}

/// What a real agent's grammar builds for a request that asks for `approvals`, on a machine whose
/// sandbox is `bwrap` or none (the runner not requiring one): its command line, its environment,
/// and whether the runner says it forced the run to ask.
async fn invocation_for<G: HarnessGrammar>(
    grammar: G,
    kind: &'static str,
    approvals: Option<Approvals>,
    with_bwrap: bool,
) -> (Vec<String>, BTreeMap<String, String>, Option<String>) {
    let shim = shim(grammar, kind, approvals, with_bwrap, false);
    let handle = shim
        .harness
        .start(&shim.spec)
        .await
        .expect("the run starts");
    let forced = shim.harness.approvals_forced_ask(&handle).await;
    shim.harness.cancel(&handle).await.expect("cancel");
    let (args, env) = shim.seen.lock().unwrap().clone();
    (args, env, forced)
}

fn permission_block(env: &BTreeMap<String, String>) -> serde_json::Value {
    let config: serde_json::Value =
        serde_json::from_str(&env["OPENCODE_CONFIG_CONTENT"]).expect("opencode config");
    config["permission"].clone()
}

/// A request for `auto` on a machine where the runner found no sandbox is run as `ask`: the
/// command line of each agent is the one that pauses on every action.
#[tokio::test]
async fn auto_without_a_sandbox_runs_as_ask() {
    let has = |args: &[String], flag: &str| args.iter().any(|arg| arg == flag);
    let after = |args: &[String], flag: &str| {
        args.iter()
            .position(|arg| arg == flag)
            .map(|at| args[at + 1].clone())
    };
    for approvals in [None, Some(Approvals::Auto)] {
        let (args, _, forced) =
            invocation_for(ClaudeCodeGrammar, "claude-code", approvals, false).await;
        assert_eq!(
            after(&args, "--permission-mode").as_deref(),
            Some("default")
        );
        assert!(has(&args, "--permission-prompt-tool"), "{args:?}");
        assert!(!has(&args, "bypassPermissions"), "{args:?}");
        assert!(forced.is_some_and(|why| why.contains("no sandbox")));

        let (args, _, forced) = invocation_for(CodexGrammar, "codex", approvals, false).await;
        assert!(has(&args, "app-server") && !has(&args, "exec"), "{args:?}");
        assert!(forced.is_some());

        let (args, env, forced) =
            invocation_for(OpencodeGrammar, "opencode", approvals, false).await;
        assert!(has(&args, "acp") && !has(&args, "run"), "{args:?}");
        let permission = permission_block(&env);
        assert!(
            ["edit", "bash"]
                .iter()
                .all(|tool| permission[tool] != "allow"),
            "{permission}"
        );
        assert!(forced.is_some());
    }
}

/// The twin: where there is a sandbox, `auto` is `auto`, and no event says otherwise.
#[tokio::test]
async fn auto_under_a_sandbox_is_unchanged() {
    let has = |args: &[String], flag: &str| args.iter().any(|arg| arg == flag);
    let (args, _, forced) = invocation_for(
        ClaudeCodeGrammar,
        "claude-code",
        Some(Approvals::Auto),
        true,
    )
    .await;
    assert!(has(&args, "bypassPermissions"), "{args:?}");
    assert_eq!(forced, None);

    let (args, _, forced) =
        invocation_for(CodexGrammar, "codex", Some(Approvals::Auto), true).await;
    assert!(has(&args, "exec") && !has(&args, "app-server"), "{args:?}");
    assert_eq!(forced, None);

    let (args, env, forced) =
        invocation_for(OpencodeGrammar, "opencode", Some(Approvals::Auto), true).await;
    assert!(has(&args, "run") && !has(&args, "acp"), "{args:?}");
    assert_eq!(permission_block(&env)["bash"], "deny");
    assert_eq!(forced, None);
}

/// An explicit `ask` on a machine without a sandbox is not "forced": it was asked for.
#[tokio::test]
async fn ask_without_a_sandbox_says_nothing_extra() {
    let (_, _, forced) = invocation_for(
        ClaudeCodeGrammar,
        "claude-code",
        Some(Approvals::Ask),
        false,
    )
    .await;
    assert_eq!(forced, None);
}

/// With the requirement on (the default), an agent on a machine with no `bwrap` does not run:
/// its probe says why, so nothing schedules it, and a request that reaches it is refused.
async fn does_not_run_without_bwrap<G: HarnessGrammar>(grammar: G, kind: &'static str) {
    let shim = shim(grammar, kind, Some(Approvals::Auto), false, true);
    let probe = tack_runner::harness::HarnessProbe::probe(&shim.harness).await;
    let reason = probe
        .probe_error
        .unwrap_or_else(|| panic!("{kind}: the probe says it may not run"));
    assert!(
        reason.contains("install bubblewrap") && reason.contains("does not run here"),
        "{kind}: {reason}"
    );
    assert_eq!(probe.additional["sandbox"], "none", "{kind}");
    assert_eq!(probe.additional["sandbox_required"], true, "{kind}");
    match shim.harness.start(&shim.spec).await {
        Err(HarnessError::Rejected { reason: refused }) => assert_eq!(refused, reason, "{kind}"),
        other => panic!("{kind}: expected a refusal before spawn, got {other:?}"),
    }
    assert_eq!(*shim.seen.lock().unwrap(), Built::default(), "{kind}");
}

#[tokio::test]
async fn a_required_sandbox_that_is_missing_stops_every_local_agent() {
    does_not_run_without_bwrap(ClaudeCodeGrammar, "claude-code").await;
    does_not_run_without_bwrap(CodexGrammar, "codex").await;
    does_not_run_without_bwrap(OpencodeGrammar, "opencode").await;
}

/// The twin: where `bwrap` is there, the requirement changes nothing and the run is `auto`.
#[tokio::test]
async fn a_required_sandbox_that_is_present_runs_as_asked() {
    let shim = shim(
        ClaudeCodeGrammar,
        "claude-code",
        Some(Approvals::Auto),
        true,
        true,
    );
    let probe = tack_runner::harness::HarnessProbe::probe(&shim.harness).await;
    assert_eq!(probe.probe_error, None);
    assert_eq!(probe.additional["sandbox"], "bwrap");
    let handle = shim
        .harness
        .start(&shim.spec)
        .await
        .expect("the run starts");
    assert_eq!(shim.harness.approvals_forced_ask(&handle).await, None);
    shim.harness.cancel(&handle).await.expect("cancel");
}

/// The explicit opt-out: with no `bwrap` the agent runs, as `ask`, and the report and the run's
/// event both say the requirement is off.
#[tokio::test]
async fn opting_out_of_the_sandbox_runs_the_agent_as_ask() {
    let has = |args: &[String], flag: &str| args.iter().any(|arg| arg == flag);
    let shim = shim(
        ClaudeCodeGrammar,
        "claude-code",
        Some(Approvals::Auto),
        false,
        false,
    );
    let probe = tack_runner::harness::HarnessProbe::probe(&shim.harness).await;
    assert_eq!(probe.probe_error, None);
    assert_eq!(probe.additional["sandbox"], "none");
    assert_eq!(probe.additional["sandbox_required"], false);
    let handle = shim
        .harness
        .start(&shim.spec)
        .await
        .expect("the run starts");
    let forced = shim.harness.approvals_forced_ask(&handle).await;
    assert!(
        forced.is_some_and(|why| why.contains("does not require one")),
        "the event says why it asks"
    );
    shim.harness.cancel(&handle).await.expect("cancel");
    let (args, _) = shim.seen.lock().unwrap().clone();
    assert!(!has(&args, "bypassPermissions") && has(&args, "--permission-prompt-tool"));
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

    fn capabilities(&self) -> tack_protocol::execution::FeatureCapabilities {
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
        cancels_over_api: real.cancels_over_api,
        reports_progress: real.reports_progress,
        sandbox_binds: &[],
        model_endpoints: &[],
        parity: &[],
        readiness: None,
        models_docs: "",
        sign_in_docs: "",
    }))
}

/// The runner's own environment holds a model credential of every kind; none reaches an agent.
#[tokio::test]
async fn the_runner_passes_no_model_credential_of_its_own() {
    let env = tack_runner::ProcessEnv::from_pairs(
        std::env::vars_os().chain(
            MODEL_CREDENTIAL_VARIABLES
                .iter()
                .map(|name| ((*name).into(), "runner-own-model-credential".into())),
        ),
    );
    for real in DESCRIPTORS {
        let dir = root("no-model-credential");
        let workspace = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace).expect("workspace");
        let harness = LocalProcessHarness::discover(
            EchoGrammar(on_the_fake_harness(real)),
            ProcessLimits::new(1_000_000, 1_000_000, Duration::from_secs(60)),
            dir.path().join("staging"),
            SecretStore::file(dir.path().join("secrets.json")),
            &env,
        )
        .expect("the fake harness script is locatable");
        let mut spec = spec(workspace.clone(), &[]);
        spec.work.request.requested_harness_kind =
            tack_protocol::execution::HarnessKind::new(real.kind);

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
            host: &tack_runner::ProcessEnv::default(),
        };
        for (kind, invocation) in [
            ("codex", CodexGrammar.invocation(&run)),
            ("claude-code", ClaudeCodeGrammar.invocation(&run)),
        ] {
            if let Ok(invocation) = invocation {
                for name in MODEL_CREDENTIAL_VARIABLES {
                    assert!(!invocation.env.contains_key(name), "{kind}: {name}");
                }
            }
        }
    }
}

/// opencode's `debug` commands, answered from the captured fixture with a
/// seeded key, and a harness that reports what its scratch home holds.
struct LoginGrammar(&'static HarnessDescriptor);

impl HarnessGrammar for LoginGrammar {
    fn descriptor(&self) -> &'static HarnessDescriptor {
        self.0
    }

    fn capabilities(&self) -> tack_protocol::execution::FeatureCapabilities {
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
    let host = tack_runner::ProcessEnv::from_pairs([
        ("PATH", path.as_str()),
        ("HOME", home.to_str().unwrap()),
    ]);

    let mut spec = spec(workspace.clone(), &[]);
    spec.work.request.requested_harness_kind =
        tack_protocol::execution::HarnessKind::new("opencode");
    spec.work.request.permission_policy.network = true;
    spec.work.request.permission_policy.approvals = None;
    spec.work.request.requested_model = Some("llamacpp/chosen".to_owned());
    let run = RunContext {
        spec: &spec,
        scratch: dir.path(),
        program: Some(&opencode),
        host: &host,
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
        host: &host,
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
        cancels_over_api: false,
        reports_progress: false,
        sandbox_binds: &[],
        model_endpoints: &[],
        parity: &[],
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
            &tack_runner::ProcessEnv::from_process(),
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

/// The docket daemon is started with `PATH`, `HOME` and `DOCKET_HOME` and
/// nothing else the runner holds: no model key, no enrollment credential, no
/// variable a request or the operator's shell carried in.
#[tokio::test]
async fn the_docket_daemon_gets_path_home_and_docket_home_only() {
    use std::os::unix::fs::PermissionsExt;
    use tack_runner::{config::DocketConfig, harness::docket::daemon};

    let dir = root("docket-daemon-env");
    let script = dir.path().join("docket");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nenv > '{}/env.txt'\nexec sleep 30\n",
            dir.path().display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let config = DocketConfig {
        binary: Some(script),
        home: Some(dir.path().join("docket-home")),
        port: 1,
        process: tack_runner::ProcessEnv::from_pairs([
            ("PATH", "/usr/bin"),
            ("HOME", "/home/someone"),
            ("ANTHROPIC_API_KEY", "never-forwarded"),
        ]),
        ..DocketConfig::default()
    };
    let explicit: std::collections::BTreeSet<String> = daemon::command(&config)
        .get_envs()
        .map(|(name, _)| name.to_string_lossy().into_owned())
        .collect();
    assert!(
        explicit.is_subset(&["PATH", "HOME", "DOCKET_HOME"].map(String::from).into()),
        "{explicit:?}"
    );

    let options = daemon::Options {
        ready_timeout: Duration::from_millis(600),
        backoff: Duration::from_secs(60),
        grace: Duration::from_secs(5),
    };
    let handle = daemon::ensure_running_with(&config, &dir.path().join("logs"), options).await;
    let seen = std::fs::read_to_string(dir.path().join("env.txt")).expect("the daemon started");
    let names: std::collections::BTreeSet<&str> = seen
        .lines()
        .filter_map(|line| line.split_once('=').map(|(name, _)| name))
        .collect();
    // What a shell adds to its own environment is not the runner's to answer for.
    let shell = ["PWD", "OLDPWD", "SHLVL", "_"];
    for name in &names {
        assert!(
            ["PATH", "HOME", "DOCKET_HOME"].contains(name) || shell.contains(name),
            "the daemon got {name}"
        );
    }
    assert!(names.contains("DOCKET_HOME"), "{names:?}");
    handle.shutdown().await;
}

// ---- the parity matrix ----------------------------------------------------------------------
//
// Every rule every agent must meet, one cell per agent. A cell is fine when somebody enforces it,
// or when no card can ever fill it (`by design:`). Every other gap is listed below with the card
// that closes it, so the suite is green while a gap is known, and a card that closes its cells
// must delete their entries; a descriptor with a gap nobody explained is red.

/// `(rule, agent, card)` for each cell that is not met today and that a card will meet.
const PENDING_CELLS: &[(ParityRule, &str, &str)] = &[];

/// What is wrong with the matrix, one line per problem; empty when it holds.
fn parity_problems(
    descriptors: &[&HarnessDescriptor],
    pending: &[(ParityRule, &str, &str)],
) -> Vec<String> {
    let mut problems = Vec::new();
    for descriptor in descriptors {
        let agent = descriptor.kind;
        if let Err(why) = validate_parity(descriptor.parity) {
            problems.push(format!("{agent}: {why}"));
            continue;
        }
        for (rule, cell) in descriptor.parity {
            let listed = pending.iter().find(|(pending_rule, pending_agent, _)| {
                pending_rule == rule && *pending_agent == agent
            });
            match (cell, listed) {
                (ParityCell::Missing { .. }, _) if cell.is_by_design() => {
                    if listed.is_some() {
                        problems.push(format!(
                            "{rule:?} x {agent} is by design: delete it from PENDING_CELLS"
                        ));
                    }
                }
                (ParityCell::Missing { reason }, Some((_, _, card))) => {
                    if !reason.starts_with(&format!("{card}:")) {
                        problems.push(format!(
                            "{rule:?} x {agent}: the reason must start with its card `{card}:`"
                        ));
                    }
                }
                (ParityCell::Missing { reason }, None) => {
                    problems.push(format!(
                        "{rule:?} x {agent} is missing and not pending any card: {reason}"
                    ));
                }
                (_, Some((_, _, card))) => {
                    problems.push(format!(
                        "{rule:?} x {agent} is met now: delete it from PENDING_CELLS ({card})"
                    ));
                }
                (_, None) => {}
            }
        }
    }
    for (rule, agent, card) in pending {
        if !descriptors
            .iter()
            .any(|descriptor| descriptor.kind == *agent)
        {
            problems.push(format!(
                "PENDING_CELLS names {agent}, which is not a registered agent ({rule:?}, {card})"
            ));
        }
    }
    problems
}

fn leaked_descriptor(
    kind: &'static str,
    column: Vec<(ParityRule, ParityCell)>,
) -> &'static HarnessDescriptor {
    Box::leak(Box::new(HarnessDescriptor {
        kind,
        program: "unused",
        model_selection: ModelSelection::Optional,
        inherited_env: &[],
        probe_notes: &[],
        credential_note: "",
        reports_process_groups: false,
        cancels_over_api: false,
        reports_progress: false,
        sandbox_binds: &[],
        model_endpoints: &[],
        parity: Box::leak(column.into_boxed_slice()),
        readiness: None,
        models_docs: "",
        sign_in_docs: "",
    }))
}

fn met_column() -> Vec<(ParityRule, ParityCell)> {
    ParityRule::ALL
        .into_iter()
        .map(|rule| (rule, ParityCell::Runner))
        .collect()
}

#[test]
fn parity_matrix() {
    let problems = parity_problems(&DESCRIPTORS, PENDING_CELLS);
    assert!(
        problems.is_empty(),
        "the parity matrix has gaps:\n{}",
        problems.join("\n")
    );
}

#[test]
fn parity_a_gap_nobody_explained_is_red() {
    let mut empty_reason = met_column();
    empty_reason[1].1 = ParityCell::missing("");
    let mut unlisted = met_column();
    unlisted[2].1 = ParityCell::missing("GR5: not done yet");
    let mut by_design = met_column();
    by_design[10].1 = ParityCell::missing("by design: it prints no steps");
    let descriptors = [
        leaked_descriptor("empty-reason", empty_reason),
        leaked_descriptor("unlisted", unlisted),
        leaked_descriptor("by-design", by_design),
    ];
    let problems = parity_problems(&descriptors, &[]);
    assert_eq!(problems.len(), 2, "{problems:#?}");
    assert!(problems[0].contains("empty-reason") && problems[0].contains("NoSandboxMeansAsk"));
    assert!(problems[1].contains("unlisted") && problems[1].contains("EgressByAllowList"));
}

#[test]
fn parity_a_listed_gap_is_green_until_it_is_met() {
    let mut gap = met_column();
    gap[2].1 = ParityCell::missing("GR5: not done yet");
    let pending = [(ParityRule::EgressByAllowList, "gap", "GR5")];
    assert!(parity_problems(&[leaked_descriptor("gap", gap)], &pending).is_empty());

    let met = leaked_descriptor("gap", met_column());
    let problems = parity_problems(&[met], &pending);
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert!(problems[0].contains("is met now"));
}

#[test]
fn parity_a_pending_cell_names_a_registered_agent_and_its_card() {
    let mut gap = met_column();
    gap[2].1 = ParityCell::missing("GR5: not done yet");
    let descriptor = leaked_descriptor("gap", gap);
    let wrong_card = [(ParityRule::EgressByAllowList, "gap", "GR6")];
    assert_eq!(parity_problems(&[descriptor], &wrong_card).len(), 1);
    let ghost = [
        (ParityRule::EgressByAllowList, "gap", "GR5"),
        (ParityRule::Redaction, "nobody", "GR18"),
    ];
    assert_eq!(parity_problems(&[descriptor], &ghost).len(), 1);
}

#[test]
fn parity_steps_are_visible_exactly_where_the_cli_reports_progress() {
    for descriptor in DESCRIPTORS {
        let steps = descriptor
            .parity
            .iter()
            .find(|(rule, _)| *rule == ParityRule::StepsVisible)
            .map(|(_, cell)| cell)
            .expect("every column answers StepsVisible");
        assert_eq!(
            steps.is_met(),
            descriptor.reports_progress,
            "{}: StepsVisible must agree with `reports_progress`",
            descriptor.kind
        );
    }
}

/// The board decides the clock and the auto-start of an agent from a list in the protocol crate,
/// because it has no descriptors; this keeps that list equal to what the descriptors say.
#[test]
fn the_board_knows_which_agents_report_no_usage_while_they_run() {
    for descriptor in DESCRIPTORS {
        assert_eq!(
            tack_protocol::execution::reports_usage_while_running(descriptor.kind),
            descriptor.reports_progress,
            "{}: `WITHOUT_LIVE_USAGE` must list exactly the agents with `reports_progress: false`",
            descriptor.kind
        );
    }
}

// ---- Web access (the runner's proxy) ----

mod egress {
    use std::{
        collections::HashMap,
        io,
        net::{IpAddr, SocketAddr},
        sync::{Arc, Mutex},
    };

    use async_trait::async_trait;
    use tack_runner::harness::{
        HarnessProbe,
        egress::{Network, NetworkRefusal},
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
    };

    use super::*;

    /// A CLI stand-in that says where the runner told it to send its traffic, then waits.
    const CLI: &str = "#!/bin/sh\n\
        { echo \"HTTPS_PROXY=$HTTPS_PROXY\"; echo \"HTTP_PROXY=$HTTP_PROXY\"; echo \"NO_PROXY=$NO_PROXY\"; } > \"$TACK_TEST_ENV_FILE.tmp\"\n\
        mv \"$TACK_TEST_ENV_FILE.tmp\" \"$TACK_TEST_ENV_FILE\"\n\
        exec sleep 60\n";

    /// Names resolve as the test says; a connection to any address reaches a local echo server,
    /// and the address it was asked for is kept.
    struct Fake {
        names: HashMap<&'static str, Vec<IpAddr>>,
        echo: SocketAddr,
        dialled: Mutex<Vec<SocketAddr>>,
    }

    #[async_trait]
    impl Network for Fake {
        async fn resolve(&self, host: &str) -> io::Result<Vec<IpAddr>> {
            self.names
                .get(host)
                .cloned()
                .ok_or_else(|| io::Error::other("unknown name"))
        }

        async fn connect(&self, to: SocketAddr) -> io::Result<TcpStream> {
            self.dialled.lock().unwrap().push(to);
            TcpStream::connect(self.echo).await
        }
    }

    async fn echo_server() -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut buffer = [0u8; 1024];
                    while let Ok(n) = stream.read(&mut buffer).await {
                        if n == 0 || stream.write_all(&buffer[..n]).await.is_err() {
                            return;
                        }
                    }
                });
            }
        });
        address
    }

    struct ScriptGrammar(&'static HarnessDescriptor);

    impl HarnessGrammar for ScriptGrammar {
        fn descriptor(&self) -> &'static HarnessDescriptor {
            self.0
        }

        fn capabilities(&self) -> tack_protocol::execution::FeatureCapabilities {
            RunGrammar.capabilities()
        }

        fn invocation(&self, _run: &RunContext<'_>) -> Result<Invocation, HarnessError> {
            Ok(Invocation::default())
        }

        fn report(&self, _run: &RunContext<'_>, _result: &ProcessResult) -> RunReport {
            RunReport::verdict(true, serde_json::json!({}))
        }
    }

    /// A running stand-in CLI whose traffic goes through the runner's proxy.
    struct Live {
        _dir: tempfile::TempDir,
        harness: LocalProcessHarness<ScriptGrammar>,
        handle: tack_runner::client::LocalRunHandle,
        network: Arc<Fake>,
        told: HashMap<String, String>,
    }

    impl Live {
        /// The proxy the CLI was told about, when it was told one.
        fn proxy(&self) -> SocketAddr {
            let url = self
                .told
                .get("HTTPS_PROXY")
                .expect("the CLI was told a proxy");
            url.trim_start_matches("http://")
                .parse()
                .expect("a socket address")
        }

        /// What the proxy answers to a `CONNECT`.
        async fn connect(&self, target: &str) -> String {
            let mut stream = TcpStream::connect(self.proxy()).await.unwrap();
            let request = format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n\r\n");
            stream.write_all(request.as_bytes()).await.unwrap();
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") && stream.read(&mut byte).await.unwrap_or(0) == 1 {
                head.push(byte[0]);
            }
            String::from_utf8(head)
                .unwrap()
                .lines()
                .next()
                .unwrap()
                .to_owned()
        }
    }

    /// Starts the stand-in CLI. `mode` and `listed` are what the profile chose; `enforce` is the
    /// runner's `[egress]` switch.
    async fn live(mode: Option<&str>, listed: &[&str], enforce: bool) -> Live {
        let dir = root("egress");
        let program = dir.path().join("cli.sh");
        std::fs::write(&program, CLI).unwrap();
        std::fs::set_permissions(
            &program,
            std::os::unix::fs::PermissionsExt::from_mode(0o755),
        )
        .unwrap();
        let descriptor: &'static HarnessDescriptor = Box::leak(Box::new(HarnessDescriptor {
            kind: "egress-fake",
            program: Box::leak(program.display().to_string().into_boxed_str()),
            model_endpoints: &["api.model.test"],
            ..FAKE
        }));
        let network = Arc::new(Fake {
            names: HashMap::from([
                ("api.model.test", vec!["203.0.113.10".parse().unwrap()]),
                ("docs.listed.test", vec!["203.0.113.20".parse().unwrap()]),
                ("blog.public.test", vec!["203.0.113.30".parse().unwrap()]),
                ("rebind.test", vec!["10.0.0.9".parse().unwrap()]),
            ]),
            echo: echo_server().await,
            dialled: Mutex::new(Vec::new()),
        });
        let harness = sandboxed_harness(
            ScriptGrammar(descriptor),
            dir.path(),
            sandbox::Sandbox::none("test"),
        )
        .with_egress(tack_runner::EgressConfig {
            enforce,
            model_hosts: Vec::new(),
        })
        .with_egress_network(network.clone());
        let workspace = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        let env_file = dir.path().join("env.txt");
        let mut spec = spec(
            workspace,
            &[(
                "TACK_TEST_ENV_FILE",
                literal(&env_file.display().to_string()),
            )],
        );
        spec.work.request.requested_harness_kind =
            tack_protocol::execution::HarnessKind::new(descriptor.kind);
        let profile = &mut spec.work.request.resolved_agent_profile;
        if let Some(mode) = mode {
            profile
                .additional
                .insert("egress_mode".into(), serde_json::json!(mode));
        }
        profile.tool_policy = serde_json::json!({"allowed_hosts": listed});
        let handle = harness.start(&spec).await.expect("the run starts");
        let mut told = None;
        for _ in 0..200 {
            if let Ok(text) = std::fs::read_to_string(&env_file) {
                told = Some(text);
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let told = told
            .expect("the CLI reported its environment")
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(name, value)| (name.to_owned(), value.to_owned()))
            .collect();
        Live {
            _dir: dir,
            harness,
            handle,
            network,
            told,
        }
    }

    impl Live {
        async fn stop(self) {
            self.harness
                .cancel(&self.handle)
                .await
                .expect("the run stops");
        }
    }

    #[tokio::test]
    async fn without_enforcement_the_cli_is_told_no_proxy() {
        let run = live(Some("closed"), &[], false).await;
        for name in ["HTTPS_PROXY", "HTTP_PROXY", "NO_PROXY"] {
            assert_eq!(run.told[name], "", "{name}");
        }
        run.stop().await;
    }

    #[tokio::test]
    async fn the_runner_tells_the_cli_its_proxy_and_keeps_loopback_off_it() {
        let run = live(Some("open"), &[], true).await;
        assert!(
            run.told["HTTPS_PROXY"].starts_with("http://127.0.0.1:"),
            "{:?}",
            run.told
        );
        assert_eq!(run.told["HTTPS_PROXY"], run.told["HTTP_PROXY"]);
        assert!(run.told["NO_PROXY"].contains("127.0.0.1"), "{:?}", run.told);
        run.stop().await;
    }

    #[tokio::test]
    async fn closed_reaches_the_cli_endpoint_and_refuses_a_public_host_that_is_not_listed() {
        let run = live(Some("closed"), &["docs.listed.test"], true).await;
        // The twin: the CLI still reaches the model.
        assert!(run.connect("api.model.test:443").await.contains("200"));
        assert_eq!(
            *run.network.dialled.lock().unwrap(),
            ["203.0.113.10:443".parse::<SocketAddr>().unwrap()]
        );
        // A public host that is not its model is refused, and so is one the profile lists: a
        // closed profile has no list.
        assert!(run.connect("blog.public.test:443").await.contains("403"));
        assert!(run.connect("docs.listed.test:443").await.contains("403"));
        assert_eq!(run.network.dialled.lock().unwrap().len(), 1);
        run.stop().await;
    }

    #[tokio::test]
    async fn listed_reaches_the_cli_endpoint_and_the_profiles_hosts_and_no_other() {
        let run = live(Some("listed"), &["docs.listed.test"], true).await;
        assert!(run.connect("api.model.test:443").await.contains("200"));
        assert!(run.connect("docs.listed.test:443").await.contains("200"));
        assert!(run.connect("blog.public.test:443").await.contains("403"));
        run.stop().await;
    }

    #[tokio::test]
    async fn a_profile_with_no_mode_on_the_request_is_listed() {
        let run = live(None, &["docs.listed.test"], true).await;
        assert!(run.connect("docs.listed.test:443").await.contains("200"));
        assert!(run.connect("blog.public.test:443").await.contains("403"));
        run.stop().await;
    }

    #[tokio::test]
    async fn open_reaches_any_public_host_and_refuses_internal_addresses_by_name_or_number() {
        let run = live(Some("open"), &[], true).await;
        assert!(run.connect("blog.public.test:443").await.contains("200"));
        let dialled = run.network.dialled.lock().unwrap().len();
        for target in [
            "127.0.0.1:3210",
            "10.1.2.3:443",
            "169.254.169.254:80",
            "[fd00:ec2::254]:80",
            // a name that resolves to a private address
            "rebind.test:443",
        ] {
            assert!(run.connect(target).await.contains("403"), "{target}");
        }
        assert_eq!(
            run.network.dialled.lock().unwrap().len(),
            dialled,
            "a refused destination was dialled"
        );
        run.stop().await;
    }

    #[tokio::test]
    async fn each_refused_destination_reaches_the_engine_as_an_event_to_report() {
        let run = live(Some("closed"), &[], true).await;
        let mut refusals = run
            .harness
            .network_refusals(&run.handle)
            .await
            .expect("an enforced run reports its refusals");
        run.connect("blog.public.test:443").await;
        run.connect("169.254.169.254:80").await;
        let first: NetworkRefusal = refusals.recv().await.unwrap();
        assert_eq!((first.host.as_str(), first.port), ("blog.public.test", 443));
        let second = refusals.recv().await.unwrap();
        assert_eq!(second.host, "169.254.169.254");
        assert!(second.reason.contains("private"), "{}", second.reason);
        run.stop().await;
    }

    #[tokio::test]
    async fn a_run_without_enforcement_has_no_refusals_to_report() {
        let run = live(Some("closed"), &[], false).await;
        assert!(run.harness.network_refusals(&run.handle).await.is_none());
        run.stop().await;
    }

    /// A CLI stand-in for a sandbox with no network: it tries to leave directly, then through the
    /// proxy it was told, and writes both results for the test to read.
    const CLI_IN_BWRAP: &str = r#"#!/usr/bin/python3
import json, os, socket, sys, time
if sys.argv[1:2] == ["--version"]:
    print("1.0.0")
    sys.exit(0)

def direct(target):
    host, port = target.rsplit(":", 1)
    try:
        socket.create_connection((host, int(port)), timeout=3).close()
        return "connected"
    except OSError:
        return "failed"

def via_proxy(target):
    host, port = os.environ["HTTPS_PROXY"].removeprefix("http://").rsplit(":", 1)
    s = socket.create_connection((host, int(port)), timeout=5)
    s.sendall(f"CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n\r\n".encode())
    head = b""
    while not head.endswith(b"\r\n\r\n"):
        c = s.recv(1)
        if not c:
            break
        head += c
    status = head.split(b"\r\n")[0].decode()
    echo = None
    if " 200 " in status:
        s.sendall(b"ping")
        echo = s.recv(4).decode()
    return [status, echo]

try:
    out = {
        "direct": {t: direct(t) for t in os.environ["DIRECT"].split(",")},
        "proxy": {t: via_proxy(t) for t in os.environ["TARGETS"].split(",")},
        "proxy_env": os.environ["HTTPS_PROXY"],
    }
except Exception as error:
    out = {"error": repr(error)}
tmp = os.environ["OUT"] + ".tmp"
open(tmp, "w").write(json.dumps(out))
os.rename(tmp, os.environ["OUT"])
time.sleep(60)
"#;

    /// Whether this machine has a `bwrap` to run the real-sandbox tests with (unless
    /// `TACK_REQUIRE_BWRAP=1`, which makes that a failure) and the interpreter the stand-in needs.
    fn real_bwrap_here() -> Option<sandbox::Sandbox> {
        let path = tack_runner::ProcessEnv::from_process().search_path();
        let found = sandbox::detect(path.as_deref(), None);
        let usable = found.kind == sandbox::SandboxKind::Bwrap
            && std::path::Path::new("/usr/bin/python3").is_file();
        if !usable {
            if std::env::var("TACK_REQUIRE_BWRAP").as_deref() == Ok("1") {
                panic!("bwrap and /usr/bin/python3 required by TACK_REQUIRE_BWRAP");
            }
            println!("skipped: bwrap or /usr/bin/python3 is not here");
            return None;
        }
        Some(found)
    }

    /// What the stand-in CLI saw from inside a real sandbox: what it could reach directly, and
    /// what the proxy answered for each of `targets`.
    struct Inside {
        out: serde_json::Value,
        network: Arc<Fake>,
        capability: serde_json::Value,
        sandbox: Option<String>,
    }

    async fn inside_bwrap(mode: &str, listed: &[&str], targets: &[&str]) -> Option<Inside> {
        real_bwrap_here()?;
        let dir = root("egress-bwrap");
        let home = dir.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let program = dir.path().join("cli.py");
        std::fs::write(&program, CLI_IN_BWRAP).unwrap();
        std::fs::set_permissions(
            &program,
            std::os::unix::fs::PermissionsExt::from_mode(0o755),
        )
        .unwrap();
        let descriptor: &'static HarnessDescriptor = Box::leak(Box::new(HarnessDescriptor {
            kind: "egress-fake",
            program: Box::leak(program.display().to_string().into_boxed_str()),
            model_endpoints: &["api.model.test"],
            ..FAKE
        }));
        let echo = echo_server().await;
        let network = Arc::new(Fake {
            names: HashMap::from([
                ("api.model.test", vec!["203.0.113.10".parse().unwrap()]),
                ("docs.listed.test", vec!["203.0.113.20".parse().unwrap()]),
                ("blog.public.test", vec!["203.0.113.30".parse().unwrap()]),
                ("rebind.test", vec!["10.0.0.9".parse().unwrap()]),
            ]),
            echo,
            dialled: Mutex::new(Vec::new()),
        });
        let machine = sandbox::detect(
            tack_runner::ProcessEnv::from_process()
                .search_path()
                .as_deref(),
            Some(&home),
        )
        .requiring(true);
        assert_eq!(machine.kind, sandbox::SandboxKind::Bwrap);
        let harness = sandboxed_harness(ScriptGrammar(descriptor), dir.path(), machine)
            .with_egress(tack_runner::EgressConfig {
                enforce: true,
                model_hosts: Vec::new(),
            })
            .with_egress_network(network.clone())
            .with_egress_helper(env!("CARGO_BIN_EXE_tack-runner").into());
        let workspace = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        let out_file = workspace.join("out.json");
        let direct = format!(
            "1.1.1.1:443,10.0.0.9:80,169.254.169.254:80,{echo},{}",
            echo.to_string().replace("127.0.0.1", "localhost")
        );
        let mut spec = spec(
            workspace,
            &[
                ("OUT", literal(&out_file.display().to_string())),
                ("DIRECT", literal(&direct)),
                ("TARGETS", literal(&targets.join(","))),
            ],
        );
        spec.work.request.requested_harness_kind =
            tack_protocol::execution::HarnessKind::new(descriptor.kind);
        let profile = &mut spec.work.request.resolved_agent_profile;
        profile
            .additional
            .insert("egress_mode".into(), serde_json::json!(mode));
        profile.tool_policy = serde_json::json!({"allowed_hosts": listed});
        let handle = harness.start(&spec).await.expect("the run starts");
        let sandbox = harness.sandbox(&handle).await;
        let mut out: Option<serde_json::Value> = None;
        for _ in 0..200 {
            if let Ok(text) = std::fs::read_to_string(&out_file) {
                out = Some(serde_json::from_str(&text).unwrap());
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let capability = harness.probe().await.additional["egress"].clone();
        harness.cancel(&handle).await.expect("the run stops");
        let out = out.expect("the CLI inside the sandbox reported");
        assert!(out.get("error").is_none(), "the CLI failed: {out}");
        Some(Inside {
            out,
            network,
            capability,
            sandbox,
        })
    }

    fn answers(inside: &Inside, target: &str) -> (String, Option<String>) {
        let answer = &inside.out["proxy"][target];
        (
            answer[0].as_str().unwrap().to_owned(),
            answer[1].as_str().map(str::to_owned),
        )
    }

    fn cannot_go_around(inside: &Inside) {
        assert_eq!(inside.sandbox.as_deref(), Some("bwrap"));
        let direct: Vec<&str> = inside.out["direct"]
            .as_object()
            .unwrap()
            .values()
            .map(|result| result.as_str().unwrap())
            .collect();
        assert_eq!(direct.len(), 5, "{:?}", inside.out);
        assert!(
            direct.iter().all(|result| *result == "failed"),
            "the agent connected without the proxy: {:?}",
            inside.out["direct"]
        );
        assert!(
            inside.out["proxy_env"]
                .as_str()
                .unwrap()
                .starts_with("http://127.0.0.1:"),
            "{:?}",
            inside.out
        );
        assert_eq!(inside.capability["enforced"], true);
    }

    #[tokio::test]
    async fn in_bwrap_closed_has_no_way_out_but_the_proxy_and_the_proxy_lets_only_the_model_through()
     {
        let Some(inside) = inside_bwrap(
            "closed",
            &["docs.listed.test"],
            &[
                "api.model.test:443",
                "blog.public.test:443",
                "docs.listed.test:443",
            ],
        )
        .await
        else {
            return;
        };
        cannot_go_around(&inside);
        // The twin: the CLI still reaches its model, and what it sends comes back.
        assert_eq!(
            answers(&inside, "api.model.test:443"),
            (
                "HTTP/1.1 200 Connection Established".to_owned(),
                Some("ping".to_owned())
            )
        );
        assert!(answers(&inside, "blog.public.test:443").0.contains("403"));
        assert!(answers(&inside, "docs.listed.test:443").0.contains("403"));
        assert_eq!(
            *inside.network.dialled.lock().unwrap(),
            ["203.0.113.10:443".parse::<SocketAddr>().unwrap()]
        );
    }

    #[tokio::test]
    async fn in_bwrap_listed_lets_the_model_and_the_profiles_hosts_through_and_nothing_else() {
        let Some(inside) = inside_bwrap(
            "listed",
            &["docs.listed.test"],
            &[
                "api.model.test:443",
                "docs.listed.test:443",
                "blog.public.test:443",
            ],
        )
        .await
        else {
            return;
        };
        cannot_go_around(&inside);
        assert_eq!(
            answers(&inside, "api.model.test:443").1.as_deref(),
            Some("ping")
        );
        assert_eq!(
            answers(&inside, "docs.listed.test:443").1.as_deref(),
            Some("ping")
        );
        assert!(answers(&inside, "blog.public.test:443").0.contains("403"));
    }

    #[tokio::test]
    async fn in_bwrap_open_reaches_public_hosts_through_the_proxy_and_still_refuses_private_addresses()
     {
        let Some(inside) = inside_bwrap(
            "open",
            &[],
            &[
                "blog.public.test:443",
                "127.0.0.1:3210",
                "10.1.2.3:443",
                "169.254.169.254:80",
                "rebind.test:443",
            ],
        )
        .await
        else {
            return;
        };
        cannot_go_around(&inside);
        assert_eq!(
            answers(&inside, "blog.public.test:443").1.as_deref(),
            Some("ping")
        );
        for target in [
            "127.0.0.1:3210",
            "10.1.2.3:443",
            "169.254.169.254:80",
            "rebind.test:443",
        ] {
            assert!(answers(&inside, target).0.contains("403"), "{target}");
        }
        assert_eq!(
            *inside.network.dialled.lock().unwrap(),
            ["203.0.113.30:443".parse::<SocketAddr>().unwrap()],
            "only the public host was dialled"
        );
    }

    /// Without the runner's switch nothing changes: the sandbox keeps the network it always had.
    #[test]
    fn without_enforcement_the_sandbox_keeps_its_network() {
        let Some(machine) = real_bwrap_here() else {
            return;
        };
        let dir = root("egress-bwrap-off");
        let (workspace, scratch) = (dir.path().join("w"), dir.path().join("s"));
        for path in [&workspace, &scratch] {
            std::fs::create_dir_all(path).unwrap();
        }
        let (_, plain) = machine
            .wrap(
                std::path::Path::new("/bin/true"),
                &[],
                &workspace,
                &scratch,
                &[],
                sandbox::SharedFiles::default(),
            )
            .unwrap();
        assert!(!plain.iter().any(|arg| arg == "--unshare-net"), "{plain:?}");
        let (_, bridged) = machine
            .bridged(sandbox::Bridge {
                helper: "/bin/true".into(),
                socket_dir: scratch.clone(),
            })
            .wrap(
                std::path::Path::new("/bin/true"),
                &[],
                &workspace,
                &scratch,
                &[],
                sandbox::SharedFiles::default(),
            )
            .unwrap();
        assert!(
            bridged.iter().any(|arg| arg == "--unshare-net"),
            "{bridged:?}"
        );
    }

    /// The twin of the boundary: what a run needs besides the network still starts behind the
    /// forwarder, and the forwarder hands back the program's own exit status.
    #[test]
    fn behind_the_forwarder_git_node_and_python_still_start_and_the_exit_status_is_kept() {
        if real_bwrap_here().is_none() {
            return;
        }
        let dir = root("egress-bwrap-tools");
        let (workspace, scratch, sockets) = (
            dir.path().join("workspace"),
            dir.path().join("scratch"),
            dir.path().join("sockets"),
        );
        for path in [&workspace, &scratch, &sockets, &dir.path().join("home")] {
            std::fs::create_dir_all(path).unwrap();
        }
        let machine = sandbox::detect(
            tack_runner::ProcessEnv::from_process()
                .search_path()
                .as_deref(),
            Some(&dir.path().join("home")),
        )
        .bridged(sandbox::Bridge {
            helper: env!("CARGO_BIN_EXE_tack-runner").into(),
            socket_dir: sockets,
        });
        assert_eq!(machine.kind, sandbox::SandboxKind::Bwrap);
        let search: Vec<PathBuf> = ["/usr/bin", "/bin"]
            .map(PathBuf::from)
            .into_iter()
            .chain(std::env::split_paths(
                &std::env::var_os("PATH").unwrap_or_default(),
            ))
            .collect();
        let run = |program: &std::path::Path, args: &[&str]| {
            let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
            let (bwrap, argv) = machine
                .wrap(
                    program,
                    &args,
                    &workspace,
                    &scratch,
                    &[],
                    sandbox::SharedFiles::default(),
                )
                .unwrap();
            std::process::Command::new(bwrap)
                .args(argv)
                .current_dir(&workspace)
                .output()
                .expect("bwrap runs")
        };
        for tool in ["git", "node", "python3"] {
            let Some(program) = search
                .iter()
                .map(|directory| directory.join(tool))
                .find(|candidate| candidate.is_file())
            else {
                println!("skipped: {tool} is not installed");
                continue;
            };
            let output = run(&program, &["--version"]);
            assert!(
                output.status.success(),
                "{tool} does not start behind the forwarder: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let failed = run(std::path::Path::new("/bin/sh"), &["-c", "exit 7"]);
        assert_eq!(failed.status.code(), Some(7));
    }

    /// With the switch on and no `bwrap`, the proxy is a request the agent can ignore: the
    /// capability says it is not held and the run says so.
    #[tokio::test]
    async fn without_bwrap_the_runner_says_the_proxy_is_not_held() {
        let run = live(Some("closed"), &[], true).await;
        let capability = run.harness.probe().await.additional["egress"].clone();
        assert_eq!(capability["enforced"], false);
        assert_eq!(capability["proxy"], true);
        let reason = run
            .harness
            .egress_not_held(&run.handle)
            .await
            .expect("an unheld proxy is reported");
        assert!(
            reason.contains("no sandbox") && reason.contains("nothing stops"),
            "{reason}"
        );
        run.stop().await;
        let off = live(Some("closed"), &[], false).await;
        assert!(off.harness.egress_not_held(&off.handle).await.is_none());
        off.stop().await;
    }

    #[test]
    fn every_agent_the_runner_spawns_names_its_model_endpoint_or_leaves_it_to_the_administrator() {
        for descriptor in DESCRIPTORS {
            let named = !descriptor.model_endpoints.is_empty();
            let expected = matches!(descriptor.kind, "claude-code" | "codex");
            assert_eq!(
                named, expected,
                "{}: only claude-code and codex know their endpoint; opencode takes the provider \
                 its configuration names and docket confines its own",
                descriptor.kind
            );
            for host in descriptor.model_endpoints {
                assert!(
                    host.chars().all(|c| c.is_ascii_lowercase()
                        || c.is_ascii_digit()
                        || c == '.'
                        || c == '-'),
                    "{host}"
                );
            }
        }
    }
}

/// The install of a project's dependencies runs under the same rules as the agent after it.
mod install {
    use std::path::{Path, PathBuf};

    use tack_protocol::prepare::{PrepareSetting, REPLACE_REFUSED};
    use tack_runner::{
        ProcessEnv,
        harness::sandbox::{self, Sandbox, SandboxKind},
        prepare::{Rules, prepare},
    };

    use super::root;

    const LIMIT: std::time::Duration = std::time::Duration::from_secs(60);

    /// A runner's state directory, a real home, a checkout holding a lockfile and a directory of
    /// stand-in tools, all under one temporary directory.
    struct Machine {
        _dir: tempfile::TempDir,
        home: PathBuf,
        state: PathBuf,
        checkout: PathBuf,
        tools: PathBuf,
    }

    impl Machine {
        fn new(label: &str) -> Self {
            let dir = root(label);
            let base = dir.path().to_path_buf();
            let (home, state, tools) = (base.join("home"), base.join("state"), base.join("tools"));
            let checkout = state.join("workspaces/a1");
            for path in [&home, &checkout, &tools] {
                std::fs::create_dir_all(path).expect("directory");
            }
            std::fs::write(checkout.join("package-lock.json"), "{}").expect("lockfile");
            Self {
                _dir: dir,
                home,
                state,
                checkout,
                tools,
            }
        }

        /// A tool called `name` that runs `script`.
        fn tool(&self, name: &str, script: &str) {
            let path = self.tools.join(name);
            std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).expect("tool");
            std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755))
                .expect("executable");
        }

        /// The runner's environment: its home, a `PATH` holding the tools, and a variable that
        /// holds a credential no install needs.
        fn env(&self) -> ProcessEnv {
            ProcessEnv::from_pairs([
                ("PATH", format!("{}:/usr/bin:/bin", self.tools.display())),
                ("HOME", self.home.display().to_string()),
                (
                    "AWS_SECRET_ACCESS_KEY",
                    "the-runners-own-credential".to_owned(),
                ),
            ])
        }

        async fn install(&self, rules: &Rules) -> Result<bool, String> {
            prepare(
                &self.checkout,
                &PrepareSetting::Detect,
                &self.state,
                LIMIT,
                &self.env(),
                rules,
            )
            .await
            .map_err(|error| error.to_string())
        }

        fn seen(&self, file: &str) -> String {
            std::fs::read_to_string(self.checkout.join(file)).unwrap_or_default()
        }

        /// The sandbox of this machine under the administrator's `[sandbox]` table, or `None`
        /// (unless `TACK_REQUIRE_BWRAP=1`, which makes that a failure) without `bwrap`.
        fn sandbox(&self, required: bool) -> Option<Sandbox> {
            let path = ProcessEnv::from_process().search_path();
            let found = sandbox::detect(path.as_deref(), Some(&self.home));
            if found.kind != SandboxKind::Bwrap {
                if std::env::var("TACK_REQUIRE_BWRAP").as_deref() == Ok("1") {
                    panic!("bwrap required by TACK_REQUIRE_BWRAP but not on PATH");
                }
                println!("skipped: bwrap is not on PATH");
                return None;
            }
            Some(found.requiring(required))
        }

        /// A machine with no `bwrap` at all.
        fn without_sandbox(&self, required: bool) -> Sandbox {
            sandbox::detect(Some(self.state.as_os_str()), Some(&self.home)).requiring(required)
        }
    }

    /// What a package's `postinstall` does: writes into the home it is given and looks for the
    /// runner's credential, then installs.
    const POSTINSTALL: &str = "echo owned > \"$HOME/pwned\"; \
        echo \"$AWS_SECRET_ACCESS_KEY\" > seen-credential; \
        mkdir node_modules && echo ok > node_modules/installed";

    #[tokio::test]
    async fn a_postinstall_cannot_reach_the_real_home_or_the_runners_credentials() {
        let machine = Machine::new("install-home");
        machine.tool("npm", POSTINSTALL);
        let Some(sandbox) = machine.sandbox(true) else {
            return;
        };

        // Without the sandbox the same script does reach the home: the control for the checks below.
        let open = Rules::default();
        assert_eq!(machine.install(&open).await, Ok(true));
        assert!(
            machine.home.join("pwned").exists(),
            "the control reaches the home"
        );
        std::fs::remove_file(machine.home.join("pwned")).expect("clean the control");
        std::fs::remove_dir_all(machine.checkout.join("node_modules")).expect("clean");
        std::fs::remove_dir_all(machine.state.join("prepare-cache")).expect("clean");
        std::fs::remove_file(machine.checkout.join("seen-credential")).expect("clean");

        let rules = Rules {
            sandbox,
            ..Rules::default()
        };
        assert_eq!(machine.install(&rules).await, Ok(true));
        assert!(
            !machine.home.join("pwned").exists(),
            "the install wrote into the real home"
        );
        assert_eq!(
            machine.seen("seen-credential").trim(),
            "",
            "the install saw a variable outside its allow-list"
        );
        assert_eq!(
            machine.seen("node_modules/installed").trim(),
            "ok",
            "the install still installs"
        );
    }

    #[tokio::test]
    async fn the_install_goes_through_the_proxy_only_when_the_runner_enforces_web_access() {
        let machine = Machine::new("install-proxy");
        machine.tool(
            "npm",
            "echo \"$HTTPS_PROXY\" > seen-proxy; mkdir node_modules",
        );
        let Some(sandbox) = machine.sandbox(true) else {
            return;
        };
        let mut rules = Rules {
            sandbox,
            ..Rules::default()
        };
        assert_eq!(machine.install(&rules).await, Ok(true));
        assert_eq!(machine.seen("seen-proxy").trim(), "");

        std::fs::remove_dir_all(machine.checkout.join("node_modules")).expect("clean");
        std::fs::remove_dir_all(machine.state.join("prepare-cache")).expect("clean");
        rules.enforce_egress = true;
        assert_eq!(machine.install(&rules).await, Ok(true));
        assert!(
            machine.seen("seen-proxy").starts_with("http://127.0.0.1:"),
            "{}",
            machine.seen("seen-proxy")
        );
    }

    #[tokio::test]
    async fn a_command_the_board_replaced_is_refused_with_a_reason_unless_the_runner_allows_it() {
        let machine = Machine::new("install-replace");
        let replace = PrepareSetting::Replace {
            command: "touch ran".to_owned(),
        };
        let run = |rules: Rules| {
            let (machine, replace) = (&machine, replace.clone());
            async move {
                prepare(
                    &machine.checkout,
                    &replace,
                    &machine.state,
                    LIMIT,
                    &machine.env(),
                    &rules,
                )
                .await
                .map_err(|error| error.to_string())
            }
        };

        assert_eq!(run(Rules::default()).await, Err(REPLACE_REFUSED.to_owned()));
        assert!(!machine.checkout.join("ran").exists(), "the command ran");

        let allowed = Rules {
            board_may_replace: true,
            ..Rules::default()
        };
        assert_eq!(run(allowed).await, Ok(true));
        assert!(machine.checkout.join("ran").exists());
    }

    #[tokio::test]
    async fn a_machine_with_no_sandbox_installs_only_where_the_administrator_turned_the_requirement_off()
     {
        let machine = Machine::new("install-optout");
        machine.tool("npm", "mkdir node_modules");

        let required = Rules {
            sandbox: machine.without_sandbox(true),
            ..Rules::default()
        };
        let reason = machine.install(&required).await.expect_err("refused");
        assert!(reason.contains("no sandbox"), "{reason}");
        assert!(!machine.checkout.join("node_modules").exists());

        let opted_out = Rules {
            sandbox: machine.without_sandbox(false),
            ..Rules::default()
        };
        assert!(opted_out.unsandboxed() && !required.unsandboxed());
        assert_eq!(machine.install(&opted_out).await, Ok(true));
        assert!(machine.checkout.join("node_modules").exists());
    }

    /// `npm ci` of a tiny project whose one dependency is a folder beside it, so no network is
    /// needed, with the machine's own npm (found wherever the person installed it).
    #[tokio::test]
    async fn npm_ci_of_a_small_project_still_installs_in_the_sandbox() {
        let machine = Machine::new("install-npm");
        let Some(sandbox) = machine.sandbox(true) else {
            return;
        };
        let process = ProcessEnv::from_process();
        let npm_on_path = std::env::split_paths(&process.search_path().unwrap_or_default())
            .any(|dir| dir.join("npm").is_file());
        if !npm_on_path {
            println!("skipped: npm is not installed");
            return;
        }
        let checkout: &Path = &machine.checkout;
        std::fs::create_dir_all(checkout.join("dep")).expect("dependency");
        std::fs::write(
            checkout.join("package.json"),
            r#"{"name":"calc","version":"1.0.0","dependencies":{"dep":"file:./dep"}}"#,
        )
        .expect("manifest");
        std::fs::write(
            checkout.join("dep/package.json"),
            r#"{"name":"dep","version":"1.0.0","main":"index.js"}"#,
        )
        .expect("dependency manifest");
        std::fs::write(checkout.join("dep/index.js"), "module.exports = 1;\n").expect("source");
        std::fs::write(
            checkout.join("package-lock.json"),
            r#"{"name":"calc","version":"1.0.0","lockfileVersion":3,"requires":true,"packages":{"":{"name":"calc","version":"1.0.0","dependencies":{"dep":"file:./dep"}},"dep":{"version":"1.0.0"},"node_modules/dep":{"resolved":"dep","link":true}}}"#,
        )
        .expect("lockfile");

        let rules = Rules {
            sandbox,
            enforce_egress: true,
            ..Rules::default()
        };
        // The machine's own PATH and a home of its own, as a runner started by a person has.
        let env = ProcessEnv::from_pairs([
            ("PATH", process.path_or_standard()),
            ("HOME", machine.home.display().to_string()),
        ]);
        let ran = prepare(
            checkout,
            &PrepareSetting::Detect,
            &machine.state,
            LIMIT,
            &env,
            &rules,
        )
        .await
        .expect("npm ci installs in the sandbox");
        assert!(ran);
        assert!(
            checkout.join("node_modules/dep/index.js").is_file(),
            "the dependency is installed"
        );
        assert!(
            checkout.join("node_modules/.tack-prepared").is_file(),
            "the install is marked as the runner's own"
        );
    }
}
