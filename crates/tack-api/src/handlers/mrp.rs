//! The operator's view of an attempt's Merge-Readiness Pack. The pack is the newest artifact of
//! the attempt whose media type is `application/vnd.tack.mrp+json`; `tack_protocol::mrp` is its
//! wire shape. The human verdict on it is the attempt's one review (`attempt_lists::review_execution_attempt`).
//! Nothing here moves an item.

use crate::extract::{Json, Path};
use axum::extract::State;
use serde::Serialize;
use tack_protocol::{Clock, SystemClock};
use tokio::io::AsyncReadExt;
use tracing::instrument;
use utoipa::ToSchema;

use tack_db::queries::{attempt_reviews, execution_artifacts};
use tack_db::repo::execution::ExecutionArtifactRow;
use tack_protocol::mrp::MergeReadinessPack;

use crate::error::{ApiError, ApiResult};
use crate::handlers::executions::AttemptReview;
use crate::router::AppState;

const MRP_MEDIA_TYPE: &str = "application/vnd.tack.mrp+json";

#[derive(Debug, Serialize, ToSchema)]
pub struct MrpResponse {
    /// The parsed Merge-Readiness Pack (`docs/contracts/mrp-v1/`).
    #[schema(value_type = Object)]
    pub pack: MergeReadinessPack,
    /// When the operator first opened the pack; `null` until then.
    pub viewed_at: Option<String>,
    /// The attempt's review; `null` until a person has given a verdict.
    pub review: Option<AttemptReview>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MrpViewed {
    pub viewed_at: Option<String>,
}

/// The newest MRP artifact of the attempt, or 404.
async fn newest_mrp_artifact(
    state: &AppState,
    request_id: &str,
    attempt_number: i64,
) -> ApiResult<ExecutionArtifactRow> {
    let artifacts =
        execution_artifacts::artifacts_of_attempt(&state.repo, request_id, attempt_number).await?;
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
    let storage = state.artifact_storage();
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
        (status = 200, description = "The parsed pack, when it was opened and the attempt's review", body = MrpResponse),
        (status = 404, description = "No pack for this attempt", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn get_mrp(
    State(state): State<AppState>,
    Path((request_id, attempt_number)): Path<(String, i64)>,
) -> ApiResult<Json<MrpResponse>> {
    let artifact = newest_mrp_artifact(&state, &request_id, attempt_number).await?;
    let pack = read_pack(&state, &artifact).await?;
    let viewed_at = state
        .repo
        .pack_viewed_at(&artifact.attempt_id, &artifact.artifact_id)
        .await?;
    let review = attempt_reviews::review_of_attempt(&state.repo, &artifact.attempt_id)
        .await?
        .map(AttemptReview::from);
    Ok(Json(MrpResponse {
        pack,
        viewed_at,
        review,
    }))
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
        (status = 200, description = "When the pack was first opened, stamped once", body = MrpViewed),
        (status = 404, description = "No pack for this attempt", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn mark_viewed(
    State(state): State<AppState>,
    Path((request_id, attempt_number)): Path<(String, i64)>,
) -> ApiResult<Json<MrpViewed>> {
    let artifact = newest_mrp_artifact(&state, &request_id, attempt_number).await?;
    let viewed_at = state
        .repo
        .mark_pack_viewed(
            &artifact.attempt_id,
            &artifact.artifact_id,
            SystemClock.now(),
        )
        .await?;
    Ok(Json(MrpViewed { viewed_at }))
}
