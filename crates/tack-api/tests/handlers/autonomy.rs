//! Autonomy settings (`GET/PUT /api/projects/{id}/autonomy`) and the automatic
//! merge: `autonomy::tick` against a mock GitHub and a fake prill. A pack held
//! for "CI still running" merges on the next tick with the head sha, then a
//! revert demotes its domain L4 to L3; one row each for the holds.

use crate::common;
use crate::mrp::{
    BASE_REVISION, ClaimedAttempt, MRP_MEDIA_TYPE, READY, accept_and_start,
    completion_with_pushed_branch, headers_ref, operator_headers, request_and_claim, sha256_hex,
};
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::{AppState, router::build_router};
use tack_db::{Repository, init_pool, migrations};
use tower::ServiceExt;
use uuid::Uuid;
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
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_all(&pool).await.expect("migrations");
    let workspace_id = Uuid::new_v4();
    sqlx::query("INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'A2', '{}')")
        .bind(workspace_id.to_string())
        .execute(&pool)
        .await
        .unwrap();
    let repo = Repository::new(pool);
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
            "budgets": {"tokens_per_day": null, "usd_per_day": null, "auto_merges_per_day": setup.merges_per_day, "queue_items_max": null},
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
    sqlx::query_as(
        "SELECT trigger, domain, level, held_reason, merged_at, merge_sha FROM tack_merges WHERE attempt_id = ?",
    )
    .bind(attempt_id)
    .fetch_optional(b.state.repo.pool())
    .await
    .unwrap()
}

pub(crate) async fn comments(b: &Board) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT content FROM comments WHERE item_id = ? AND author = 'system'",
    )
    .bind(&b.item_id)
    .fetch_all(b.state.repo.pool())
    .await
    .unwrap()
    .join("\n")
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
    let snapshot: String =
        sqlx::query_scalar("SELECT request_snapshot FROM execution_requests WHERE id = ?")
            .bind(&a.request_id)
            .fetch_one(b.state.repo.pool())
            .await
            .unwrap();
    let snapshot: Value = serde_json::from_str(&snapshot).unwrap();
    assert_eq!(snapshot["verify_require"], json!(["criteria", "suite"]));

    tack_api::autonomy::tick(&b.state).await;
    let row = merge_row(&b, &a.attempt_id).await.expect("a held row");
    assert_eq!(
        row.3.as_deref(),
        Some("CI is still running on the head commit"),
        "{row:?}"
    );
    assert!(row.4.is_none());

    tack_api::autonomy::tick(&b.state).await;
    let row = merge_row(&b, &a.attempt_id).await.unwrap();
    assert_eq!(row.0, "auto");
    assert_eq!(row.1.as_deref(), Some("core"));
    assert_eq!(row.2.as_deref(), Some("L4"));
    assert!(row.3.is_none() && row.4.is_some(), "{row:?}");
    assert_eq!(
        row.5.as_deref(),
        Some("e5bd3914e2e596debea16f433f57875b5b90bcd6")
    );
    let keyid: Option<String> =
        sqlx::query_scalar("SELECT attestation_keyid FROM tack_merges WHERE attempt_id = ?")
            .bind(&a.attempt_id)
            .fetch_one(b.state.repo.pool())
            .await
            .unwrap();
    assert_eq!(keyid.as_deref(), Some(KEY_ID));

    // G sees the revert; the next tick takes the domain one level down.
    tack_db::repo::pull_requests::mark_reverted(b.state.repo.pool(), "acme/widgets", 7, Some(9))
        .await
        .unwrap();
    tack_api::autonomy::tick(&b.state).await;
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
    let demoted: Option<String> =
        sqlx::query_scalar("SELECT demoted_at FROM tack_merges WHERE attempt_id = ?")
            .bind(&a.attempt_id)
            .fetch_one(b.state.repo.pool())
            .await
            .unwrap();
    assert!(demoted.is_some());
    let said = comments(&b).await;
    assert!(
        said.contains("core") && said.contains("L4") && said.contains("L3") && said.contains("#7"),
        "{said}"
    );
    tack_api::autonomy::tick(&b.state).await; // a demotion is made once
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
        tack_api::autonomy::tick(&b.state).await;
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
        let pool = b.state.repo.pool();
        match case {
            "item done" => {
                sqlx::query(
                    "UPDATE items SET completed_at = datetime('now') WHERE id = \
                     (SELECT r.item_id FROM execution_attempts a \
                      JOIN execution_requests r ON r.id = a.request_id WHERE a.id = ?)",
                )
                .bind(&a.attempt_id)
                .execute(pool)
                .await
                .unwrap();
            }
            _ => {
                // A newer run of the same item makes this attempt a replaced one.
                let _ = request_and_claim(&b.app, &b.item_id, "a2-newer", None).await;
            }
        }
        tack_api::autonomy::tick(&b.state).await;
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
            Some("fab-dev"),
            0,
            Some("Team fab-dev is not reported yet by this computer"),
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
        let pool = b.state.repo.pool();
        let kind: String = sqlx::query_scalar(
            "SELECT json_extract(request_snapshot, '$.requested_harness_kind') FROM execution_requests WHERE id = ?",
        )
        .bind(&a.request_id)
        .fetch_one(pool)
        .await
        .unwrap();
        if let Some(team) = team {
            sqlx::query(
                "UPDATE execution_requests SET request_snapshot = json_set(request_snapshot, '$.resolved_agent_profile.tool_policy.docket.team', ?) WHERE id = ?",
            )
            .bind(team)
            .bind(&a.request_id)
            .execute(pool)
            .await
            .unwrap();
        }
        sqlx::query("UPDATE agent_runners SET capability_snapshot = ? WHERE id = ?")
            .bind(reviewers_snapshot(&kind))
            .bind(&a.runner_id)
            .execute(pool)
            .await
            .unwrap();

        tack_api::autonomy::tick(&b.state).await;
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

    let pool = b.state.repo.pool();
    for sql in [
        "INSERT INTO agent_profiles (id, name, instructions, tool_policy, limits, created_at, updated_at) VALUES ('p1', 'Reviewer', 'x', '{\"docket\":{\"team\":\"reviewers\"}}', '{}', 'now', 'now')",
    ] {
        sqlx::query(sql).execute(pool).await.unwrap();
    }
    sqlx::query("INSERT INTO roles (id, project_id, name, agent_profile_id) VALUES ('r1', ?, 'review', 'p1')")
        .bind(&b.project_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO agent_runners (id, name, credential_hash, capability_snapshot, protocol_version, created_at, updated_at) VALUES ('ru1', 'ru', 'h', ?, 1, 'now', 'now')")
        .bind(reviewers_snapshot("docket"))
        .execute(pool)
        .await
        .unwrap();

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
    sqlx::query("INSERT INTO agent_profiles (id, name, instructions, tool_policy, limits, created_at, updated_at) VALUES ('p2', 'Writer', 'x', '{\"docket\":{\"team\":\"writers\"}}', '{}', 'now', 'now'), ('p3', 'Ghost', 'x', '{\"docket\":{\"team\":\"ghosts\"}}', '{}', 'now', 'now')")
        .execute(pool).await.unwrap();
    sqlx::query("INSERT INTO roles (id, project_id, name, agent_profile_id) VALUES ('r2', ?, 'write', 'p2'), ('r3', ?, 'haunt', 'p3')")
        .bind(&b.project_id).bind(&b.project_id).execute(pool).await.unwrap();
    sqlx::query("UPDATE agent_runners SET capability_snapshot = ? WHERE id = 'ru1'")
        .bind(json!({"harnesses": [{
            "harness_kind": "docket", "installed_version": "0.2.0", "probe_error": null,
            "probed_at": "2026-08-06T11:59:55Z", "autonomy_ceiling": "L3",
            "teams": [
                {"name": "reviewers", "profile": "default", "violations": [{"requirement": "r", "fix": "run it lights-out"}]},
                {"name": "writers", "profile": "lights-out", "violations": []}],
        }]}).to_string())
        .execute(pool).await.unwrap();
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
            "budgets": {"tokens_per_day": null, "usd_per_day": null, "auto_merges_per_day": 5, "queue_items_max": null},
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
    tack_api::autonomy::tick(&b.state).await;
    tack_api::autonomy::tick(&b.state).await;
    assert_eq!(merge_row(&b, &a.attempt_id).await.unwrap().0, "auto");
    let (_, project) = common::send(&b.app, "GET", &uri, Value::Null, &operator_headers()).await;
    assert_eq!(project["auto_start_enabled"], true, "{project}");

    // The project had already dropped a level when this merge is reverted: L4 to L3.
    sqlx::query("UPDATE projects SET autonomy_level = 'L4' WHERE id = ?")
        .bind(&b.project_id)
        .execute(b.state.repo.pool())
        .await
        .unwrap();
    tack_db::repo::pull_requests::mark_reverted(b.state.repo.pool(), "acme/widgets", 7, Some(9))
        .await
        .unwrap();
    tack_api::autonomy::tick(&b.state).await;
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
