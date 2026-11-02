//! Triggers (ADR 0034, decision 8): what wakes a profile. Only the operator reads and changes
//! them (no agent credential reaches these routes). The rules live in `tack_core::trigger`
//! and `tack_app::triggers`; this file extracts, calls and answers.

use axum::extract::State;
use serde::Deserialize;
use tracing::instrument;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use tack_app::triggers::{self, EditTrigger, OpenTrigger};
use tack_core::thread::ThreadScope;
use tack_core::trigger::{Source, Trigger};
use tack_protocol::SystemClock;

use crate::error::{ApiError, ApiResult};
use crate::extract::{Json, Path, Query};
use crate::router::AppState;

fn default_quiet() -> u32 {
    60
}

fn default_cap() -> u32 {
    5
}

fn default_enabled() -> bool {
    true
}

/// What makes a trigger.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateTrigger {
    /// The project the runs happen in; with no credential, the only project whose events count.
    pub project_id: Uuid,
    /// When given, the events of the projects this credential may read count, and the
    /// credential's own changes never wake the profile.
    #[serde(default)]
    pub credential_id: Option<Uuid>,
    /// The agent profile that runs.
    pub profile: String,
    pub name: String,
    pub source: Source,
    /// How long events stay quiet before the run starts, in seconds (0 to 3600).
    #[serde(default = "default_quiet")]
    pub quiet_seconds: u32,
    /// The most runs in a day (1 to 100).
    #[serde(default = "default_cap")]
    pub daily_cap: u32,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

/// What an edit changes; a field left out stays.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateTrigger {
    pub name: Option<String>,
    pub profile: Option<String>,
    pub source: Option<Source>,
    pub quiet_seconds: Option<u32>,
    pub daily_cap: Option<u32>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct TriggerFilter {
    /// Only the triggers whose home is this project.
    project_id: Option<Uuid>,
}

/// POST /api/triggers
#[utoipa::path(
    post,
    path = "/api/triggers",
    tag = "triggers",
    request_body = CreateTrigger,
    responses(
        (status = 200, description = "The trigger", body = Trigger),
        (status = 400, description = "The trigger, its profile or its credential is not acceptable", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "Project not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
#[instrument(skip(state, input))]
pub async fn create_trigger(
    State(state): State<AppState>,
    Json(input): Json<CreateTrigger>,
) -> ApiResult<Json<Trigger>> {
    let scope = match input.credential_id {
        Some(credential) => ThreadScope::Credential {
            credential,
            home: input.project_id,
        },
        None => ThreadScope::Project {
            project: input.project_id,
        },
    };
    Ok(Json(
        triggers::create(
            &state.repo,
            &SystemClock,
            OpenTrigger {
                scope,
                profile: input.profile,
                name: input.name,
                source: input.source,
                quiet_seconds: input.quiet_seconds,
                daily_cap: input.daily_cap,
                enabled: input.enabled,
            },
        )
        .await?,
    ))
}

/// GET /api/triggers
#[utoipa::path(
    get,
    path = "/api/triggers",
    tag = "triggers",
    params(TriggerFilter),
    responses((status = 200, description = "Triggers, oldest first", body = Vec<Trigger>)),
)]
#[instrument(skip(state))]
pub async fn list_triggers(
    State(state): State<AppState>,
    Query(filter): Query<TriggerFilter>,
) -> ApiResult<Json<Vec<Trigger>>> {
    Ok(Json(state.repo.list_triggers(filter.project_id).await?))
}

/// GET /api/triggers/{id}
#[utoipa::path(
    get,
    path = "/api/triggers/{id}",
    tag = "triggers",
    params(("id" = Uuid, Path, description = "Trigger ID")),
    responses(
        (status = 200, description = "The trigger", body = Trigger),
        (status = 404, description = "Not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
#[instrument(skip(state))]
pub async fn get_trigger(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Trigger>> {
    state
        .repo
        .get_trigger(id)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::NotFound(format!("Trigger {id} not found")))
}

/// PATCH /api/triggers/{id}
#[utoipa::path(
    patch,
    path = "/api/triggers/{id}",
    tag = "triggers",
    params(("id" = Uuid, Path, description = "Trigger ID")),
    request_body = UpdateTrigger,
    responses(
        (status = 200, description = "The trigger, changed", body = Trigger),
        (status = 400, description = "The change is not acceptable", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "Not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
#[instrument(skip(state, input))]
pub async fn update_trigger(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateTrigger>,
) -> ApiResult<Json<Trigger>> {
    Ok(Json(
        triggers::edit(
            &state.repo,
            id,
            EditTrigger {
                name: input.name,
                profile: input.profile,
                source: input.source,
                quiet_seconds: input.quiet_seconds,
                daily_cap: input.daily_cap,
                enabled: input.enabled,
            },
        )
        .await?,
    ))
}

/// DELETE /api/triggers/{id}
#[utoipa::path(
    delete,
    path = "/api/triggers/{id}",
    tag = "triggers",
    params(("id" = Uuid, Path, description = "Trigger ID")),
    responses(
        (status = 204, description = "Deleted, with the events it kept"),
        (status = 404, description = "Not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
#[instrument(skip(state))]
pub async fn delete_trigger(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<axum::http::StatusCode> {
    triggers::delete(&state.repo, id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
