//! The digest (ADR 0034, decision 6): a bounded summary of the projects a caller may read, so a
//! caller asks for it before it asks for detail. What it says is `tack_core::digest`'s and the
//! rows are `tack_app::digest`'s; this file chooses the projects the caller's scopes allow.

use axum::Extension;
use axum::extract::State;
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use tracing::instrument;
use utoipa::IntoParams;
use uuid::Uuid;

use tack_core::digest::Digest;
use tack_core::principal::{Capability, Principal, may, missing_scope};
use tack_protocol::{Clock, SystemClock};

use crate::error::{ApiError, ApiResult};
use crate::extract::{Json, Query};
use crate::router::AppState;

/// How far back the failed runs reach when `since` is left out.
const DEFAULT_WINDOW_HOURS: i64 = 24;

#[derive(Debug, Deserialize, IntoParams)]
pub struct DigestParams {
    /// The projects to describe, as ids separated by commas. Absent: every project the caller
    /// may read.
    projects: Option<String>,
    /// Runs that failed before this instant (RFC 3339) are left out. Absent: the last 24 hours.
    since: Option<String>,
}

fn project_ids(text: &str) -> Result<Vec<Uuid>, ApiError> {
    let mut ids: Vec<Uuid> = Vec::new();
    for part in text
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        let id = Uuid::parse_str(part)
            .map_err(|_| ApiError::BadRequest(format!("`projects` names `{part}`, not an id")))?;
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    Ok(ids)
}

/// GET /api/digest
#[instrument(skip(state, principal))]
#[utoipa::path(
    get,
    path = "/api/digest",
    tag = "digest",
    params(DigestParams),
    responses(
        (status = 200, description = "A computed, bounded summary of the projects: items per status, what waits on a person and since when, blocked chains, tasks no team will pick up and why, stale items, runs failed since `since`, tokens against the limits, each team's current work and queue, and the change sets waiting. Every list shows a fixed number of rows and says how many more there are", body = Digest),
        (status = 400, description = "`projects` or `since` is not what it should be", body = crate::openapi::ErrorEnvelope),
        (status = 403, description = "The credential lacks the scope `read` on a project it asked for", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "A project it asked for does not exist", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn get_digest(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Query(params): Query<DigestParams>,
) -> ApiResult<Json<Digest>> {
    let clock = SystemClock;
    let since = match params.since.as_deref() {
        Some(text) => DateTime::parse_from_rfc3339(text)
            .map_err(|_| {
                ApiError::BadRequest(format!("`since` is not an RFC 3339 instant: {text}"))
            })?
            .with_timezone(&Utc),
        None => clock.now() - Duration::hours(DEFAULT_WINDOW_HOURS),
    };
    let requested = params.projects.as_deref().map(project_ids).transpose()?;
    let all = state.repo.list_projects(state.workspace_id).await?;
    let projects = match &requested {
        // Asked for by name: each one must be readable, and be there. The scope is checked
        // first, so the answer does not tell a project that is missing from one that is hidden.
        Some(ids) => {
            let mut chosen = Vec::new();
            for id in ids {
                if !may(&principal, *id, Capability::Read) {
                    return Err(ApiError::Forbidden(missing_scope(*id, Capability::Read)));
                }
                chosen.push(
                    all.iter()
                        .find(|project| project.id == *id)
                        .cloned()
                        .ok_or_else(|| ApiError::NotFound(format!("Project {id} not found")))?,
                );
            }
            chosen
        }
        None => all
            .into_iter()
            .filter(|project| !project.archived && may(&principal, project.id, Capability::Read))
            .collect(),
    };
    // Work the board did not start belongs to no project: it is shown only to a caller that may
    // read them all and did not narrow the digest.
    let outside_board = requested.is_none() && principal.may_everywhere(Capability::Read);
    Ok(Json(
        tack_app::digest::read(&state.repo, &clock, &projects, since, outside_board).await?,
    ))
}
