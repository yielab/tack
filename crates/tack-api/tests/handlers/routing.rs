//! `GET /api/projects/{id}/routing`: who would do each open card, by the choice a run takes, and
//! the team or agent that choice runs in; and `children_progress` on the reads of a parent item.

use crate::common;
use axum::Router;
use axum::http::StatusCode;
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::{AppState, router::build_router};
use tack_core::models::{CreateItem, CreateProject, CreateRole, ProjectType};
use tack_db::repo::execution::NewAgentProfile;
use tack_protocol::execution::ExecutionState;
use tack_protocol::{Clock, SystemClock};
use tack_test_support::seed;
use uuid::Uuid;

const TOKEN: &str = "operator-token";

struct Board {
    app: Router,
    state: AppState,
    project: Uuid,
}

fn harness(kind: &str, teams: Option<Value>) -> Value {
    let mut harness = json!({
        "harness_kind": kind, "installed_version": "1.0", "probe_error": null,
        "probed_at": "2026-11-03T10:00:00Z",
    });
    if let Some(teams) = teams {
        harness["teams"] = teams;
    }
    harness
}

/// A board with one runner that reports `claude-code` and docket's team `fab-build`.
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
                code_origin: None,
                repository: None,
                default_branch: None,
                workspace_mode: None,
            },
        )
        .await
        .unwrap();
    tack_db::repo::execution::seed_builtin_profiles(&repo, chrono::Utc::now())
        .await
        .unwrap();
    seed::agent_runner_reporting(
        &repo,
        "ru1",
        &json!({"harnesses": [
            harness("claude-code", None),
            harness("docket", Some(json!([{"name": "fab-build", "profile": "attended", "violations": []}]))),
        ]})
        .to_string(),
    )
    .await;
    Board {
        app: common::with_default_host(build_router(state.clone())),
        state,
        project: project.id,
    }
}

impl Board {
    async fn profile(&self, id: &str, tool_policy: Value) {
        self.state
            .repo
            .create_agent_profile(
                NewAgentProfile {
                    id,
                    name: id,
                    instructions: "work safely",
                    tool_policy: &tool_policy.to_string(),
                    limits: "{}",
                },
                SystemClock.now(),
            )
            .await
            .unwrap();
    }

    async fn item(&self, title: &str, status: &str, parent: Option<Uuid>) -> Uuid {
        self.state
            .repo
            .create_item(
                self.project,
                status,
                CreateItem {
                    title: title.into(),
                    parent_id: parent,
                    ..CreateItem::default()
                },
            )
            .await
            .unwrap()
            .id
    }

    /// A role of the project, with `profile` as its profile when there is one, given to `item`.
    async fn role(&self, name: &str, profile: Option<&str>, item: Uuid) {
        let repo = &self.state.repo;
        let role = repo
            .create_role(
                self.project,
                CreateRole {
                    name: name.into(),
                    color: None,
                    icon: None,
                },
            )
            .await
            .unwrap();
        if let Some(profile) = profile {
            seed::role_agent_profile(repo, &role.id.to_string(), profile).await;
        }
        repo.assign_role_to_item(item, role.id).await.unwrap();
    }

    /// The profile chosen on the card itself.
    async fn choose(&self, item: Uuid, profile: &str) {
        let (status, body) = common::send(
            &self.app,
            "PATCH",
            &format!("/api/items/{item}"),
            json!({"run_settings": {"agent_profile_id": profile}}),
            &[("authorization", &format!("Bearer {TOKEN}"))],
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }

    async fn get(&self, uri: &str) -> Value {
        let (status, body) = common::send(
            &self.app,
            "GET",
            uri,
            Value::Null,
            &[("authorization", &format!("Bearer {TOKEN}"))],
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }

    /// The routing of the project, by item.
    async fn routing(&self) -> std::collections::HashMap<Uuid, Value> {
        self.get(&format!("/api/projects/{}/routing", self.project))
            .await
            .as_array()
            .unwrap()
            .iter()
            .map(|routed| {
                (
                    routed["item_id"].as_str().unwrap().parse().unwrap(),
                    routed.clone(),
                )
            })
            .collect()
    }
}

#[tokio::test]
async fn a_card_goes_where_a_run_would_and_a_team_is_one_lane_among_others() {
    let b = board().await;
    b.profile("solo", json!({"agent": "claude-code"})).await;
    b.profile(
        "builder",
        json!({"agent": "docket", "docket": {"team": "fab-build"}}),
    )
    .await;
    b.profile("other", json!({"agent": "claude-code"})).await;
    let by_role = b.item("one role", "To Do", None).await;
    b.role("Builder", Some("builder"), by_role).await;
    let disagree = b.item("two roles", "To Do", None).await;
    b.role("Alpha", Some("solo"), disagree).await;
    b.role("Beta", Some("other"), disagree).await;
    let bare = b.item("no role", "To Do", None).await;
    let chosen = b.item("chosen", "To Do", None).await;
    b.choose(chosen, "solo").await;
    let closed = b.item("closed", "Done", None).await;
    seed::project_default_profile(&b.state.repo, &b.project.to_string(), "solo").await;

    let routing = b.routing().await;

    let r = &routing[&by_role];
    assert_eq!(
        (
            r["profile_id"].as_str(),
            r["source"].as_str(),
            r["role"].as_str()
        ),
        (Some("builder"), Some("role:Builder"), Some("Builder"))
    );
    assert_eq!(
        (r["team"].as_str(), r["harness"].as_str(), &r["nobody"]),
        (Some("fab-build"), Some("docket"), &Value::Null),
        "{r}"
    );
    let r = &routing[&disagree];
    assert_eq!(
        (r["profile_id"].as_str(), r["source"].as_str()),
        (Some("solo"), Some("ambiguous")),
        "two roles that disagree fall back to the project's default"
    );
    assert_eq!(
        (r["team"].as_str(), r["harness"].as_str()),
        (None, Some("claude-code"))
    );
    let r = &routing[&bare];
    assert_eq!(
        (r["profile_id"].as_str(), r["source"].as_str()),
        (Some("solo"), Some("project_default"))
    );
    let r = &routing[&chosen];
    assert_eq!(
        (r["profile_id"].as_str(), r["source"].as_str()),
        (Some("solo"), Some("run_settings"))
    );
    assert!(!routing.contains_key(&closed), "a done card is not open");
}

#[tokio::test]
async fn a_card_with_nothing_named_goes_to_the_built_in_implementer() {
    let b = board().await;
    let bare = b.item("no role", "To Do", None).await;

    let r = b.routing().await.remove(&bare).unwrap();

    assert_eq!(r["source"], "implementer", "{r}");
    assert!(r["profile_id"].is_string(), "{r}");
}

#[tokio::test]
async fn nobody_takes_a_card_for_each_reason_with_its_facts() {
    let b = board().await;
    b.profile(
        "ghost",
        json!({"agent": "docket", "docket": {"team": "ghosts"}}),
    )
    .await;
    b.profile("codex", json!({"agent": "codex"})).await;
    let no_team = b.item("team nobody reports", "To Do", None).await;
    b.role("Ghostly", Some("ghost"), no_team).await;
    let no_harness = b.item("agent nobody reports", "To Do", None).await;
    b.role("Coder", Some("codex"), no_harness).await;
    let gone = b.item("profile gone", "To Do", None).await;
    b.choose(gone, "deleted").await;

    let routing = b.routing().await;

    let r = &routing[&no_team];
    assert_eq!(
        (
            r["nobody"].as_str(),
            r["team"].as_str(),
            r["profile_id"].as_str()
        ),
        (Some("team_not_reported"), Some("ghosts"), Some("ghost")),
        "{r}"
    );
    let r = &routing[&no_harness];
    assert_eq!(
        (r["nobody"].as_str(), r["harness"].as_str()),
        (Some("harness_not_reported"), Some("codex")),
        "{r}"
    );
    assert_eq!(routing[&gone]["nobody"], "profile_missing");

    // With no profile left on the board, a card says whether it lacked a role or its role lacked a profile.
    seed::agent_profiles_cleared(&b.state.repo).await;
    let bare = b.item("no role", "To Do", None).await;
    let stranded = b.item("role without profile", "To Do", None).await;
    b.role("Empty", None, stranded).await;
    let routing = b.routing().await;
    assert_eq!(routing[&bare]["nobody"], "no_role");
    let r = &routing[&stranded];
    assert_eq!(
        (r["nobody"].as_str(), r["role"].as_str()),
        (Some("role_without_profile"), Some("Empty")),
        "{r}"
    );
}

#[tokio::test]
async fn a_parent_counts_its_children_and_the_teams_their_latest_runs_went_to() {
    let b = board().await;
    let parent = b.item("parent", "To Do", None).await;
    let finished = b.item("finished", "Done", Some(parent)).await;
    let waiting = b.item("waiting", "To Do", Some(parent)).await;
    let lone = b.item("lone", "To Do", None).await;
    let repo = &b.state.repo;
    for (request, item, policy, state) in [
        (
            "req-1",
            finished,
            json!({"agent": "docket", "docket": {"team": "fab-build"}}),
            ExecutionState::Succeeded,
        ),
        (
            "req-2",
            waiting,
            json!({"agent": "claude-code"}),
            ExecutionState::Running,
        ),
    ] {
        seed::execution_request(
            repo,
            request,
            &item.to_string(),
            &format!("r-{request}"),
            "2026-11-03T10:00:00Z",
        )
        .await;
        seed::execution_request_snapshot(
            repo,
            request,
            &json!({"resolved_agent_profile": {"tool_policy": policy}}).to_string(),
        )
        .await;
        seed::execution_request_state(repo, request, state).await;
    }

    let detail = b.get(&format!("/api/items/{parent}")).await;
    assert_eq!(
        detail["item"]["children_progress"],
        json!({"total": 2, "done": 1, "teams": ["claude-code", "fab-build"]}),
        "{detail}"
    );
    let listed = b.get(&format!("/api/projects/{}/items", b.project)).await;
    let of = |id: Uuid| {
        listed["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|card| card["id"] == id.to_string())
            .unwrap()
            .clone()
    };
    assert_eq!(of(parent)["children_progress"]["total"], 2);
    assert!(
        of(lone).get("children_progress").is_none(),
        "an item without children carries none"
    );
    let tree = b
        .get(&format!("/api/projects/{}/items/tree", b.project))
        .await;
    let node = tree
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == parent.to_string())
        .unwrap();
    assert_eq!(node["children_progress"]["done"], 1, "{tree}");
}
