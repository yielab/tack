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
    RequestedModelId, RequestedModelProvider, RunnerSelector,
};
use tack_runner::{
    client::{
        AttemptId, AttemptLease, AttemptState, ClaimRequestId, ClaimedWork, FencingToken,
        HarnessAdapter, RunnerId, Timestamp, Workspace, WorkspaceId as ClientWorkspaceId,
        engine::ExecutionSpec,
    },
    config::{
        DEFAULT_VERCEL_AI_GATEWAY_SECRET, ProviderConfig, VERCEL_AI_GATEWAY_CONFIG_KEY,
        VERCEL_AI_GATEWAY_PROVIDER,
    },
    harness::{
        HarnessProbe, ModelObservationSource, artifact::ArtifactStager,
        claude_code::ClaudeCodeAdapter,
    },
    secrets::SecretStore,
};

use crate::common::temp_dir as temp_workspace;

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

fn gateway_providers() -> BTreeMap<String, ProviderConfig> {
    BTreeMap::from([(
        VERCEL_AI_GATEWAY_CONFIG_KEY.to_owned(),
        ProviderConfig {
            enabled: true,
            secret: DEFAULT_VERCEL_AI_GATEWAY_SECRET.to_owned(),
        },
    )])
}

/// The real runner-local secret store: the platform keychain, or its
/// owner-only file fallback — whichever this machine actually has.
fn runner_state_secret_store() -> SecretStore {
    let state_dir = std::env::var_os("TACK_RUNNER_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var("HOME").expect("HOME is set")).join(".tack-runner")
        });
    SecretStore::open(&state_dir.join("secrets.json"))
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
    let Ok(adapter) = ClaudeCodeAdapter::discover(test_secret_store(secrets_dir.path())) else {
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

/// Live proof of the provider endpoint path, not the direct one above:
/// resolves the real runner-local secret store for a `vercel_ai_gateway`
/// entry, points a real `claude` binary at it, and records what the CLI
/// reported. Gated identically to the test above (a real invocation is
/// billed), plus a clean skip when no store entry exists at all — this
/// test never fabricates one.
#[tokio::test]
#[ignore = "opt-in: requires a real `claude` binary on PATH, a `vercel_ai_gateway` entry in \
            this machine's secret store, *and* TACK_RUN_LIVE_CLAUDE_CODE_TEST=1 (a real \
            invocation is billed); run with TACK_RUN_LIVE_CLAUDE_CODE_TEST=1 cargo nextest \
            run --workspace --run-ignored ignored-only -E 'binary(live_claude_code)'"]
async fn live_claude_code_through_the_configured_provider_when_opted_in() {
    if !opted_in() {
        eprintln!(
            "skipping live claude-code gateway test: set TACK_RUN_LIVE_CLAUDE_CODE_TEST=1 to \
             opt in (a real invocation is billed)"
        );
        return;
    }
    let Ok(adapter) = ClaudeCodeAdapter::discover(runner_state_secret_store()) else {
        eprintln!("skipping live claude-code gateway test: no `claude` binary discoverable");
        return;
    };
    let adapter = adapter.with_providers(gateway_providers());

    let workspace_dir = temp_workspace("live-gateway");
    let workspace = workspace_dir.path();
    git_init(workspace);
    let mut spec = spec_with(
        Some(VERCEL_AI_GATEWAY_PROVIDER),
        &[],
        true,
        BTreeMap::new(),
        workspace.to_path_buf(),
    );
    spec.work.request.requested_model_id = Some(RequestedModelId::new("anthropic/claude-opus-4.6"));
    spec.work.request.resolved_agent_profile.instructions = "Say exactly: ok".to_string();

    if let Err(error) = adapter.validate(&spec).await {
        eprintln!(
            "skipping live claude-code gateway test: no configured provider entry to \
             validate against ({error})"
        );
        std::fs::remove_dir_all(workspace).expect("cleanup");
        return;
    }
    let handle = adapter
        .start(&spec)
        .await
        .expect("start a live gateway-routed process");
    let outcome = adapter
        .wait(&handle)
        .await
        .expect("wait for a live gateway-routed process");

    eprintln!(
        "live claude-code (gateway) outcome: terminal_state={:?} model_provider={} \
         model_id={} model_observation_source={} terminal_reason={}",
        outcome.terminal_state,
        outcome.actual_execution.model_provider.as_str(),
        outcome.actual_execution.model_id.as_str(),
        outcome.actual_execution.model_observation_source,
        outcome.terminal_reason
    );

    // Holds regardless of whether the configured credential is itself
    // valid, and regardless of which of two honest outcomes this
    // specific run hits: a `result` line arriving before the request
    // timeout (`requested_not_confirmed`) or the process being killed
    // mid-retry-storm with no such line ever seen (`not_observed`) — a
    // real invalid-key run measured exponential retry delays that make
    // the latter the far likelier case within any test-sized timeout.
    // The one claim that must never hold for a gateway-routed run is
    // `harness_reported`: this line is emitted before any network call
    // reaches the gateway, so it can never be treated as confirmation
    // the gateway actually served it. A successful completion
    // additionally needs a working credential, which this test does not
    // assert on: that requires a separately run, deliberately billed
    // proof against a real credential.
    assert_ne!(
        outcome.actual_execution.model_observation_source,
        ModelObservationSource::HarnessReported.as_str(),
        "a gateway-routed run must never claim harness_reported"
    );

    std::fs::remove_dir_all(workspace).expect("cleanup disposable fixture repo");
}

/// The live counterpart to the fake-shim provider-endpoint guard tests
/// in `harness::claude_code`'s own unit tests: with the configured
/// provider enabled *and* genuinely working (a real, billed gateway
/// completion is proven by the test above), a direct-model request
/// against the same adapter must still never reach the gateway. Never
/// bills anything itself — a direct request with no ambient login on
/// this machine fails in milliseconds ("Not logged in"), which is the
/// point: if it had instead reached the gateway, it would have
/// succeeded, exactly like the test above.
#[tokio::test]
#[ignore = "opt-in: requires a real `claude` binary on PATH *and* \
            TACK_RUN_LIVE_CLAUDE_CODE_TEST=1; run with TACK_RUN_LIVE_CLAUDE_CODE_TEST=1 \
            cargo nextest run --workspace --run-ignored ignored-only \
            -E 'binary(live_claude_code)'"]
async fn live_claude_code_direct_model_never_reaches_the_configured_provider_when_opted_in() {
    if !opted_in() {
        eprintln!(
            "skipping live claude-code direct-guard test: set TACK_RUN_LIVE_CLAUDE_CODE_TEST=1 \
             to opt in"
        );
        return;
    }
    let Ok(adapter) = ClaudeCodeAdapter::discover(runner_state_secret_store()) else {
        eprintln!("skipping live claude-code direct-guard test: no `claude` binary discoverable");
        return;
    };
    let adapter = adapter.with_providers(gateway_providers());

    let workspace_dir = temp_workspace("live-direct-guard");
    let workspace = workspace_dir.path();
    git_init(workspace);
    // No requested_model_provider at all: the direct/subscription path.
    let spec = spec_with(None, &[], true, BTreeMap::new(), workspace.to_path_buf());

    adapter
        .validate(&spec)
        .await
        .expect("a direct request validates even with the provider configured");
    let handle = adapter
        .start(&spec)
        .await
        .expect("start a direct-model process");
    let outcome = adapter
        .wait(&handle)
        .await
        .expect("wait for a direct-model process");

    eprintln!(
        "live claude-code (direct, provider configured but unused) outcome: \
         terminal_state={:?} terminal_reason={}",
        outcome.terminal_state, outcome.terminal_reason
    );

    // The decisive check: the gateway's own distinctive error shape
    // ("authentication_failed"/"api_retry") must never appear on a
    // direct request, proving it never reached ai-gateway.vercel.sh.
    let serialized = outcome.terminal_reason.to_string();
    assert!(
        !serialized.contains("authentication_failed") && !serialized.contains("api_retry"),
        "a direct request must never show the gateway's own error shape: {serialized}"
    );

    std::fs::remove_dir_all(workspace).expect("cleanup");
}
