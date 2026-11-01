//! Agent credentials (ADR 0034, decision 2): list, issue, rotate, revoke. The operator's alone:
//! `agent_access` answers an agent `403` on every one of these. The secret is in the answer to
//! issue and rotate and nowhere else.

use axum::extract::State;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::instrument;
use utoipa::ToSchema;
use uuid::Uuid;

use tack_app::runs::agent_credential;
use tack_core::principal::{AgentCredential, Scope};

use crate::error::{ApiError, ApiResult};
use crate::extract::{Json, Path};
use crate::router::AppState;

/// What issues a credential.
#[derive(Debug, Deserialize, ToSchema)]
pub struct IssueCredential {
    /// What the agent's comments and changes are signed with.
    pub name: String,
    /// The projects and capabilities the credential holds.
    pub scopes: Vec<Scope>,
}

/// A credential together with its secret, which is shown this once and never again.
#[derive(Debug, Serialize, ToSchema)]
pub struct IssuedCredential {
    pub credential: AgentCredential,
    /// `tack_agent_...`; Tack keeps only its hash.
    pub secret: String,
}

impl From<agent_credential::Issued> for IssuedCredential {
    fn from(issued: agent_credential::Issued) -> Self {
        Self {
            credential: issued.credential,
            secret: issued.secret,
        }
    }
}

/// GET /api/agent-credentials
#[utoipa::path(
    get,
    path = "/api/agent-credentials",
    tag = "agent-credentials",
    responses((status = 200, description = "Every credential, the newest first; never a secret", body = Vec<AgentCredential>)),
)]
#[instrument(skip(state))]
pub async fn list_credentials(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<AgentCredential>>> {
    Ok(Json(state.repo.list_agent_credentials().await?))
}

/// POST /api/agent-credentials
#[utoipa::path(
    post,
    path = "/api/agent-credentials",
    tag = "agent-credentials",
    request_body = IssueCredential,
    responses(
        (status = 200, description = "Issued; the secret is in this answer only", body = IssuedCredential),
        (status = 400, description = "The name or a scope is refused", body = crate::openapi::ErrorEnvelope),
        (status = 409, description = "Another credential not revoked has that name", body = crate::openapi::ErrorEnvelope),
    ),
)]
#[instrument(skip(state, input))]
pub async fn issue_credential(
    State(state): State<AppState>,
    Json(input): Json<IssueCredential>,
) -> ApiResult<Json<IssuedCredential>> {
    let issued =
        agent_credential::issue(&state.repo, Utc::now(), &input.name, input.scopes).await?;
    Ok(Json(issued.into()))
}

/// POST /api/agent-credentials/{id}/rotate
#[utoipa::path(
    post,
    path = "/api/agent-credentials/{id}/rotate",
    tag = "agent-credentials",
    params(("id" = Uuid, Path, description = "Credential ID")),
    responses(
        (status = 200, description = "A new secret, valid ninety days; the old one stops working", body = IssuedCredential),
        (status = 404, description = "No such credential, or it was revoked", body = crate::openapi::ErrorEnvelope),
    ),
)]
#[instrument(skip(state))]
pub async fn rotate_credential(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<IssuedCredential>> {
    let issued = agent_credential::rotate(&state.repo, Utc::now(), id)
        .await
        .map_err(|error| match error {
            tack_app::AppError::NotFound => {
                ApiError::NotFound(format!("Credential {id} not found, or it was revoked"))
            }
            other => other.into(),
        })?;
    Ok(Json(issued.into()))
}

/// POST /api/agent-credentials/{id}/revoke
#[utoipa::path(
    post,
    path = "/api/agent-credentials/{id}/revoke",
    tag = "agent-credentials",
    params(("id" = Uuid, Path, description = "Credential ID")),
    responses(
        (status = 200, description = "Revoked for good", body = AgentCredential),
        (status = 404, description = "No such credential", body = crate::openapi::ErrorEnvelope),
    ),
)]
#[instrument(skip(state))]
pub async fn revoke_credential(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<AgentCredential>> {
    let credential = agent_credential::revoke(&state.repo, Utc::now(), id)
        .await
        .map_err(|error| match error {
            tack_app::AppError::NotFound => {
                ApiError::NotFound(format!("Credential {id} not found"))
            }
            other => other.into(),
        })?;
    Ok(Json(credential))
}
