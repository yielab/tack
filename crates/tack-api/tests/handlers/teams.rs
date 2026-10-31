//! `GET /api/teams`: every team any active runner reports, once per name and agent, with the
//! profiles that name it, plus the names profiles use that nobody reports.

use crate::common;
use crate::handlers::autonomy::{board, fake_prill};
use crate::handlers::mrp::operator_headers;
use serde_json::{Value, json};
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
        json!([{"id": "p1", "name": "Writer"}]),
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
        json!([{"id": "p2", "name": "Ghost"}]),
        "{got}"
    );
}
