//! The run Settings starts to check an agent: the harness once, on this machine, from an empty
//! scratch directory, with no remote and no typed model. It is filed under an archived
//! `Agent tests` project, created on first use so it never shows in the project list.

use serde_json::{Value, json};
use tack_core::models::{CreateItem, CreateProject, ProjectType, UpdateProject};
use tack_db::{Repository, queries};
use tack_protocol::Clock;
use uuid::Uuid;

use super::{LocalRunnerControl, RuntimeState};
use crate::runs::start_run::{Refusal, RunRequest, enqueue_run};

const PROJECT_NAME: &str = "Agent tests";
/// Who the test run is filed as; its idempotency keys never meet another caller's.
const PRINCIPAL: &str = "local-runner-test";
const TIMEOUT_SECONDS: u64 = 300;

#[derive(Debug, thiserror::Error)]
pub enum TestRunError {
    #[error("The embedded runner is not configured")]
    NotConfigured,
    #[error("The embedded runner is not running")]
    NotRunning,
    #[error("The embedded runner is not enrolled")]
    NotEnrolled,
    #[error("The Implementer profile does not exist")]
    NoImplementerProfile,
    /// A store or board failure, with the sentence that says which step.
    #[error("{0}")]
    Internal(&'static str),
    #[error("{}", .0.message)]
    Refused(Refusal),
}

/// Maps any failure of a step to [`TestRunError::Internal`] with the sentence that names it.
fn internal<E>(message: &'static str) -> impl FnOnce(E) -> TestRunError + Copy {
    move |_| TestRunError::Internal(message)
}

/// Queues the test run and returns the new request's id.
pub async fn start_test_run(
    repo: &Repository,
    clock: &dyn Clock,
    control: Option<&dyn LocalRunnerControl>,
    workspace_id: Uuid,
    harness_kind: String,
) -> Result<String, TestRunError> {
    let control = control.ok_or(TestRunError::NotConfigured)?;
    if control.status().await.state != RuntimeState::Running {
        return Err(TestRunError::NotRunning);
    }
    let runner_id = control.runner_id().await.ok_or(TestRunError::NotEnrolled)?;
    let lookup = internal("Could not look up the Implementer profile");
    let profile_id = queries::agent_profiles::implementer_profile_id(repo)
        .await
        .map_err(lookup)?
        .ok_or(TestRunError::NoImplementerProfile)?;
    let profile = queries::agent_profiles::profile_by_id(repo, &profile_id)
        .await
        .map_err(lookup)?
        .ok_or(TestRunError::NoImplementerProfile)?;

    let project = test_project(repo, workspace_id).await?;
    let initial_status = project
        .workflow
        .initial_status()
        .map_err(internal("The test project has no initial status"))?;
    let now = clock.now();
    let item = repo
        .create_item(
            project.id,
            &initial_status,
            CreateItem {
                title: format!("Agent test {}", now.format("%Y-%m-%d %H:%M:%S")),
                ..Default::default()
            },
        )
        .await
        .map_err(internal("Could not create the test item"))?;

    let parse = |raw: &str| serde_json::from_str::<Value>(raw).unwrap_or_else(|_| json!({}));
    let tool_policy = parse(&profile.tool_policy);
    // The profile's tools for this harness, as the Run dialog sends them; none would leave the
    // agent unable to act.
    let tools = tool_policy["tools"][harness_kind.as_str()].clone();
    let tools = if tools.is_array() { tools } else { json!([]) };
    let request = RunRequest {
        item_id: item.id,
        idempotency_key: Uuid::new_v4().to_string(),
        selector_kind: Some("exact_runner".to_owned()),
        selector_id: Some(runner_id),
        agent_profile_id: Some(profile.id),
        requested_harness_kind: Some(harness_kind),
        requested_model: None,
        agent_profile_snapshot: None,
        repository_snapshot: None,
        permission_policy: json!({ "tools": tools, "network": false }),
        budgets: json!({}),
        environment: json!({}),
        metadata: json!({}),
        timeout_seconds: Some(TIMEOUT_SECONDS),
        status_map_policy_id: None,
        verify: None,
        push_branch: None,
        autonomy: None,
        origin: None,
    };
    let resolved_profile = json!({
        "name": profile.name,
        "instructions": profile.instructions,
        "tool_policy": tool_policy,
        "timeout_seconds": TIMEOUT_SECONDS,
        "budgets": parse(&profile.limits),
    });
    let repository = json!({
        "kind": "scratch", "remote": "", "base_revision": "", "subdirectory": null,
    });
    enqueue_run(
        repo,
        clock,
        PRINCIPAL,
        request,
        resolved_profile,
        repository,
    )
    .await
    .map(|started| started.request_id)
    .map_err(TestRunError::Refused)
}

/// The archived project test runs are filed under, created and archived on first use.
async fn test_project(
    repo: &Repository,
    workspace_id: Uuid,
) -> Result<tack_core::models::Project, TestRunError> {
    let existing = queries::projects::project_id_named(repo, workspace_id, PROJECT_NAME)
        .await
        .map_err(internal("Could not look up the test project"))?;
    if let Some(id) = existing.and_then(|id| id.parse::<Uuid>().ok()) {
        const MESSAGE: &str = "Could not load the test project";
        return repo
            .get_project(id)
            .await
            .map_err(internal(MESSAGE))?
            .ok_or(TestRunError::Internal(MESSAGE));
    }
    let created = repo
        .create_project(
            workspace_id,
            CreateProject {
                name: PROJECT_NAME.to_owned(),
                description: Some("Runs started from Settings to check an agent".to_owned()),
                project_type: ProjectType::Software,
                template: None,
                code_origin: None,
                repository: None,
                default_branch: None,
                workspace_mode: None,
            },
        )
        .await
        .map_err(internal("Could not create the test project"))?;
    repo.update_project(
        created.id,
        UpdateProject {
            archived: Some(true),
            ..Default::default()
        },
    )
    .await
    .map_err(internal("Could not archive the test project"))?;
    Ok(created)
}
