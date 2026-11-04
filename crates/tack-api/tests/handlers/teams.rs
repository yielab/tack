//! `GET /api/teams`: every team any active runner reports, once per name and agent, with the
//! profiles that name it, plus the names profiles use that nobody reports.

use crate::common;
use crate::handlers::autonomy::{board, fake_prill};
use crate::handlers::mrp::operator_headers;
use serde_json::{Value, json};
use tack_db::repo::team_activity::TeamTaskReport;
use tack_test_support::seed;

fn snapshot(harnesses: Value) -> String {
    json!({"harnesses": harnesses}).to_string()
}

fn harness(kind: &str, probed_at: &str, teams: Option<Value>) -> Value {
    let mut harness = json!({
        "harness_kind": kind, "installed_version": "0.2.0", "probe_error": null,
        "probed_at": probed_at,
    });
    if let Some(teams) = teams {
        harness["teams"] = teams;
    }
    harness
}

#[tokio::test]
async fn teams_join_two_runners_and_a_name_only_a_profile_uses() {
    let root = tempfile::tempdir().unwrap();
    let prill = fake_prill(root.path());
    let b = board(root.path(), None, &prill).await;
    let repo = &b.state.repo;
    let get = || async {
        common::send(
            &b.app,
            "GET",
            "/api/teams",
            Value::Null,
            &operator_headers(),
        )
        .await
        .1
    };
    assert_eq!(get().await, json!({"teams": []}));

    // Two runners report `writers`; the later report wins. The second also runs an agent that
    // reports no teams at all.
    seed::agent_runner_reporting(
        repo,
        "ru1",
        &snapshot(json!([harness(
            "docket",
            "2026-08-06T11:00:00Z",
            Some(json!([
                {"name": "writers", "profile": "lights-out", "violations": [], "shape": "routine"},
                {"name": "reviewers", "profile": "attended", "violations": []}
            ]))
        )])),
    )
    .await;
    seed::agent_runner(repo, "ru2", "now").await;
    seed::agent_runner_capabilities(
        repo,
        "ru2",
        &snapshot(json!([
            harness(
                "docket",
                "2026-08-06T12:00:00Z",
                Some(json!([{
                    "name": "writers", "profile": "lights-out", "violations": [],
                    "shape": "build", "decides": "to_the_end", "network": "none",
                    "queue": {"waiting": 1, "running": 2},
                    "members": [{"role": "lead", "model": "m"}],
                    "steps": [{"name": "plan", "kind": "agent", "role": "lead"}]
                }]))
            ),
            harness("codex", "2026-08-06T12:00:00Z", None)
        ])),
    )
    .await;
    seed::agent_profile_for_team(repo, "p1", "Writer", "writers").await;
    seed::agent_profile_for_team(repo, "p2", "Ghost", "ghosts").await;

    let got = get().await;
    let teams = got["teams"].as_array().unwrap();
    let names: Vec<&str> = teams.iter().map(|t| t["name"].as_str().unwrap()).collect();
    // One entry per name and agent; the agent that reports no team contributes none.
    assert_eq!(names, ["ghosts", "reviewers", "writers"], "{got}");

    let writers = &teams[2];
    assert_eq!(writers["harness"], "docket", "{got}");
    assert_eq!(writers["reported"], true, "{got}");
    assert_eq!(writers["runner"], "p", "{got}");
    assert_eq!(writers["reported_at"], "2026-08-06T12:00:00Z", "{got}");
    assert_eq!(writers["report"]["shape"], "build", "{got}");
    assert_eq!(writers["report"]["decides"], "to_the_end", "{got}");
    assert_eq!(
        writers["report"]["queue"],
        json!({"waiting": 1, "running": 2})
    );
    assert_eq!(
        writers["profiles"],
        json!([{"id": "p1", "name": "Writer", "misfits": []}]),
        "{got}"
    );

    let reviewers = &teams[1];
    assert_eq!(reviewers["runner"], "ru", "{got}");
    assert_eq!(reviewers["profiles"], json!([]), "{got}");
    assert!(reviewers["report"].get("shape").is_none(), "{got}");

    let ghosts = &teams[0];
    assert_eq!(ghosts["reported"], false, "{got}");
    assert_eq!(ghosts["harness"], Value::Null, "{got}");
    assert_eq!(ghosts["runner"], Value::Null, "{got}");
    assert_eq!(ghosts["report"], Value::Null, "{got}");
    assert_eq!(
        ghosts["profiles"],
        json!([{"id": "p2", "name": "Ghost", "misfits": []}]),
        "{got}"
    );
}

/// A profile whose team says it cannot do what the profile needs is listed with each misfit, so the
/// Teams page says it before a run is refused for it.
#[tokio::test]
async fn a_profile_lists_what_its_team_says_it_cannot_do() {
    let root = tempfile::tempdir().unwrap();
    let prill = fake_prill(root.path());
    let b = board(root.path(), None, &prill).await;
    let repo = &b.state.repo;
    let member = |can_propose: bool, reads_web: bool| json!({"role": "researcher", "can_propose": can_propose, "reads_web": reads_web});
    seed::agent_runner_reporting(
        repo,
        "ru1",
        &snapshot(json!([harness(
            "docket",
            "2026-08-06T11:00:00Z",
            Some(json!([
                {"name": "closed", "profile": "attended", "violations": [], "network": "none",
                 "members": [member(false, false), member(false, false)]},
                {"name": "fetches", "profile": "attended", "violations": [], "network": "none",
                 "members": [member(true, false), member(false, true)]}
            ]))
        )])),
    )
    .await;
    seed::agent_profile_of_kind_for_team(repo, "p1", "Closed planner", "closed", "planner", true)
        .await;
    seed::agent_profile_of_kind_for_team(repo, "p2", "Offline", "closed", "researcher", false)
        .await;
    seed::agent_profile_of_kind_for_team(repo, "p3", "Fetching", "fetches", "planner", true).await;

    let (_, got) = common::send(
        &b.app,
        "GET",
        "/api/teams",
        Value::Null,
        &operator_headers(),
    )
    .await;
    let misfits = |team: &str| -> Vec<(String, Value)> {
        got["teams"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["name"] == team)
            .unwrap()["profiles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (p["name"].as_str().unwrap().to_owned(), p["misfits"].clone()))
            .collect()
    };
    assert_eq!(
        misfits("closed"),
        [
            (
                "Closed planner".to_owned(),
                json!(["no_web", "no_proposals"])
            ),
            ("Offline".to_owned(), json!([])),
        ],
        "{got}"
    );
    assert_eq!(
        misfits("fetches"),
        [("Fetching".to_owned(), json!([]))],
        "{got}"
    );
}

/// `GET /api/teams/{id}/activity`: what the team holds, the work the board did not start included,
/// and an agent that cannot say answers why.
#[tokio::test]
async fn a_teams_activity_shows_work_outside_the_board_and_says_why_an_agent_cannot() {
    let root = tempfile::tempdir().unwrap();
    let prill = fake_prill(root.path());
    let b = board(root.path(), None, &prill).await;
    let repo = &b.state.repo;
    let get = |uri: &'static str| {
        let (app, headers) = (&b.app, operator_headers());
        async move { common::send(app, "GET", uri, Value::Null, &headers).await }
    };
    let snapshot = json!({"harnesses": [
        {"harness_kind": "docket", "installed_version": "0.3.0", "probe_error": null,
         "probed_at": "2026-08-06T12:00:00Z",
         "teams": [{"name": "writers", "profile": "lights-out", "violations": []}],
         "features": {"cancel": {"support": "supported"}, "resume": {"support": "supported"},
             "decisions": {"support": "supported"}, "artifacts": {"support": "supported"},
             "usage": {"support": "supported"}, "activity": {"support": "supported", "reason": "the task list"}}},
        {"harness_kind": "codex", "installed_version": "0.149", "probe_error": null,
         "probed_at": "2026-08-06T12:00:00Z",
         "teams": [{"name": "solo", "profile": "attended", "violations": []}],
         "features": {"cancel": {"support": "advisory"}, "resume": {"support": "unsupported"},
             "decisions": {"support": "supported"}, "artifacts": {"support": "advisory"},
             "usage": {"support": "advisory"},
             "activity": {"support": "unsupported", "reason": "codex holds no team"}}},
    ]})
    .to_string();
    seed::agent_runner_reporting(repo, "ru1", &snapshot).await;

    let (status, _) = get("/api/teams/nobody/activity").await;
    assert_eq!(status, 404);

    let step = r#"{"name":"lead","verdict":"approve","summary":"Planned the change."}"#;
    for (task, on_board, state, current, steps) in [
        (
            "task-cli",
            None,
            "running",
            Some(r#"{"name":"implementer","role":"implementer"}"#),
            vec![],
        ),
        ("task-board", Some("item-1"), "done", None, vec![step]),
    ] {
        repo.record_team_task(&TeamTaskReport {
            runner_id: "ru1",
            harness: "docket",
            team: "writers",
            task_ref: task,
            on_board,
            state,
            waits_on: None,
            current_step: current,
            changed_at: None,
            reported_at: "2026-08-06T12:00:00+00:00",
            steps: steps
                .into_iter()
                .map(|step| ("2026-08-06T11:59:00+00:00".to_owned(), step.to_owned()))
                .collect(),
        })
        .await
        .unwrap();
    }
    let (status, got) = get("/api/teams/writers/activity").await;
    assert_eq!(status, 200, "{got}");
    assert_eq!(
        (got["supported"].clone(), got["harness"].clone()),
        (json!(true), json!("docket"))
    );
    let tasks = got["tasks"].as_array().unwrap();
    // The one still going comes first.
    assert_eq!(tasks[0]["ref"], "task-cli", "{got}");
    assert!(
        tasks[0].get("on_board").is_none(),
        "outside the board: {got}"
    );
    assert_eq!(tasks[0]["current_step"]["name"], "implementer");
    assert_eq!(tasks[1]["on_board"], "item-1");
    assert_eq!(tasks[1]["finished_steps"][0]["verdict"], "approve", "{got}");
    assert_eq!(
        tasks[1]["finished_steps"][0]["summary"],
        "Planned the change."
    );

    let (status, got) = get("/api/teams/solo/activity").await;
    assert_eq!(status, 200, "{got}");
    assert_eq!(got["supported"], false);
    assert_eq!(got["reason"], "codex holds no team");
    assert_eq!(got["tasks"], json!([]));
}

/// An agent credential reads a team's activity through the real router, limited to its projects:
/// the tasks the board started for another project's items and the work the board did not start
/// stay out unless the credential may read every project. The operator sees all of it.
#[tokio::test]
async fn an_agent_credential_reads_a_teams_activity_limited_to_its_projects() {
    let root = tempfile::tempdir().unwrap();
    let prill = fake_prill(root.path());
    let b = board(root.path(), None, &prill).await;
    let repo = &b.state.repo;
    let item_of = |project: &str| {
        let project = project.to_owned();
        async move {
            let project: uuid::Uuid = project.parse().unwrap();
            repo.create_item(
                project,
                "To Do",
                tack_core::models::CreateItem {
                    title: "Another task".into(),
                    ..Default::default()
                },
            )
            .await
            .unwrap()
            .id
            .to_string()
        }
    };
    let other_project = repo
        .create_project(
            b.state.workspace_id,
            tack_core::models::CreateProject {
                name: "Other".into(),
                description: None,
                project_type: tack_core::models::ProjectType::Software,
                template: None,
                code_origin: Some(tack_core::models::CodeOrigin::Url),
                repository: Some("https://example.test/other.git".into()),
                default_branch: None,
                workspace_mode: None,
            },
        )
        .await
        .unwrap()
        .id
        .to_string();
    let other_item = item_of(&other_project).await;

    let snapshot = json!({"harnesses": [
        {"harness_kind": "docket", "installed_version": "0.3.0", "probe_error": null,
         "probed_at": "2026-08-06T12:00:00Z",
         "teams": [{"name": "writers", "profile": "lights-out", "violations": []}],
         "features": {"cancel": {"support": "supported"}, "resume": {"support": "supported"},
             "decisions": {"support": "supported"}, "artifacts": {"support": "supported"},
             "usage": {"support": "supported"}, "activity": {"support": "supported", "reason": "the task list"}}},
    ]})
    .to_string();
    seed::agent_runner_reporting(repo, "ru1", &snapshot).await;
    for (task, on_board) in [
        ("mine", Some(b.item_id.as_str())),
        ("theirs", Some(other_item.as_str())),
        ("from-the-cli", None),
    ] {
        repo.record_team_task(&TeamTaskReport {
            runner_id: "ru1",
            harness: "docket",
            team: "writers",
            task_ref: task,
            on_board,
            state: "running",
            waits_on: None,
            current_step: Some(r#"{"name":"implementer","role":"implementer"}"#),
            changed_at: None,
            reported_at: "2026-08-06T12:00:00+00:00",
            steps: Vec::new(),
        })
        .await
        .unwrap();
    }

    let credential = |name: &'static str, scopes: Value| {
        let app = &b.app;
        async move {
            let (status, issued) = common::send(
                app,
                "POST",
                "/api/agent-credentials",
                json!({"name": name, "scopes": scopes}),
                &operator_headers(),
            )
            .await;
            assert_eq!(status, 200, "{issued}");
            format!("Bearer {}", issued["secret"].as_str().unwrap())
        }
    };
    let activity = |authorization: String| {
        let app = &b.app;
        async move {
            let (status, got) = common::send(
                app,
                "GET",
                "/api/teams/writers/activity",
                Value::Null,
                &[("authorization", authorization.as_str())],
            )
            .await;
            let mut refs: Vec<String> = got["tasks"]
                .as_array()
                .map(|tasks| {
                    tasks
                        .iter()
                        .map(|task| task["ref"].as_str().unwrap().to_owned())
                        .collect()
                })
                .unwrap_or_default();
            refs.sort();
            (status, refs, got)
        }
    };

    // The operator sees every task the team holds.
    let (status, refs, got) = activity("Bearer e2-mrp-operator-token".to_owned()).await;
    assert_eq!(status, 200, "{got}");
    assert_eq!(refs, ["from-the-cli", "mine", "theirs"]);

    // `read` on one project: that project's task, not the other project's and not the work the
    // board did not start.
    let reader = credential(
        "Reader",
        json!([{"projects": [b.project_id], "capabilities": ["read"]}]),
    )
    .await;
    let (status, refs, got) = activity(reader).await;
    assert_eq!(status, 200, "{got}");
    assert_eq!(refs, ["mine"], "{got}");
    assert_eq!(got["supported"], true, "{got}");

    // `read` on every project also sees the work the board did not start.
    let everything = credential(
        "Everything",
        json!([{"all_projects": true, "capabilities": ["read"]}]),
    )
    .await;
    let (status, refs, got) = activity(everything).await;
    assert_eq!(status, 200, "{got}");
    assert_eq!(refs, ["from-the-cli", "mine", "theirs"], "{got}");

    // A credential that may not read any project is refused, as for the list of teams.
    let proposer = credential(
        "Proposer",
        json!([{"projects": [b.project_id], "capabilities": ["propose"]}]),
    )
    .await;
    let (status, _, got) = activity(proposer).await;
    assert_eq!(status, 403, "{got}");
}

/// What a runner reports of a local agent: the agent's own report, with the team of one the
/// runner fills from it.
fn local_agent(kind: &str, signed_in: bool) -> Value {
    let mut harness = json!({
        "harness_kind": kind, "installed_version": "1", "probe_error": null,
        "probed_at": "2026-11-03T12:00:00Z",
        "sign_in": {
            "state": if signed_in { "signed_in" } else { "signed_out" },
            "detail": "d", "command": format!("{kind} login"),
        },
    });
    let capability: tack_protocol::execution::HarnessCapability =
        serde_json::from_value(harness.clone()).expect("a capability");
    harness["teams"] = json!([tack_protocol::execution::TeamReport::of_agent(
        kind,
        &capability
    )]);
    harness
}

/// A local agent is a team of one: listed beside the pods, with the profiles that run on it, and
/// a profile whose agent is not signed in is listed with the need that is not met.
#[tokio::test]
async fn a_local_agent_is_a_team_of_one_with_the_profiles_that_run_on_it() {
    let root = tempfile::tempdir().unwrap();
    let prill = fake_prill(root.path());
    let b = board(root.path(), None, &prill).await;
    let repo = &b.state.repo;
    seed::agent_runner_reporting(
        repo,
        "ru1",
        &snapshot(json!([
            local_agent("claude-code", true),
            local_agent("codex", false),
            harness(
                "docket",
                "2026-11-03T12:00:00Z",
                Some(json!([
                    {"name": "writers", "profile": "lights-out", "violations": []}
                ]))
            ),
        ])),
    )
    .await;
    seed::agent_profile_with_policy(
        repo,
        "p1",
        "Coder",
        "implementer",
        &json!({"agent": "claude-code"}),
    )
    .await;
    seed::agent_profile_with_policy(repo, "p2", "Bare", "implementer", &json!({})).await;
    seed::agent_profile_with_policy(
        repo,
        "p3",
        "Codex coder",
        "implementer",
        &json!({"agent": "codex"}),
    )
    .await;
    // Not reported: opencode has no runner, and docket reports no team of one.
    seed::agent_profile_with_policy(
        repo,
        "p4",
        "Open",
        "implementer",
        &json!({"agent": "opencode"}),
    )
    .await;
    seed::agent_profile_with_policy(
        repo,
        "p5",
        "Docket default",
        "implementer",
        &json!({"agent": "docket"}),
    )
    .await;

    let (_, got) = common::send(
        &b.app,
        "GET",
        "/api/teams",
        Value::Null,
        &operator_headers(),
    )
    .await;
    let teams = got["teams"].as_array().unwrap();
    let names: Vec<(&str, Option<&str>)> = teams
        .iter()
        .map(|t| (t["name"].as_str().unwrap(), t["harness"].as_str()))
        .collect();
    assert_eq!(
        names,
        [
            ("claude-code", Some("claude-code")),
            ("codex", Some("codex")),
            ("writers", Some("docket")),
        ],
        "a profile that runs where no team of one is reported adds no team: {got}"
    );
    let claude = &teams[0];
    assert_eq!(claude["reported"], true, "{got}");
    assert_eq!(claude["report"]["delivers"], "branch", "{got}");
    assert!(claude["report"].get("profile").is_none(), "{got}");
    assert_eq!(claude["report"]["methods"][0]["name"], "agent", "{got}");
    // A profile naming no agent runs on Claude Code.
    assert_eq!(
        claude["profiles"],
        json!([
            {"id": "p2", "name": "Bare", "misfits": []},
            {"id": "p1", "name": "Coder", "misfits": []},
        ]),
        "{got}"
    );
    assert_eq!(
        teams[1]["profiles"],
        json!([{"id": "p3", "name": "Codex coder", "misfits": [{"needs_unmet": {"name": "signed in"}}]}]),
        "{got}"
    );
    assert_eq!(teams[2]["profiles"], json!([]), "{got}");
}

async fn save_profile(b: &crate::handlers::autonomy::Board, tool_policy: Value) -> (u16, Value) {
    let (status, body) = common::send(
        &b.app,
        "POST",
        "/api/agent-profiles",
        json!({"name": format!("P{}", uuid::Uuid::new_v4().simple()), "instructions": "x", "tool_policy": tool_policy}),
        &operator_headers(),
    )
    .await;
    (status.as_u16(), body)
}

/// Saving a profile checks its methods against what runs it: a name outside the team's catalogue
/// is refused naming the catalogue, a method the team lacks a member for is saved with a warning,
/// and a local agent, which has one method, takes none.
#[tokio::test]
async fn a_profile_save_checks_its_methods_against_the_team_that_runs_it() {
    let root = tempfile::tempdir().unwrap();
    let prill = fake_prill(root.path());
    let b = board(root.path(), None, &prill).await;
    let repo = &b.state.repo;
    seed::agent_runner_reporting(
        repo,
        "ru1",
        &snapshot(json!([
            local_agent("claude-code", true),
            harness(
                "docket",
                "2026-11-03T12:00:00Z",
                Some(json!([
                    {"name": "fab", "profile": "attended", "violations": [], "methods": [
                        {"name": "tdd", "source": "recipe", "default": true},
                        {"name": "secure-build", "source": "recipe", "default": false,
                         "missing_roles": ["security-vetter"]},
                    ]},
                    {"name": "plain", "profile": "attended", "violations": []},
                ]))
            ),
        ])),
    )
    .await;
    let on = |team: &str, method: &str| json!({"agent": "docket", "docket": {"team": team, "pipeline_by_type": {"bug": method}}});

    let (status, ok) = save_profile(&b, on("fab", "tdd")).await;
    assert_eq!(status, 200, "{ok}");
    assert!(ok.get("warnings").is_none(), "{ok}");

    let (status, warned) = save_profile(&b, on("fab", "secure-build")).await;
    assert_eq!(status, 200, "{warned}");
    let warning = warned["warnings"][0].as_str().unwrap_or_default();
    assert!(
        warning.contains("secure-build") && warning.contains("security-vetter"),
        "{warned}"
    );

    let (status, refused) = save_profile(&b, on("fab", "review")).await;
    assert_eq!(status, 400, "{refused}");
    let message = refused["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("no method named review") && message.contains("offers tdd, secure-build"),
        "{refused}"
    );

    // A team that reports no catalogue, or that nobody reports, is not second-guessed.
    for team in ["plain", "ghosts"] {
        let (status, body) = save_profile(&b, on(team, "anything")).await;
        assert_eq!(status, 200, "{team}: {body}");
    }

    // A local agent has one method, its own loop.
    let (status, local) = save_profile(
        &b,
        json!({"agent": "claude-code", "docket": {"pipeline_by_type": {"bug": "tdd"}}}),
    )
    .await;
    assert_eq!(status, 400, "{local}");
    assert!(
        local["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("its own loop"),
        "{local}"
    );
    // An empty map names nothing; a map of anything but names is refused.
    let (status, body) = save_profile(
        &b,
        json!({"agent": "claude-code", "docket": {"pipeline_by_type": {}}}),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = save_profile(
        &b,
        json!({"agent": "docket", "docket": {"pipeline_by_type": {"bug": 3}}}),
    )
    .await;
    assert_eq!(status, 400, "{body}");

    // The update answers with the same warning.
    let (_, created) = save_profile(&b, on("fab", "tdd")).await;
    let id = created["agent_profile_id"].as_str().unwrap();
    let (status, updated) = common::send(
        &b.app,
        "PATCH",
        &format!("/api/agent-profiles/{id}"),
        json!({"tool_policy": on("fab", "secure-build")}),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, 200, "{updated}");
    assert!(
        updated["warnings"][0]
            .as_str()
            .unwrap_or_default()
            .contains("security-vetter"),
        "{updated}"
    );
}

/// `POST /api/teams/{harness}/{name}/resume`: the runner that reports the team is asked, and what
/// the agent answers is what the person gets. A team no runner of that agent reports is not asked.
#[tokio::test]
async fn resuming_a_team_asks_the_runner_that_reports_it_and_answers_with_its_report() {
    use tack_protocol::execution::{OperationOutcome, TeamReport};
    let root = tempfile::tempdir().unwrap();
    let prill = fake_prill(root.path());
    let b = board(root.path(), None, &prill).await;
    let repo = &b.state.repo;
    let resume = |harness: &'static str, team: &'static str| {
        let (app, headers) = (&b.app, operator_headers());
        async move {
            common::send(
                app,
                "POST",
                &format!("/api/teams/{harness}/{team}/resume"),
                Value::Null,
                &headers,
            )
            .await
        }
    };
    let (status, body) = resume("docket", "writers").await;
    assert_eq!(status, 404, "no runner reports it: {body}");

    seed::agent_runner_reporting(
        repo,
        "ru-resume",
        &snapshot(json!([
            harness(
                "docket",
                "2026-08-06T12:00:00Z",
                Some(json!([{"name": "writers", "profile": "lights-out", "violations": []}]))
            ),
            harness(
                "codex",
                "2026-08-06T12:00:00Z",
                Some(json!([{"name": "solo", "violations": []}]))
            )
        ])),
    )
    .await;
    let (status, body) = resume("codex", "writers").await;
    assert_eq!(status, 404, "the team is docket's: {body}");

    // The runner answers its next poll: the first time with the team's report, the second with a
    // refusal.
    let report: TeamReport = serde_json::from_value(json!({
        "name": "writers", "violations": [],
        "budget": {"tokens": 2000000, "used": 1200000, "window": "day"}
    }))
    .unwrap();
    let runner = tokio::spawn(async move {
        let queue = tack_app::runner_operations::operations();
        let mut answers = vec![
            OperationOutcome::Done { team: Some(report) },
            OperationOutcome::Refused {
                reason: "docket cannot resume a team from the board".to_owned(),
            },
        ]
        .into_iter();
        while let Some(answer) = answers.next() {
            let handed = loop {
                let handed = queue.take("ru-resume");
                if let Some(first) = handed.into_iter().next() {
                    break first;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            };
            assert!(queue.finish("ru-resume", &handed.operation_id, answer));
        }
    });
    let (status, body) = resume("docket", "writers").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["runner"], "ru", "{body}");
    assert_eq!(body["report"]["budget"]["used"], 1_200_000, "{body}");
    let (status, body) = resume("docket", "writers").await;
    assert_eq!(status, 409, "{body}");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("cannot resume a team from the board"),
        "{body}"
    );
    runner.await.unwrap();
}
