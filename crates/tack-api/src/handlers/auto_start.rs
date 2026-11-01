//! Why a task that a project would start by itself is still waiting. The rules are
//! `tack_core::auto_start`'s and the rows `tack_app::auto_start`'s; this file answers.

use axum::extract::State;
use serde::Serialize;
use tracing::instrument;
use utoipa::ToSchema;
use uuid::Uuid;

use tack_protocol::SystemClock;

use crate::error::{ApiError, ApiResult};
use crate::extract::{Json, Path};
use crate::router::AppState;

/// Why one task is not started.
#[derive(Debug, Serialize, ToSchema)]
pub struct WaitingReason {
    /// `open_request`, `blocked`, `no_profile`, `no_machine`, `wip`, `budget` or `max_open`.
    pub code: String,
    /// The reason in a sentence, with the way out where there is one.
    pub text: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WaitingTask {
    pub item_id: Uuid,
    pub why: WaitingReason,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AutoStartState {
    /// Whether the project starts tasks by itself.
    pub enabled: bool,
    /// The status a task waits in to be started; `null` when none is chosen.
    pub status: Option<String>,
    /// The tasks in that status the next pass would not start, most urgent first. Empty when
    /// auto-start is off.
    pub waiting: Vec<WaitingTask>,
}

/// GET /api/projects/{id}/auto-start
#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/projects/{id}/auto-start",
    tag = "projects",
    params(("id" = Uuid, Path, description = "Project ID")),
    responses(
        (status = 200, description = "Whether the project starts tasks by itself and why each waiting task is not started yet", body = AutoStartState),
        (status = 404, description = "No such project", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn get_auto_start(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<AutoStartState>> {
    let project = state
        .repo
        .get_project(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {id} not found")))?;
    let enabled = project.auto_start_enabled && project.auto_start_status.is_some();
    let waiting = if enabled {
        tack_app::auto_start::explain(&state.repo, &SystemClock, &project)
            .await?
            .into_iter()
            .map(|(item_id, why)| WaitingTask {
                item_id,
                why: WaitingReason {
                    code: why.code.to_owned(),
                    text: why.text,
                },
            })
            .collect()
    } else {
        Vec::new()
    };
    Ok(Json(AutoStartState {
        enabled,
        status: project.auto_start_status,
        waiting,
    }))
}
