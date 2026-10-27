//! Live acceptance tests against a real, installed `codex` binary. Moved
//! out of `crates/tack-runner/src/harness/codex/tests.rs` (audit
//! `docs/closed-cycles/plans/harness-maintainability-audit.md` §5 rule 4: "anything that
//! runs a real binary lives under `tests/live/`") — only the crate's public
//! API is used here, since an integration test binary cannot see private
//! items.

use std::time::Duration;

use tack_runner::{
    SecretStore,
    harness::{
        HarnessProbe,
        artifact::ArtifactStager,
        codex::{CodexAdapter, CodexGrammar},
        locate::locate,
        process::ProcessLimits,
        sha256::sha256_hex,
    },
};

use crate::common;

const CODEX_PROGRAM_NAME: &str = "codex";

fn deterministic_fixture_repo(label: &str) -> tempfile::TempDir {
    let root = common::temp_dir(label);
    std::fs::write(root.path().join("README.md"), b"# fixture repo\n").expect("write README");
    root
}

fn test_secret_store(dir: &std::path::Path) -> SecretStore {
    SecretStore::file(dir.join("secrets.json"))
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
            `cargo nextest run --workspace --run-ignored ignored-only -E 'binary_id(tack-runner::it) & test(/^live::/)'`"]
async fn live_probe_and_artifact_staging_against_a_real_codex_binary_when_present() {
    if locate(CODEX_PROGRAM_NAME, &tack_runner::ProcessEnv::from_process()).is_err() {
        eprintln!("skipping live codex test: `codex` not found on PATH");
        return;
    }

    let scratch = common::temp_dir("live-artifacts");
    let adapter = CodexAdapter::discover(
        CodexGrammar,
        ProcessLimits::new(1_048_576, 1_048_576, Duration::from_secs(30)),
        scratch.path().to_path_buf(),
        test_secret_store(scratch.path()),
        &tack_runner::ProcessEnv::from_process(),
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
