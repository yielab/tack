//! Applying the repository's teams from the board: the route needs a runner that reports a docket
//! listing `podShape`, asks that runner for each team, shows the commands docket asks a person to
//! read, and sends the person's code back with the same team.

use crate::common;
use axum::Router;
use axum::http::StatusCode;
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::{AppState, router::build_router};
use tack_core::models::{CodeOrigin, CreateProject, ProjectType};
use tack_protocol::execution::{OperationOutcome, TeamOperation};
use tack_test_support::seed;
use uuid::Uuid;

const TOKEN: &str = "operator-token";

struct Board {
    app: Router,
    project: Uuid,
}

fn docket(features: Value) -> Value {
    json!({"harnesses": [{
        "harness_kind": "docket", "installed_version": "0.3", "probe_error": null,
        "probed_at": "2026-11-04T10:00:00Z", "api_features": features,
    }]})
}

/// A board with a project whose code is the folder `/work/saas`, and one runner `runner` whose
/// report is `snapshot`.
async fn board(runner: &str, snapshot: Value, origin: CodeOrigin) -> Board {
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
    let folder = origin != CodeOrigin::None;
    let project = repo
        .create_project(
            workspace,
            CreateProject {
                name: "My SaaS".into(),
                description: None,
                project_type: ProjectType::Software,
                template: None,
                code_origin: folder.then_some(origin),
                repository: folder.then(|| "/work/saas".to_owned()),
                default_branch: None,
                workspace_mode: None,
            },
        )
        .await
        .unwrap();
    seed::agent_runner_reporting(&repo, runner, &snapshot.to_string()).await;
    // Another runner that reports a docket without the feature must not satisfy the check.
    let old = format!("{runner}-old");
    seed::agent_runner(&repo, &old, "now").await;
    seed::agent_runner_capabilities(&repo, &old, &docket(json!([])).to_string()).await;
    Board {
        app: common::with_default_host(build_router(state)),
        project: project.id,
    }
}

impl Board {
    async fn apply(&self, teams: Value) -> (StatusCode, Value) {
        common::send(
            &self.app,
            "POST",
            &format!("/api/projects/{}/teams/apply", self.project),
            json!({"teams": teams}),
            &[("authorization", &format!("Bearer {TOKEN}"))],
        )
        .await
    }
}

/// A runner that answers the next operations it is handed with `answers`, in order, and gives back
/// what it was asked.
fn runner_answering(
    runner: &'static str,
    answers: Vec<OperationOutcome>,
) -> tokio::task::JoinHandle<Vec<TeamOperation>> {
    tokio::spawn(async move {
        let queue = tack_app::runner_operations::operations();
        let mut asked = Vec::new();
        for answer in answers {
            let handed = loop {
                if let Some(first) = queue.take(runner).into_iter().next() {
                    break first;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            };
            assert!(queue.finish(runner, &handed.operation_id, answer));
            asked.push(handed.operation);
        }
        asked
    })
}

fn support() -> Value {
    json!([{"name": "fab-support", "shape": "respond"}])
}

#[tokio::test]
async fn the_route_is_refused_without_a_docket_that_lists_podshape() {
    let board = board("ru-apply-none", docket(json!([])), CodeOrigin::Folder).await;

    let (status, body) = board.apply(support()).await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let sentence = body["error"]["message"].as_str().unwrap_or_default();
    assert!(sentence.contains("podShape"), "{body}");
}

#[tokio::test]
async fn the_route_is_refused_for_a_project_without_a_folder_and_for_names_docket_refuses() {
    let no_folder = board("ru-apply-url", docket(json!(["podShape"])), CodeOrigin::Url).await;
    let (status, body) = no_folder.apply(support()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    let board = board(
        "ru-apply-bad",
        docket(json!(["podShape"])),
        CodeOrigin::Folder,
    )
    .await;
    for (teams, wanted) in [
        (json!([]), "at least one team"),
        (
            json!([{"name": "Fab Support", "shape": "respond"}]),
            "team name",
        ),
        (json!([{"name": "fab-support", "shape": "blog"}]), "shape"),
    ] {
        let (status, body) = board.apply(teams).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert!(
            body["error"]["message"].as_str().unwrap().contains(wanted),
            "{body}"
        );
    }
}

#[tokio::test]
async fn each_team_is_asked_of_the_runner_with_the_projects_folder_and_a_review_comes_back_to_the_person()
 {
    let board = board(
        "ru-apply-flow",
        docket(json!(["podShape"])),
        CodeOrigin::Folder,
    )
    .await;
    let runner = runner_answering(
        "ru-apply-flow",
        vec![
            OperationOutcome::NeedsReview {
                code: "3f9a1c7e2b40".to_owned(),
                commands: vec!["allow-commands allowCommands: pytest, ruff".to_owned()],
            },
            OperationOutcome::Done { team: None },
            OperationOutcome::Refused {
                reason: "docket did not make the team".to_owned(),
            },
        ],
    );

    // Two teams: the first waits for a person to read what its folder runs, the second is applied.
    let (status, body) = board
        .apply(json!([
            {"name": "fab-support", "shape": "respond"},
            {"name": "fab-ops", "shape": "operate"},
        ]))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body[0]["team"], "fab-support");
    assert_eq!(body[0]["state"], "needs_review");
    assert_eq!(body[0]["review"]["code"], "3f9a1c7e2b40");
    assert_eq!(
        body[0]["review"]["commands"],
        json!(["allow-commands allowCommands: pytest, ruff"])
    );
    assert_eq!(body[1]["team"], "fab-ops");
    assert_eq!(body[1]["state"], "applied");

    // The person read the commands: the same team goes again with the code, and docket refuses.
    let (status, body) = board
        .apply(json!([
            {"name": "fab-support", "shape": "respond", "accept": "3f9a1c7e2b40"},
        ]))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body[0]["state"], "refused");
    assert_eq!(body[0]["message"], "docket did not make the team");

    let asked = runner.await.unwrap();
    let TeamOperation::ApplyTeam {
        harness,
        team,
        shape,
        directory,
        accept,
    } = &asked[0]
    else {
        panic!("{asked:?}");
    };
    assert_eq!(
        (
            harness.as_str(),
            team.as_str(),
            shape.as_str(),
            directory.as_str(),
            accept
        ),
        ("docket", "fab-support", "respond", "/work/saas", &None)
    );
    let TeamOperation::ApplyTeam { accept, .. } = &asked[2] else {
        panic!("{asked:?}");
    };
    assert_eq!(accept.as_deref(), Some("3f9a1c7e2b40"));
}
