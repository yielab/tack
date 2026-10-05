//! The operator's view of an attempt's Merge-Readiness Pack and the human
//! verdict on it. The pack is the newest artifact of the attempt whose media
//! type is `application/vnd.tack.mrp+json`; `tack_core::mrp` is its wire shape.
//! The first review is the record, a second one is a 409. Accepting under the
//! `done_on_mrp_accepted` status-map policy is the only thing here that moves
//! an item.

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
use tracing::instrument;
use utoipa::ToSchema;

use tack_core::mrp::MergeReadinessPack;
use tack_core::workflow::{StatusMapEvent, StatusMapPolicy};
use tack_db::repo::execution::{ExecutionArtifactRow, SystemExecutionClock};

use crate::error::{ApiError, ApiResult};
use crate::handlers::runner_protocol::artifact_storage::ArtifactStorage;
use crate::handlers::websocket::{self, BoardEvent};
use crate::router::AppState;

const MRP_MEDIA_TYPE: &str = "application/vnd.tack.mrp+json";
/// `reviewed_by` until the operator surface carries a per-caller identity.
const REVIEWER: &str = "operator";

#[derive(Debug, Serialize, ToSchema)]
pub struct MrpResponse {
    /// The parsed Merge-Readiness Pack (`docs/contracts/mrp-v1/`).
    #[schema(value_type = Object)]
    pub pack: MergeReadinessPack,
    /// The review record; absent until the pack is viewed or reviewed.
    #[schema(value_type = Option<Object>)]
    pub review: Option<tack_db::repo::execution::MrpReviewRow>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MrpVerdict {
    Accept,
    Reject,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MrpReviewRequest {
    pub verdict: MrpVerdict,
    /// Required; a blank reason is a 400.
    pub reason: String,
}

/// The newest MRP artifact of the attempt, or 404.
async fn newest_mrp_artifact(
    state: &AppState,
    request_id: &str,
    attempt_number: i64,
) -> ApiResult<ExecutionArtifactRow> {
    let artifacts = state
        .repo
        .list_execution_artifacts_for_attempt_number(request_id, attempt_number)
        .await?;
    artifacts
        .unwrap_or_default()
        .into_iter()
        .rfind(|a| a.media_type.as_deref() == Some(MRP_MEDIA_TYPE))
        .ok_or_else(|| ApiError::NotFound("No merge-readiness pack for this attempt".into()))
}

async fn read_pack(
    state: &AppState,
    artifact: &ExecutionArtifactRow,
) -> ApiResult<MergeReadinessPack> {
    let reference = artifact
        .content_reference
        .as_deref()
        .ok_or_else(|| ApiError::NotFound("The pack's content has not been uploaded".into()))?;
    let storage = ArtifactStorage::new(format!("{}/execution-artifacts", state.config.storage_dir));
    let mut file = storage
        .open_for_read(reference)
        .await
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("could not open the pack: {e:?}")))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;
    serde_json::from_slice(&bytes)
        .map_err(|e| ApiError::Unprocessable(format!("the stored pack does not parse: {e}")))
}

/// GET /api/executions/:request_id/attempts/:attempt_number/mrp
#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/executions/{request_id}/attempts/{attempt_number}/mrp",
    tag = "mrp",
    params(
        ("request_id" = String, Path, description = "Execution request ID"),
        ("attempt_number" = i64, Path, description = "1-based attempt number"),
    ),
    responses(
        (status = 200, description = "The parsed pack and its review record", body = MrpResponse),
        (status = 404, description = "No pack for this attempt", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn get_mrp(
    State(state): State<AppState>,
    Path((request_id, attempt_number)): Path<(String, i64)>,
) -> ApiResult<Json<MrpResponse>> {
    let artifact = newest_mrp_artifact(&state, &request_id, attempt_number).await?;
    let pack = read_pack(&state, &artifact).await?;
    let review = state.repo.get_mrp_review(&artifact.attempt_id).await?;
    Ok(Json(MrpResponse { pack, review }))
}

/// POST /api/executions/:request_id/attempts/:attempt_number/mrp/viewed
#[instrument(skip(state))]
#[utoipa::path(
    post,
    path = "/api/executions/{request_id}/attempts/{attempt_number}/mrp/viewed",
    tag = "mrp",
    params(
        ("request_id" = String, Path, description = "Execution request ID"),
        ("attempt_number" = i64, Path, description = "1-based attempt number"),
    ),
    responses(
        (status = 200, description = "The review record, with viewed_at stamped once", body = Object),
        (status = 404, description = "No pack for this attempt", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn mark_viewed(
    State(state): State<AppState>,
    Path((request_id, attempt_number)): Path<(String, i64)>,
) -> ApiResult<Json<tack_db::repo::execution::MrpReviewRow>> {
    let artifact = newest_mrp_artifact(&state, &request_id, attempt_number).await?;
    let review = state
        .repo
        .mark_mrp_viewed(
            &artifact.attempt_id,
            &artifact.artifact_id,
            &SystemExecutionClock,
        )
        .await?;
    Ok(Json(review))
}

/// POST /api/executions/:request_id/attempts/:attempt_number/mrp/review
#[instrument(skip(state, input))]
#[utoipa::path(
    post,
    path = "/api/executions/{request_id}/attempts/{attempt_number}/mrp/review",
    tag = "mrp",
    params(
        ("request_id" = String, Path, description = "Execution request ID"),
        ("attempt_number" = i64, Path, description = "1-based attempt number"),
    ),
    request_body = MrpReviewRequest,
    responses(
        (status = 200, description = "The recorded review", body = Object),
        (status = 400, description = "Blank reason", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "No pack for this attempt", body = crate::openapi::ErrorEnvelope),
        (status = 409, description = "The pack was already reviewed", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn review_mrp(
    State(state): State<AppState>,
    Path((request_id, attempt_number)): Path<(String, i64)>,
    Json(input): Json<MrpReviewRequest>,
) -> ApiResult<Json<tack_db::repo::execution::MrpReviewRow>> {
    let reason = input.reason.trim();
    if reason.is_empty() {
        return Err(ApiError::BadRequest("reason is required".into()));
    }
    let artifact = newest_mrp_artifact(&state, &request_id, attempt_number).await?;
    let verdict = match input.verdict {
        MrpVerdict::Accept => "accept",
        MrpVerdict::Reject => "reject",
    };
    let review = state
        .repo
        .record_mrp_review(
            &artifact.attempt_id,
            &artifact.artifact_id,
            verdict,
            reason,
            REVIEWER,
            &SystemExecutionClock,
        )
        .await?
        .ok_or_else(|| ApiError::Conflict("This pack was already reviewed".into()))?;
    if input.verdict == MrpVerdict::Accept {
        apply_mrp_accepted(&state, &request_id).await?;
    }
    Ok(Json(review))
}

/// Moves the request's item to the first Done status when the request's
/// `status_map_policy_id` is `done_on_mrp_accepted`; any other policy leaves it.
async fn apply_mrp_accepted(state: &AppState, request_id: &str) -> ApiResult<()> {
    let row: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT item_id, json_extract(request_snapshot, '$.status_map_policy_id') \
         FROM execution_requests WHERE id = ?",
    )
    .bind(request_id)
    .fetch_optional(state.repo.pool())
    .await?;
    let Some((item_id, Some(policy_id))) = row else {
        return Ok(());
    };
    let Ok(policy) = policy_id.parse::<StatusMapPolicy>() else {
        return Ok(());
    };
    let Ok(item_id) = item_id.parse::<uuid::Uuid>() else {
        return Ok(());
    };
    let Some(item) = state.repo.get_item(item_id).await? else {
        return Ok(());
    };
    let Some(project) = state.repo.get_project(item.project_id).await? else {
        return Ok(());
    };
    let Some(target) = policy.target_status(&project.workflow, StatusMapEvent::MrpAccepted) else {
        return Ok(());
    };
    if item.status == target {
        return Ok(());
    }
    let update = tack_core::models::UpdateItem {
        status: Some(target.clone()),
        ..Default::default()
    };
    match state
        .repo
        .update_item_atomically(item_id, update, &project.workflow, None)
        .await?
    {
        tack_db::repo::items::AtomicItemUpdateOutcome::Updated {
            item, old_status, ..
        } => {
            websocket::broadcast_event(
                state,
                BoardEvent::ItemUpdated {
                    project_id: item.project_id,
                    item_id: item.id,
                    old_status: Some(old_status),
                    new_status: item.status.clone(),
                },
            );
        }
        outcome => {
            tracing::warn!(request_id, %item_id, target, ?outcome, "mrp accepted: transition refused, item untouched");
        }
    }
    Ok(())
}
