//! Autonomy settings (`GET/PUT /api/projects/{id}/autonomy`) and the automatic
//! merge: `autonomy::tick` against a mock GitHub and a fake prill. A pack held
//! for "CI still running" merges on the next tick with the head sha, then a
//! revert demotes its domain L4 to L3; one row each for the holds.

use crate::common;
use crate::handlers::mrp::{
    BASE_REVISION, ClaimedAttempt, MRP_MEDIA_TYPE, READY, accept_and_start,
    completion_with_pushed_branch, headers_ref, operator_headers, request_and_claim, sha256_hex,
};
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::{AppState, router::build_router};
use tack_protocol::execution::ExecutionState;
use tack_test_support::seed;
use tower::ServiceExt;
use wiremock::matchers::{body_json, method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

const OPERATOR_TOKEN: &str = "e2-mrp-operator-token";
const ATTESTATION_MEDIA_TYPE: &str = "application/vnd.in-toto+json";
const KEY_ID: &str = "f239d6c81feedf3a944a2a940f89f0ca4af0a2f05207185c19997eb09d7476f6";

pub(crate) fn fixture(name: &str) -> Value {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/github/");
    serde_json::from_str(&std::fs::read_to_string(format!("{dir}{name}")).unwrap()).unwrap()
}

/// A prill that answers `attest verify` with the captured valid output.
pub(crate) fn fake_prill(dir: &std::path::Path) -> String {
    use std::os::unix::fs::PermissionsExt;
    let fixtures = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/prill/");
    let script = dir.join("prill");
    std::fs::write(
        &script,
        format!("#!/bin/sh\ncat {fixtures}attest-ok.stderr >&2\nexit 0\n"),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    script.display().to_string()
}

pub(crate) struct Board {
    pub(crate) app: axum::Router,
    pub(crate) state: AppState,
    pub(crate) item_id: String,
    pub(crate) project_id: String,
}

pub(crate) async fn board(storage: &std::path::Path, github: Option<&str>, prill: &str) -> Board {
    let repo = tack_test_support::setup_test_db().await;
    let workspace_id = tack_test_support::create_test_workspace(&repo).await;
    let (tx, _rx) = tokio::sync::broadcast::channel(16);
    let state = AppState {
        repo: repo.clone(),
        config: AppConfig {
            api_token: Some(OPERATOR_TOKEN.into()),
            database_url: "sqlite::memory:".into(),
            storage_dir: storage.to_string_lossy().into_owned(),
            github_token: github.map(|_| "gh-tok".into()),
            github_api_base: github
                .map_or_else(|| AppConfig::default().github_api_base, Into::into),
            prill_program: prill.into(),
            ..AppConfig::default()
        },
        workspace_id,
        broadcast_tx: tx,
        webhook: None,
        local_runner: None,
        tasks: Default::default(),
    };
    let app = common::with_default_host(build_router(state.clone()));
    let project = repo
        .create_project(
            workspace_id,
            tack_core::models::CreateProject {
                name: "A2".into(),
                description: None,
                project_type: tack_core::models::ProjectType::Software,
                template: None,
                code_origin: Some(tack_core::models::CodeOrigin::Url),
                repository: Some("https://example.test/e2.git".into()),
                default_branch: None,
                workspace_mode: None,
            },
        )
        .await
        .unwrap();
    let item = repo
        .create_item(
            project.id,
            "To Do",
            tack_core::models::CreateItem {
                recurrence: None,
                recurrence_of: None,
                title: "Add sub".into(),
                description: None,
                item_type: None,
                parent_id: None,
                priority: None,
                estimate: None,
                estimate_unit: None,
                tags: None,
                due_date: None,
                sprint_id: None,
                assignee: None,
                source_artifact_id: None,
            },
        )
        .await
        .unwrap();
    Board {
        app,
        state,
        item_id: item.id.to_string(),
        project_id: project.id.to_string(),
    }
}

/// What a scenario changes from the all-green acceptance setup.
pub(crate) struct Setup {
    pub(crate) domain_level: &'static str,
    pub(crate) domain_globs: &'static str,
    pub(crate) linked: bool,
    pub(crate) github: bool,
    pub(crate) signed: bool,
    pub(crate) verify_require: Vec<&'static str>,
    /// The stages the pack reports, as (name, status)
    pub(crate) pack_stages: Vec<(&'static str, &'static str)>,
    pub(crate) merges_per_day: Option<i64>,
}

impl Default for Setup {
    fn default() -> Self {
        Setup {
            domain_level: "L4",
            domain_globs: "src/**",
            linked: true,
            github: true,
            signed: true,
            verify_require: vec!["criteria", "suite"],
            pack_stages: vec![("criteria", "passed"), ("suite", "passed")],
            merges_per_day: Some(5),
        }
    }
}

pub(crate) async fn put_autonomy(b: &Board, setup: &Setup) -> (StatusCode, Value) {
    common::send(
        &b.app,
        "PUT",
        &format!("/api/projects/{}/autonomy", b.project_id),
        json!({
            "level": "L3",
            "domains": [{"name": "core", "path_globs": [setup.domain_globs], "item_types": [], "level": setup.domain_level}],
            "verify_require": setup.verify_require,
            "attestation_public_key": include_str!("../fixtures/prill/prill.pub"),
            "verify_loop_max_attempts": 2,
            "budgets": {"tokens_per_day": null, "auto_merges_per_day": setup.merges_per_day, "queue_items_max": null},
            "merge_on_done": false,
        }),
        &operator_headers(),
    )
    .await
}

async fn upload(
    app: &axum::Router,
    a: &ClaimedAttempt,
    id: &str,
    kind: &str,
    name: &str,
    media: &str,
    bytes: &[u8],
) {
    let auth = headers_ref(&a.auth);
    let (status, manifest, _) = common::send_with_raw(
        app,
        "POST",
        &format!("/api/runner/v1/attempts/{}/artifacts", a.attempt_id),
        json!({
            "protocol_version": 1, "runner_id": a.runner_id, "attempt_id": a.attempt_id,
            "fencing_token": a.fencing_token,
            "artifacts": [{"artifact_id": id, "kind": kind, "name": name, "media_type": media,
                "size_bytes": bytes.len(), "sha256": sha256_hex(bytes),
                "content_disposition": "inline_upload", "metadata": {}}],
        }),
        &auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{manifest}");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!(
                    "/api/runner/v1/attempts/{}/artifacts/{id}/content",
                    a.attempt_id
                ))
                .header("authorization", a.auth[0].1.clone())
                .header("x-tack-fencing-token", a.fencing_token.to_string())
                .header("content-type", media)
                .body(Body::from(bytes.to_vec()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 8 << 20).await.unwrap();
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
}

/// A finished attempt with its pack, evidence and (when `signed`) attestation.
pub(crate) async fn finished_attempt(b: &Board, setup: &Setup) -> ClaimedAttempt {
    if setup.linked {
        b.state
            .repo
            .set_github_link(b.item_id.parse().unwrap(), "acme/widgets", 3)
            .await
            .unwrap();
    }
    let a = request_and_claim(&b.app, &b.item_id, "a2", None).await;
    accept_and_start(&b.app, &a).await;
    let mut pack: Value = serde_json::from_str(READY).unwrap();
    pack["stages"] = setup
        .pack_stages
        .iter()
        .map(|(name, status)| json!({"name": name, "status": status}))
        .collect();
    upload(
        &b.app,
        &a,
        "mrp-1",
        "report",
        "mrp.json",
        MRP_MEDIA_TYPE,
        pack.to_string().as_bytes(),
    )
    .await;
    let evidence = json!({"v": "1", "files": [{"path": "src/new.rs", "op": "added"}]}).to_string();
    upload(
        &b.app,
        &a,
        "ev-1",
        "evidence",
        "evidence.json",
        "application/json",
        evidence.as_bytes(),
    )
    .await;
    if setup.signed {
        let envelope = include_bytes!("../fixtures/prill/mrp.intoto.json");
        upload(
            &b.app,
            &a,
            "att-1",
            "mrp_attestation",
            "mrp.intoto.json",
            ATTESTATION_MEDIA_TYPE,
            envelope,
        )
        .await;
    }
    let (status, body, _) = common::send_with_raw(
        &b.app,
        "POST",
        &format!("/api/runner/v1/attempts/{}/completion", a.attempt_id),
        completion_with_pushed_branch(&a),
        &headers_ref(&a.auth),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    a
}

/// The fake GitHub: opening the PR, then CI, then the merge. `ci_running_first`
/// makes the first CI read show a check run still in progress.
async fn github(ci_running_first: bool, merges: u64) -> MockServer {
    let gh = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/widgets"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"default_branch": "trunk"})))
        .mount(&gh)
        .await;
    Mock::given(method("POST"))
        .and(path("/repos/acme/widgets/pulls"))
        .respond_with(ResponseTemplate::new(201).set_body_json(
            json!({"number": 7, "html_url": "https://github.com/acme/widgets/pull/7"}),
        ))
        .mount(&gh)
        .await;
    let runs = || path_regex(r"^/repos/acme/widgets/commits/[0-9a-f]+/check-runs$");
    if ci_running_first {
        Mock::given(method("GET"))
            .and(runs())
            .respond_with(
                ResponseTemplate::new(200).set_body_json(fixture("check-runs-in-progress.json")),
            )
            .up_to_n_times(1)
            .mount(&gh)
            .await;
    }
    Mock::given(method("GET"))
        .and(runs())
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fixture("check-runs-all-success.json")),
        )
        .mount(&gh)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(
            r"^/repos/acme/widgets/commits/[0-9a-f]+/status$",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture("status-success.json")))
        .mount(&gh)
        .await;
    Mock::given(method("PUT"))
        .and(path("/repos/acme/widgets/pulls/7/merge"))
        .and(body_json(json!({"sha": BASE_REVISION})))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture("merge-200.json")))
        .expect(merges)
        .mount(&gh)
        .await;
    gh
}

pub(crate) async fn merge_row(
    b: &Board,
    attempt_id: &str,
) -> Option<(
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
)> {
    tack_db::queries::autonomy::merge_of_attempt(&b.state.repo, attempt_id)
        .await
        .unwrap()
        .map(|row| {
            (
                row.trigger,
                row.domain,
                row.level,
                row.held_reason,
                row.merged_at,
                row.merge_sha,
            )
        })
}

pub(crate) async fn comments(b: &Board) -> String {
    b.state
        .repo
        .list_comments(b.item_id.parse().unwrap())
        .await
        .unwrap()
        .into_iter()
        .filter(|comment| comment.author.as_deref() == Some("system"))
        .map(|comment| comment.content)
        .collect::<Vec<_>>()
        .join("\n")
}

/// One pass of the automatic merge's turn, as the poll loop runs it.
pub(crate) async fn tick(state: &AppState) {
    tack_app::autonomy::tick(&state.repo, &tack_protocol::SystemClock, &state.ports())
        .await
        .unwrap();
}

#[tokio::test]
async fn a_pack_held_for_ci_merges_on_the_next_tick_then_a_revert_demotes_its_domain() {
    let root = tempfile::tempdir().unwrap();
    let prill = fake_prill(root.path());
    let gh = github(true, 1).await;
    let b = board(root.path(), Some(&gh.uri()), &prill).await;
    let setup = Setup::default();

    let (status, put) = put_autonomy(&b, &setup).await;
    assert_eq!(status, StatusCode::OK, "{put}");
    let (status, got) = common::send(
        &b.app,
        "GET",
        &format!("/api/projects/{}/autonomy", b.project_id),
        Value::Null,
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{got}");
    assert_eq!(got["domains"][0]["name"], "core");
    assert_eq!(got["verify_require"], json!(["criteria", "suite"]));

    let a = finished_attempt(&b, &setup).await;
    let snapshot =
        tack_db::queries::execution_requests::request_snapshot(&b.state.repo, &a.request_id)
            .await
            .unwrap()
            .unwrap();
    let snapshot: Value = serde_json::from_str(&snapshot).unwrap();
    assert_eq!(snapshot["verify_require"], json!(["criteria", "suite"]));

    tick(&b.state).await;
    let row = merge_row(&b, &a.attempt_id).await.expect("a held row");
    assert_eq!(
        row.3.as_deref(),
        Some("CI is still running on the head commit"),
        "{row:?}"
    );
    assert!(row.4.is_none());

    tick(&b.state).await;
    let row = merge_row(&b, &a.attempt_id).await.unwrap();
    assert_eq!(row.0, "auto");
    assert_eq!(row.1.as_deref(), Some("core"));
    assert_eq!(row.2.as_deref(), Some("L4"));
    assert!(row.3.is_none() && row.4.is_some(), "{row:?}");
    assert_eq!(
        row.5.as_deref(),
        Some("e5bd3914e2e596debea16f433f57875b5b90bcd6")
    );
    let keyid = tack_db::queries::autonomy::merge_of_attempt(&b.state.repo, &a.attempt_id)
        .await
        .unwrap()
        .and_then(|row| row.attestation_keyid);
    assert_eq!(keyid.as_deref(), Some(KEY_ID));

    // G sees the revert; the next tick takes the domain one level down.
    b.state
        .repo
        .mark_pull_request_reverted("acme/widgets", 7, Some(9))
        .await
        .unwrap();
    tick(&b.state).await;
    let (_, got) = common::send(
        &b.app,
        "GET",
        &format!("/api/projects/{}/autonomy", b.project_id),
        Value::Null,
        &operator_headers(),
    )
    .await;
    assert_eq!(got["domains"][0]["level"], "L3", "{got}");
    assert_eq!(got["domains"][0]["merged"], 1, "{got}");
    assert_eq!(got["domains"][0]["undone"], 1, "{got}");
    assert!(got["automatic_merge_blocked_by"].is_array(), "{got}");
    let demoted = tack_db::queries::autonomy::merge_of_attempt(&b.state.repo, &a.attempt_id)
        .await
        .unwrap()
        .and_then(|row| row.demoted_at);
    assert!(demoted.is_some());
    let said = comments(&b).await;
    assert!(
        said.contains("core") && said.contains("L4") && said.contains("L3") && said.contains("#7"),
        "{said}"
    );
    tick(&b.state).await; // a demotion is made once
    assert_eq!(comments(&b).await, said);
}

/// One row per hold: the setup that stops the merge and the words the hold gives.
#[tokio::test]
async fn holds_table() {
    let rows: Vec<(&str, Setup, Option<&str>)> = vec![
        (
            "L3 is untouched",
            Setup {
                domain_level: "L3",
                ..Setup::default()
            },
            None,
        ),
        (
            "a change outside the L4 domain is L3: nothing held, nothing said",
            Setup {
                domain_globs: "docs/**",
                ..Setup::default()
            },
            None,
        ),
        (
            "unlinked item",
            Setup {
                linked: false,
                ..Setup::default()
            },
            Some("not linked to a GitHub issue"),
        ),
        (
            "poll off",
            Setup {
                github: false,
                ..Setup::default()
            },
            Some("GitHub is not reachable"),
        ),
        (
            "unsigned pack",
            Setup {
                signed: false,
                ..Setup::default()
            },
            Some("not signed"),
        ),
        (
            "empty verify_require",
            Setup {
                verify_require: vec![],
                ..Setup::default()
            },
            Some("required verification stages"),
        ),
        (
            "a required stage the verifier passed over",
            Setup {
                pack_stages: vec![("criteria", "passed"), ("suite", "not_applicable")],
                ..Setup::default()
            },
            Some("The required stage suite is not_applicable"),
        ),
        (
            "a required stage the pack does not list",
            Setup {
                pack_stages: vec![("criteria", "passed")],
                ..Setup::default()
            },
            Some("The required stage suite is not in the pack"),
        ),
        (
            "merge budget spent",
            Setup {
                merges_per_day: Some(0),
                ..Setup::default()
            },
            Some("automatic merges"),
        ),
    ];
    for (name, setup, expected) in rows {
        let root = tempfile::tempdir().unwrap();
        let prill = fake_prill(root.path());
        let gh = github(false, 0).await;
        let b = board(
            root.path(),
            setup.github.then(|| gh.uri()).as_deref(),
            &prill,
        )
        .await;
        let (status, put) = put_autonomy(&b, &setup).await;
        assert_eq!(status, StatusCode::OK, "{name}: {put}");
        let a = finished_attempt(&b, &setup).await;
        tick(&b.state).await;
        let row = merge_row(&b, &a.attempt_id).await;
        match expected {
            None => assert!(row.is_none(), "{name}: {row:?}"),
            Some(words) => {
                let row = row.unwrap_or_else(|| panic!("{name}: no row"));
                let held = row.3.unwrap_or_default();
                assert!(
                    held.contains(words),
                    "{name}: held for {held:?}, wanted {words:?}"
                );
                assert!(row.4.is_none(), "{name}: merged");
            }
        }
    }
}

/// Only the live run of an unfinished item is ever merged by itself: an item
/// already Done, or a run a newer request replaced, is left alone.
#[tokio::test]
async fn a_finished_item_or_a_replaced_run_is_never_merged_by_itself() {
    for case in ["item done", "replaced by a newer request"] {
        let root = tempfile::tempdir().unwrap();
        let prill = fake_prill(root.path());
        let gh = github(false, 0).await;
        let b = board(root.path(), Some(gh.uri().as_str()), &prill).await;
        let setup = Setup::default();
        let (status, put) = put_autonomy(&b, &setup).await;
        assert_eq!(status, StatusCode::OK, "{case}: {put}");
        let a = finished_attempt(&b, &setup).await;
        match case {
            "item done" => {
                tack_test_support::seed::item_completed(&b.state.repo, &b.item_id).await;
            }
            _ => {
                // A newer run of the same item makes this attempt a replaced one.
                let _ = request_and_claim(&b.app, &b.item_id, "a2-newer", None).await;
            }
        }
        tick(&b.state).await;
        let row = merge_row(&b, &a.attempt_id).await;
        assert!(row.is_none(), "{case}: {row:?}");
    }
}

fn reviewers_snapshot(kind: &str) -> String {
    json!({"harnesses": [{
        "harness_kind": kind, "installed_version": "0.2.0", "probe_error": null,
        "probed_at": "2026-08-06T11:59:55Z", "autonomy_ceiling": "L3",
        "teams": [
            {"name": "reviewers", "profile": "default", "violations": [
                {"requirement": "no operator prompts during a run", "fix": "run it lights-out"}]},
            {"name": "writers", "profile": "lights-out", "violations": []}],
    }]})
    .to_string()
}

/// A succeeded attempt is held for the ceiling of the team that ran it, and only
/// that team's: a lights-out team beside an attended one merges, and a team the
/// computer does not report is held with the sentence that says so.
#[tokio::test]
async fn the_ceiling_is_the_ceiling_of_the_team_that_ran_it() {
    let rows: [(Option<&str>, u64, Option<&str>); 4] = [
        (
            Some("reviewers"),
            0,
            Some("The team that ran this stays at L3: it needs a person during a run"),
        ),
        (Some("writers"), 1, None),
        (
            Some("saas-dev"),
            0,
            Some("Team saas-dev is not reported yet by this computer"),
        ),
        (
            None,
            0,
            Some("The team that ran this stays at L3: it needs a person during a run"),
        ),
    ];
    for (team, merges, expected) in rows {
        let root = tempfile::tempdir().unwrap();
        let prill = fake_prill(root.path());
        let gh = github(false, merges).await;
        let b = board(root.path(), Some(&gh.uri()), &prill).await;
        let setup = Setup::default();
        let (status, put) = put_autonomy(&b, &setup).await;
        assert_eq!(status, StatusCode::OK, "{put}");
        let a = finished_attempt(&b, &setup).await;
        let snapshot =
            tack_db::queries::execution_requests::request_snapshot(&b.state.repo, &a.request_id)
                .await
                .unwrap()
                .unwrap();
        let kind = serde_json::from_str::<Value>(&snapshot).unwrap()["requested_harness_kind"]
            .as_str()
            .unwrap()
            .to_owned();
        if let Some(team) = team {
            tack_test_support::seed::execution_request_team(&b.state.repo, &a.request_id, team)
                .await;
        }
        tack_test_support::seed::agent_runner_capabilities(
            &b.state.repo,
            &a.runner_id,
            &reviewers_snapshot(&kind),
        )
        .await;

        tick(&b.state).await;
        let row = merge_row(&b, &a.attempt_id).await.expect("a row");
        let held = row.3.clone().unwrap_or_default();
        match expected {
            Some(words) => {
                assert!(held.contains(words), "{team:?}: {held:?}");
                assert!(row.4.is_none(), "{team:?}: merged");
            }
            None => assert!(row.4.is_some(), "{team:?}: held for {held:?}"),
        }
    }
}

/// A role pointing at a profile whose team runs attended is said on the settings
/// page, and a PUT answers the same words as the GET.
#[tokio::test]
async fn the_settings_name_an_attended_team_and_put_answers_like_get() {
    let root = tempfile::tempdir().unwrap();
    let prill = fake_prill(root.path());
    let b = board(root.path(), None, &prill).await;
    let url = format!("/api/projects/{}/autonomy", b.project_id);
    let get = || async {
        common::send(&b.app, "GET", &url, Value::Null, &operator_headers())
            .await
            .1
    };
    let before = get().await;
    // Whatever stops Tack merging by itself lowers the level the page prints.
    assert!(
        !before["automatic_merge_blocked_by"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(before["ceiling"], "L3", "{before}");
    assert_eq!(before["teams"], json!([]), "{before}");

    let repo = &b.state.repo;
    seed::agent_profile_for_team(repo, "p1", "Reviewer", "reviewers").await;
    seed::role_running(repo, "r1", &b.project_id, "review", "p1").await;
    seed::agent_runner_reporting(repo, "ru1", &reviewers_snapshot("docket")).await;

    let got = get().await;
    let sentence = "The team reviewers runs attended: its tasks stay at L3. Fix: run it lights-out";
    assert!(
        got["automatic_merge_blocked_by"]
            .as_array()
            .unwrap()
            .iter()
            .any(|line| line == sentence),
        "{got}"
    );
    assert_eq!(got["ceiling"], "L3", "{got}");
    assert_eq!(got["teams"][0]["name"], "reviewers", "{got}");
    assert_eq!(got["teams"][0]["profile"], "Reviewer", "{got}");
    assert_eq!(got["teams"][0]["caps_at"], "L3", "{got}");
    assert_eq!(got["teams"][0]["fix"], "run it lights-out", "{got}");

    // A second role on a lights-out team is listed too, without a cap; a team no runner reports is "unknown".
    seed::agent_profile_for_team(repo, "p2", "Writer", "writers").await;
    seed::agent_profile_for_team(repo, "p3", "Ghost", "ghosts").await;
    seed::role_running(repo, "r2", &b.project_id, "write", "p2").await;
    seed::role_running(repo, "r3", &b.project_id, "haunt", "p3").await;
    seed::agent_runner_capabilities(
        repo,
        "ru1",
        &json!({"harnesses": [{
            "harness_kind": "docket", "installed_version": "0.2.0", "probe_error": null,
            "probed_at": "2026-08-06T11:59:55Z", "autonomy_ceiling": "L3",
            "teams": [
                {"name": "reviewers", "profile": "default", "violations": [{"requirement": "r", "fix": "run it lights-out"}]},
                {"name": "writers", "profile": "lights-out", "violations": [], "read_only": ["/opt/node-22"]}],
        }]})
        .to_string(),
    )
    .await;
    let got = get().await;
    let lines: Vec<(String, Value, Value)> = got["teams"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            (
                t["name"].as_str().unwrap().to_owned(),
                t["caps_at"].clone(),
                t["fix"].clone(),
            )
        })
        .collect();
    assert_eq!(lines.len(), 3, "{got}");
    let line = |n: &str| {
        lines
            .iter()
            .find(|l| l.0 == n)
            .unwrap_or_else(|| panic!("{n} missing: {got}"))
    };
    // The folders a team's sandbox reads travel with its line; a team that did not say, or that
    // no runner reports, has none to show.
    let reads = |n: &str| {
        got["teams"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == n)
            .unwrap()["read_only"]
            .clone()
    };
    assert_eq!(reads("writers"), json!(["/opt/node-22"]), "{got}");
    assert_eq!(reads("reviewers"), Value::Null, "{got}");
    assert_eq!(reads("ghosts"), Value::Null, "{got}");
    assert_eq!(line("writers").1, Value::Null, "{got}");
    assert_eq!(line("writers").2, Value::Null, "{got}");
    assert_eq!(line("ghosts").1, "L3", "{got}");
    assert_eq!(line("ghosts").2, "No runner reports this team yet", "{got}");
    assert_eq!(
        got["teams"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == "ghosts")
            .unwrap()["profile"],
        "unknown"
    );
    let blocked = got["automatic_merge_blocked_by"].as_array().unwrap();
    assert!(
        !blocked
            .iter()
            .any(|l| l.as_str().unwrap().contains("writers")),
        "{got}"
    );

    let (status, put) = put_autonomy(&b, &Setup::default()).await;
    assert_eq!(status, StatusCode::OK, "{put}");
    assert_eq!(
        put["automatic_merge_blocked_by"],
        get().await["automatic_merge_blocked_by"],
        "{put}"
    );
    assert_eq!(put["ceiling"], "L3", "{put}");
}

/// A task with no role runs with the project's default profile, so that profile's team is listed
/// with its state and holds the ceiling, though no role points at it.
#[tokio::test]
async fn the_default_profiles_team_is_a_team_of_the_project_too() {
    let root = tempfile::tempdir().unwrap();
    let prill = fake_prill(root.path());
    let b = board(root.path(), None, &prill).await;
    let url = format!("/api/projects/{}/autonomy", b.project_id);
    let repo = &b.state.repo;
    seed::agent_runner_reporting(repo, "ru1", &reviewers_snapshot("docket")).await;
    let teams = || async {
        common::send(&b.app, "GET", &url, Value::Null, &operator_headers())
            .await
            .1
    };
    // No role and no default profile: no team.
    assert_eq!(teams().await["teams"], json!([]));

    // Only the default profile names a team: it is listed, attended, and caps the ceiling.
    seed::agent_profile_for_team(repo, "p1", "Reviewer", "reviewers").await;
    seed::project_default_profile(repo, &b.project_id, "p1").await;
    let got = teams().await;
    assert_eq!(got["teams"][0]["name"], "reviewers", "{got}");
    assert_eq!(got["teams"][0]["profile"], "Reviewer", "{got}");
    assert_eq!(got["teams"][0]["caps_at"], "L3", "{got}");
    assert!(
        got["automatic_merge_blocked_by"]
            .as_array()
            .unwrap()
            .iter()
            .any(|line| line
                == "The team reviewers runs attended: its tasks stay at L3. Fix: run it lights-out"),
        "{got}"
    );

    // A role on the same profile does not list the team twice.
    seed::role_running(repo, "r1", &b.project_id, "review", "p1").await;
    assert_eq!(teams().await["teams"].as_array().unwrap().len(), 1);
}

// A project-level demotion to L3 switches auto-start off and the project says why.
#[tokio::test]
async fn a_demotion_to_l3_switches_auto_start_off() {
    let root = tempfile::tempdir().unwrap();
    let prill = fake_prill(root.path());
    let gh = github(false, 1).await;
    let b = board(root.path(), Some(&gh.uri()), &prill).await;
    let setup = Setup::default();
    let (status, put) = common::send(
        &b.app,
        "PUT",
        &format!("/api/projects/{}/autonomy", b.project_id),
        json!({
            "level": "L5", "domains": [], "verify_require": ["criteria", "suite"],
            "attestation_public_key": include_str!("../fixtures/prill/prill.pub"),
            "verify_loop_max_attempts": 2,
            "budgets": {"tokens_per_day": null, "auto_merges_per_day": 5, "queue_items_max": null},
            "merge_on_done": false,
        }),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{put}");
    let uri = format!("/api/projects/{}", b.project_id);
    let (status, _) = common::send(
        &b.app,
        "PATCH",
        &uri,
        json!({"auto_start_enabled": true, "auto_start_status": "To Do"}),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let a = finished_attempt(&b, &setup).await;
    tick(&b.state).await;
    tick(&b.state).await;
    assert_eq!(merge_row(&b, &a.attempt_id).await.unwrap().0, "auto");
    let (_, project) = common::send(&b.app, "GET", &uri, Value::Null, &operator_headers()).await;
    assert_eq!(project["auto_start_enabled"], true, "{project}");

    // The project had already dropped a level when this merge is reverted: L4 to L3.
    seed::project_autonomy_level(&b.state.repo, &b.project_id, "L4").await;
    b.state
        .repo
        .mark_pull_request_reverted("acme/widgets", 7, Some(9))
        .await
        .unwrap();
    tick(&b.state).await;
    let (_, project) = common::send(&b.app, "GET", &uri, Value::Null, &operator_headers()).await;
    let (_, got) = common::send(
        &b.app,
        "GET",
        &format!("/api/projects/{}/autonomy", b.project_id),
        Value::Null,
        &operator_headers(),
    )
    .await;
    assert_eq!(got["level"], "L3", "{got}");
    assert_eq!(project["auto_start_enabled"], false, "{project}");
    assert_eq!(
        project["auto_start_disabled_reason"], "Auto-start is off: the project dropped to L3",
        "{project}"
    );
    assert!(
        comments(&b)
            .await
            .contains("Auto-start is off: the project dropped to L3")
    );
}

/// "Run again at L3" is offered on an attempt that ended asking for a person: enabled for a docket
/// run that did not already run at L3, whatever the project's level or the team, and disabled,
/// with its reason, otherwise; no other attempt carries it.
#[tokio::test]
async fn an_attempt_that_needed_a_person_offers_a_run_at_l3_and_says_when_it_cannot() {
    // (harness, project level, team, the level the run was created with, the button's reason, or
    // none when it is enabled)
    let cases = [
        ("docket", "L3", Some("writers"), None, None),
        ("docket", "L4", Some("reviewers"), None, None),
        ("docket", "L5", Some("saas-dev"), None, None),
        ("docket", "L4", Some("writers"), Some("L4"), None),
        (
            "docket",
            "L4",
            Some("writers"),
            Some("L3"),
            Some("already ran at L3"),
        ),
        (
            "codex",
            "L4",
            None,
            None,
            Some("Only a run by a docket team can be run again"),
        ),
    ];
    for (harness, level, team, asked, reason) in cases {
        let root = tempfile::tempdir().unwrap();
        let prill = fake_prill(root.path());
        let b = board(root.path(), None, &prill).await;
        let repo = &b.state.repo;
        let a = request_and_claim(&b.app, &b.item_id, "rerun", None).await;
        seed::project_autonomy_level(repo, &b.project_id, level).await;
        seed::execution_request_harness(repo, &a.request_id, harness).await;
        if let Some(team) = team {
            seed::execution_request_team(repo, &a.request_id, team).await;
        }
        if let Some(asked) = asked {
            seed::execution_request_autonomy(repo, &a.request_id, asked).await;
        }
        let needs_human =
            r#"{"code":"needs_human","message":"docket needs a person to approve a tool call"}"#;
        seed::execution_attempt_ended(repo, &a.attempt_id, ExecutionState::Failed, needs_human)
            .await;

        let (status, list) = common::send(
            &b.app,
            "GET",
            &format!("/api/executions/{}/attempts", a.request_id),
            Value::Null,
            &operator_headers(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{list}");
        let button = &list["data"][0]["rerun_at_l3"];
        assert_eq!(
            button["enabled"],
            reason.is_none(),
            "{harness} {level} {team:?} {asked:?}: {list}"
        );
        match reason {
            Some(words) => {
                let said = button["reason"].as_str().unwrap_or_default();
                assert!(said.contains(words), "{harness} {level} {team:?}: {said:?}");
            }
            None => assert!(button["reason"].is_null(), "{list}"),
        }

        // Another way of failing carries no button.
        seed::execution_attempt_ended(
            repo,
            &a.attempt_id,
            ExecutionState::Failed,
            r#"{"code":"token_limit"}"#,
        )
        .await;
        let (_, list) = common::send(
            &b.app,
            "GET",
            &format!("/api/executions/{}/attempts", a.request_id),
            Value::Null,
            &operator_headers(),
        )
        .await;
        assert!(list["data"][0].get("rerun_at_l3").is_none(), "{list}");
    }
}

#[tokio::test]
async fn a_docket_run_created_at_l3_keeps_that_level_and_any_other_run_is_refused_a_level() {
    let root = tempfile::tempdir().unwrap();
    let prill = fake_prill(root.path());
    let b = board(root.path(), None, &prill).await;
    let a = request_and_claim(&b.app, &b.item_id, "cap", None).await;
    let profile =
        tack_db::queries::execution_requests::request_snapshot(&b.state.repo, &a.request_id)
            .await
            .unwrap()
            .map(|raw| serde_json::from_str::<Value>(&raw).unwrap()["agent_profile_id"].clone())
            .unwrap();
    // (harness, level, key, the field the refusal names, or none when the run is created)
    for (harness, autonomy, key, refused) in [
        ("docket", "L3", "again", None),
        ("docket", "L9", "unknown", Some("autonomy")),
        ("codex", "L3", "not-docket", Some("autonomy")),
    ] {
        let mut body = crate::handlers::mrp::execution_request_body(
            &b.item_id,
            key,
            &a.runner_id,
            profile.as_str().unwrap(),
            None,
        );
        body["requested_harness_kind"] = json!(harness);
        body["autonomy"] = json!(autonomy);
        let (status, created) =
            common::send(&b.app, "POST", "/api/executions", body, &operator_headers()).await;
        if let Some(field) = refused {
            assert_eq!(
                status,
                StatusCode::BAD_REQUEST,
                "{harness} {autonomy}: {created}"
            );
            assert_eq!(created["error"]["details"]["field"], field, "{created}");
            continue;
        }
        assert_eq!(status, StatusCode::OK, "{created}");
        let stored = tack_db::queries::execution_requests::request_snapshot(
            &b.state.repo,
            created["request_id"].as_str().unwrap(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&stored).unwrap()["autonomy"],
            "L3"
        );
    }
}
