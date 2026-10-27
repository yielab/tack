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
    let repo = tack_test_support::setup_test_db().await;
    tack_db::repo::execution::seed_builtin_profiles(&repo)
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
    let state = AppState {
        repo,
        config,
        workspace_id,
        broadcast_tx: tx,
        webhook: None,
        local_runner: None,
        tasks: Default::default(),
    };
    common::with_default_host(build_router(state))
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
    assert_eq!(project["vocabulary"]["epic"], "Colección");
    for (key, value) in [
        ("repository", "/home/person/Sites/my-saas"),
        ("on_finish_status", "En revisión"),
    ] {
        assert_eq!(project[key], value, "{key}");
    }
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
    let fox = item("El zorro que no podía dormir");
    assert_eq!(fox["roles"], json!(["Contenido"]));
    assert_eq!(fox["sprint"], "Semana 42");
    assert_eq!(fox["fields"]["Dialecto"], "rioplatense");
    assert_eq!(fox["fields"]["Edad"], "3-5");
    assert_eq!(fox["fields"]["Canal"], json!(["app"]));
    assert_eq!(fox["estimate_unit"], "hours");
    let news = item("Newsletter semanal");
    assert_eq!(news["roles"], json!(["Contenido", "Crecimiento"]));
    assert_eq!(news["recurrence"], "weekly");
}

#[tokio::test]
async fn a_dry_run_writes_nothing_and_a_second_import_adds_and_updates() {
    let app = app().await;
    let projects = count(&app, "/api/projects").await;
    let profiles = count(&app, "/api/agent-profiles").await;
    // An older file still says `usd_per_day`: it imports without it, and the preview says so.
    let older = MY_SAAS.replace(
        "tokens_per_day: 2000000,",
        "tokens_per_day: 2000000, usd_per_day: 20,",
    );
    let (status, out) = import(&app, "/api/projects/outline?dry_run=true", &older).await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert_eq!(out["dry_run"], true);
    let ignored = json!("Dollars per day is no longer a limit; this value is ignored.");
    assert!(
        out["warnings"].as_array().unwrap().contains(&ignored),
        "{out}"
    );
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
async fn a_file_that_cannot_be_written_is_refused_whole() {
    let app = app().await;
    let projects = count(&app, "/api/projects").await;
    let bad_value = MY_SAAS.replacen("Edad: \"3-5\"", "Edad: \"12\"", 1);
    let bad_role = MY_SAAS.replacen(
        "roles: [Contenido], sprint: Semana 42, fields: { Dialecto: rioplatense, Edad: \"6-8\"",
        "roles: [Nadie], sprint: Semana 42, fields: { Dialecto: rioplatense, Edad: \"6-8\"",
        1,
    );
    for (file, wanted) in [
        (bad_value, "Edad"),
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
    let (status, again) = import(&app, &format!("/api/projects/{pid}/outline"), &first).await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(get(&app, &uri).await["auto_start_enabled"], false);

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
