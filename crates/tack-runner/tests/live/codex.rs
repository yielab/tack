//! Live acceptance tests against a real, installed `codex` binary. Moved
//! out of `crates/tack-runner/src/harness/codex/tests.rs` (audit
//! `docs/closed-cycles/plans/harness-maintainability-audit.md` §5 rule 4: "anything that
//! runs a real binary lives under `tests/live/`") — only the crate's public
//! API is used here, since an integration test binary cannot see private
//! items.

use std::{collections::BTreeMap, path::PathBuf, time::Duration};

use tack_orch::execution::{
    AttemptSnapshot, ExecutionRequestSnapshot, HarnessKind as DomainHarnessKind, RequestedModelId,
    RequestedModelProvider,
};
use tack_runner::{
    SecretStore,
    client::{
        AttemptId, AttemptLease, AttemptState, ClaimRequestId, ClaimedWork, FencingToken,
        HarnessAdapter, RunnerId, Timestamp, Workspace, WorkspaceId, engine::ExecutionSpec,
    },
    config::{
        DEFAULT_VERCEL_AI_GATEWAY_SECRET, ProviderConfig, VERCEL_AI_GATEWAY_CONFIG_KEY,
        VERCEL_AI_GATEWAY_PROVIDER,
    },
    harness::{
        HarnessProbe, ModelObservationSource,
        artifact::ArtifactStager,
        codex::{CodexAdapter, CodexGrammar},
        locate::locate_installed,
        process::ProcessLimits,
        sha256::sha256_hex,
    },
};

use crate::common;

const CODEX_HARNESS_KIND: &str = "codex";
const CODEX_PROGRAM_NAME: &str = "codex";
const MODEL_OBSERVATION_SOURCE: &str = ModelObservationSource::RequestedNotConfirmed.as_str();

fn deterministic_fixture_repo(label: &str) -> tempfile::TempDir {
    let root = common::temp_dir(label);
    std::fs::write(root.path().join("README.md"), b"# fixture repo\n").expect("write README");
    root
}

fn test_secret_store(dir: &std::path::Path) -> SecretStore {
    SecretStore::file(dir.join("secrets.json"))
}

/// The frozen claim fixture, retargeted at `codex` with an explicit model —
/// `codex`'s `validate` rejects an auto-selected model pre-spawn, so every
/// live spec needs one, matching `codex/tests.rs`'s own `spec_with`.
fn spec_with(workspace_path: PathBuf, provider: &str, model_id: &str) -> ExecutionSpec {
    let claim: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../docs/contracts/runner-v1/claim.response.json"
    ))
    .expect("claim fixture");
    let mut request: ExecutionRequestSnapshot =
        serde_json::from_value(claim["request"].clone()).expect("request fixture");
    request.requested_harness_kind = DomainHarnessKind::new(CODEX_HARNESS_KIND);
    request.requested_model_provider = Some(RequestedModelProvider::new(provider));
    request.requested_model_id = Some(RequestedModelId::new(model_id));
    request.timeout_seconds = 60;
    let attempt: AttemptSnapshot =
        serde_json::from_value(claim["attempt"].clone()).expect("attempt fixture");

    ExecutionSpec {
        work: ClaimedWork {
            claim_request_id: ClaimRequestId::new("claim-live-codex"),
            lease: AttemptLease {
                attempt_id: AttemptId::new("attempt-live-codex"),
                runner_id: RunnerId::new("runner-live-codex"),
                fencing_token: FencingToken(1),
                attempt_number: 1,
                state: AttemptState::Leased,
                issued_at: Timestamp::new("2026-08-09T11:59:00Z"),
                expires_at: Timestamp::new("2026-08-09T12:59:00Z"),
            },
            request,
            attempt,
        },
        workspace: Workspace {
            attempt_id: AttemptId::new("attempt-live-codex"),
            id: WorkspaceId::new("ws_live_codex"),
            path: workspace_path,
            base_revision: "revision".into(),
        },
    }
}

/// Acceptance: "an opt-in live test records version and artifact."
///
/// Deliberately does **not** attempt a real, non-interactive `codex exec`
/// run: whether that requires network access and provider credentials, and
/// rule 8 ("live harness tests ... never require secrets in CI") makes that
/// an unacceptable risk to take on a guess. Instead this test performs two
/// things that are safe without any credential:
///
/// 1. Real version probing against whatever `codex` is actually on `PATH`
///    (the operation most CLIs support without authentication).
/// 2. Staging a real artifact (a fixed local file, not one produced by
///    running a task) through the exact same [`ArtifactStager`] path
///    `wait()` uses, proving the local mechanism end-to-end.
///
/// Both `#[ignore]`d (so a plain `cargo test` never runs this) and
/// self-skipping at runtime if `codex` is absent, so it can never fail CI
/// and is never the only proof of either behavior (`codex/tests.rs`'s
/// fake-binary tests already cover both independently).
#[tokio::test]
#[ignore = "opt-in: requires a real `codex` binary on PATH; run with \
            `cargo nextest run --workspace --run-ignored ignored-only -E 'binary(live)'`"]
async fn live_probe_and_artifact_staging_against_a_real_codex_binary_when_present() {
    if locate_installed(CODEX_PROGRAM_NAME).is_err() {
        eprintln!("skipping live codex test: `codex` not found on PATH");
        return;
    }

    let scratch = common::temp_dir("live-artifacts");
    let adapter = CodexAdapter::discover(
        CodexGrammar,
        ProcessLimits::new(1_048_576, 1_048_576, Duration::from_secs(30)),
        scratch.path().to_path_buf(),
        test_secret_store(scratch.path()),
    )
    .expect("codex was located above");

    let capability = adapter.probe().await;
    eprintln!(
        "live codex probe: installed_version={:?} probe_error={:?}",
        capability.installed_version, capability.probe_error
    );
    // The observed real CLI prints a program-name-prefixed version
    // (`codex-cli 0.149.1`); a probe error here means either the installed
    // binary changed its output shape again or the token scan regressed —
    // either way this is the signal to look again, not an assertion to
    // weaken back to "ran without panicking".
    assert_eq!(
        capability.probe_error, None,
        "codex version probe must recognize the installed binary's real output"
    );

    let workspace_dir = deterministic_fixture_repo("live-artifact");
    let workspace = workspace_dir.path();
    let staging = common::temp_dir("live-artifact-staging");
    let stager = ArtifactStager::new(staging.path().to_path_buf());
    let staged = stager
        .stage_file(
            "live-attempt",
            workspace,
            std::path::Path::new("README.md"),
            "log",
            "text/plain",
        )
        .expect("stage a real local artifact");
    assert!(staged.size_bytes > 0);
    assert_eq!(staged.sha256, sha256_hex(b"# fixture repo\n"));
    eprintln!(
        "live codex artifact staged at {}",
        staged.staged_path.display()
    );
}

/// Live proof of the provider endpoint path: resolves the real runner-local
/// secret store for a `vercel_ai_gateway` entry, points a real `codex`
/// binary at it via the per-invocation `-c` overrides (never
/// `~/.codex/config.toml`), and records what the CLI reported. Requires an
/// explicit opt-in even under `--ignored`, matching `claude_code`'s
/// identical gateway test and unlike the credential-free live test above:
/// this one does attempt a real, billed `exec`.
#[tokio::test]
#[ignore = "opt-in: requires a real `codex` binary on PATH, a `vercel_ai_gateway` entry in \
            this machine's secret store, *and* TACK_RUN_LIVE_CODEX_GATEWAY_TEST=1 (a real \
            invocation is billed); run with TACK_RUN_LIVE_CODEX_GATEWAY_TEST=1 cargo nextest \
            run --workspace --run-ignored ignored-only -E 'binary(live)'"]
async fn live_codex_through_the_configured_provider_when_opted_in() {
    if std::env::var("TACK_RUN_LIVE_CODEX_GATEWAY_TEST").as_deref() != Ok("1") {
        eprintln!(
            "skipping live codex gateway test: set TACK_RUN_LIVE_CODEX_GATEWAY_TEST=1 to opt in \
             (a real invocation is billed)"
        );
        return;
    }
    if locate_installed(CODEX_PROGRAM_NAME).is_err() {
        eprintln!("skipping live codex gateway test: `codex` not found on PATH");
        return;
    }
    let state_dir = std::env::var_os("TACK_RUNNER_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var("HOME").expect("HOME is set")).join(".tack-runner")
        });
    let secrets = SecretStore::open(&state_dir.join("secrets.json"));
    let providers = BTreeMap::from([(
        VERCEL_AI_GATEWAY_CONFIG_KEY.to_owned(),
        ProviderConfig {
            enabled: true,
            secret: DEFAULT_VERCEL_AI_GATEWAY_SECRET.to_owned(),
        },
    )]);

    let scratch = common::temp_dir("live-gateway-artifacts");
    let adapter = CodexAdapter::discover(
        CodexGrammar,
        ProcessLimits::new(1_048_576, 1_048_576, Duration::from_secs(60)),
        scratch.path().to_path_buf(),
        secrets,
    )
    .expect("codex was located above")
    .with_providers(providers);

    let workspace_dir = deterministic_fixture_repo("live-gateway");
    let workspace = workspace_dir.path();
    // Codex refuses to run outside a git repository; a real workspace is
    // always a git checkout (`WorkspaceManager`), so this makes the fixture
    // structurally match production rather than special-casing the check
    // away.
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.email", "tack-live-test@example.invalid"],
        vec!["config", "user.name", "tack-live-test"],
    ] {
        std::process::Command::new("git")
            .args(&args)
            .current_dir(workspace)
            .status()
            .expect("git init the fixture repo");
    }
    std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(workspace)
        .status()
        .expect("git add");
    std::process::Command::new("git")
        .args(["commit", "-q", "-m", "fixture"])
        .current_dir(workspace)
        .status()
        .expect("git commit");
    // `openai/gpt-5.1` and `openai/gpt-5.1-codex` were both measured live to
    // fail here on a codex-side tool the resolved model doesn't support
    // ("Tool 'tool_search' is not supported with ...") — a model-
    // compatibility rejection, not an auth or routing failure (the
    // gateway's own routing metadata confirmed both requests reached and
    // were resolved by the real gateway). `openai/gpt-5.6-sol` is Vercel's
    // own documented default model for Codex through the gateway.
    let spec = spec_with(
        workspace.to_path_buf(),
        VERCEL_AI_GATEWAY_PROVIDER,
        "openai/gpt-5.6-sol",
    );

    if let Err(error) = adapter.validate(&spec).await {
        eprintln!(
            "skipping live codex gateway test: no configured provider entry to validate against \
             ({error})"
        );
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
        "live codex (gateway) outcome: terminal_state={:?} model_provider={} model_id={} \
         model_observation_source={} terminal_reason={}",
        outcome.terminal_state,
        outcome.actual_execution.model_provider.as_str(),
        outcome.actual_execution.model_id.as_str(),
        outcome.actual_execution.model_observation_source,
        outcome.terminal_reason
    );

    // Codex always echoes the requested provider/model rather than
    // attempting to observe one (`fixtures/codex/README.md`, "Unverified"
    // finding 3) — this holds regardless of whether the configured
    // credential is valid.
    assert_eq!(
        outcome.actual_execution.model_provider.as_str(),
        VERCEL_AI_GATEWAY_PROVIDER
    );
    assert_eq!(
        outcome.actual_execution.model_id.as_str(),
        "openai/gpt-5.6-sol"
    );
    assert_eq!(
        outcome.actual_execution.model_observation_source,
        MODEL_OBSERVATION_SOURCE
    );
}
