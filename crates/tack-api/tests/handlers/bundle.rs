//! The project file (ADR 0022) through the outline, export and template
//! routes: My SaaS's whole project in one file, a dry run, a second import
//! into the same project, the refusals, the autonomy rule and the round trip.

use crate::common;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use serde_json::{Value, json};
use tack_api::handlers::websocket::BoardEvent;
use tack_api::{AppState, config::AppConfig, router::build_router};
use tokio::sync::broadcast;
use tower::ServiceExt;

/// An app whose board has its built-in profiles and templates, as a started server does.
pub(crate) async fn app() -> Router {
    app_and_repo().await.0
}

/// [`app`], with the repository it runs on, for a test that plants what a runner reports.
pub(crate) async fn app_and_repo() -> (Router, tack_db::Repository) {
    let repo = tack_test_support::setup_test_db().await;
    tack_db::repo::execution::seed_builtin_profiles(&repo, chrono::Utc::now())
        .await
        .expect("built-in profiles");
    tack_db::repo::templates::seed_builtin_templates(&repo)
        .await
        .expect("built-in templates");
    let workspace_id = tack_test_support::create_test_workspace(&repo).await;
    let (tx, _rx) = broadcast::channel::<BoardEvent>(16);
    let config = AppConfig {
        database_url: "sqlite::memory:".to_string(),
        ..AppConfig::default()
    };
    let handle = repo.clone();
    let state = AppState {
        notifications: crate::common::inbox(&repo),
        repo,
        config,
        workspace_id,
        broadcast_tx: tx,
        event_bus: Default::default(),
        local_runner: None,
        tasks: Default::default(),
    };
    (common::with_default_host(build_router(state)), handle)
}

pub(crate) const MY_SAAS: &str = include_str!("../fixtures/bundles/my-saas.yaml");

pub(crate) async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    body: &str,
) -> (StatusCode, String) {
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("Content-Type", "application/yaml")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let bytes = to_bytes(res.into_body(), 1 << 22).await.unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

pub(crate) async fn import(app: &Router, uri: &str, file: &str) -> (StatusCode, Value) {
    let (status, text) = call(app, Method::POST, uri, file).await;
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

pub(crate) async fn get(app: &Router, uri: &str) -> Value {
    common::send(app, "GET", uri, Value::Null, &[]).await.1
}

fn section<'a>(out: &'a Value, name: &str) -> &'a Value {
    out["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["section"] == name)
        .unwrap_or_else(|| panic!("no section {name} in {}", out["sections"]))
}

async fn count(app: &Router, uri: &str) -> usize {
    let list = get(app, uri).await;
    list.get("data")
        .unwrap_or(&list)
        .as_array()
        .map_or(0, Vec::len)
}

#[tokio::test]
async fn the_fixture_makes_a_whole_project() {
    let app = app().await;
    let (status, out) = import(&app, "/api/projects/outline", MY_SAAS).await;
    assert_eq!(status, StatusCode::OK, "{out}");
    let pid = out["project"]["id"].as_str().unwrap();
    let project = get(&app, &format!("/api/projects/{pid}")).await;

    assert_eq!(project["workflow"]["statuses"].as_array().unwrap().len(), 5);
    assert_eq!(
        project["vocabulary"]["epic"],
        Value::Null,
        "types are not words"
    );
    assert_eq!(project["vocabulary"]["sprint"], "Semana");
    let named = |key: &str| {
        project["item_types"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["key"] == key)
            .unwrap_or_else(|| panic!("no type {key}"))
            .clone()
    };
    assert_eq!(named("epic")["name"], "Iniciativa");
    assert_eq!(named("epic")["level"], "group");
    assert_eq!(named("task")["name"], "Tarea");
    assert_eq!(named("task")["default_work"], true);
    assert_eq!(project["item_types"].as_array().unwrap().len(), 5);
    assert_eq!(project["repository"], "/home/person/Sites/my-saas");
    assert_eq!(project["push_after_run"], true);
    assert_eq!(project["auto_start_enabled"], false);

    let profiles = get(&app, "/api/agent-profiles").await;
    let profiles = profiles["data"].as_array().unwrap();
    let id_of = |name: &str| {
        profiles
            .iter()
            .find(|p| p["name"] == name)
            .unwrap_or_else(|| panic!("no profile {name}"))["agent_profile_id"]
            .clone()
    };
    assert_eq!(project["default_profile_id"], id_of("My SaaS implementer"));
    assert_eq!(section(&out, "profiles")["created"], 6);

    let roles = get(&app, &format!("/api/projects/{pid}/roles")).await;
    let roles = roles.as_array().unwrap();
    assert_eq!(roles.len(), 7);
    for (role, profile) in [
        ("Contenido", "My SaaS content writer"),
        ("Seguridad", "Security review"),
        ("Planificación", "Planner"),
    ] {
        let found = roles.iter().find(|r| r["name"] == role).unwrap();
        assert_eq!(found["agent_profile_id"], id_of(profile), "{role}");
    }
    assert_eq!(
        count(&app, &format!("/api/projects/{pid}/custom-fields")).await,
        4
    );
    let sprints = get(&app, &format!("/api/projects/{pid}/sprints")).await;
    assert!(
        sprints
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["status"] == "planning")
    );
    assert_eq!(sprints.as_array().unwrap().len(), 2);

    let autonomy = get(&app, &format!("/api/projects/{pid}/autonomy")).await;
    assert_eq!(autonomy["level"], "L3");
    assert_eq!(autonomy["budgets"]["waiting_max"], 2);
    assert_eq!(autonomy["domains"][0]["level"], "L3");
    let warnings = section(&out, "automation")["warnings"].to_string();
    assert!(
        warnings.contains("autonomy.level asked: L4; applied: L3"),
        "{warnings}"
    );
    assert!(
        warnings.contains("auto_start asked: true; applied: false"),
        "{warnings}"
    );

    assert_eq!(out["created"], 20);
    assert_eq!(out["dependencies"], 6); // the fixture has six depends_on entries; the plan said 5
    let file = get(
        &app,
        &format!("/api/projects/{pid}/export?scope=bundle&format=json"),
    )
    .await;
    let item = |title: &str| {
        file["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["title"] == title)
            .unwrap_or_else(|| panic!("no item {title}"))
            .clone()
    };
    // The file carries the catalog back out, in order, and the items name their type by key.
    let exported: Vec<&str> = file["types"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["key"].as_str().unwrap())
        .collect();
    assert_eq!(exported, ["epic", "feature", "task", "bug", "requirement"]);
    assert_eq!(file["types"][2]["default"], true);
    assert_eq!(file["types"][0]["level"], "group");
    assert_eq!(item("Facturación recurrente")["type"], "epic");
    let guide = item("Guía para crear tu primera factura");
    assert_eq!(guide["roles"], json!(["Contenido"]));
    assert_eq!(guide["sprint"], "Semana 42");
    assert_eq!(guide["fields"]["Plan"], "pro");
    assert_eq!(guide["fields"]["Segmento"], "autónomo");
    assert_eq!(guide["fields"]["Canal"], json!(["app"]));
    assert_eq!(guide["estimate_unit"], "hours");
    let news = item("Newsletter semanal");
    assert_eq!(news["roles"], json!(["Contenido", "Crecimiento"]));
    assert_eq!(news["recurrence"], "weekly");

    // The two briefs of the file are two brief versions, one per item, and the export carries them.
    let listed = get(&app, &format!("/api/projects/{pid}/items")).await;
    let mut versions = 0;
    for card in listed["data"].as_array().unwrap() {
        let uri = format!("/api/items/{}/brief/versions", card["id"].as_str().unwrap());
        versions += get(&app, &uri).await.as_array().map_or(0, Vec::len);
    }
    assert_eq!(versions, 2);
    let bug = item("El PDF se corta al imprimir facturas largas");
    assert_eq!(bug["brief"]["risk"], "medium");
    assert_eq!(bug["brief"]["acceptance"][0]["id"], "pdf-largo");
    assert!(news["brief"].is_null(), "an item with no brief writes none");
}

#[tokio::test]
async fn the_item_list_says_whether_a_task_can_be_verified() {
    let app = app().await;
    let (_, out) = import(&app, "/api/projects/outline", MY_SAAS).await;
    let pid = out["project"]["id"].as_str().unwrap();
    let listed = get(&app, &format!("/api/projects/{pid}/items")).await;
    let card = |title: &str| {
        listed["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["title"] == title)
            .unwrap_or_else(|| panic!("no item {title}"))
            .clone()
    };
    let bug = card("El PDF se corta al imprimir facturas largas");
    assert_eq!(
        bug["brief_summary"],
        json!({"done_when": true, "checks": 1, "risk": "medium", "not_ready": null})
    );
    // An item with no brief has the project's Done when, which the file gives.
    let news = card("Newsletter semanal");
    assert_eq!(
        news["brief_summary"],
        json!({"done_when": true, "checks": 0, "risk": null, "not_ready": null})
    );
    // The run dialog reads the same summary from the one item.
    let one = get(
        &app,
        &format!("/api/items/{}", news["id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(one["item"]["brief_summary"], news["brief_summary"]);

    // A project with no Done when says why its bare task cannot be verified.
    let tiny = "project: { name: Tiny }\nitems: [{ title: Sin criterios }]";
    let (_, out) = import(&app, "/api/projects/outline", tiny).await;
    let pid = out["project"]["id"].as_str().unwrap();
    let listed = get(&app, &format!("/api/projects/{pid}/items")).await;
    assert_eq!(
        listed["data"][0]["brief_summary"]["not_ready"],
        "No Done when and no check a machine runs"
    );
}

#[tokio::test]
async fn a_dry_run_writes_nothing_and_a_second_import_adds_and_updates() {
    let app = app().await;
    let projects = count(&app, "/api/projects").await;
    let profiles = count(&app, "/api/agent-profiles").await;
    let (status, out) = import(&app, "/api/projects/outline?dry_run=true", MY_SAAS).await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert_eq!(out["dry_run"], true);
    assert_eq!(section(&out, "roles")["created"], 7);
    assert_eq!(count(&app, "/api/projects").await, projects);
    assert_eq!(count(&app, "/api/agent-profiles").await, profiles);

    let (_, out) = import(&app, "/api/projects/outline", MY_SAAS).await;
    let pid = out["project"]["id"].as_str().unwrap().to_string();
    let uri = format!("/api/projects/{pid}/outline");
    let (status, again) = import(&app, &uri, MY_SAAS).await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(section(&again, "roles")["updated"], 7);
    assert_eq!(section(&again, "fields")["updated"], 4);
    let items = get(&app, &format!("/api/projects/{pid}/items")).await;
    assert_eq!(items["data"].as_array().unwrap().len(), 40);

    let (_, made) = common::send(
        &app,
        "POST",
        &format!("/api/projects/{pid}/roles"),
        json!({"name": "A mano"}),
        &[],
    )
    .await;
    assert!(made["id"].is_string(), "{made}");
    let (_, third) = import(&app, &uri, MY_SAAS).await;
    assert_eq!(section(&third, "roles")["not_mentioned"], 1);
    assert_eq!(count(&app, &format!("/api/projects/{pid}/roles")).await, 8);
}

#[tokio::test]
async fn a_budget_the_file_names_but_tack_does_not_have_is_refused() {
    let app = app().await;
    let projects = count(&app, "/api/projects").await;
    let unknown = MY_SAAS.replace(
        "tokens_per_day: 2000000,",
        "tokens_per_day: 2000000, turns_per_day: 20,",
    );
    let (status, out) = import(&app, "/api/projects/outline", &unknown).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{out}");
    assert!(out.to_string().contains("turns_per_day"), "{out}");
    assert_eq!(count(&app, "/api/projects").await, projects);
}

#[tokio::test]
async fn a_file_that_cannot_be_written_is_refused_whole() {
    let (app, repo) = app_and_repo().await;
    let projects = count(&app, "/api/projects").await;
    let bad_value = MY_SAAS.replacen("Segmento: autónomo", "Segmento: gigante", 1);
    let bad_role = MY_SAAS.replacen(
        "roles: [Contenido], sprint: Semana 42, fields: { Plan: pro, Segmento: pyme",
        "roles: [Nadie], sprint: Semana 42, fields: { Plan: pro, Segmento: pyme",
        1,
    );
    for (file, wanted) in [
        (bad_value, "Segmento"),
        (bad_role, "Nadie"),
        (
            "project: { name: X, workflow: { preset: agile } }\n".to_string(),
            "scrum",
        ),
    ] {
        let (status, text) = call(&app, Method::POST, "/api/projects/outline", &file).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{text}");
        assert!(text.contains(wanted), "{wanted} not in {text}");
    }
    assert_eq!(count(&app, "/api/projects").await, projects);

    // A file the checks pass and the store cannot write: its last section fails, and nothing
    // of the sections before it is kept, in the project that exists or in the one it would make.
    let (_, out) = import(&app, "/api/projects/outline", MY_SAAS).await;
    let pid = out["project"]["id"].as_str().unwrap().to_string();
    tack_test_support::seed::statement(
        &repo,
        "CREATE TRIGGER fail_planted_item BEFORE INSERT ON items WHEN NEW.title = 'Planted' BEGIN SELECT RAISE(ABORT, 'injected failure'); END",
        &[],
    )
    .await;
    let counts = || async {
        let mut counts = Vec::new();
        for part in ["roles", "custom-fields", "sprints", "items"] {
            counts.push(count(&app, &format!("/api/projects/{pid}/{part}")).await);
        }
        counts.push(count(&app, "/api/agent-profiles").await);
        counts.push(count(&app, "/api/projects").await);
        counts
    };
    let before = counts().await;
    let unwritable = "profiles:\n  - { name: Planted profile }\n\
        fields:\n  - { name: Planted field, type: text }\n\
        roles:\n  - { name: Planted role }\n\
        sprints:\n  - { name: Planted sprint }\n\
        items:\n  - { title: Planted }\n";
    let (status, text) = call(
        &app,
        Method::POST,
        &format!("/api/projects/{pid}/outline"),
        unwritable,
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{text}");
    assert!(
        text.contains("The items section could not be written; nothing was changed."),
        "{text}"
    );
    assert_eq!(counts().await, before);
    let named = format!("project: {{ name: Planted project }}\n{unwritable}");
    let (status, text) = call(&app, Method::POST, "/api/projects/outline", &named).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{text}");
    assert!(text.contains("The items section"), "{text}");
    assert_eq!(counts().await, before);
}

#[tokio::test]
async fn an_import_never_raises_autonomy_past_the_projects_own() {
    let app = app().await;
    let (_, out) = import(&app, "/api/projects/outline", MY_SAAS).await;
    let pid = out["project"]["id"].as_str().unwrap();
    let mut settings = get(&app, &format!("/api/projects/{pid}/autonomy")).await;
    settings["level"] = json!("L5");
    let (status, put) = common::send(
        &app,
        "PUT",
        &format!("/api/projects/{pid}/autonomy"),
        settings,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{put}");

    let (status, out) = import(&app, &format!("/api/projects/{pid}/outline"), MY_SAAS).await;
    assert_eq!(status, StatusCode::OK, "{out}");
    let autonomy = get(&app, &format!("/api/projects/{pid}/autonomy")).await;
    assert_eq!(autonomy["level"], "L4");
    let warnings = section(&out, "automation")["warnings"].to_string();
    assert!(!warnings.contains("autonomy.level"), "{warnings}");
}

#[tokio::test]
async fn the_export_is_the_import() {
    let app = app().await;
    let (_, out) = import(&app, "/api/projects/outline", MY_SAAS).await;
    let pid = out["project"]["id"].as_str().unwrap();
    let uri = format!("/api/projects/{pid}/export?scope=bundle&format=yaml");
    let (status, first) = call(&app, Method::GET, &uri, "").await;
    assert_eq!(status, StatusCode::OK, "{first}");
    // Auto-start is off, and the file says so: an absent key would leave a project's switch on.
    assert!(first.contains("enabled: false"), "{first}");
    let uri = format!("/api/projects/{pid}");
    let on = json!({"auto_start_enabled": true, "auto_start_status": "Por hacer"});
    let (status, patched) = common::send(&app, "PATCH", &uri, on, &[]).await;
    assert_eq!(status, StatusCode::OK, "{patched}");
    assert_eq!(get(&app, &uri).await["auto_start_enabled"], true);
    let items_uri = format!("/api/projects/{pid}/items");
    let items_before = get(&app, &items_uri).await;
    let (status, again) = import(&app, &format!("/api/projects/{pid}/outline"), &first).await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(get(&app, &uri).await["auto_start_enabled"], false);
    // Its items carry their keys, so the same file adds none and changes none.
    assert_eq!(section(&again, "items")["created"], 0);
    assert_eq!(get(&app, &items_uri).await, items_before);

    // A new title under the same key renames the card.
    let edited = first.replace("Newsletter semanal", "Newsletter quincenal");
    let (status, answer) = import(&app, &format!("/api/projects/{pid}/outline"), &edited).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(section(&answer, "items")["updated"], 20);
    let titles = |list: &Value| -> Vec<String> {
        list["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["title"].as_str().unwrap().to_owned())
            .collect()
    };
    let after = titles(&get(&app, &items_uri).await);
    assert_eq!(after.len(), 20);
    assert!(after.contains(&"Newsletter quincenal".to_string()));
    assert!(!after.contains(&"Newsletter semanal".to_string()));

    let (status, copy) = import(&app, "/api/projects/outline", &first).await;
    assert_eq!(status, StatusCode::OK, "{copy}");
    let copy_id = copy["project"]["id"].as_str().unwrap();
    let (_, second) = call(
        &app,
        Method::GET,
        &format!("/api/projects/{copy_id}/export?scope=bundle&format=yaml"),
        "",
    )
    .await;
    assert_eq!(first, second);
    let file: Value = serde_saphyr::from_str(&first).expect("the export is YAML");
    for section in ["fields", "profiles", "roles", "sprints"] {
        let entries = file[section].as_array().expect("the section is a list");
        assert!(!entries.is_empty(), "{section} in the export");
        for entry in entries {
            assert!(entry["key"].is_string(), "{section}: {entry}");
        }
    }

    // A second import of the file that renames Contenido under its key updates the same row.
    let roles_uri = format!("/api/projects/{pid}/roles");
    let before = get(&app, &roles_uri).await;
    let contenido = before
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == "contenido")
        .expect("the role Contenido has its key");
    let renamed = first.replace("Contenido", "Content");
    let (status, answer) = import(&app, &format!("/api/projects/{pid}/outline"), &renamed).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    let after = get(&app, &roles_uri).await;
    assert_eq!(
        after.as_array().unwrap().len(),
        before.as_array().unwrap().len()
    );
    let content = after
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == "contenido")
        .expect("the role keeps its key");
    assert_eq!(content["name"], "Content");
    assert_eq!(content["id"], contenido["id"]);

    let (_, json_text) = call(
        &app,
        Method::GET,
        &format!("/api/projects/{pid}/export?scope=bundle&format=json"),
        "",
    )
    .await;
    fn keys(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                for (k, v) in map {
                    // A brief's criteria have ids of their own; they are not the board's.
                    if k == "brief" {
                        continue;
                    }
                    out.push(k.clone());
                    keys(v, out);
                }
            }
            Value::Array(list) => list.iter().for_each(|v| keys(v, out)),
            _ => {}
        }
    }
    let mut found = Vec::new();
    keys(&serde_json::from_str(&json_text).unwrap(), &mut found);
    for key in ["id", "project_id", "created_at", "updated_at"] {
        assert!(!found.contains(&key.to_string()), "key {key} in the export");
    }
    let csv = call(
        &app,
        Method::GET,
        &format!("/api/projects/{pid}/export?scope=bundle&format=csv"),
        "",
    )
    .await;
    assert_eq!(csv.0, StatusCode::BAD_REQUEST);
}

/// A runner that reports `claude-code` signed in, and nothing else.
async fn claude_only(repo: &tack_db::Repository) {
    let snapshot = json!({"harnesses": [{
        "harness_kind": "claude-code", "installed_version": "2", "probe_error": null,
        "probed_at": "2026-11-03T12:00:00Z",
    }]})
    .to_string();
    tack_test_support::seed::agent_runner_reporting(repo, "ru1", &snapshot).await;
}

/// The preview lists what the file expects and no computer reports, each with its fix, and an
/// import creates nothing from it.
#[tokio::test]
async fn the_preview_lists_the_team_and_the_agent_no_computer_reports_with_their_fixes() {
    let (app, repo) = app_and_repo().await;
    claude_only(&repo).await;
    let file = r#"
project: { name: Business }
teams:
  - { name: biz-respond, shape: respond, delivers: report, methods: [research-review] }
profiles:
  - { name: Coder, tool_policy: { agent: claude-code } }
  - { name: Support, tool_policy: { agent: codex } }
roles:
  - { name: Support, profile: Support }
"#;
    let (status, out) = import(&app, "/api/projects/outline?dry_run=true", file).await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert_eq!(
        out["missing"],
        json!([
            {
                "kind": "team", "name": "biz-respond", "shape": "respond",
                "fix": "docket init biz-respond --blueprint respond --recipe research-review"
            },
            {
                "kind": "harness", "name": "codex",
                "fix": "No computer reports codex. Install it on a computer that runs a runner, then run `tack runner doctor` there to check it."
            },
        ]),
        "{out}"
    );
    // The same list comes with a real import, and the import wrote no team anywhere.
    let (status, made) = import(&app, "/api/projects/outline", file).await;
    assert_eq!(status, StatusCode::OK, "{made}");
    assert_eq!(made["missing"], out["missing"], "{made}");
    // A file that expects nothing of the kind has an empty list.
    let (_, plain) = import(
        &app,
        "/api/projects/outline?dry_run=true",
        "project: { name: Plain }\n",
    )
    .await;
    assert_eq!(plain["missing"], json!([]), "{plain}");
}

/// A team a runner reports is not missing, and a file naming only reported things lists nothing.
#[tokio::test]
async fn a_reported_team_and_a_reported_agent_are_not_missing() {
    let (app, repo) = app_and_repo().await;
    let snapshot = json!({"harnesses": [
        {"harness_kind": "docket", "installed_version": "0.3", "probe_error": null,
         "probed_at": "2026-11-03T12:00:00Z",
         "teams": [{"name": "biz-respond", "profile": "attended", "violations": []}]},
    ]})
    .to_string();
    tack_test_support::seed::agent_runner_reporting(&repo, "ru1", &snapshot).await;
    let file = r#"
project: { name: Business }
teams: [{ name: biz-respond, shape: respond }]
profiles:
  - { name: Support, tool_policy: { agent: docket, docket: { team: biz-respond } } }
"#;
    let (status, out) = import(&app, "/api/projects/outline?dry_run=true", file).await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert_eq!(out["missing"], json!([]), "{out}");
}

/// "Digital business" imports on a board whose runners report no docket at all, brings six
/// departments that name no agent, and its export imports to the same export again.
#[tokio::test]
async fn digital_business_imports_with_no_docket_and_exports_round_trip() {
    let (app, repo) = app_and_repo().await;
    claude_only(&repo).await;
    let templates = get(&app, "/api/templates").await;
    let template = templates
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "Digital business")
        .expect("the built-in template");
    let teams: Vec<&str> = template["teams"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["shape"].as_str().unwrap())
        .collect();
    assert_eq!(
        teams,
        ["build", "respond", "operate", "routine"],
        "{template}"
    );

    let (status, project) = common::send(
        &app,
        "POST",
        &format!(
            "/api/projects/from-template/{}",
            template["id"].as_str().unwrap()
        ),
        json!({"name": "Acme"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{project}");
    let pid = project["id"].as_str().unwrap();
    let roles = get(&app, &format!("/api/projects/{pid}/roles")).await;
    let names: Vec<&str> = roles
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    for department in [
        "Software",
        "Operations",
        "Support",
        "Growth",
        "Finance",
        "Insight",
    ] {
        assert!(names.contains(&department), "{names:?}");
    }
    let profiles = get(&app, "/api/agent-profiles").await;
    let departments: Vec<&Value> = profiles["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| {
            p["name"]
                .as_str()
                .is_some_and(|n| n.ends_with(" analyst") || n == "Software engineer")
        })
        .collect();
    assert_eq!(departments.len(), 6, "{profiles}");
    for profile in departments {
        assert!(profile["tool_policy"].get("agent").is_none(), "{profile}");
        assert!(profile["tool_policy"].get("docket").is_none(), "{profile}");
    }

    let uri = format!("/api/projects/{pid}/export?scope=bundle&format=yaml");
    let (status, first) = call(&app, Method::GET, &uri, "").await;
    assert_eq!(status, StatusCode::OK, "{first}");
    let (status, copy) = import(&app, "/api/projects/outline", &first).await;
    assert_eq!(status, StatusCode::OK, "{copy}");
    assert_eq!(
        copy["missing"],
        json!([]),
        "a department names no agent, so nothing is missing: {copy}"
    );
    let copy_id = copy["project"]["id"].as_str().unwrap();
    let (_, second) = call(
        &app,
        Method::GET,
        &format!("/api/projects/{copy_id}/export?scope=bundle&format=yaml"),
        "",
    )
    .await;
    assert_eq!(first, second);
}

/// A project saved as a template keeps the teams its profiles name, as the runners describe them.
#[tokio::test]
async fn a_project_saved_as_a_template_keeps_the_teams_its_profiles_name() {
    let (app, repo) = app_and_repo().await;
    let snapshot = json!({"harnesses": [
        {"harness_kind": "docket", "installed_version": "0.3", "probe_error": null,
         "probed_at": "2026-11-03T12:00:00Z",
         "teams": [{
            "name": "biz-respond", "profile": "attended", "violations": [],
            "shape": "respond", "delivers": "report",
            "methods": [{"name": "research-review", "source": "recipe", "default": true}],
         }]},
    ]})
    .to_string();
    tack_test_support::seed::agent_runner_reporting(&repo, "ru1", &snapshot).await;
    let file = r#"
project: { name: Business }
profiles:
  - { name: Support, tool_policy: { agent: docket, docket: { team: biz-respond } } }
roles:
  - { name: Support, profile: Support }
"#;
    let (_, made) = import(&app, "/api/projects/outline", file).await;
    let (status, saved) = common::send(
        &app,
        "POST",
        &format!(
            "/api/projects/{}/save-as-template",
            made["project"]["id"].as_str().unwrap()
        ),
        json!({"name": "Mine"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(
        saved["teams"],
        json!([{
            "name": "biz-respond", "shape": "respond", "delivers": "report",
            "methods": ["research-review"],
        }]),
        "{saved}"
    );
}
