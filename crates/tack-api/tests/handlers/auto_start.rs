//! `GET /api/projects/{id}/auto-start`: why a task that the project would start by itself is
//! still waiting. An unfinished blocker, the cap on open runs and the day's tokens each name
//! themselves, a full In Progress column is not a reason; a task the next pass would start is not listed.

use crate::common;
use axum::Router;
use axum::http::StatusCode;
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::{AppState, router::build_router};
use tack_core::models::{
    CreateDependency, CreateItem, CreateProject, CreateRole, DependencyType, ProjectType,
};
use tack_db::repo::execution::{NewAgentProfile, NewRunner};
use tack_protocol::execution::ExecutionState;
use tack_protocol::{Clock, SystemClock};
use tack_test_support::probe::execution_requests_of_item;
use tack_test_support::seed;
use uuid::Uuid;

const TOKEN: &str = "operator-token";

struct Board {
    app: Router,
    state: AppState,
    project: Uuid,
}

async fn board() -> Board {
    let repo = tack_test_support::setup_test_db().await;
    let workspace = tack_test_support::create_test_workspace(&repo).await;
    let (tx, _rx) = tokio::sync::broadcast::channel(16);
    let state = AppState {
        notifications: crate::common::inbox(&repo.clone()),
        repo: repo.clone(),
        config: AppConfig {
            api_token: Some(TOKEN.into()),
            database_url: "sqlite::memory:".into(),
            ..AppConfig::default()
        },
        workspace_id: workspace,
        broadcast_tx: tx,
        event_bus: Default::default(),
        local_runner: None,
        tasks: Default::default(),
    };
    let project = repo
        .create_project(
            workspace,
            CreateProject {
                name: "My SaaS".into(),
                description: None,
                project_type: ProjectType::Software,
                template: None,
                code_origin: Some(tack_core::models::CodeOrigin::Url),
                repository: Some("https://example.test/my-saas.git".into()),
                default_branch: None,
                workspace_mode: None,
            },
        )
        .await
        .unwrap();
    tack_test_support::seed::register_runner(
        &repo,
        NewRunner {
            id: "runner-1",
            name: "Runner",
            credential_hash: "hash",
            labels: "{}",
            total_capacity: 1,
            available_capacity: 1,
            capability_snapshot: "{}",
            protocol_version: 1,
        },
        SystemClock.now(),
    )
    .await
    .unwrap();
    tack_test_support::seed::create_agent_profile(
        &repo,
        NewAgentProfile {
            id: "profile-1",
            name: "Writer",
            instructions: "work safely",
            tool_policy: r#"{"mode":"safe"}"#,
            limits: "{}",
        },
        SystemClock.now(),
    )
    .await
    .unwrap();
    let id = project.id.to_string();
    seed::project_auto_start(&repo, &id, "To Do", "codex").await;
    seed::project_auto_start_max_open(&repo, &id, 10).await;
    Board {
        app: common::with_default_host(build_router(state.clone())),
        state,
        project: project.id,
    }
}

impl Board {
    /// A task in `status`, done by a role whose profile is `profile` when there is one.
    async fn task(&self, title: &str, status: &str, profile: Option<&str>) -> Uuid {
        let repo = &self.state.repo;
        let item = repo
            .create_item(
                self.project,
                status,
                CreateItem {
                    title: title.into(),
                    ..CreateItem::default()
                },
            )
            .await
            .unwrap();
        let role = repo
            .create_role(
                self.project,
                CreateRole {
                    name: format!("Role of {title}"),
                    color: None,
                    icon: None,
                },
            )
            .await
            .unwrap();
        if let Some(profile) = profile {
            seed::role_agent_profile(repo, &role.id.to_string(), profile).await;
        }
        repo.assign_role_to_item(item.id, role.id).await.unwrap();
        item.id
    }

    async fn auto_start(&self) -> Value {
        let (status, body) = common::send(
            &self.app,
            "GET",
            &format!("/api/projects/{}/auto-start", self.project),
            Value::Null,
            &[("authorization", &format!("Bearer {TOKEN}"))],
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }

    async fn tick(&self) {
        tack_app::auto_start::tick(&self.state.repo, &SystemClock)
            .await
            .unwrap();
    }
}

/// The (item, code) pairs of an answer.
fn codes(body: &Value) -> Vec<(String, String)> {
    body["waiting"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| {
            (
                w["item_id"].as_str().unwrap().to_owned(),
                w["why"]["code"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

#[tokio::test]
async fn a_full_in_progress_column_does_not_hold_a_ready_task_back() {
    let b = board().await;
    for n in 0..5 {
        b.task(&format!("busy {n}"), "In Progress", Some("profile-1"))
            .await;
    }
    let ready = b.task("ready", "To Do", Some("profile-1")).await;

    let body = b.auto_start().await;

    assert_eq!(body["enabled"], true);
    assert_eq!(body["status"], "To Do");
    assert_eq!(body["waiting"], json!([]), "nothing waits on a column");

    b.tick().await;

    // It started, and it is still where it was: the board moved nothing into the full column.
    let requests = execution_requests_of_item(&b.state.repo, &ready.to_string())
        .await
        .unwrap();
    assert_eq!(requests.len(), 1);
    let item = b.state.repo.get_item(ready).await.unwrap().unwrap();
    assert_eq!(item.status, "To Do");
}

#[tokio::test]
async fn a_task_that_has_run_is_not_listed_or_started_again() {
    let b = board().await;
    let ran = b.task("ran", "To Do", Some("profile-1")).await;
    b.tick().await;
    let request = execution_requests_of_item(&b.state.repo, &ran.to_string())
        .await
        .unwrap()
        .remove(0);
    seed::execution_request_state(&b.state.repo, &request.id, ExecutionState::Failed).await;

    let body = b.auto_start().await;
    b.tick().await;

    assert_eq!(body["waiting"], json!([]), "{body}");
    let requests = execution_requests_of_item(&b.state.repo, &ran.to_string())
        .await
        .unwrap();
    assert_eq!(requests.len(), 1);
}

#[tokio::test]
async fn an_unfinished_blocker_is_named_and_a_task_that_would_start_is_not_listed() {
    let b = board().await;
    let blocker = b.task("blocker", "Backlog", Some("profile-1")).await;
    let blocked = b.task("blocked", "To Do", Some("profile-1")).await;
    let ready = b.task("ready", "To Do", Some("profile-1")).await;
    b.state
        .repo
        .create_dependency(
            blocker,
            CreateDependency {
                target_item_id: blocked,
                dependency_type: DependencyType::Blocks,
            },
        )
        .await
        .unwrap();

    let body = b.auto_start().await;

    assert_eq!(codes(&body), [(blocked.to_string(), "blocked".to_owned())]);
    assert_eq!(
        body["waiting"][0]["why"]["text"],
        "Blocked by 1 unfinished task"
    );
    assert!(!codes(&body).iter().any(|(id, _)| *id == ready.to_string()));
}

#[tokio::test]
async fn a_task_with_no_one_chosen_to_do_it_says_so() {
    let b = board().await;
    let nobody = b.task("nobody", "To Do", None).await;
    let body = b.auto_start().await;
    assert_eq!(
        codes(&body),
        [(nobody.to_string(), "no_profile".to_owned())]
    );
    assert_eq!(
        body["waiting"][0]["why"]["text"],
        "No one is chosen to do it"
    );
}

#[tokio::test]
async fn the_cap_on_open_runs_is_named_once_a_run_is_open() {
    let b = board().await;
    seed::project_auto_start_max_open(&b.state.repo, &b.project.to_string(), 1).await;
    b.task("first", "To Do", Some("profile-1")).await;
    b.task("second", "To Do", Some("profile-1")).await;
    // Both are ready, but the cap leaves room for one: the other would wait its turn.
    let before = b.auto_start().await;
    assert_eq!(codes(&before).len(), 1, "{before}");
    assert_eq!(codes(&before)[0].1, "max_open");
    assert_eq!(
        before["waiting"][0]["why"]["text"],
        "This project allows 1 run open at once, and the tasks ahead of this one take the places left"
    );

    b.tick().await;

    let body = b.auto_start().await;
    assert_eq!(codes(&body).len(), 1, "{body}");
    assert_eq!(codes(&body)[0].1, "max_open");
    assert_eq!(
        body["waiting"][0]["why"]["text"],
        "1 run is already open, the most this project allows"
    );
}

#[tokio::test]
async fn a_reached_token_limit_is_named() {
    let b = board().await;
    seed::project_budgets(&b.state.repo, &b.project.to_string(), Some(0), None).await;
    let waiting = b.task("ready", "To Do", Some("profile-1")).await;

    let body = b.auto_start().await;

    assert_eq!(codes(&body), [(waiting.to_string(), "budget".to_owned())]);
    assert_eq!(
        body["waiting"][0]["why"]["text"],
        "This project's token limit is reached"
    );
}

/// A profile whose agent reports no usage while it runs (Codex) is left to a person once its last
/// run went past the limit; the same history does not hold back an agent that is stopped at it.
#[tokio::test]
async fn a_profile_whose_unmeasured_agent_went_over_its_limit_is_not_started() {
    for (harness, started) in [("codex", 0), ("claude-code", 1)] {
        let b = board().await;
        seed::project_auto_start(&b.state.repo, &b.project.to_string(), "To Do", harness).await;
        let ran = b.task("ran", "To Do", Some("profile-1")).await;
        b.tick().await;
        let request = execution_requests_of_item(&b.state.repo, &ran.to_string())
            .await
            .unwrap()
            .remove(0);
        seed::execution_request_token_limit(&b.state.repo, &request.id, 1_000, false).await;
        seed::execution_attempt(
            &b.state.repo,
            seed::AttemptSeed {
                id: "attempt-over",
                request_id: &request.id,
                runner_id: "runner-1",
                state: ExecutionState::Failed,
                at: "2026-11-07T10:00:00+00:00",
                usage: Some(r#"{"tokens_in":{"value":5000,"source":"measured"},"tokens_out":{"value":0,"source":"measured"}}"#),
                actual_execution: None,
            },
        )
        .await;
        seed::execution_request_state(&b.state.repo, &request.id, ExecutionState::Failed).await;
        let next = b.task("next", "To Do", Some("profile-1")).await;

        let body = b.auto_start().await;
        b.tick().await;

        let requests = execution_requests_of_item(&b.state.repo, &next.to_string())
            .await
            .unwrap();
        assert_eq!(requests.len(), started, "{harness}");
        if started == 0 {
            assert_eq!(codes(&body), [(next.to_string(), "went_over".to_owned())]);
        } else {
            assert_eq!(body["waiting"], json!([]), "{harness}");
        }
    }
}

#[tokio::test]
async fn a_project_that_does_not_start_tasks_answers_off_with_nothing_listed() {
    let b = board().await;
    b.task("ready", "To Do", Some("profile-1")).await;
    let (status, _) = common::send(
        &b.app,
        "PATCH",
        &format!("/api/projects/{}", b.project),
        json!({"auto_start_enabled": false}),
        &[("authorization", &format!("Bearer {TOKEN}"))],
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let body = b.auto_start().await;

    assert_eq!(body["enabled"], false);
    assert_eq!(body["waiting"], json!([]));
}

#[tokio::test]
async fn an_agent_with_read_on_the_project_may_ask_and_one_without_may_not() {
    let b = board().await;
    let operator = [("authorization", format!("Bearer {TOKEN}"))];
    let operator = [(operator[0].0, operator[0].1.as_str())];
    let issue = |name: &'static str, project: Uuid| {
        let app = b.app.clone();
        async move {
            let (status, issued) = common::send(
                &app,
                "POST",
                "/api/agent-credentials",
                json!({"name": name, "scopes": [{"projects": [project], "capabilities": ["read"]}]}),
                &operator,
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{issued}");
            issued["secret"].as_str().unwrap().to_owned()
        }
    };
    let uri = format!("/api/projects/{}/auto-start", b.project);
    let secret = issue("Reader", b.project).await;
    let (status, body) = common::send(
        &b.app,
        "GET",
        &uri,
        Value::Null,
        &[("authorization", &format!("Bearer {secret}"))],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let elsewhere = b
        .state
        .repo
        .create_project(
            b.state.workspace_id,
            CreateProject {
                name: "Elsewhere".into(),
                description: None,
                project_type: ProjectType::Software,
                template: None,
                code_origin: None,
                repository: None,
                default_branch: None,
                workspace_mode: None,
            },
        )
        .await
        .unwrap();
    let other = issue("Outsider", elsewhere.id).await;
    let (status, _) = common::send(
        &b.app,
        "GET",
        &uri,
        Value::Null,
        &[("authorization", &format!("Bearer {other}"))],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}
