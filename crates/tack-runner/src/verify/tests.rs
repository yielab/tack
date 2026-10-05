use super::*;
use crate::client::{AttemptId, WorkspaceId};
use crate::harness::fixtures::fake_harness_path;

/// The fake verifier, with its knob passed through `env` because the
/// verifier's own environment is `PATH` only.
fn fake(knob: &str) -> VerifyConfig {
    let script = fake_harness_path().with_file_name("fake_verifier.sh");
    VerifyConfig {
        enabled: true,
        program: "env".to_owned(),
        args: vec![
            knob.to_owned(),
            "sh".to_owned(),
            script.display().to_string(),
        ],
        timeout_seconds: 60,
    }
}

async fn run_with(config: &VerifyConfig) -> Result<serde_json::Value, VerifyFailure> {
    let root = tempfile::tempdir().expect("temp dir");
    let path = root.path().join("ws");
    let scratch = root.path().join("scratch");
    fs::create_dir_all(&path).expect("workspace");
    fs::create_dir_all(scratch.join("src")).expect("scratch");
    fs::write(scratch.join("src/evidence.json"), "{}").expect("evidence");
    let workspace = Workspace {
        attempt_id: AttemptId::new("attempt"),
        id: WorkspaceId::new("ws_attempt"),
        path,
        base_revision: "base".to_owned(),
    };
    let limits = ProcessLimits::new(1024 * 1024, 1024 * 1024, Duration::from_secs(60));
    run(config, &limits, &workspace, &scratch).await
}

#[tokio::test]
async fn a_fixture_pack_is_staged_as_an_artifact() {
    let staged = run_with(&fake("TACK_FAKE_VERIFIER_FIXTURE=ready"))
        .await
        .expect("a pack");
    assert_eq!(staged["kind"], "mrp");
    assert_eq!(staged["name"], "mrp.json");
    assert_eq!(staged["media_type"], MRP_MEDIA_TYPE);
    let fixture = fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/contracts/mrp-v1/fixtures/ready.json"),
    )
    .expect("fixture");
    assert_eq!(staged["size_bytes"], fixture.len());
    assert_eq!(
        staged["sha256"],
        crate::harness::sha256::sha256_hex(&fixture)
    );
}

#[tokio::test]
async fn every_way_to_produce_no_pack_is_a_failure() {
    for (config, exit_code, reason) in [
        (fake("TACK_FAKE_VERIFIER_EXIT_CODE=1"), Some(1), "code 1"),
        (
            fake("TACK_FAKE_VERIFIER_FIXTURE=../schema"),
            Some(0),
            "does not parse",
        ),
        (
            VerifyConfig {
                program: "tack-no-such-verifier".to_owned(),
                ..VerifyConfig::default()
            },
            None,
            "could not be started",
        ),
    ] {
        let error = run_with(&config).await.expect_err("a failure");
        assert_eq!(error.exit_code, exit_code, "{}", error.reason);
        assert!(error.reason.contains(reason), "{}", error.reason);
    }
}
