//! Control-plane link API (Phase 33 / card A4, task 33.5): register docket
//! control planes, link a Tack project to one, and read the Fleet view's
//! aggregate.
//!
//! **Off by default.** Every route in this module is gated behind
//! `TACK_ORCH_ENABLE` via [`require_orch_enabled`], applied once as a layer on
//! the orch sub-router in `router.rs` rather than repeated per-handler — with
//! the flag unset, every route here 404s (TODO.md §0 rule 8 / §4 cross-cutting
//! acceptance).
//!
//! **Token discipline** mirrors the S3 backup secret precedent
//! (`handlers/settings.rs`'s `secret_key_set`): the docket Bearer token is
//! write-only over this API. [`ControlPlaneResponse`] never carries it — only
//! `token_set: bool`. A `PATCH` with the `token` field **absent** leaves the
//! stored token untouched; an explicit `"token": null` clears it; a string sets
//! or replaces it. See [`UpdateControlPlaneRequest`] and [`deserialize_some`].
//!
//! **A control-plane failure never fails a user request here.** Every handler
//! reads Tack's own database, populated out-of-band by the reconciler
//! (`tack-orch`) — a docket outage can only leave `health`/`last_seen_at` stale,
//! never turn into a 500 on a user's request.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use tracing::instrument;
use uuid::Uuid;

use tack_core::workflow::WorkflowConfig;
use tack_db::repo::orch::{
    ControlPlane, CreateControlPlane, OrchApproval, OrchEvent, OrchLink, OrchMetricLatest, OrchRun,
    OrchTask, PendingOrchApproval, UpdateControlPlane, UpsertOrchLink,
};
use tack_orch::adapters::docket::DocketAdapter;
use tack_orch::{ControlPlane as OrchControlPlane, OrchError};

use crate::dispatcher::{self, DispatchOutcome};
use crate::error::{ApiError, ApiResult};
use crate::openapi::ErrorEnvelope;
use crate::router::AppState;
use crate::sprint_dispatch::{self, ItemResult, PreviewDecision};

// ════════════════════════════════════════════════════════════════════════════
// Gate — TACK_ORCH_ENABLE
// ════════════════════════════════════════════════════════════════════════════

/// Gate every route in this module behind `TACK_ORCH_ENABLE`. Applied once as a
/// layer on the orch sub-router (`router.rs`) so no individual handler needs to
/// repeat the check. With the flag unset the route 404s — indistinguishable from
/// the route not existing, which is the point (TODO.md §0 rule 8).
pub async fn require_orch_enabled(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if !state.config.orch_enable {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(next.run(req).await)
}

/// Standard trick for a JSON tri-state field: with `#[serde(default)]` on the
/// field, a missing key never calls this function and the field stays `None`
/// ("absent" / "leave untouched"). When the key **is** present — including
/// `null` — this runs and always wraps the result in `Some(..)`, so `null`
/// becomes `Some(None)` ("clear it") and a value becomes `Some(Some(v))` ("set
/// it"). Plain `#[derive(Deserialize)]` on `Option<Option<T>>` cannot make this
/// distinction on its own; this is the whole trick.
fn deserialize_some<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    T::deserialize(deserializer).map(Some)
}

// ════════════════════════════════════════════════════════════════════════════
// control_planes
// ════════════════════════════════════════════════════════════════════════════

/// Client-safe view of a `control_planes` row. Deliberately has **no** `token`
/// field — see the module doc.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ControlPlaneResponse {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub api_version: Option<String>,
    /// `"unknown"` | `"healthy"` | `"degraded"` | `"unreachable"` — driven by
    /// the reconciler's health state machine (`tack-orch::reconciler`),
    /// persisted verbatim.
    pub health: String,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub consecutive_failures: i64,
    /// True when a docket Bearer token is currently stored for this plane.
    /// The token itself is write-only over this API.
    pub token_set: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<ControlPlane> for ControlPlaneResponse {
    fn from(c: ControlPlane) -> Self {
        Self {
            id: c.id,
            name: c.name,
            kind: c.kind,
            base_url: c.base_url,
            api_version: c.api_version,
            health: c.health,
            last_seen_at: c.last_seen_at,
            consecutive_failures: c.consecutive_failures,
            token_set: c.token_set,
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

/// `POST /api/control-planes` body.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateControlPlaneRequest {
    pub name: String,
    /// Defaults to `"docket"` when omitted — the only kind implemented today.
    #[serde(default)]
    pub kind: Option<String>,
    pub base_url: String,
    /// docket Bearer token. Write-only: never echoed back in any response.
    #[serde(default)]
    pub token: Option<String>,
}

/// `PATCH /api/control-planes/{id}` body. `token` is tri-state — see the module
/// doc and [`deserialize_some`]. `name`/`base_url` follow the ordinary
/// "absent means untouched" convention every other partial update in this API
/// uses.
#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub struct UpdateControlPlaneRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    /// Absent = leave the stored token untouched. `null` = clear it. A string
    /// = set/replace it.
    #[serde(default, deserialize_with = "deserialize_some")]
    #[schema(value_type = Option<String>, nullable)]
    pub token: Option<Option<String>>,
}

const CONTROL_PLANE_NOT_FOUND: &str = "Control plane not found";

async fn get_control_plane_or_404(state: &AppState, id: Uuid) -> ApiResult<ControlPlane> {
    match state.repo.get_control_plane(id).await {
        Ok(cp) => Ok(cp),
        Err(sqlx::Error::RowNotFound) => Err(ApiError::NotFound(format!(
            "{CONTROL_PLANE_NOT_FOUND}: {id}"
        ))),
        Err(e) => Err(e.into()),
    }
}

/// `POST /api/control-planes` — register a control plane.
#[utoipa::path(
    post,
    path = "/api/control-planes",
    tag = "orchestration",
    request_body = CreateControlPlaneRequest,
    responses(
        (status = 200, description = "Control plane registered (token never returned)", body = ControlPlaneResponse),
        (status = 400, description = "Validation error", body = ErrorEnvelope),
        (status = 404, description = "Orchestration disabled (TACK_ORCH_ENABLE unset)"),
    ),
)]
#[instrument(skip(state, input))]
pub async fn create_control_plane(
    State(state): State<AppState>,
    Json(input): Json<CreateControlPlaneRequest>,
) -> ApiResult<Json<ControlPlaneResponse>> {
    if input.name.trim().is_empty() {
        return Err(ApiError::BadRequest("name must not be empty".into()));
    }
    if input.base_url.trim().is_empty() {
        return Err(ApiError::BadRequest("base_url must not be empty".into()));
    }

    let cp = state
        .repo
        .create_control_plane(CreateControlPlane {
            name: input.name,
            kind: input.kind,
            base_url: input.base_url,
            token: input.token,
        })
        .await?;

    Ok(Json(cp.into()))
}

/// `GET /api/control-planes` — every registered control plane (tokens never
/// included).
#[utoipa::path(
    get,
    path = "/api/control-planes",
    tag = "orchestration",
    responses(
        (status = 200, description = "All registered control planes", body = Vec<ControlPlaneResponse>),
        (status = 404, description = "Orchestration disabled (TACK_ORCH_ENABLE unset)"),
    ),
)]
#[instrument(skip(state))]
pub async fn list_control_planes(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<ControlPlaneResponse>>> {
    let planes = state.repo.list_control_planes().await?;
    Ok(Json(planes.into_iter().map(Into::into).collect()))
}

/// `GET /api/control-planes/{id}`.
#[utoipa::path(
    get,
    path = "/api/control-planes/{id}",
    tag = "orchestration",
    params(("id" = Uuid, Path, description = "Control plane ID")),
    responses(
        (status = 200, description = "The control plane", body = ControlPlaneResponse),
        (status = 404, description = "Not found, or orchestration disabled", body = ErrorEnvelope),
    ),
)]
#[instrument(skip(state))]
pub async fn get_control_plane(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ControlPlaneResponse>> {
    let cp = get_control_plane_or_404(&state, id).await?;
    Ok(Json(cp.into()))
}

/// `PATCH /api/control-planes/{id}`.
#[utoipa::path(
    patch,
    path = "/api/control-planes/{id}",
    tag = "orchestration",
    params(("id" = Uuid, Path, description = "Control plane ID")),
    request_body = UpdateControlPlaneRequest,
    responses(
        (status = 200, description = "Updated control plane (token never returned)", body = ControlPlaneResponse),
        (status = 404, description = "Not found, or orchestration disabled", body = ErrorEnvelope),
    ),
)]
#[instrument(skip(state, input))]
pub async fn update_control_plane(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateControlPlaneRequest>,
) -> ApiResult<Json<ControlPlaneResponse>> {
    let cp = match state
        .repo
        .update_control_plane(
            id,
            UpdateControlPlane {
                name: input.name,
                base_url: input.base_url,
                token: input.token,
            },
        )
        .await
    {
        Ok(cp) => cp,
        Err(sqlx::Error::RowNotFound) => {
            return Err(ApiError::NotFound(format!(
                "{CONTROL_PLANE_NOT_FOUND}: {id}"
            )));
        }
        Err(e) => return Err(e.into()),
    };

    Ok(Json(cp.into()))
}

/// `DELETE /api/control-planes/{id}`.
#[utoipa::path(
    delete,
    path = "/api/control-planes/{id}",
    tag = "orchestration",
    params(("id" = Uuid, Path, description = "Control plane ID")),
    responses(
        (status = 200, description = "Deleted", body = serde_json::Value),
        (status = 404, description = "Not found, or orchestration disabled", body = ErrorEnvelope),
    ),
)]
#[instrument(skip(state))]
pub async fn delete_control_plane(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let deleted = state.repo.delete_control_plane(id).await?;
    if !deleted {
        return Err(ApiError::NotFound(format!(
            "{CONTROL_PLANE_NOT_FOUND}: {id}"
        )));
    }
    Ok(Json(serde_json::json!({"deleted": true})))
}

// ════════════════════════════════════════════════════════════════════════════
// orch_links (project_id is the PK — one link per project)
// ════════════════════════════════════════════════════════════════════════════

/// `status_map` — TODO.md §1.3. All keys optional except `dispatch_from` (which
/// may be an empty list before a dispatch policy is configured — Wave 3 needs
/// it non-empty to actually dispatch, but registering a link ahead of that is
/// a normal, valid state in this wave). Every named status is validated against
/// the project's `WorkflowConfig` at save time — see [`validate_status_map`].
/// An absent key means "do not touch the item's status on that transition."
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct StatusMap {
    #[serde(default)]
    pub dispatch_from: Vec<String>,
    #[serde(default)]
    pub on_running: Option<String>,
    #[serde(default)]
    pub on_waiting_approval: Option<String>,
    #[serde(default)]
    pub on_succeeded: Option<String>,
    #[serde(default)]
    pub on_failed: Option<String>,
    #[serde(default)]
    pub on_cancelled: Option<String>,
}

/// Client-facing view of a project's control-plane link.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OrchLinkResponse {
    pub project_id: Uuid,
    pub control_plane_id: Uuid,
    pub remote_project: String,
    pub pipeline_file: Option<String>,
    pub blueprint: Option<String>,
    pub auto_dispatch: bool,
    /// User-set cap, not a derived spend figure — deliberately unsuffixed
    /// (matches `orch_links.budget_usd`; TODO.md §0 rule 6).
    pub budget_usd: Option<f64>,
    pub status_map: StatusMap,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<OrchLink> for OrchLinkResponse {
    fn from(l: OrchLink) -> Self {
        Self {
            project_id: l.project_id,
            control_plane_id: l.control_plane_id,
            remote_project: l.remote_project,
            pipeline_file: l.pipeline_file,
            blueprint: l.blueprint,
            auto_dispatch: l.auto_dispatch,
            budget_usd: l.budget_usd,
            status_map: serde_json::from_value(l.status_map).unwrap_or_default(),
            created_at: l.created_at,
            updated_at: l.updated_at,
        }
    }
}

/// `GET /api/projects/{id}/orch-link` response. `linked: false` (with
/// `link: null`) is the ordinary state for a project that has never registered
/// a control plane — not an error, matching the `settings.rs` precedent for
/// optional per-scope config (no 404 for "not configured yet").
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OrchLinkView {
    pub linked: bool,
    pub link: Option<OrchLinkResponse>,
}

/// `PUT /api/projects/{id}/orch-link` body.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpsertOrchLinkRequest {
    pub control_plane_id: Uuid,
    pub remote_project: String,
    #[serde(default)]
    pub pipeline_file: Option<String>,
    #[serde(default)]
    pub blueprint: Option<String>,
    #[serde(default)]
    pub auto_dispatch: bool,
    #[serde(default)]
    pub budget_usd: Option<f64>,
    #[serde(default)]
    pub status_map: StatusMap,
}

/// Every named status in `status_map` must exist in the project's workflow —
/// TODO.md §1.3's "validated against the project's WorkflowConfig at save
/// time." Validation, not a raw-SQL status write: this never bypasses the
/// workflow engine, it only checks the *names* used to configure future
/// auto-transitions are real (TODO.md §0 rule 7 is about the transition itself,
/// applied later by the dispatcher/reconciler — this is the save-time guard
/// that keeps a typo from silently becoming a no-op transition).
fn validate_status_map(status_map: &StatusMap, workflow: &WorkflowConfig) -> ApiResult<()> {
    let exists = |name: &str| workflow.statuses.iter().any(|s| s.name == name);

    for name in &status_map.dispatch_from {
        if !exists(name) {
            return Err(ApiError::BadRequest(format!(
                "status_map.dispatch_from: unknown status {name:?} for this project's workflow"
            )));
        }
    }

    let named = [
        ("on_running", &status_map.on_running),
        ("on_waiting_approval", &status_map.on_waiting_approval),
        ("on_succeeded", &status_map.on_succeeded),
        ("on_failed", &status_map.on_failed),
        ("on_cancelled", &status_map.on_cancelled),
    ];
    for (key, value) in named {
        if let Some(name) = value
            && !exists(name)
        {
            return Err(ApiError::BadRequest(format!(
                "status_map.{key}: unknown status {name:?} for this project's workflow"
            )));
        }
    }

    Ok(())
}

/// `GET /api/projects/{id}/orch-link`.
#[utoipa::path(
    get,
    path = "/api/projects/{id}/orch-link",
    tag = "orchestration",
    params(("id" = Uuid, Path, description = "Project ID")),
    responses(
        (status = 200, description = "The project's control-plane link, if any", body = OrchLinkView),
        (status = 404, description = "Orchestration disabled (TACK_ORCH_ENABLE unset)"),
    ),
)]
#[instrument(skip(state))]
pub async fn get_orch_link(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> ApiResult<Json<OrchLinkView>> {
    let link = state.repo.get_orch_link(project_id).await?;
    Ok(Json(match link {
        Some(l) => OrchLinkView {
            linked: true,
            link: Some(l.into()),
        },
        None => OrchLinkView {
            linked: false,
            link: None,
        },
    }))
}

/// `PUT /api/projects/{id}/orch-link` — create or replace the project's link.
#[utoipa::path(
    put,
    path = "/api/projects/{id}/orch-link",
    tag = "orchestration",
    params(("id" = Uuid, Path, description = "Project ID")),
    request_body = UpsertOrchLinkRequest,
    responses(
        (status = 200, description = "Saved link", body = OrchLinkResponse),
        (status = 400, description = "Validation error (e.g. an unknown status name in status_map)", body = ErrorEnvelope),
        (status = 404, description = "Project or control plane not found, or orchestration disabled", body = ErrorEnvelope),
    ),
)]
#[instrument(skip(state, input))]
pub async fn put_orch_link(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Json(input): Json<UpsertOrchLinkRequest>,
) -> ApiResult<Json<OrchLinkResponse>> {
    let project = state
        .repo
        .get_project(project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {project_id} not found")))?;

    // The referenced control plane must exist.
    get_control_plane_or_404(&state, input.control_plane_id).await?;

    if input.remote_project.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "remote_project must not be empty".into(),
        ));
    }

    validate_status_map(&input.status_map, &project.workflow)?;

    let status_map_json =
        serde_json::to_value(&input.status_map).unwrap_or_else(|_| serde_json::json!({}));

    let link = state
        .repo
        .upsert_orch_link(
            project_id,
            UpsertOrchLink {
                control_plane_id: input.control_plane_id,
                remote_project: input.remote_project,
                pipeline_file: input.pipeline_file,
                blueprint: input.blueprint,
                auto_dispatch: input.auto_dispatch,
                budget_usd: input.budget_usd,
                status_map: status_map_json,
            },
        )
        .await?;

    Ok(Json(link.into()))
}

// ════════════════════════════════════════════════════════════════════════════
// GET /api/fleet — the Fleet view's aggregate
// ════════════════════════════════════════════════════════════════════════════
//
// This shape is reconciled against `frontend/src/features/fleet/api.ts`
// (agent A5, landed concurrently — see TODO.md §6 "A5 — 2026-08-04"), field
// for field, so the frontend swap is mechanical. Where Wave 1 genuinely has
// no data source yet (`gateway`, `roster`, `pricing_snapshot_at`), the field
// is still present on the wire with an honest placeholder, so A5's existing
// TypeScript needs no shape changes later — only real values flowing in.

/// One roster member — projected from a future live `FleetAgent` snapshot.
/// **Always an empty list in Wave 1**: no agent-roster table exists yet
/// (migrations 019–024 mirror control planes/links/tasks/runs/events/
/// approvals only). A later wave that adds roster mirroring populates this;
/// until then the field stays on the wire as `[]` rather than being removed,
/// so `frontend/src/features/fleet/api.ts`'s `FleetRow.roster` never needs a
/// shape change.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct FleetRosterMember {
    pub id: String,
    pub name: String,
    pub role: String,
    pub model: String,
}

/// One row per Tack project that has an `orch_links` row, joining: the link,
/// its control plane's reconciler-observed health, and mirrored cost/token/
/// approval data summed from `orch_tasks`/`orch_approvals`.
///
/// **Staleness must be representable.** `cost_usd_estimated` is `None`
/// whenever the plane is `unreachable` — never coerced to zero — so the UI can
/// grey the row and say "last seen Nm ago" instead of rendering a confident
/// zero (TODO.md §0 rule 6 / this card's acceptance bar). `Some(0.0)` means the
/// plane is reachable and genuinely has no mirrored cost yet. `tokens_in`/
/// `tokens_out` are always a plain (never-null) sum — per A5's contract, the
/// row component gates on `health`/`isStale()`, not per-field nullability, to
/// decide whether a number is trustworthy to render.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct FleetEntry {
    pub project_id: Uuid,
    pub project_name: String,
    pub control_plane_id: Uuid,
    pub control_plane_name: String,
    pub control_plane_kind: String,
    pub remote_project: String,
    /// `"unknown"` | `"healthy"` | `"degraded"` | `"unreachable"`.
    pub health: String,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub consecutive_failures: i64,
    pub api_version: Option<String>,
    /// `"active"` | `"inactive"` | `"unknown"`. **Always `"unknown"` in Wave
    /// 1** — `control_planes` has no persisted gateway column (see migration
    /// 019) and the reconciler only polls `/health` + `/status.json` for the
    /// health state machine, not a stored gateway snapshot (card A2, 33.6). A
    /// later wave that mirrors `FleetStatus.gateway` populates this for real.
    pub gateway: String,
    /// Always `[]` in Wave 1 — see [`FleetRosterMember`].
    pub roster: Vec<FleetRosterMember>,
    /// Most recent `orch_tasks.dispatched_at` for this project's items, or
    /// `None` if nothing has ever been dispatched. Real data (not a
    /// placeholder) — computed from the same join as the cost/token sums —
    /// but will always be `None` until Wave 3 (C1) starts dispatching.
    pub last_activity_at: Option<DateTime<Utc>>,
    pub auto_dispatch: bool,
    pub blueprint: Option<String>,
    /// User-set cap, not a derived figure — deliberately unsuffixed.
    pub budget_usd: Option<f64>,
    /// Summed from `orch_tasks.tokens_in`/`tokens_out` for this project's
    /// items. Real data, always `0` until Wave 3 dispatches anything — not a
    /// placeholder, just an honest current total.
    pub tokens_in: i64,
    pub tokens_out: i64,
    /// Estimated cumulative spend, summed from `orch_tasks` for this
    /// project's items. `None` = plane unreachable, figure is stale/unknown.
    /// `Some(0.0)` = plane reachable, nothing dispatched yet.
    pub cost_usd_estimated: Option<f64>,
    /// Pricing-table snapshot date backing `cost_usd_estimated`. **Always
    /// `None` in Wave 1** — no pricing-snapshot mechanism exists yet; a later
    /// wave that adds one should populate this alongside real cost figures.
    pub pricing_snapshot_at: Option<String>,
    /// Pending docket approvals correlated to an item in this project (via
    /// `orch_approvals.item_id`). Approvals with no item correlation surface
    /// in the fleet-wide approvals inbox (Wave 4 / D1) instead of here.
    pub pending_approval_count: i64,
}

/// `GET /api/fleet` response envelope — matches
/// `frontend/src/features/fleet/api.ts`'s `FleetResponse` exactly (`{ rows:
/// [...] }`, not a bare array).
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct FleetListResponse {
    pub rows: Vec<FleetEntry>,
}

/// `GET /api/fleet`.
#[utoipa::path(
    get,
    path = "/api/fleet",
    tag = "orchestration",
    responses(
        (status = 200, description = "One row per project linked to a control plane", body = FleetListResponse),
        (status = 404, description = "Orchestration disabled (TACK_ORCH_ENABLE unset)"),
    ),
)]
#[instrument(skip(state))]
pub async fn get_fleet(State(state): State<AppState>) -> ApiResult<Json<FleetListResponse>> {
    let planes = state.repo.list_control_planes().await?;
    let mut rows = Vec::new();

    for plane in planes {
        let links = state.repo.list_orch_links_for_plane(plane.id).await?;
        for link in links {
            let project_name = state
                .repo
                .get_project(link.project_id)
                .await?
                .map(|p| p.name)
                .unwrap_or_else(|| "(deleted project)".to_string());

            let usage = project_task_usage(state.pool(), link.project_id).await?;

            let cost_usd_estimated = if plane.health == "unreachable" {
                None
            } else {
                Some(usage.cost_usd_estimated)
            };

            let pending_approval_count =
                count_pending_approvals(state.pool(), plane.id, link.project_id).await?;

            rows.push(FleetEntry {
                project_id: link.project_id,
                project_name,
                control_plane_id: plane.id,
                control_plane_name: plane.name.clone(),
                control_plane_kind: plane.kind.clone(),
                remote_project: link.remote_project,
                health: plane.health.clone(),
                last_seen_at: plane.last_seen_at,
                consecutive_failures: plane.consecutive_failures,
                api_version: plane.api_version.clone(),
                gateway: "unknown".to_string(),
                roster: Vec::new(),
                last_activity_at: usage.last_activity_at,
                auto_dispatch: link.auto_dispatch,
                blueprint: link.blueprint,
                budget_usd: link.budget_usd,
                tokens_in: usage.tokens_in,
                tokens_out: usage.tokens_out,
                cost_usd_estimated,
                pricing_snapshot_at: None,
                pending_approval_count,
            });
        }
    }

    Ok(Json(FleetListResponse { rows }))
}

/// Aggregate of `orch_tasks` for one project's items — one query backs
/// `tokens_in`/`tokens_out`/`cost_usd_estimated`/`last_activity_at` together
/// rather than four round trips.
struct ProjectTaskUsage {
    tokens_in: i64,
    tokens_out: i64,
    cost_usd_estimated: f64,
    last_activity_at: Option<DateTime<Utc>>,
}

async fn project_task_usage(
    pool: &sqlx::SqlitePool,
    project_id: Uuid,
) -> Result<ProjectTaskUsage, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct Row {
        tokens_in: Option<i64>,
        tokens_out: Option<i64>,
        cost_usd_estimated: Option<f64>,
        last_activity_at: Option<String>,
    }

    let row: Row = sqlx::query_as(
        "SELECT SUM(t.tokens_in) AS tokens_in, SUM(t.tokens_out) AS tokens_out, \
                SUM(t.cost_usd_estimated) AS cost_usd_estimated, \
                MAX(t.dispatched_at) AS last_activity_at \
         FROM orch_tasks t \
         JOIN items i ON i.id = t.item_id \
         WHERE i.project_id = ?",
    )
    .bind(project_id.to_string())
    .fetch_one(pool)
    .await?;

    Ok(ProjectTaskUsage {
        tokens_in: row.tokens_in.unwrap_or(0),
        tokens_out: row.tokens_out.unwrap_or(0),
        cost_usd_estimated: row.cost_usd_estimated.unwrap_or(0.0),
        last_activity_at: row.last_activity_at.as_deref().map(|s| {
            DateTime::parse_from_rfc3339(s)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now())
        }),
    })
}

/// Pending docket approvals correlated to an item in `project_id`, for one
/// control plane. Approval ingestion lands in Wave 2 (B1) — until then this
/// reads as `0`.
async fn count_pending_approvals(
    pool: &sqlx::SqlitePool,
    control_plane_id: Uuid,
    project_id: Uuid,
) -> Result<i64, sqlx::Error> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM orch_approvals a \
         JOIN items i ON i.id = a.item_id \
         WHERE a.control_plane_id = ? AND a.state = 'pending' AND i.project_id = ?",
    )
    .bind(control_plane_id.to_string())
    .bind(project_id.to_string())
    .fetch_one(pool)
    .await?;
    Ok(count)
}

// ════════════════════════════════════════════════════════════════════════════
// GET /api/metrics — Tack's own work-tracking metrics merged with the latest
// mirrored docket sample per metric/label set (card B3, task 34.7)
// ════════════════════════════════════════════════════════════════════════════
//
// Prometheus text exposition format — not JSON, unlike every other handler in
// this module. Round-trips cleanly through `tack_orch::adapters::prometheus::
// parse` (A1's parser); see `tests/orch_metrics_test.rs`'s
// `get_metrics_response_parses_with_the_real_prometheus_parser` for the proof.
//
// Sits behind the same gates as every other route in this sub-router
// (`require_orch_enabled`, plus the ordinary Bearer-token gate applied to the
// whole `/api` router in `router.rs`) rather than being fully unauthenticated
// like docket's own `/metrics` — this endpoint lives in the same
// TACK_ORCH_ENABLE-gated sub-router as `/fleet`/`/control-planes`, and A4's
// `router.rs` marked exactly this insertion point for it, so it inherits that
// sub-router's gates rather than getting a bespoke exemption. Documented as a
// known deviation from the roadmap's "unauthenticated like docket's" wording
// in the TODO.md §6 B3 handoff.

fn escape_prometheus_label_value(v: &str) -> String {
    v.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn format_prometheus_labels(labels: &BTreeMap<String, String>) -> String {
    if labels.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = labels
        .iter()
        .map(|(k, v)| format!("{k}=\"{}\"", escape_prometheus_label_value(v)))
        .collect();
    format!("{{{}}}", parts.join(","))
}

#[derive(sqlx::FromRow)]
struct ItemStatusCount {
    project_id: String,
    project_name: String,
    status: String,
    count: i64,
}

#[derive(sqlx::FromRow)]
struct ItemCycleTime {
    project_id: String,
    project_name: String,
    avg_seconds: Option<f64>,
}

#[derive(sqlx::FromRow)]
struct ItemThroughput {
    project_id: String,
    project_name: String,
    count: i64,
}

async fn tack_item_status_counts(
    pool: &sqlx::SqlitePool,
) -> Result<Vec<ItemStatusCount>, sqlx::Error> {
    sqlx::query_as(
        "SELECT i.project_id AS project_id, p.name AS project_name, i.status AS status, \
                COUNT(*) AS count \
         FROM items i JOIN projects p ON p.id = i.project_id \
         GROUP BY i.project_id, i.status",
    )
    .fetch_all(pool)
    .await
}

async fn tack_item_cycle_time(pool: &sqlx::SqlitePool) -> Result<Vec<ItemCycleTime>, sqlx::Error> {
    sqlx::query_as(
        "SELECT i.project_id AS project_id, p.name AS project_name, \
                AVG((julianday(i.completed_at) - julianday(i.started_at)) * 86400.0) AS avg_seconds \
         FROM items i JOIN projects p ON p.id = i.project_id \
         WHERE i.completed_at IS NOT NULL AND i.started_at IS NOT NULL \
         GROUP BY i.project_id",
    )
    .fetch_all(pool)
    .await
}

async fn tack_item_throughput_7d(
    pool: &sqlx::SqlitePool,
) -> Result<Vec<ItemThroughput>, sqlx::Error> {
    let since = (Utc::now() - chrono::Duration::days(7)).to_rfc3339();
    sqlx::query_as(
        "SELECT i.project_id AS project_id, p.name AS project_name, COUNT(*) AS count \
         FROM items i JOIN projects p ON p.id = i.project_id \
         WHERE i.completed_at IS NOT NULL AND i.completed_at >= ? \
         GROUP BY i.project_id",
    )
    .bind(since)
    .fetch_all(pool)
    .await
}

fn render_metrics_text(
    item_status: &[ItemStatusCount],
    cycle_time: &[ItemCycleTime],
    throughput: &[ItemThroughput],
    docket: &[OrchMetricLatest],
) -> String {
    let mut out = String::new();

    out.push_str("# HELP tack_items_total Number of items by project and status.\n");
    out.push_str("# TYPE tack_items_total gauge\n");
    for row in item_status {
        let mut labels = BTreeMap::new();
        labels.insert("project_id".to_string(), row.project_id.clone());
        labels.insert("project_name".to_string(), row.project_name.clone());
        labels.insert("status".to_string(), row.status.clone());
        out.push_str(&format!(
            "tack_items_total{} {}\n",
            format_prometheus_labels(&labels),
            row.count
        ));
    }

    out.push_str(
        "# HELP tack_item_cycle_time_seconds_avg Average seconds from an item's started_at to completed_at.\n",
    );
    out.push_str("# TYPE tack_item_cycle_time_seconds_avg gauge\n");
    for row in cycle_time {
        if let Some(avg) = row.avg_seconds {
            let mut labels = BTreeMap::new();
            labels.insert("project_id".to_string(), row.project_id.clone());
            labels.insert("project_name".to_string(), row.project_name.clone());
            out.push_str(&format!(
                "tack_item_cycle_time_seconds_avg{} {}\n",
                format_prometheus_labels(&labels),
                avg
            ));
        }
    }

    out.push_str("# HELP tack_items_completed_7d Items completed in the trailing 7 days.\n");
    out.push_str("# TYPE tack_items_completed_7d gauge\n");
    for row in throughput {
        let mut labels = BTreeMap::new();
        labels.insert("project_id".to_string(), row.project_id.clone());
        labels.insert("project_name".to_string(), row.project_name.clone());
        out.push_str(&format!(
            "tack_items_completed_7d{} {}\n",
            format_prometheus_labels(&labels),
            row.count
        ));
    }

    if !docket.is_empty() {
        out.push_str(
            "# Mirrored docket metrics: latest known sample per metric/label set, per linked \
             control plane. A control_plane label is added to disambiguate multiple planes; \
             every other label is docket's own, verbatim.\n",
        );
    }
    for row in docket {
        let mut labels = row.labels.clone();
        labels.insert("control_plane".to_string(), row.control_plane_name.clone());
        out.push_str(&format!(
            "{}{} {}\n",
            row.name,
            format_prometheus_labels(&labels),
            row.value
        ));
    }

    out
}

/// `GET /api/metrics` — Prometheus text exposition merging Tack's own
/// work-tracking metrics (items by status, average cycle time, 7-day
/// throughput) with the latest mirrored sample of every metric docket has
/// reported for each linked control plane (task 34.7). One Grafana/Prometheus
/// scrape of this endpoint covers the whole factory.
#[utoipa::path(
    get,
    path = "/api/metrics",
    tag = "orchestration",
    responses(
        (status = 200, description = "Prometheus text exposition: Tack's own work-tracking metrics plus the latest mirrored docket sample per metric/label set", content_type = "text/plain"),
        (status = 404, description = "Orchestration disabled (TACK_ORCH_ENABLE unset)"),
    ),
)]
#[instrument(skip(state))]
pub async fn get_metrics(State(state): State<AppState>) -> ApiResult<Response> {
    let pool = state.pool();
    let item_status = tack_item_status_counts(pool).await?;
    let cycle_time = tack_item_cycle_time(pool).await?;
    let throughput = tack_item_throughput_7d(pool).await?;
    let docket = state.repo.list_latest_orch_metrics().await?;

    let body = render_metrics_text(&item_status, &cycle_time, &throughput, &docket);

    Ok((
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
        .into_response())
}

// ════════════════════════════════════════════════════════════════════════════
// GET /api/items/{id}/agent-activity, GET /api/projects/{id}/agent-activity
// (Wave 2 / card B6, tasks 34.8/34.9's backend half)
// ════════════════════════════════════════════════════════════════════════════
//
// Reconciled field-for-field against B5's boundary file
// `frontend/src/shared/agentActivity/api.ts` — see that file's header comment
// for the full field-provenance table (which migration/column backs each
// field, and which `tack-orch` enum backs each raw status string). Two
// decisions B5 left open (TODO.md §6 "B5 — 2026-08-04"), resolved here:
//
// 1. **"Latest attempt" tie-break** — B5's own assumption, confirmed: highest
//    `attempt` number wins; ties broken by `dispatched_at` desc. See
//    `Repository::list_latest_orch_task_status_for_project`'s doc comment for
//    the (purely mechanical) final tie-break that makes the SQL deterministic.
// 2. **Inner join vs. left join for the bulk badge endpoint** — kept B5's
//    inner join. An item with no `orch_tasks` row simply has no row in
//    `AgentBadgeResponse`, which is exactly the "no chip" signal
//    `useAgentActivityMap` already implements on the frontend. A left join
//    would need a nullable-status contract to express "never dispatched" vs.
//    "dispatched, but the reconciler hasn't polled since" — no UI reads that
//    distinction today, so it isn't worth the wire-shape complexity yet.
//
// One field each endpoint adds beyond B5's original contract:
// `ItemAgentActivityResponse.events_truncated` / `.events_retention_days`.
// `orch_events_daily` (B3's retention rollup) aggregates by
// day/control_plane_id/event_type only — it drops `item_id` entirely, so
// there is no way to ask "were *this item's* events rolled up." The honest
// signal this endpoint can give instead: whether the item has any attempt
// dispatched before the current retention cutoff, meaning some of its
// history may already be gone from the raw `orch_events` table this
// endpoint reads. `events` itself is always exactly what's left in that
// table — never silently presented as the complete history when it might
// not be (this card's brief, echoing TODO.md §0 rule 6's spirit one level
// up: an estimate presented as certainty is the same failure whether the
// number is money or a timeline). Additive fields an old client ignores
// safely; see this card's TODO.md §6 handoff for why the UI doesn't yet
// render them (out of this card's file ownership).

/// One `orch_events` row. See `ItemAgentEventResponse` in
/// `frontend/src/shared/agentActivity/api.ts`.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ItemAgentEventResponse {
    pub id: Uuid,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub occurred_at: DateTime<Utc>,
}

impl From<OrchEvent> for ItemAgentEventResponse {
    fn from(e: OrchEvent) -> Self {
        Self {
            id: e.id,
            event_type: e.event_type,
            payload: e.payload,
            occurred_at: e.occurred_at,
        }
    }
}

/// The `orch_runs` row correlated to an attempt via `remote_run_id`, if any
/// has been mirrored yet.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ItemAgentRunResponse {
    pub run_id: String,
    pub source: String,
    pub state: String,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    /// Non-empty only when `state == "failed"`. Projected as an empty string,
    /// never `null` — matches `ItemAgentRun.error: string` on the frontend
    /// (deliberately not `string | null`, unlike `OrchRun.error` in the Rust
    /// repo layer, which is `Option<String>`).
    pub error: String,
}

impl From<OrchRun> for ItemAgentRunResponse {
    fn from(r: OrchRun) -> Self {
        Self {
            run_id: r.run_id,
            source: r.source,
            state: r.state,
            started_at: r.started_at,
            ended_at: r.ended_at,
            error: r.error.unwrap_or_default(),
        }
    }
}

/// One `orch_tasks` row (one dispatch attempt) for the item.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ItemAgentAttemptResponse {
    pub remote_task_id: String,
    pub remote_run_id: Option<String>,
    pub remote_status: String,
    pub attempt: i64,
    pub dispatched_at: DateTime<Utc>,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub cost_usd_estimated: Option<f64>,
    /// Always `null` — no pricing-snapshot mechanism exists anywhere in the
    /// system yet (TODO.md §0 rule 6; same gap A4 found for the Fleet view's
    /// identical field). Left `null` rather than invented.
    pub pricing_snapshot_at: Option<String>,
    pub run: Option<ItemAgentRunResponse>,
    pub events: Vec<ItemAgentEventResponse>,
}

/// One `orch_approvals` row for the item (pending or already decided).
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ItemAgentApprovalResponse {
    pub token: String,
    pub remote_task_id: Option<String>,
    pub agent: Option<String>,
    pub action: Option<String>,
    pub state: String,
    pub requested_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
}

impl From<OrchApproval> for ItemAgentApprovalResponse {
    fn from(a: OrchApproval) -> Self {
        Self {
            token: a.token,
            remote_task_id: a.remote_task_id,
            agent: a.agent,
            action: a.action,
            state: a.state,
            requested_at: a.requested_at,
            decided_at: a.decided_at,
        }
    }
}

/// `GET /api/items/{id}/agent-activity` response.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ItemAgentActivityResponse {
    /// Newest attempt first (`orch_tasks.attempt DESC` — the repo layer's
    /// `list_orch_tasks_for_item` already returns this order).
    pub attempts: Vec<ItemAgentAttemptResponse>,
    /// Newest-requested first; pending and decided both included.
    pub approvals: Vec<ItemAgentApprovalResponse>,
    /// See the module-level doc comment above `ItemAgentEventResponse` for
    /// why this can't be a precise per-item fact and what it means instead.
    pub events_truncated: bool,
    pub events_retention_days: u32,
}

/// `GET /api/items/{id}/agent-activity` — every mirrored dispatch attempt and
/// approval for one item, newest first.
#[utoipa::path(
    get,
    path = "/api/items/{id}/agent-activity",
    tag = "orchestration",
    params(("id" = Uuid, Path, description = "Item ID")),
    responses(
        (status = 200, description = "Dispatch attempts and approvals mirrored for this item", body = ItemAgentActivityResponse),
        (status = 404, description = "Item not found, or orchestration disabled", body = ErrorEnvelope),
    ),
)]
#[instrument(skip(state))]
pub async fn get_item_agent_activity(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
) -> ApiResult<Json<ItemAgentActivityResponse>> {
    state
        .repo
        .get_item(item_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Item {item_id} not found")))?;

    let tasks = state.repo.list_orch_tasks_for_item(item_id).await?;
    let runs = state.repo.list_orch_runs_for_item(item_id).await?;
    let approvals = state.repo.list_orch_approvals_for_item(item_id).await?;
    let events = state.repo.list_orch_events_for_item(item_id, None).await?;

    let runs_by_id: BTreeMap<String, OrchRun> =
        runs.into_iter().map(|r| (r.run_id.clone(), r)).collect();

    let mut events_by_run: BTreeMap<String, Vec<ItemAgentEventResponse>> = BTreeMap::new();
    for e in events {
        if let Some(run_id) = e.run_id.clone() {
            events_by_run.entry(run_id).or_default().push(e.into());
        }
    }

    let retention_days = state.config.orch_event_retention_days;
    let cutoff = Utc::now() - chrono::Duration::days(retention_days as i64);
    let events_truncated = tasks.iter().any(|t| t.dispatched_at < cutoff);

    let attempts = tasks
        .into_iter()
        .map(|t| {
            let run = t
                .remote_run_id
                .as_ref()
                .and_then(|rid| runs_by_id.get(rid))
                .cloned()
                .map(ItemAgentRunResponse::from);
            let events = t
                .remote_run_id
                .as_ref()
                .and_then(|rid| events_by_run.get(rid))
                .cloned()
                .unwrap_or_default();

            ItemAgentAttemptResponse {
                remote_task_id: t.remote_task_id,
                remote_run_id: t.remote_run_id,
                remote_status: t.remote_status,
                attempt: t.attempt,
                dispatched_at: t.dispatched_at,
                tokens_in: t.tokens_in,
                tokens_out: t.tokens_out,
                cost_usd_estimated: t.cost_usd_estimated,
                pricing_snapshot_at: None,
                run,
                events,
            }
        })
        .collect();

    Ok(Json(ItemAgentActivityResponse {
        attempts,
        approvals: approvals.into_iter().map(Into::into).collect(),
        events_truncated,
        events_retention_days: retention_days,
    }))
}

/// One row of `GET /api/projects/{id}/agent-activity` — the minimum a
/// Board/List/Table badge needs: an item's latest dispatch attempt's raw
/// status. See the module-level doc comment for the inner-join / tie-break
/// decisions.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AgentBadgeRowResponse {
    pub item_id: Uuid,
    pub remote_status: String,
    pub attempt: i64,
    pub updated_at: DateTime<Utc>,
}

/// `GET /api/projects/{id}/agent-activity` response envelope — matches
/// `frontend/src/shared/agentActivity/api.ts`'s `AgentBadgeResponse` exactly
/// (`{ rows: [...] }`, not a bare array).
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AgentBadgeResponse {
    pub rows: Vec<AgentBadgeRowResponse>,
}

/// `GET /api/projects/{id}/agent-activity` — one row per item in this project
/// that has at least one mirrored dispatch attempt, each carrying only its
/// latest attempt's raw status (the Board/List/Table badge's data source).
/// Does not 404 for an unknown `project_id` — mirrors `list_items`'s
/// precedent (`handlers/items.rs`) of just returning an empty result for a
/// project-scoped list rather than a bulk-fetch racing a project's own
/// lifecycle (e.g. deleted between the page loading and this poll).
#[utoipa::path(
    get,
    path = "/api/projects/{id}/agent-activity",
    tag = "orchestration",
    params(("id" = Uuid, Path, description = "Project ID")),
    responses(
        (status = 200, description = "Latest dispatch-attempt status per item with agent activity", body = AgentBadgeResponse),
        (status = 404, description = "Orchestration disabled (TACK_ORCH_ENABLE unset)"),
    ),
)]
#[instrument(skip(state))]
pub async fn get_project_agent_activity(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> ApiResult<Json<AgentBadgeResponse>> {
    let rows = state
        .repo
        .list_latest_orch_task_status_for_project(project_id)
        .await?
        .into_iter()
        .map(|r| AgentBadgeRowResponse {
            item_id: r.item_id,
            remote_status: r.remote_status,
            attempt: r.attempt,
            updated_at: r.updated_at,
        })
        .collect();

    Ok(Json(AgentBadgeResponse { rows }))
}

// ════════════════════════════════════════════════════════════════════════════
// POST /api/items/{id}/dispatch (card C1, Wave 3, tasks 35.2/35.3/35.6)
// ════════════════════════════════════════════════════════════════════════════
//
// The actual dispatch flow lives in `crate::dispatcher` — this section is
// just the HTTP↔`DispatchOutcome` translation. See that module's doc
// comment for the idempotency guarantee, the `trusted` boundary, and what
// this card deliberately left unwired (terminal-state application via the
// reconciler).

/// A dispatched (or already-in-flight) `orch_tasks` row, projected for the
/// dispatch response. Deliberately smaller than `ItemAgentAttemptResponse`
/// (B6, `GET /items/{id}/agent-activity`) — no `run`/`events`/token-cost
/// fields, since a task this fresh has none of that mirrored yet.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DispatchedTaskResponse {
    pub remote_task_id: String,
    pub remote_status: String,
    pub attempt: i64,
    pub dispatched_at: DateTime<Utc>,
    pub trusted: bool,
}

impl From<OrchTask> for DispatchedTaskResponse {
    fn from(t: OrchTask) -> Self {
        Self {
            remote_task_id: t.remote_task_id,
            remote_status: t.remote_status,
            attempt: t.attempt,
            dispatched_at: t.dispatched_at,
            trusted: t.trusted,
        }
    }
}

/// `POST /api/items/{id}/dispatch` response. `outcome` is one of
/// `"dispatched"`, `"waiting_approval"`, `"already_in_flight"`,
/// `"no_dispatch_policy"`, `"not_eligible"`, `"blocked"` — every one of
/// these is a `200`, including `"blocked"`: docket gave a definitive,
/// well-formed refusal, which is a successful round-trip from Tack's HTTP
/// perspective, not a Tack-side error. Callers must branch on `outcome`,
/// not on HTTP status, to tell these apart. See `dispatcher::DispatchOutcome`
/// for the same taxonomy on the Rust side.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DispatchItemResponse {
    pub outcome: String,
    pub task: Option<DispatchedTaskResponse>,
    /// Present only when `outcome == "waiting_approval"`.
    pub approval_token: Option<String>,
    /// Present only when `outcome == "not_eligible"`.
    pub current_status: Option<String>,
    /// Present only when `outcome == "not_eligible"`.
    pub dispatch_from: Option<Vec<String>>,
    /// Present only when `outcome == "blocked"` — the id of the guardrail
    /// policy that fired (`OrchError::PolicyBlocked::policy_id`, card R1),
    /// as a typed field rather than something a caller has to parse back out
    /// of `message`.
    pub policy_id: Option<String>,
    /// Present only when `outcome == "blocked"` — docket's own message,
    /// verbatim, for display.
    pub message: Option<String>,
    /// The Tack status `status_map` named for this trigger and actually
    /// applied. Absent when `status_map` named no target for this trigger,
    /// when the item was already there, or when the workflow engine
    /// rejected it (see `status_map_rejected` below).
    pub status_applied: Option<String>,
    /// Set when the workflow engine refused the `status_map`-driven
    /// transition (TODO.md §0 rule 7 / task 35.6's `status_map_rejected`
    /// outcome). The item was left exactly as it was; this is the engine's
    /// own reason (e.g. an invalid transition or a WIP limit).
    pub status_map_rejected: Option<String>,
}

impl DispatchItemResponse {
    fn empty(outcome: &str) -> Self {
        Self {
            outcome: outcome.to_string(),
            task: None,
            approval_token: None,
            current_status: None,
            dispatch_from: None,
            policy_id: None,
            message: None,
            status_applied: None,
            status_map_rejected: None,
        }
    }
}

impl From<DispatchOutcome> for DispatchItemResponse {
    fn from(outcome: DispatchOutcome) -> Self {
        match outcome {
            DispatchOutcome::NoDispatchPolicy => Self::empty("no_dispatch_policy"),
            DispatchOutcome::NotEligible {
                current_status,
                dispatch_from,
            } => Self {
                current_status: Some(current_status),
                dispatch_from: Some(dispatch_from),
                ..Self::empty("not_eligible")
            },
            DispatchOutcome::AlreadyInFlight { task } => Self {
                task: Some(task.into()),
                ..Self::empty("already_in_flight")
            },
            DispatchOutcome::Blocked { policy_id, message } => Self {
                policy_id: Some(policy_id),
                message: Some(message),
                ..Self::empty("blocked")
            },
            DispatchOutcome::Success(s) => Self {
                task: Some(s.task.into()),
                approval_token: s.approval_token,
                status_applied: s
                    .status_application
                    .as_ref()
                    .filter(|a| a.applied)
                    .map(|a| a.target_status.clone()),
                status_map_rejected: s
                    .status_application
                    .as_ref()
                    .and_then(|a| a.rejected_reason.clone()),
                ..Self::empty(if s.waiting_approval {
                    "waiting_approval"
                } else {
                    "dispatched"
                })
            },
        }
    }
}

/// `POST /api/items/{id}/dispatch` — enqueue a governed task on the item's
/// project's linked control plane. See the `dispatcher` module for the full
/// flow; this handler is just the HTTP boundary.
///
/// **Trust default.** Unlike `dispatcher::dispatch_item`'s own required
/// `trusted: bool` parameter (no default, by design — see that module's
/// doc), this direct/manual entry point has no request body asking the
/// caller to state trust explicitly, so it resolves one via
/// `dispatcher::resolve_default_trust` — a conservative stopgap pending
/// card C2's `source: imported` marker (task 35.7). See that function's own
/// doc comment for exactly what it checks today and what it doesn't yet.
#[utoipa::path(
    post,
    path = "/api/items/{id}/dispatch",
    tag = "orchestration",
    params(("id" = Uuid, Path, description = "Item ID")),
    responses(
        (status = 200, description = "Dispatch outcome — branch on the `outcome` field, not HTTP status", body = DispatchItemResponse),
        (status = 404, description = "Item not found, or orchestration disabled", body = ErrorEnvelope),
        (status = 409, description = "Project not linked to a control plane, a dispatch for this item is already in flight, or the control plane could not be reached", body = ErrorEnvelope),
    ),
)]
#[instrument(skip(state))]
pub async fn dispatch_item(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
) -> ApiResult<Json<DispatchItemResponse>> {
    let trusted = dispatcher::resolve_default_trust(&state, item_id).await?;
    let outcome = dispatcher::dispatch_item(&state, item_id, trusted).await?;
    Ok(Json(outcome.into()))
}

// ════════════════════════════════════════════════════════════════════════════
// POST /api/sprints/{id}/dispatch, GET /api/sprints/{id}/dispatch/dry-run
// (card C3, Wave 3, task 35.4 — DAG-ordered sprint dispatch)
// ════════════════════════════════════════════════════════════════════════════
//
// The planning (dependency order + readiness gating) and execution
// (bounded-concurrency dispatch) logic lives in `crate::sprint_dispatch` —
// see that module's doc comment for the five design decisions TODO.md
// called out (partial failure, dependency readiness, concurrency, the
// no-write-txn-across-HTTP rule, and dry-run honesty). This section is just
// the HTTP↔domain-type translation, the same split C1 used for the
// single-item dispatch endpoint above.

/// Query params shared by both sprint-dispatch routes. A bare query param
/// rather than a request body — `POST /dispatch` has nothing else to say,
/// and using the same shape for both routes keeps the dry-run preview and
/// the real run trivially comparable (same input, same knob).
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct SprintDispatchQuery {
    /// Bound on concurrent HTTP calls to the control plane for this run.
    /// Omit to use `sprint_dispatch::DEFAULT_MAX_IN_FLIGHT`; any value is
    /// clamped to `[1, sprint_dispatch::MAX_MAX_IN_FLIGHT]`. The dry-run
    /// response's own `max_in_flight` field reports the clamped value, so
    /// the UI can show exactly what a real run with this input would use.
    #[serde(default)]
    pub max_in_flight: Option<u32>,
}

/// One item's place in a sprint-dispatch plan or report — shared shape for
/// both the dry-run preview and the real run's per-item result, so the two
/// responses read the same way side by side. `decision` is one of:
/// `"waiting_on_dependencies"`, `"no_dispatch_policy"`, `"not_eligible"`,
/// `"already_in_flight"`, `"blocked"`, `"waiting_approval"`, `"dispatched"`,
/// `"would_dispatch"` (dry-run only — a real run would have called docket
/// and resolved to one of the outcomes above instead), or `"error"`
/// (real run only — this item's own dispatch failed or its worker task
/// panicked; every other item in the sprint still ran).
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SprintDispatchItemResponse {
    pub item_id: Uuid,
    pub title: String,
    pub status: String,
    pub order: usize,
    pub decision: String,
    /// Present only when `decision == "waiting_on_dependencies"` — every
    /// direct dependency that hasn't reached a Done-category status yet.
    pub blocked_by: Option<Vec<Uuid>>,
    /// Present only when `decision == "error"` (real run only).
    pub error: Option<String>,
    pub task: Option<DispatchedTaskResponse>,
    pub approval_token: Option<String>,
    pub current_status: Option<String>,
    pub dispatch_from: Option<Vec<String>>,
    pub policy_id: Option<String>,
    pub message: Option<String>,
    pub status_applied: Option<String>,
    pub status_map_rejected: Option<String>,
}

impl SprintDispatchItemResponse {
    fn empty(item_id: Uuid, title: String, status: String, order: usize, decision: &str) -> Self {
        Self {
            item_id,
            title,
            status,
            order,
            decision: decision.to_string(),
            blocked_by: None,
            error: None,
            task: None,
            approval_token: None,
            current_status: None,
            dispatch_from: None,
            policy_id: None,
            message: None,
            status_applied: None,
            status_map_rejected: None,
        }
    }

    /// Overlay `DispatchItemResponse`'s fields (produced from a real
    /// `DispatchOutcome` via C1's own `From` impl — reused verbatim, not
    /// reimplemented) onto this row.
    fn with_dispatch_outcome(mut self, mapped: DispatchItemResponse) -> Self {
        self.decision = mapped.outcome;
        self.task = mapped.task;
        self.approval_token = mapped.approval_token;
        self.current_status = mapped.current_status;
        self.dispatch_from = mapped.dispatch_from;
        self.policy_id = mapped.policy_id;
        self.message = mapped.message;
        self.status_applied = mapped.status_applied;
        self.status_map_rejected = mapped.status_map_rejected;
        self
    }
}

impl From<sprint_dispatch::DryRunItem> for SprintDispatchItemResponse {
    fn from(i: sprint_dispatch::DryRunItem) -> Self {
        let base = |decision: &str| {
            Self::empty(
                i.item_id,
                i.title.clone(),
                i.status.clone(),
                i.order,
                decision,
            )
        };
        match i.decision {
            PreviewDecision::WaitingOnDependencies { blocked_by } => Self {
                blocked_by: Some(blocked_by),
                ..base("waiting_on_dependencies")
            },
            PreviewDecision::NoDispatchPolicy => base("no_dispatch_policy"),
            PreviewDecision::NotEligible {
                current_status,
                dispatch_from,
            } => Self {
                current_status: Some(current_status),
                dispatch_from: Some(dispatch_from),
                ..base("not_eligible")
            },
            PreviewDecision::AlreadyInFlight { task } => Self {
                task: Some(task.into()),
                ..base("already_in_flight")
            },
            PreviewDecision::WouldDispatch => base("would_dispatch"),
        }
    }
}

impl From<sprint_dispatch::SprintDispatchItem> for SprintDispatchItemResponse {
    fn from(i: sprint_dispatch::SprintDispatchItem) -> Self {
        let (title, status, order, item_id) =
            (i.title.clone(), i.status.clone(), i.order, i.item_id);
        match i.result {
            ItemResult::WaitingOnDependencies { blocked_by } => Self {
                blocked_by: Some(blocked_by),
                ..Self::empty(item_id, title, status, order, "waiting_on_dependencies")
            },
            ItemResult::Error(error) => Self {
                error: Some(error),
                ..Self::empty(item_id, title, status, order, "error")
            },
            ItemResult::Outcome(outcome) => {
                let mapped: DispatchItemResponse = (*outcome).into();
                Self::empty(item_id, title, status, order, "").with_dispatch_outcome(mapped)
            }
        }
    }
}

/// Summary counts over a real dispatch run's per-item `decision` values —
/// the UI's headline "8 dispatched, 2 waiting on dependencies" line without
/// re-deriving it client-side from the row list.
#[derive(Debug, Default, Serialize, utoipa::ToSchema)]
pub struct SprintDispatchSummary {
    pub total: usize,
    pub dispatched: usize,
    pub waiting_approval: usize,
    pub blocked: usize,
    pub already_in_flight: usize,
    pub waiting_on_dependencies: usize,
    pub not_eligible: usize,
    pub no_dispatch_policy: usize,
    pub would_dispatch: usize,
    pub errored: usize,
}

impl SprintDispatchSummary {
    fn tally(items: &[SprintDispatchItemResponse]) -> Self {
        let mut s = Self {
            total: items.len(),
            ..Self::default()
        };
        for item in items {
            match item.decision.as_str() {
                "dispatched" => s.dispatched += 1,
                "waiting_approval" => s.waiting_approval += 1,
                "blocked" => s.blocked += 1,
                "already_in_flight" => s.already_in_flight += 1,
                "waiting_on_dependencies" => s.waiting_on_dependencies += 1,
                "not_eligible" => s.not_eligible += 1,
                "no_dispatch_policy" => s.no_dispatch_policy += 1,
                "would_dispatch" => s.would_dispatch += 1,
                "error" => s.errored += 1,
                _ => {}
            }
        }
        s
    }
}

/// `GET /api/sprints/{id}/dispatch/dry-run` response. Zero side effects —
/// see `sprint_dispatch`'s module doc, decision 5.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DryRunSprintDispatchResponse {
    pub sprint_id: Uuid,
    pub max_in_flight: u32,
    pub summary: SprintDispatchSummary,
    pub items: Vec<SprintDispatchItemResponse>,
}

impl From<sprint_dispatch::DryRunPlan> for DryRunSprintDispatchResponse {
    fn from(plan: sprint_dispatch::DryRunPlan) -> Self {
        let items: Vec<SprintDispatchItemResponse> =
            plan.items.into_iter().map(Into::into).collect();
        let summary = SprintDispatchSummary::tally(&items);
        Self {
            sprint_id: plan.sprint_id,
            max_in_flight: plan.max_in_flight,
            summary,
            items,
        }
    }
}

/// `POST /api/sprints/{id}/dispatch` response.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SprintDispatchResponse {
    pub sprint_id: Uuid,
    pub max_in_flight: u32,
    pub summary: SprintDispatchSummary,
    pub items: Vec<SprintDispatchItemResponse>,
}

impl From<sprint_dispatch::SprintDispatchReport> for SprintDispatchResponse {
    fn from(report: sprint_dispatch::SprintDispatchReport) -> Self {
        let items: Vec<SprintDispatchItemResponse> =
            report.items.into_iter().map(Into::into).collect();
        let summary = SprintDispatchSummary::tally(&items);
        Self {
            sprint_id: report.sprint_id,
            max_in_flight: report.max_in_flight,
            summary,
            items,
        }
    }
}

/// `GET /api/sprints/{id}/dispatch/dry-run` — the exact plan a real
/// `POST .../dispatch` call would execute (same order, same
/// dependency-readiness skips), with zero database writes and zero HTTP
/// calls to the control plane. See `sprint_dispatch::dry_run_sprint_dispatch`.
#[utoipa::path(
    get,
    path = "/api/sprints/{id}/dispatch/dry-run",
    tag = "orchestration",
    params(("id" = Uuid, Path, description = "Sprint ID"), SprintDispatchQuery),
    responses(
        (status = 200, description = "Dependency-ordered dispatch plan — zero side effects", body = DryRunSprintDispatchResponse),
        (status = 404, description = "Sprint not found, or orchestration disabled", body = ErrorEnvelope),
        (status = 409, description = "Project not linked to a control plane", body = ErrorEnvelope),
    ),
)]
#[instrument(skip(state))]
pub async fn dry_run_sprint_dispatch(
    State(state): State<AppState>,
    Path(sprint_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<SprintDispatchQuery>,
) -> ApiResult<Json<DryRunSprintDispatchResponse>> {
    let plan =
        sprint_dispatch::dry_run_sprint_dispatch(&state, sprint_id, query.max_in_flight).await?;
    Ok(Json(plan.into()))
}

/// `POST /api/sprints/{id}/dispatch` — dispatch every dependency-ready item
/// in the sprint, in topological order, bounded by `max_in_flight`
/// concurrent control-plane calls. Each item's own `ItemSource` decides its
/// `trusted` flag (never a blanket value for the batch — TODO.md's
/// non-negotiable). A failure dispatching one item never aborts the rest;
/// see `sprint_dispatch`'s module doc, decision 1.
#[utoipa::path(
    post,
    path = "/api/sprints/{id}/dispatch",
    tag = "orchestration",
    params(("id" = Uuid, Path, description = "Sprint ID"), SprintDispatchQuery),
    responses(
        (status = 200, description = "Sprint dispatch report — one row per item, in dependency order", body = SprintDispatchResponse),
        (status = 404, description = "Sprint not found, or orchestration disabled", body = ErrorEnvelope),
        (status = 409, description = "Project not linked to a control plane", body = ErrorEnvelope),
    ),
)]
#[instrument(skip(state))]
pub async fn dispatch_sprint(
    State(state): State<AppState>,
    Path(sprint_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<SprintDispatchQuery>,
) -> ApiResult<Json<SprintDispatchResponse>> {
    let report = sprint_dispatch::dispatch_sprint(&state, sprint_id, query.max_in_flight).await?;
    Ok(Json(report.into()))
}

// ════════════════════════════════════════════════════════════════════════════
// GET /api/approvals, POST /api/approvals/{token}
// (card D1, Wave 4, tasks 36.1/36.2 — approvals inbox + decision proxy)
// ════════════════════════════════════════════════════════════════════════════
//
// The fleet-wide inbox of approvals **currently blocking an agent fleet**,
// plus the proxy that actually grants or denies one via docket's own
// `POST /approvals/{token}`. This is a two-privilege-level surface —
// reading the inbox needs only the ordinary orchestration gate (this
// module's `require_orch_enabled` layer + the outer `TACK_API_TOKEN` Bearer
// gate `router.rs` applies to the whole API), but *deciding* one needs the
// separate `TACK_ORCH_APPROVAL_TOKEN` on top (see
// `require_approval_token`'s own doc comment for the full "why", and
// TODO.md §0 rule — "granting an approval is a higher-privilege action than
// editing a card").
//
// Uncorrelated approvals (`item_id: null` — B1 could not attribute the
// gate to a Tack item, e.g. a CLI-dispatched run) are included here, not
// filtered out: the per-project Fleet view (`GET /fleet`) deliberately
// excludes them (an inner join on `items`/`projects`, per A4's handoff)
// specifically because this inbox is where they're meant to surface — an
// approval Tack can't attribute to a project is the one most likely to be
// silently blocking a fleet, per this card's own rationale.

/// Header carrying the operator's `TACK_ORCH_APPROVAL_TOKEN` on a decision
/// request. Deliberately not `Authorization` (already spoken for by the
/// ordinary `TACK_API_TOKEN` Bearer gate — this is a second, independent
/// credential, not a replacement for the first) and deliberately a header,
/// not a request-body field (so it never ends up echoed into a JSON log
/// line the way a body field might).
pub const APPROVAL_TOKEN_HEADER: &str = "x-tack-approval-token";

/// One row of the fleet-wide approvals inbox — a pending `orch_approvals`
/// record enriched with the correlated control plane / item / project, when
/// known. See the module-doc section above on why uncorrelated rows
/// (`item_id: null`) are never filtered out.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PendingApprovalResponse {
    pub token: String,
    pub control_plane_id: Uuid,
    pub control_plane_name: String,
    pub item_id: Option<Uuid>,
    pub item_title: Option<String>,
    pub item_status: Option<String>,
    pub project_id: Option<Uuid>,
    pub project_name: Option<String>,
    pub remote_task_id: Option<String>,
    /// `orch_approvals.agent` — populated from docket's `role` field on
    /// ingestion (B1's handoff, TODO.md §6: "role is the closest field").
    pub agent: Option<String>,
    /// The gated action's description, already redacted by docket before it
    /// reached Tack's mirror.
    pub action: Option<String>,
    pub requested_at: DateTime<Utc>,
}

impl From<PendingOrchApproval> for PendingApprovalResponse {
    fn from(a: PendingOrchApproval) -> Self {
        Self {
            token: a.token,
            control_plane_id: a.control_plane_id,
            control_plane_name: a.control_plane_name,
            item_id: a.item_id,
            item_title: a.item_title,
            item_status: a.item_status,
            project_id: a.project_id,
            project_name: a.project_name,
            remote_task_id: a.remote_task_id,
            agent: a.agent,
            action: a.action,
            requested_at: a.requested_at,
        }
    }
}

/// `GET /api/approvals` response envelope.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PendingApprovalListResponse {
    /// Oldest-requested first — docket approvals fail closed on timeout, so
    /// surfacing the longest-waiting one first has a real cost (this
    /// card's rationale).
    pub rows: Vec<PendingApprovalResponse>,
    /// Whether `TACK_ORCH_APPROVAL_TOKEN` is configured on this server at
    /// all, **without ever exposing its value** — the same write-only-secret
    /// discipline as `ControlPlaneResponse.token_set` /
    /// `handlers::settings`'s `secret_key_set`. The frontend uses this to
    /// decide whether to render Grant/Deny controls at all (a missing
    /// server-side secret means nobody can act on this inbox today); the
    /// server still enforces the real check independently on every
    /// `POST /api/approvals/{token}` call regardless of what this flag says,
    /// so a stale/cached `true` can never grant a privilege the header check
    /// wouldn't also grant.
    pub grant_available: bool,
}

/// `GET /api/approvals` — the fleet-wide pending-approval inbox, oldest
/// first. Read-only; no `TACK_ORCH_APPROVAL_TOKEN` needed (see the
/// module-doc section above on why reading and deciding are different
/// privilege levels).
#[utoipa::path(
    get,
    path = "/api/approvals",
    tag = "orchestration",
    responses(
        (status = 200, description = "Fleet-wide pending-approval inbox, oldest first — includes uncorrelated approvals", body = PendingApprovalListResponse),
        (status = 404, description = "Orchestration disabled", body = ErrorEnvelope),
    ),
)]
#[instrument(skip(state))]
pub async fn list_pending_approvals(
    State(state): State<AppState>,
) -> ApiResult<Json<PendingApprovalListResponse>> {
    let rows = state
        .repo
        .list_pending_orch_approvals_with_context()
        .await?;
    Ok(Json(PendingApprovalListResponse {
        rows: rows.into_iter().map(Into::into).collect(),
        grant_available: state.config.orch_approval_token.is_some(),
    }))
}

/// The gate this whole card exists for. Granting or denying a docket
/// approval releases whatever an autonomous agent was paused for — a
/// materially higher-privilege action than the ordinary `TACK_API_TOKEN`
/// Bearer gate already covers (which lets a caller move any card on the
/// board). So it requires the **separate** `TACK_ORCH_APPROVAL_TOKEN` on
/// top, checked here rather than as a blanket middleware layer (unlike
/// `require_orch_enabled`) because it applies to exactly one route, not the
/// whole `orch_routes()` sub-router — reading the inbox (`GET /approvals`)
/// deliberately does not go through this function.
///
/// **The safe default when `TACK_ORCH_APPROVAL_TOKEN` is unset: always
/// reject.** There is deliberately no "no secret configured, so skip the
/// check" branch the way `middleware::require_token`'s ordinary Bearer gate
/// has for an unset `TACK_API_TOKEN` ("pure-local mode, allow everything").
/// The two gates look similar but their safe defaults point opposite ways
/// on purpose: an unconfigured `TACK_API_TOKEN` means "no auth configured
/// for this whole install, trust the network boundary instead" — a
/// deliberate, instantly-reversible operator choice. An unconfigured
/// `TACK_ORCH_APPROVAL_TOKEN` must mean "nothing on this server is
/// configured to release a gated agent action" — not "anyone holding the
/// ordinary API token can." [`PendingApprovalListResponse::grant_available`]
/// is how the frontend learns this without ever seeing the secret itself.
fn require_approval_token(state: &AppState, headers: &HeaderMap) -> ApiResult<()> {
    let Some(expected) = &state.config.orch_approval_token else {
        return Err(ApiError::Forbidden(
            "granting or denying approvals requires TACK_ORCH_APPROVAL_TOKEN to be configured \
             on this server"
                .to_string(),
        ));
    };
    let provided = headers
        .get(APPROVAL_TOKEN_HEADER)
        .and_then(|v| v.to_str().ok());
    match provided {
        Some(tok) if crate::middleware::constant_time_eq(tok.as_bytes(), expected.as_bytes()) => {
            Ok(())
        }
        _ => Err(ApiError::Forbidden(format!(
            "missing or invalid {APPROVAL_TOKEN_HEADER} header"
        ))),
    }
}

/// Resolve `control_plane_id` into a live control-plane client. Deliberately
/// duplicated from `dispatcher::build_control_plane` (a private fn in a
/// file owned by a different agent this wave — see TODO.md's
/// file-ownership map, `crates/tack-api/src/dispatcher.rs`) rather than
/// exported from there: ~15 lines, no shared state, not worth the
/// cross-agent coordination a shared export would need mid-cycle for a
/// single additional caller. If a third caller ever needs this, that's the
/// point to actually share it.
async fn build_control_plane_for_decision(
    state: &AppState,
    control_plane_id: Uuid,
) -> ApiResult<Arc<dyn OrchControlPlane>> {
    let row = match state.repo.get_control_plane(control_plane_id).await {
        Ok(row) => row,
        Err(sqlx::Error::RowNotFound) => {
            return Err(ApiError::NotFound(format!(
                "control plane {control_plane_id} not found"
            )));
        }
        Err(e) => return Err(ApiError::Database(e)),
    };
    let token = state.repo.get_control_plane_token(control_plane_id).await?;

    match row.kind.as_str() {
        "docket" => {
            let adapter = DocketAdapter::new(row.base_url.clone(), token).map_err(|e| {
                ApiError::Internal(anyhow::anyhow!(
                    "failed to construct control-plane adapter: {e}"
                ))
            })?;
            Ok(Arc::new(adapter))
        }
        other => Err(ApiError::Internal(anyhow::anyhow!(
            "unsupported control-plane kind {other:?}"
        ))),
    }
}

/// `POST /api/approvals/{token}` request body.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecisionAction {
    Grant,
    Deny,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct DecideApprovalRequest {
    pub action: ApprovalDecisionAction,
}

/// `POST /api/approvals/{token}` response — docket's own resulting state
/// (`"granted"`/`"denied"`, or an unrecognised value shown as-is per this
/// whole cycle's "never fail on an unknown remote value" discipline).
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DecideApprovalResponse {
    pub token: String,
    pub state: String,
}

/// `POST /api/approvals/{token}` — grant or deny a pending approval,
/// proxying to docket's own `POST /approvals/{token}` with `channel: "tack"`
/// so the decision is honestly attributed in docket's hash-chained audit
/// log (its P22-4) rather than reading as an anonymous/CLI decision.
///
/// **Not idempotent, not reversible** — see [`OrchError::AlreadyDecided`]'s
/// doc comment for what happens when the token was already resolved
/// elsewhere (a normal race for an inbox like this, not a bug): this
/// handler reports it as `409`, not `500`, and the frontend treats it as
/// "remove this stale row," not as an error toast.
#[utoipa::path(
    post,
    path = "/api/approvals/{token}",
    tag = "orchestration",
    params(("token" = String, Path, description = "docket approval token")),
    request_body = DecideApprovalRequest,
    responses(
        (status = 200, description = "Decision applied — docket's own resulting state", body = DecideApprovalResponse),
        (status = 403, description = "Missing/invalid X-Tack-Approval-Token header, or TACK_ORCH_APPROVAL_TOKEN not configured on this server", body = ErrorEnvelope),
        (status = 404, description = "Unknown token, orchestration disabled, or the control plane that issued it was deleted", body = ErrorEnvelope),
        (status = 409, description = "The approval was already decided (granted/denied/expired) elsewhere", body = ErrorEnvelope),
    ),
)]
#[instrument(skip(state, headers))]
pub async fn decide_approval(
    State(state): State<AppState>,
    Path(token): Path<String>,
    headers: HeaderMap,
    Json(body): Json<DecideApprovalRequest>,
) -> ApiResult<Json<DecideApprovalResponse>> {
    require_approval_token(&state, &headers)?;

    let approval = state
        .repo
        .get_orch_approval(&token)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("approval {token} not found")))?;

    let control_plane = build_control_plane_for_decision(&state, approval.control_plane_id).await?;
    let grant = matches!(body.action, ApprovalDecisionAction::Grant);

    let result_state = match control_plane.decide_approval(&token, grant).await {
        Ok(s) => s,
        Err(OrchError::AlreadyDecided(message)) => return Err(ApiError::Conflict(message)),
        Err(OrchError::NotFound(message)) => return Err(ApiError::NotFound(message)),
        Err(OrchError::Auth) => {
            return Err(ApiError::Internal(anyhow::anyhow!(
                "control plane rejected Tack's own credentials while deciding an approval"
            )));
        }
        Err(e) => {
            return Err(ApiError::Conflict(format!(
                "failed to reach control plane to decide approval: {e}"
            )));
        }
    };

    // Local mirror only — docket already made the real decision above; this
    // just keeps the row out of the next `GET /api/approvals` fetch. See
    // `mark_orch_approval_decided`'s own doc comment.
    state
        .repo
        .mark_orch_approval_decided(&token, result_state.as_str(), Utc::now())
        .await?;

    Ok(Json(DecideApprovalResponse {
        token,
        state: result_state.as_str().to_string(),
    }))
}
