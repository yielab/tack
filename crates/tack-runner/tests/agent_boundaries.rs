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
        codex::CodexGrammar,
        env_policy,
        local_process::{
            HarnessDescriptor, HarnessGrammar, Invocation, LocalProcessHarness, ModelSelection,
            RunContext, RunReport,
        },
        opencode::OpencodeGrammar,
        process::{ProcessLimits, ProcessResult},
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
    wire: tack_runner::provider::Wire::OpenAiResponses,
    model_selection: ModelSelection::Optional,
    native_provider: "native",
    inherited_env: &["PATH"],
    model_passthrough: "forwarded verbatim",
    probe_notes: &[],
    credential_note: "",
    credential_env: None,
    observes_served_model: false,
    reports_process_groups: false,
    reports_progress: false,
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
    wire: tack_runner::provider::Wire::OpenAiResponses,
    model_selection: ModelSelection::Optional,
    native_provider: "native",
    inherited_env: &["PATH"],
    model_passthrough: "forwarded verbatim",
    probe_notes: &[],
    credential_note: "",
    credential_env: None,
    observes_served_model: false,
    reports_process_groups: false,
    reports_progress: false,
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
        endpoint: None,
        scratch: dir.path(),
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
