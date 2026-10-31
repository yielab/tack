//! The rules behind the artifact routes: a manifest lands while the attempt is running or waiting
//! on a decision, stays under the attempt's byte cap and agrees with itself per `artifact_id`;
//! an upload is checked against its manifest before a byte is read and its content is recorded
//! once.

use crate::common;
use tack_protocol::Clock;
use tack_protocol::execution::{AttemptId, FencingToken, RunnerId};

use chrono::Duration;
use common::{RUNNER_ID, running_attempt, world};
use serde_json::json;
use tack_app::runs::submit_artifacts::{
    ArtifactManifest, ContentUpload, ManifestSubmission, ManifestSubmitted, UploadChecked,
    check_artifact_upload, commit_artifact_content, submit_artifacts,
};
use tack_db::queries::execution_artifacts;
use tack_db::repo::execution::ArtifactContentCommitResult;
use tack_protocol::execution::ExecutionState;
use tack_test_support::{ids, seed};

const CAP: u64 = 100;

fn manifest(artifact_id: &str, sha: char, size_bytes: i64) -> ArtifactManifest {
    ArtifactManifest {
        artifact_id: artifact_id.into(),
        kind: "patch".into(),
        name: "changes.patch".into(),
        media_type: Some("text/x-diff".into()),
        size_bytes,
        sha256: sha.to_string().repeat(64),
        content_disposition: Some("inline_upload".into()),
        metadata: json!({}),
    }
}

async fn submit(
    world: &common::World,
    attempt_id: &str,
    fence: FencingToken,
    artifacts: &[ArtifactManifest],
) -> ManifestSubmitted {
    submit_artifacts(
        &world.repo,
        world.clock.now(),
        ManifestSubmission {
            runner_id: &RunnerId::new(RUNNER_ID),
            attempt_id: &AttemptId::new(attempt_id),
            fencing_token: fence,
            artifacts,
            attempt_total_bytes_max: CAP,
            upload_window: Duration::seconds(600),
        },
    )
    .await
    .expect("submit")
}

#[tokio::test]
async fn a_manifest_is_accepted_and_the_same_artifact_id_must_agree_with_itself() {
    let world = world().await;
    let run = running_attempt(&world, "art-reuse").await;

    let accepted = submit(
        &world,
        &run.attempt_id,
        run.fence,
        &[manifest("art-1", 'a', 10)],
    )
    .await;
    assert!(matches!(accepted, ManifestSubmitted::Accepted { .. }));
    let replay = submit(
        &world,
        &run.attempt_id,
        run.fence,
        &[manifest("art-1", 'a', 10)],
    )
    .await;
    assert!(matches!(replay, ManifestSubmitted::Accepted { .. }));

    let conflict = submit(
        &world,
        &run.attempt_id,
        run.fence,
        &[manifest("art-1", 'b', 10)],
    )
    .await;
    assert_eq!(
        conflict,
        ManifestSubmitted::IdempotencyConflict {
            artifact_id: "art-1".into()
        }
    );
    let stored = world
        .repo
        .get_execution_artifact(&run.attempt_id, "art-1")
        .await
        .unwrap()
        .expect("the artifact exists");
    assert_eq!(stored.sha256, "a".repeat(64), "the original was kept");
}

#[tokio::test]
async fn the_attempts_declared_bytes_stay_under_the_cap_across_batches() {
    let world = world().await;
    let run = running_attempt(&world, "art-cap").await;

    let first = submit(
        &world,
        &run.attempt_id,
        run.fence,
        &[manifest("art-1", 'a', 60)],
    )
    .await;
    assert!(matches!(first, ManifestSubmitted::Accepted { .. }));
    let over = submit(
        &world,
        &run.attempt_id,
        run.fence,
        &[manifest("art-2", 'b', 41)],
    )
    .await;
    assert_eq!(over, ManifestSubmitted::TooLarge);
    assert_eq!(
        execution_artifacts::artifact_ids_of_attempt(&world.repo, &run.attempt_id)
            .await
            .unwrap(),
        ["art-1"],
        "the rejected artifact did not land"
    );
    assert_eq!(
        execution_artifacts::declared_bytes_of_attempt(&world.repo, &run.attempt_id)
            .await
            .unwrap(),
        60
    );
}

#[tokio::test]
async fn artifacts_land_only_while_the_attempt_runs_or_waits_and_only_under_a_live_fence() {
    let world = world().await;
    let run = running_attempt(&world, "art-states").await;

    for state in [
        ExecutionState::Lost,
        ExecutionState::NeedsOperator,
        ExecutionState::Succeeded,
    ] {
        seed::execution_attempt_state(&world.repo, &run.attempt_id, state).await;
        assert_eq!(
            submit(
                &world,
                &run.attempt_id,
                run.fence,
                &[manifest("art-1", 'a', 1)]
            )
            .await,
            ManifestSubmitted::Ineligible { state }
        );
    }
    seed::execution_attempt_state(
        &world.repo,
        &run.attempt_id,
        ExecutionState::WaitingDecision,
    )
    .await;
    assert!(matches!(
        submit(
            &world,
            &run.attempt_id,
            run.fence,
            &[manifest("art-1", 'a', 1)]
        )
        .await,
        ManifestSubmitted::Accepted { .. }
    ));

    assert_eq!(
        submit(
            &world,
            &run.attempt_id,
            FencingToken::new(run.fence.get() + 1),
            &[manifest("art-2", 'a', 1)]
        )
        .await,
        ManifestSubmitted::Stale
    );
    world.clock.0.advance(Duration::seconds(61));
    assert_eq!(
        submit(
            &world,
            &run.attempt_id,
            run.fence,
            &[manifest("art-2", 'a', 1)]
        )
        .await,
        ManifestSubmitted::Stale
    );
}

fn upload<'a>(
    attempt_id: &'a str,
    artifact_id: &'a str,
    fence: FencingToken,
    content_type: Option<&'a str>,
) -> ContentUpload<'a> {
    ContentUpload {
        runner_id: ids::runner(RUNNER_ID),
        attempt_id: ids::attempt(attempt_id),
        artifact_id,
        fencing_token: fence,
        content_type,
    }
}

#[tokio::test]
async fn an_upload_is_checked_against_its_manifest_and_its_content_is_recorded_once() {
    let world = world().await;
    let run = running_attempt(&world, "art-upload").await;
    submit(
        &world,
        &run.attempt_id,
        run.fence,
        &[manifest("art-1", 'a', 10)],
    )
    .await;
    let check =
        |artifact_id: &'static str, fence: FencingToken, content_type: Option<&'static str>| {
            let (world, attempt_id) = (&world, run.attempt_id.clone());
            async move {
                check_artifact_upload(
                    &world.repo,
                    world.clock.now(),
                    upload(&attempt_id, artifact_id, fence, content_type),
                )
                .await
                .expect("check")
            }
        };

    let UploadChecked::Ready(target) =
        check("art-1", run.fence, Some("text/x-diff; charset=utf-8")).await
    else {
        panic!("a matching upload is ready");
    };
    assert_eq!(target.size_bytes, 10);
    assert_eq!(target.sha256, "a".repeat(64));
    assert!(matches!(
        check("art-1", run.fence, None).await,
        UploadChecked::Ready(_)
    ));
    assert_eq!(
        check("art-1", run.fence, Some("application/json")).await,
        UploadChecked::ContentTypeMismatch
    );
    // An unknown artifact and a wrong fence look the same.
    assert_eq!(
        check("art-unknown", run.fence, None).await,
        UploadChecked::Stale
    );
    assert_eq!(
        check("art-1", FencingToken::new(run.fence.get() + 1), None).await,
        UploadChecked::Stale
    );

    let committed = commit_artifact_content(
        &world.repo,
        world.clock.now(),
        upload(&run.attempt_id, "art-1", run.fence, None),
        "blob-1",
    )
    .await
    .unwrap();
    assert_eq!(committed, ArtifactContentCommitResult::Committed);
    let again = commit_artifact_content(
        &world.repo,
        world.clock.now(),
        upload(&run.attempt_id, "art-1", run.fence, None),
        "blob-2",
    )
    .await
    .unwrap();
    assert_eq!(again, ArtifactContentCommitResult::AlreadySet);
    assert_eq!(
        check("art-1", run.fence, None).await,
        UploadChecked::AlreadyRecorded
    );
    assert_eq!(
        execution_artifacts::content_reference(&world.repo, &run.attempt_id, "art-1")
            .await
            .unwrap()
            .as_deref(),
        Some("blob-1")
    );

    seed::execution_attempt_state(&world.repo, &run.attempt_id, ExecutionState::Lost).await;
    assert_eq!(
        check("art-1", run.fence, None).await,
        UploadChecked::Ineligible {
            state: ExecutionState::Lost
        }
    );
}
