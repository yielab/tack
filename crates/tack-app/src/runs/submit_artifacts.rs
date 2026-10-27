//! The artifacts of an attempt: the manifest a runner declares, then the bytes it uploads for
//! each entry. The bytes themselves are streamed and verified by the caller; what is decided
//! here is whether a write is allowed at all and whether the manifest agrees with itself.

use chrono::{DateTime, Duration, Utc};
use serde_json::Value;
use tack_db::repo::execution::{ArtifactContentCommitResult, NewArtifact};
use tack_db::{Repository, queries};
use tack_protocol::execution::ExecutionState;
use uuid::Uuid;

use crate::AppError;

/// One entry of a manifest.
#[derive(Debug, Clone)]
pub struct ArtifactManifest {
    pub artifact_id: String,
    pub kind: String,
    pub name: String,
    pub media_type: Option<String>,
    pub size_bytes: i64,
    pub sha256: String,
    pub content_disposition: Option<String>,
    pub metadata: Value,
}

/// A manifest a runner declares for the attempt it holds.
pub struct ManifestSubmission<'a> {
    pub runner_id: &'a str,
    pub attempt_id: &'a str,
    pub fencing_token: i64,
    pub artifacts: &'a [ArtifactManifest],
    /// The most bytes an attempt's artifacts may declare in all.
    pub attempt_total_bytes_max: u64,
    /// How long after now the upload URL of an accepted entry is offered.
    pub upload_window: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestSubmitted {
    /// Every entry is recorded, now or by an earlier identical call.
    Accepted {
        /// When the offered upload URLs end, as RFC 3339 text.
        upload_expires_at: String,
    },
    /// The fence is not the attempt's current one, or its lease has ended.
    Stale,
    /// Artifacts are only taken while the attempt is running or waiting on a decision.
    Ineligible { state: ExecutionState },
    /// The attempt's artifacts would declare more bytes than it may.
    TooLarge,
    /// This `artifact_id` was already declared with different content. Entries before it in the
    /// manifest stay recorded.
    IdempotencyConflict { artifact_id: String },
}

fn accepts_artifacts(state: ExecutionState) -> bool {
    matches!(
        state,
        ExecutionState::Running | ExecutionState::WaitingDecision
    )
}

/// Records the manifest of an attempt. Artifacts land only while the attempt is running or
/// waiting on a decision (a lost or operator-held attempt no longer has a trusted runner), the
/// attempt's declared bytes stay under the cap, and an `artifact_id` declared twice must agree
/// with itself: the same content is a replay, other content is an idempotency conflict.
pub async fn submit_artifacts(
    repo: &Repository,
    now: DateTime<Utc>,
    submission: ManifestSubmission<'_>,
) -> Result<ManifestSubmitted, AppError> {
    let Some(lease) = queries::execution_attempts::attempt_lease(
        repo,
        submission.attempt_id,
        submission.runner_id,
        submission.fencing_token,
    )
    .await?
    else {
        return Ok(ManifestSubmitted::Stale);
    };
    if lease.lease_expires_at <= now.to_rfc3339() {
        return Ok(ManifestSubmitted::Stale);
    }
    if !accepts_artifacts(lease.state) {
        return Ok(ManifestSubmitted::Ineligible { state: lease.state });
    }

    let existing_total =
        queries::execution_artifacts::declared_bytes_of_attempt(repo, submission.attempt_id)
            .await?;
    let batch_total = submission.artifacts.iter().fold(0u64, |total, item| {
        total.saturating_add(item.size_bytes.max(0) as u64)
    });
    if (existing_total as u64).saturating_add(batch_total) > submission.attempt_total_bytes_max {
        return Ok(ManifestSubmitted::TooLarge);
    }

    let upload_expires_at = (now + submission.upload_window).to_rfc3339();
    for item in submission.artifacts {
        let row_id = format!("art_row_{}", Uuid::new_v4());
        let metadata_json = item.metadata.to_string();
        let written = repo
            .record_execution_artifact(
                submission.runner_id,
                submission.attempt_id,
                submission.fencing_token,
                NewArtifact {
                    id: &row_id,
                    artifact_id: &item.artifact_id,
                    kind: &item.kind,
                    name: &item.name,
                    media_type: item.media_type.as_deref(),
                    size_bytes: item.size_bytes,
                    sha256: &item.sha256,
                    content_disposition: item.content_disposition.as_deref(),
                    content_reference: None,
                    metadata: &metadata_json,
                },
                now,
            )
            .await?;
        if !written {
            return Ok(ManifestSubmitted::Stale);
        }

        // The store inserts with `ON CONFLICT ... DO NOTHING`: read the row back and compare it
        // before reporting success for a reused `artifact_id`.
        let stored = repo
            .get_execution_artifact(submission.attempt_id, &item.artifact_id)
            .await?
            .ok_or(AppError::NotFound)?;
        let stored_metadata: Value = serde_json::from_str(&stored.metadata).unwrap_or(Value::Null);
        let mismatch = stored.kind != item.kind
            || stored.name != item.name
            || stored.media_type != item.media_type
            || stored.size_bytes != item.size_bytes
            || stored.sha256 != item.sha256
            || stored.content_disposition != item.content_disposition
            || stored_metadata != item.metadata;
        if mismatch {
            return Ok(ManifestSubmitted::IdempotencyConflict {
                artifact_id: item.artifact_id.clone(),
            });
        }
    }
    Ok(ManifestSubmitted::Accepted { upload_expires_at })
}

/// An upload of the bytes of one declared artifact.
#[derive(Debug, Clone, Copy)]
pub struct ContentUpload<'a> {
    pub runner_id: &'a str,
    pub attempt_id: &'a str,
    pub artifact_id: &'a str,
    pub fencing_token: i64,
    /// The `Content-Type` the upload carries, when it carries one.
    pub content_type: Option<&'a str>,
}

/// What an accepted upload must match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadTarget {
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadChecked {
    /// The bytes may be streamed and must match this size and checksum.
    Ready(UploadTarget),
    /// No such artifact for this runner and fence, or the lease has ended. The three causes are
    /// deliberately indistinguishable.
    Stale,
    /// Artifacts are only taken while the attempt is running or waiting on a decision.
    Ineligible { state: ExecutionState },
    /// Content is immutable once verified.
    AlreadyRecorded,
    /// The upload's `Content-Type` is not the manifest's `media_type`.
    ContentTypeMismatch,
}

/// Case-insensitive, parameter-stripped comparison: `text/x-diff` matches `text/x-diff` and
/// `text/x-diff; charset=utf-8` alike, since HTTP clients routinely append a `charset`.
fn content_type_matches(declared: &str, provided: &str) -> bool {
    let essence = |value: &str| {
        value
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase()
    };
    essence(declared) == essence(provided)
}

/// Decides, before any byte is read, whether an artifact's content may be uploaded: the runner
/// holds the attempt under this fence with a live lease, the attempt is running or waiting on a
/// decision, nothing was recorded yet and the `Content-Type` agrees with the manifest.
pub async fn check_artifact_upload(
    repo: &Repository,
    now: DateTime<Utc>,
    upload: ContentUpload<'_>,
) -> Result<UploadChecked, AppError> {
    let Some(facts) = queries::execution_artifacts::upload_facts(
        repo,
        upload.attempt_id,
        upload.artifact_id,
        upload.runner_id,
        upload.fencing_token,
    )
    .await?
    else {
        return Ok(UploadChecked::Stale);
    };
    if facts.lease_expires_at <= now.to_rfc3339() {
        return Ok(UploadChecked::Stale);
    }
    if !accepts_artifacts(facts.attempt_state) {
        return Ok(UploadChecked::Ineligible {
            state: facts.attempt_state,
        });
    }
    if facts.content_reference.is_some() {
        return Ok(UploadChecked::AlreadyRecorded);
    }
    if let Some(declared) = facts.media_type.as_deref()
        && let Some(provided) = upload.content_type
        && !content_type_matches(declared, provided)
    {
        return Ok(UploadChecked::ContentTypeMismatch);
    }
    Ok(UploadChecked::Ready(UploadTarget {
        size_bytes: facts.size_bytes as u64,
        sha256: facts.sha256,
    }))
}

/// Records where the verified bytes of an artifact are stored. The first upload to commit wins:
/// a second one is `AlreadySet`, and a fence that went stale meanwhile is `Stale`; the caller
/// removes its own copy in both cases.
pub async fn commit_artifact_content(
    repo: &Repository,
    now: DateTime<Utc>,
    upload: ContentUpload<'_>,
    content_reference: &str,
) -> Result<ArtifactContentCommitResult, AppError> {
    Ok(repo
        .set_execution_artifact_content_reference(
            upload.runner_id,
            upload.attempt_id,
            upload.artifact_id,
            upload.fencing_token,
            content_reference,
            now,
        )
        .await?)
}
