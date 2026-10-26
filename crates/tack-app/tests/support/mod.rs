//! A board to start runs on: one project with a url repository, one item, one active runner and
//! one profile, on `setup_test_db()`.
#![allow(dead_code)]

use serde_json::{Value, json};
use tack_app::runs::start_run::{RunRequest, Started};
use tack_core::models::{CodeOrigin, CreateProject, ProjectType, UpdateProject, WorkspaceMode};
use tack_db::repo::execution::{NewAgentProfile, NewRunner};
use tack_db::{Repository, queries};
use tack_protocol::SystemClock;
use tack_test_support::{create_test_workspace, make_item, setup_test_db};
use uuid::Uuid;

pub const OPERATOR: &str = "operator-1";

pub struct Board {
    pub repo: Repository,
    pub project_id: Uuid,
    pub item_id: Uuid,
}

pub async fn board() -> Board {
    let repo = setup_test_db().await;
    let workspace = create_test_workspace(&repo).await;
    let project = repo
        .create_project(
            workspace,
            CreateProject {
                name: "C1".into(),
                description: None,
                project_type: ProjectType::Software,
                template: None,
                code_origin: Some(CodeOrigin::Url),
                repository: Some("https://example.test/c1.git".into()),
                default_branch: None,
                workspace_mode: None,
            },
        )
        .await
        .expect("project");
    let item = make_item(&repo, &project).await;
    add_runner(&repo, "runner-active").await;
    repo.create_agent_profile(
        NewAgentProfile {
            id: "profile-c1",
            name: "C1",
            instructions: "work safely",
            tool_policy: r#"{"mode":"safe"}"#,
            limits: r#"{"tokens":1000}"#,
        },
        &SystemClock,
    )
    .await
    .expect("profile");
    Board {
        repo,
        project_id: project.id,
        item_id: item.id,
    }
}

pub async fn add_runner(repo: &Repository, id: &str) {
    repo.register_runner(
        NewRunner {
            id,
            name: id,
            credential_hash: "hash-only",
            labels: "{}",
            total_capacity: 1,
            available_capacity: 1,
            capability_snapshot: "{}",
            protocol_version: 1,
        },
        &SystemClock,
    )
    .await
    .expect("runner");
}

pub async fn add_profile(repo: &Repository, id: &str, tool_policy: &str, limits: &str) {
    repo.create_agent_profile(
        NewAgentProfile {
            id,
            name: id,
            instructions: "x",
            tool_policy,
            limits,
        },
        &SystemClock,
    )
    .await
    .expect("profile");
}

/// A request naming everything, the way the CLI sends one.
pub fn full_request(item_id: Uuid) -> RunRequest {
    RunRequest {
        item_id,
        idempotency_key: "same-key".into(),
        selector_kind: Some("exact_runner".into()),
        selector_id: Some("runner-active".into()),
        agent_profile_id: Some("profile-c1".into()),
        requested_harness_kind: Some("codex".into()),
        requested_model: None,
        agent_profile_snapshot: None,
        repository_snapshot: Some(
            json!({"kind":"git","remote":"https://example.test/c1.git","base_revision":"abc123","subdirectory":null}),
        ),
        permission_policy: json!({"tools":["shell"],"network":false}),
        budgets: json!({"tokens":1000}),
        environment: json!({"MODE":{"value":"test","secret_reference":null}}),
        metadata: json!({"source":"c1-test"}),
        timeout_seconds: Some(60),
        status_map_policy_id: None,
        verify: None,
        push_branch: None,
        origin: None,
    }
}

/// A request that leaves the profile, the machine and the limits to the server.
pub fn bare_request(item_id: Uuid, key: &str) -> RunRequest {
    RunRequest {
        idempotency_key: key.into(),
        agent_profile_id: None,
        selector_kind: None,
        selector_id: None,
        ..full_request(item_id)
    }
}

pub async fn start(
    board: &Board,
    request: RunRequest,
) -> Result<Started, tack_app::runs::start_run::Refusal> {
    tack_app::runs::start_run::start_run(&board.repo, &SystemClock, OPERATOR, request).await
}

/// The snapshot a started run was stored with.
pub async fn snapshot(repo: &Repository, started: &Started) -> Value {
    let raw = queries::execution_requests::request_snapshot(repo, &started.request_id)
        .await
        .expect("read snapshot")
        .expect("request exists");
    serde_json::from_str(&raw).expect("snapshot json")
}

pub async fn set_project(repo: &Repository, project_id: Uuid, update: UpdateProject) {
    repo.update_project(project_id, update)
        .await
        .expect("update project")
        .expect("project exists");
}

pub fn code(origin: CodeOrigin, repository: Option<&str>, mode: WorkspaceMode) -> UpdateProject {
    UpdateProject {
        code_origin: Some(origin),
        repository: Some(repository.map(str::to_owned)),
        default_branch: Some(None),
        workspace_mode: Some(mode),
        push_after_run: Some(true),
        ..Default::default()
    }
}
