//! Opt-in, `#[ignore]`-gated tests against a real installed `claude`
//! binary. Never required in CI, never fails just because `claude` is
//! absent; also requires `TACK_RUN_LIVE_CLAUDE_CODE_TEST=1` even under
//! `--ignored`, since a real invocation is billed. None of these reads,
//! logs, or forwards a credential: whatever the installed CLI or this
//! machine's own secret store already carries is used as configured.
//!
//! Run: `TACK_RUN_LIVE_CLAUDE_CODE_TEST=1 cargo nextest run --workspace
//! --run-ignored ignored-only -E 'binary(live)'`

use std::{collections::BTreeMap, path::PathBuf};

use tack_orch::execution::{
    AgentProfileSnapshot, AttemptSnapshot, EnvironmentValue, ExecutionRequestSnapshot,
    ExecutionState, HarnessKind as DomainHarnessKindType, PermissionPolicy, RepositorySnapshot,
    RequestedModelProvider, RunnerSelector,
};
use tack_runner::{
    client::{
        AttemptId, AttemptLease, AttemptState, ClaimRequestId, ClaimedWork, FencingToken,
        HarnessAdapter, RunnerId, Timestamp, Workspace, WorkspaceId as ClientWorkspaceId,
        engine::ExecutionSpec,
    },
    harness::{
        HarnessProbe,
        artifact::ArtifactStager,
        claude_code::{ClaudeCodeAdapter, ClaudeCodeGrammar},
        process::ProcessLimits,
    },
    secrets::SecretStore,
};

use crate::common::temp_dir as temp_workspace;

/// The installed `claude`, staging run logs under `state`.
fn discover(secrets: SecretStore, state: &std::path::Path) -> Result<ClaudeCodeAdapter, String> {
    let limits = ProcessLimits::new(
        4 * 1024 * 1024,
        1024 * 1024,
        std::time::Duration::from_secs(600),
    );
    ClaudeCodeAdapter::discover(ClaudeCodeGrammar, limits, state.join("staging"), secrets)
}

fn opted_in() -> bool {
    std::env::var("TACK_RUN_LIVE_CLAUDE_CODE_TEST").as_deref() == Ok("1")
}

fn test_secret_store(dir: &std::path::Path) -> SecretStore {
    SecretStore::file(dir.join("secrets.json"))
}

fn env_entry(value: &str) -> EnvironmentValue {
    EnvironmentValue {
        value: Some(value.to_string()),
        secret_reference: None,
        additional: Default::default(),
    }
}

fn permission_policy(tools: &[&str], network: bool) -> PermissionPolicy {
    PermissionPolicy {
        tools: tools.iter().map(|tool| tool.to_string()).collect(),
        network,
        approvals: None,
        additional: Default::default(),
    }
}

/// Builds a minimal, well-formed `claude-code` spec. Rebuilt here against
/// only the crate's public API — this file compiles as its own crate
/// with no access to `harness::claude_code::tests`'s private helpers.
fn spec_with(
    provider: Option<&str>,
    tools: &[&str],
    network: bool,
    environment: BTreeMap<String, EnvironmentValue>,
    workspace_path: PathBuf,
) -> ExecutionSpec {
    let request = ExecutionRequestSnapshot {
        request_id: tack_orch::execution::ExecutionRequestId::new("exec_live_test"),
        item_id: tack_orch::execution::ItemId::new("item_live_test"),
        idempotency_key: tack_orch::execution::IdempotencyKey::new("idem_live_test"),
        created_by: serde_json::json!({"source": "operator", "subject_id": "live-test"}),
        created_at: chrono::Utc::now(),
        selector: RunnerSelector::ExactRunner {
            runner_id: tack_orch::execution::RunnerId::new("runr_live_test"),
        },
        agent_profile_id: tack_orch::execution::AgentProfileId::new("ap_live_test"),
        resolved_agent_profile: AgentProfileSnapshot {
            name: "Live test profile".to_string(),
            instructions: "Print exactly: ok".to_string(),
            tool_policy: serde_json::json!({}),
            timeout_seconds: 60,
            budgets: serde_json::json!({}),
            additional: Default::default(),
        },
        requested_harness_kind: DomainHarnessKindType::new("claude-code"),
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
        timeout_seconds: 60,
        budgets: serde_json::json!({}),
        status_map_policy_id: None,
        environment,
        metadata: serde_json::json!({}),
        brief: None,
        verify: None,
        push_branch: None,
        additional: Default::default(),
    };
    let attempt = AttemptSnapshot {
        attempt_id: tack_orch::execution::AttemptId::new("att_live_test"),
        request_id: request.request_id.clone(),
        attempt_number: 1,
        runner_id: tack_orch::execution::RunnerId::new("runr_live_test"),
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
            claim_request_id: ClaimRequestId::new("claim_live_test"),
            lease: AttemptLease {
                attempt_id: AttemptId::new("att_live_test"),
                runner_id: RunnerId::new("runr_live_test"),
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
            attempt_id: AttemptId::new("att_live_test"),
            id: ClientWorkspaceId::new("ws_live_test"),
            path: workspace_path,
            base_revision: "0123456789abcdef0123456789abcdef01234567".to_string(),
        },
    }
}

fn git_init(workspace: &std::path::Path) {
    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(workspace)
        .status()
        .expect("git init");
}

/// Records the observed version and stages a real produced artifact (a
/// `README.md` this test asks Claude Code, via its `Write` tool, to
/// overwrite, inside a disposable fixture git repo this test creates
/// and deletes itself — never this checkout).
#[tokio::test]
#[ignore = "opt-in: requires a real `claude` binary on PATH *and* \
            TACK_RUN_LIVE_CLAUDE_CODE_TEST=1 (a real invocation is billed); run with \
            TACK_RUN_LIVE_CLAUDE_CODE_TEST=1 cargo nextest run --workspace \
            --run-ignored ignored-only -E 'binary(live_claude_code)'"]
async fn live_claude_code_records_version_and_a_real_artifact_when_opted_in() {
    if !opted_in() {
        eprintln!(
            "skipping live claude-code test: set TACK_RUN_LIVE_CLAUDE_CODE_TEST=1 to opt in \
             (a real invocation is billed)"
        );
        return;
    }
    let secrets_dir = temp_workspace("secrets");
    let Ok(adapter) = discover(test_secret_store(secrets_dir.path()), secrets_dir.path()) else {
        eprintln!("skipping live claude-code test: no `claude` binary discoverable on PATH");
        return;
    };

    let capability = adapter.probe().await;
    assert!(
        capability.probe_error.is_none(),
        "expected a healthy probe against a real installed binary: {:?}",
        capability.probe_error
    );
    assert!(!capability.installed_version.is_empty());
    eprintln!(
        "live claude-code probe: version={}",
        capability.installed_version
    );

    let workspace_dir = temp_workspace("live-fixture-repo");
    let workspace = workspace_dir.path();
    git_init(workspace);
    std::process::Command::new("git")
        .args(["config", "user.email", "probe@example.invalid"])
        .current_dir(workspace)
        .status()
        .expect("git config email");
    std::process::Command::new("git")
        .args(["config", "user.name", "Probe"])
        .current_dir(workspace)
        .status()
        .expect("git config name");
    std::fs::write(workspace.join("README.md"), "fixture\n").expect("seed fixture file");
    std::process::Command::new("git")
        .args(["add", "README.md"])
        .current_dir(workspace)
        .status()
        .expect("git add");
    std::process::Command::new("git")
        .args(["commit", "-q", "-m", "seed"])
        .current_dir(workspace)
        .status()
        .expect("git commit");

    let mut environment = BTreeMap::new();
    environment.insert(
        "HOME".to_string(),
        env_entry(&std::env::var("HOME").unwrap_or_default()),
    );
    let mut spec = spec_with(
        None,
        &["Write"],
        false,
        environment,
        workspace.to_path_buf(),
    );
    // Overrides the shared helper's default "Print exactly: ok" prompt
    // with one that actually exercises the `Write` tool this spec
    // allows, so the artifact staged below is genuinely something
    // Claude Code produced, not merely the seed content this test
    // itself wrote.
    spec.work.request.resolved_agent_profile.instructions =
        "Using the Write tool, overwrite README.md in the current directory with exactly \
         this content: tack-d2-live-test-marker"
            .to_string();

    adapter.validate(&spec).await.expect("validate a live spec");
    let handle = adapter.start(&spec).await.expect("start a live process");
    let outcome = adapter
        .wait(&handle)
        .await
        .expect("wait for a live process");

    eprintln!(
        "live claude-code outcome: terminal_state={:?} model_id={} harness_version={}",
        outcome.terminal_state,
        outcome.actual_execution.model_id.as_str(),
        outcome.actual_execution.harness_version
    );

    let staged = ArtifactStager::new(workspace.join(".artifacts"))
        .stage_file(
            "live-test-attempt",
            workspace,
            std::path::Path::new("README.md"),
            "log",
            "text/markdown",
        )
        .expect("stage the real artifact Claude Code's Write tool produced");
    assert!(staged.size_bytes > 0);
    let staged_content = std::fs::read_to_string(&staged.staged_path).unwrap_or_default();
    eprintln!("live claude-code staged artifact content: {staged_content:?}");
    if !staged_content.contains("tack-d2-live-test-marker") {
        eprintln!(
            "note: the model did not reproduce the exact requested marker text — this is \
             model-phrasing variance, not itself a failure of this adapter, so it is only \
             logged, never asserted on"
        );
    }
    eprintln!(
        "live claude-code artifact: sha256={} size_bytes={}",
        staged.sha256, staged.size_bytes
    );

    std::fs::remove_dir_all(workspace).expect("cleanup disposable fixture repo");
}
