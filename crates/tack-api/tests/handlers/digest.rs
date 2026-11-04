//! `GET /api/digest` (ADR 0034, decision 6): a bounded summary of the projects a caller may
//! read. Each section is read from the rows it names, `since` bounds the failed runs, and an
//! agent credential sees only the projects its scopes name.

use crate::common;
use axum::Router;
use axum::http::StatusCode;
use chrono::{DateTime, TimeZone, Utc};
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::{AppState, router::build_router};
use tack_core::models::{
    CreateDependency, CreateItem, CreateProject, CreateRole, DependencyType, ProjectType,
};
use tack_db::repo::budgets::ProjectBudgets;
use tack_db::repo::team_activity::TeamTaskReport;
use tack_protocol::Clock;
use tack_protocol::execution::ExecutionState;
use tack_test_support::seed;
use uuid::Uuid;

const TOKEN: &str = "operator-token";

/// The instant the seeded requests' snapshots say they were made at.
struct SeedClock;

impl Clock for SeedClock {
    fn now(&self) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 12, 12, 0, 0).unwrap()
    }
}

struct Board {
    app: Router,
    state: AppState,
}

async fn board() -> Board {
    let repo = tack_test_support::setup_test_db().await;
    let workspace = tack_test_support::create_test_workspace(&repo).await;
    let (tx, _rx) = tokio::sync::broadcast::channel(16);
    let state = AppState {
        notifications: crate::common::inbox(&repo),
        repo,
        config: AppConfig {
            api_token: Some(TOKEN.into()),
            database_url: "sqlite::memory:".into(),
            ..AppConfig::default()
        },
        workspace_id: workspace,
        broadcast_tx: tx,
        webhook: None,
        local_runner: None,
        tasks: Default::default(),
        event_bus: Default::default(),
    };
    seed::runner_with_profile(&state.repo, &SeedClock, "ru-1", "pr-1", "hash", 20).await;
    Board {
        app: common::with_default_host(build_router(state.clone())),
        state,
    }
}

impl Board {
    async fn project(&self, name: &str) -> Uuid {
        self.state
            .repo
            .create_project(
                self.state.workspace_id,
                CreateProject {
                    name: name.into(),
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
            .unwrap()
            .id
    }

    async fn item(&self, project: Uuid, title: &str, status: &str) -> Uuid {
        self.state
            .repo
            .create_item(
                project,
                status,
                CreateItem {
                    title: title.into(),
                    ..CreateItem::default()
                },
            )
            .await
            .unwrap()
            .id
    }

    /// A request of `item`, left in `state` and last changed at `at`.
    async fn run(&self, item: Uuid, tag: &str, state: ExecutionState, at: &str) {
        let repo = &self.state.repo;
        seed::running_attempt(repo, &SeedClock, "ru-1", "pr-1", &item.to_string(), tag).await;
        seed::execution_request_state(repo, &format!("req-{tag}"), state).await;
        seed::execution_request_updated_at(repo, &format!("req-{tag}"), at).await;
    }

    async fn task(&self, team: &str, task: &str, on_board: Option<Uuid>, state: &str) {
        let on_board = on_board.map(|id| id.to_string());
        self.state
            .repo
            .record_team_task(&TeamTaskReport {
                runner_id: "ru-team",
                harness: "docket",
                team,
                task_ref: task,
                on_board: on_board.as_deref(),
                state,
                waits_on: None,
                current_step: Some(r#"{"name":"implement","role":"lead"}"#),
                changed_at: None,
                reported_at: "2026-10-30T12:00:00+00:00",
                steps: Vec::new(),
            })
            .await
            .unwrap();
    }

    async fn get(&self, bearer: &str, query: &str) -> (StatusCode, Value) {
        common::send(
            &self.app,
            "GET",
            &format!("/api/digest{query}"),
            Value::Null,
            &[("authorization", &format!("Bearer {bearer}"))],
        )
        .await
    }

    async fn operator(&self, query: &str) -> Value {
        let (status, body) = self.get(TOKEN, query).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }

    async fn credential(&self, name: &str, projects: &[Uuid], capabilities: &[&str]) -> String {
        let ids: Vec<String> = projects.iter().map(Uuid::to_string).collect();
        let (status, issued) = common::send(
            &self.app,
            "POST",
            "/api/agent-credentials",
            json!({"name": name, "scopes": [{"projects": ids, "capabilities": capabilities}]}),
            &[("authorization", &format!("Bearer {TOKEN}"))],
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{issued}");
        issued["secret"].as_str().unwrap().to_owned()
    }
}

fn project_of<'a>(digest: &'a Value, name: &str) -> &'a Value {
    digest["projects"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|project| project["name"] == name)
        .unwrap_or_else(|| panic!("no project {name} in {digest}"))
}

fn titles(section: &Value, path: &[&str]) -> Vec<String> {
    section["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let mut value = row;
            for key in path {
                value = &value[*key];
            }
            value.as_str().unwrap().to_owned()
        })
        .collect()
}

#[tokio::test]
async fn an_agent_credential_sees_only_its_projects() {
    let b = board().await;
    let mine = b.project("My SaaS").await;
    let other = b.project("My SaaS docs").await;
    let mine_item = b.item(mine, "Build the sign-up form", "To Do").await;
    let other_item = b.item(other, "Write the install page", "To Do").await;
    b.task("builders", "sign-up", Some(mine_item), "running")
        .await;
    b.task("writers", "install-page", Some(other_item), "running")
        .await;
    b.task("ops", "from-the-cli", None, "running").await;

    // The operator reads both projects, every team and the work the board did not start.
    let all = b.operator("").await;
    assert_eq!(all["projects"]["total"], 2);
    assert_eq!(all["teams"]["total"], 3);
    // Narrowed to one project, only that project's work.
    let narrowed = b.operator(&format!("?projects={mine}")).await;
    assert_eq!(narrowed["projects"]["total"], 1);
    assert_eq!(titles(&narrowed["teams"], &["team"]), ["builders"]);

    let secret = b.credential("Reader", &[mine], &["read"]).await;
    let (status, seen) = b.get(&secret, "").await;
    assert_eq!(status, StatusCode::OK, "{seen}");
    assert_eq!(titles(&seen["projects"], &["name"]), ["My SaaS"]);
    assert_eq!(titles(&seen["teams"], &["team"]), ["builders"]);

    // Asking for a project it may not read names the scope it lacks, and says nothing else.
    let (status, refused) = b.get(&secret, &format!("?projects={other}")).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");
    let message = refused["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("`read`") && message.contains(&other.to_string()),
        "{message}"
    );
    // One it may and one it may not: refused as a whole.
    let (status, _) = b.get(&secret, &format!("?projects={mine},{other}")).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // A credential that cannot read anything sees an empty digest, not another's.
    let nothing = b.credential("Proposer", &[mine], &["propose"]).await;
    let (status, empty) = b.get(&nothing, "").await;
    assert_eq!(status, StatusCode::OK, "{empty}");
    assert_eq!(empty["projects"]["total"], 0);
    assert_eq!(empty["teams"]["total"], 0);
}

#[tokio::test]
async fn a_bad_project_list_or_instant_is_refused() {
    let b = board().await;
    let (status, _) = b.get(TOKEN, "?projects=not-an-id").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = b.get(TOKEN, "?since=yesterday").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = b.get(TOKEN, &format!("?projects={}", Uuid::new_v4())).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn items_blocked_and_stale_are_read_from_the_board() {
    let b = board().await;
    let project = b.project("My SaaS").await;
    let design = b.item(project, "Design the sign-up flow", "To Do").await;
    let form = b
        .item(project, "Build the sign-up form", "In Progress")
        .await;
    let _done = b.item(project, "Set up the repository", "Done").await;
    b.state
        .repo
        .create_dependency(
            design,
            CreateDependency {
                target_item_id: form,
                dependency_type: DependencyType::Blocks,
            },
        )
        .await
        .unwrap();
    seed::item_updated_at(
        &b.state.repo,
        &form.to_string(),
        "2026-01-01T09:00:00+00:00",
    )
    .await;

    let digest = b.operator("").await;
    let project = project_of(&digest, "My SaaS");
    assert_eq!(project["items_total"], 3);
    let statuses: Vec<(String, u64)> = project["items_by_status"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row["status"].as_str().unwrap().to_owned(),
                row["count"].as_u64().unwrap(),
            )
        })
        .collect();
    assert!(
        statuses.contains(&("In Progress".to_owned(), 1)),
        "{statuses:?}"
    );
    assert!(statuses.contains(&("Done".to_owned(), 1)), "{statuses:?}");

    assert_eq!(project["blocked"]["items"], 1);
    let chain = &project["blocked"]["chains"]["rows"][0];
    assert_eq!(chain["blocker"]["title"], "Design the sign-up flow");
    assert_eq!(chain["blocks"], 1);

    assert_eq!(
        titles(&project["stale"], &["item", "title"]),
        ["Build the sign-up form"]
    );
}

#[tokio::test]
async fn runs_waiting_on_a_person_come_with_since_when() {
    let b = board().await;
    let project = b.project("My SaaS").await;
    let item = b.item(project, "Add billing webhooks", "In Progress").await;
    b.run(
        item,
        "a",
        ExecutionState::WaitingDecision,
        "2026-10-28T15:00:00+00:00",
    )
    .await;

    let digest = b.operator("").await;
    let waiting = &project_of(&digest, "My SaaS")["waiting_on_person"];
    assert_eq!(waiting["total"], 1);
    assert_eq!(waiting["rows"][0]["item"]["title"], "Add billing webhooks");
    assert_eq!(waiting["rows"][0]["kind"], "decision");
    assert!(waiting["rows"][0]["since"].as_str().is_some());
}

#[tokio::test]
async fn since_bounds_the_failed_runs() {
    let b = board().await;
    let project = b.project("My SaaS").await;
    let early = b
        .item(project, "Build the sign-up form", "In Progress")
        .await;
    let late = b
        .item(project, "Send the welcome email", "In Progress")
        .await;
    b.run(
        early,
        "a",
        ExecutionState::Failed,
        "2026-10-05T09:00:00+00:00",
    )
    .await;
    b.run(
        late,
        "b",
        ExecutionState::Failed,
        "2026-10-29T18:00:00+00:00",
    )
    .await;

    let failed = |digest: &Value| {
        titles(
            &project_of(digest, "My SaaS")["failed_runs"],
            &["item", "title"],
        )
    };
    let digest = b.operator("?since=2026-10-20T00:00:00Z").await;
    assert_eq!(failed(&digest), ["Send the welcome email"]);
    assert_eq!(digest["since"], "2026-10-20T00:00:00Z");

    let digest = b.operator("?since=2026-10-01T00:00:00Z").await;
    assert_eq!(
        failed(&digest),
        ["Send the welcome email", "Build the sign-up form"]
    );

    let digest = b.operator("?since=2026-10-30T00:00:00Z").await;
    assert!(failed(&digest).is_empty());
}

#[tokio::test]
async fn a_task_nobody_will_pick_up_says_why() {
    let b = board().await;
    let project = b.project("My SaaS").await;
    seed::project_auto_start(&b.state.repo, &project.to_string(), "To Do", "codex").await;
    let item = b.item(project, "Write the launch post", "To Do").await;
    // It has a role and no one chosen to do it.
    let role = b
        .state
        .repo
        .create_role(
            project,
            CreateRole {
                name: "Writer".into(),
                color: None,
                icon: None,
            },
        )
        .await
        .unwrap();
    b.state
        .repo
        .assign_role_to_item(item, role.id)
        .await
        .unwrap();

    let digest = b.operator("").await;
    let unclaimed = &project_of(&digest, "My SaaS")["unclaimed"];
    assert_eq!(unclaimed["auto_start"], true);
    assert_eq!(
        unclaimed["tasks"]["rows"][0]["item"]["title"],
        "Write the launch post"
    );
    assert_eq!(unclaimed["tasks"]["rows"][0]["code"], "no_profile");
    assert_eq!(
        unclaimed["by_reason"],
        json!([{"code": "no_profile", "count": 1}])
    );
}

#[tokio::test]
async fn tokens_are_set_against_the_projects_limits() {
    let b = board().await;
    let project = b.project("My SaaS").await;
    b.state
        .repo
        .put_project_budgets(
            &project.to_string(),
            &ProjectBudgets {
                tokens_per_day: Some(1_000_000),
                auto_merges_per_day: None,
                queue_items_max: None,
                provisional_tokens_per_run: Some(250_000),
            },
        )
        .await
        .unwrap();

    let digest = b.operator("").await;
    let tokens = &project_of(&digest, "My SaaS")["tokens"];
    assert_eq!(tokens["limit_per_day"], 1_000_000);
    assert_eq!(tokens["left_today"], 1_000_000);
    assert_eq!(tokens["used_today"], 0);
    assert_eq!(tokens["per_run_limit"], 250_000);
}

#[tokio::test]
async fn a_change_set_waiting_for_a_person_is_listed() {
    let b = board().await;
    let project = b.project("My SaaS").await;
    let secret = b
        .credential("Planner", &[project], &["read", "propose"])
        .await;
    let (status, proposed) = common::send(
        &b.app,
        "POST",
        &format!("/api/projects/{project}/change-sets"),
        json!({
            "reason": "The launch needs a checklist",
            "operations": [{"op": "create", "title": "Write the checklist", "acceptance": ["done"]}],
        }),
        &[("authorization", &format!("Bearer {secret}"))],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{proposed}");

    let (_, digest) = b.get(&secret, "").await;
    let sets = &project_of(&digest, "My SaaS")["change_sets"];
    assert_eq!(sets["total"], 1);
    assert_eq!(sets["rows"][0]["reason"], "The launch needs a checklist");
    assert_eq!(sets["rows"][0]["principal"], "Planner");
    assert_eq!(sets["rows"][0]["operations"], 1);
}

#[tokio::test]
async fn a_team_shows_its_current_work_and_its_queue() {
    let b = board().await;
    let project = b.project("My SaaS").await;
    let item = b.item(project, "Add billing webhooks", "In Progress").await;
    b.task("builders", "billing-webhooks", Some(item), "running")
        .await;
    b.task("builders", "invoice-rounding", None, "queued").await;
    b.task("builders", "finished-job", None, "done").await;

    let digest = b.operator("").await;
    let team = &digest["teams"]["rows"][0];
    assert_eq!(team["team"], "builders");
    assert_eq!(
        (team["running"].as_u64(), team["queued"].as_u64()),
        (Some(1), Some(1))
    );
    assert_eq!(team["tasks"]["total"], 2, "the finished task is not work");
    let first = &team["tasks"]["rows"][0];
    assert_eq!(first["task"], "billing-webhooks");
    assert_eq!(first["item"]["title"], "Add billing webhooks");
    assert_eq!(first["step"]["name"], "implement");
    assert_eq!(first["project"], project.to_string());
}
