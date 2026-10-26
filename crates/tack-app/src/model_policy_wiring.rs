//! Live wiring between the pure `tack_orch::model_policy::resolve_model_policy` and a real
//! `projects` row.
//!
//! A model the request names is used as is; only a request naming none reads
//! the project's default model and default agent.

use tack_db::{Repository, StoreError};
use tack_orch::model_policy::{ModelPolicySources, ResolvedModelPolicy, resolve_model_policy};
use uuid::Uuid;

/// Resolves the model for a run of `run_agent` in `project_id`.
pub async fn resolve_request_model_policy(
    repo: &Repository,
    project_id: Uuid,
    run_agent: &str,
    request: Option<&str>,
) -> Result<ResolvedModelPolicy, StoreError> {
    let project = match request {
        Some(_) => None,
        None => repo
            .get_project(project_id)
            .await
            .map_err(StoreError::from)?,
    };
    Ok(resolve_model_policy(&ModelPolicySources {
        request,
        project_default_model: project.as_ref().and_then(|p| p.default_model.as_deref()),
        project_default_agent: project.as_ref().and_then(|p| p.default_harness.as_deref()),
        run_agent,
    }))
}
