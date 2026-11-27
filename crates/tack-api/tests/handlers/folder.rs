//! The project folder (ADR 0025) through its six routes: the export and its revisions, the plan
//! and what it costs, the apply in one transaction, and who may apply.

use std::collections::BTreeMap;

use axum::Router;
use axum::http::StatusCode;
use serde_json::{Value, json};
use tack_api::config::AppConfig;

use crate::common::{send, test_app_with_config};
use crate::handlers::bundle::{MY_SAAS, app, import};

/// My SaaS's folder, changed: the role Contenido renamed under its key and a role added, the field
/// Plan and the sprint Semana 42 gone, one brief edited, two items gone.
fn v2() -> BTreeMap<String, String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/bundles/my-saas-folder-v2");
    let mut files = BTreeMap::new();
    let mut read = |relative: String| {
        files.insert(
            relative.clone(),
            std::fs::read_to_string(root.join(&relative)).unwrap(),
        );
    };
    read("project.yaml".into());
    for entry in std::fs::read_dir(root.join("items")).unwrap() {
        read(format!(
            "items/{}",
            entry.unwrap().file_name().to_str().unwrap()
        ));
    }
    files
}

/// A project made from My SaaS's file, and the app it lives in.
async fn my_saas() -> (Router, String) {
    let app = app().await;
    let (status, out) = import(&app, "/api/projects/outline", MY_SAAS).await;
    assert_eq!(status, StatusCode::OK, "{out}");
    let pid = out["project"]["id"].as_str().unwrap().to_owned();
    (app, pid)
}

async fn get(app: &Router, uri: &str) -> Value {
    send(app, "GET", uri, Value::Null, &[]).await.1
}

async fn exported(app: &Router, pid: &str) -> BTreeMap<String, String> {
    let folder = get(app, &format!("/api/projects/{pid}/folder")).await;
    serde_json::from_value(folder["files"].clone()).unwrap()
}

async fn post(
    app: &Router,
    pid: &str,
    route: &str,
    query: &str,
    files: &BTreeMap<String, String>,
) -> (StatusCode, Value) {
    let uri = format!("/api/projects/{pid}/folder/{route}{query}");
    send(app, "POST", &uri, json!({ "files": files }), &[]).await
}

async fn revisions(app: &Router, pid: &str) -> Vec<Value> {
    get(app, &format!("/api/projects/{pid}/folder/revisions"))
        .await
        .as_array()
        .unwrap()
        .clone()
}

async fn rows(app: &Router, pid: &str, what: &str) -> Vec<Value> {
    let list = get(app, &format!("/api/projects/{pid}/{what}")).await;
    list.get("data")
        .unwrap_or(&list)
        .as_array()
        .unwrap()
        .clone()
}

fn change<'a>(plan: &'a Value, section: &str, key: &str, verb: &str) -> Option<&'a Value> {
    plan["changes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["section"] == section && c["key"] == key && c["verb"] == verb)
}

fn removal<'a>(plan: &'a Value, section: &str, key: &str) -> &'a Value {
    plan["removals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["section"] == section && r["key"] == key)
        .unwrap_or_else(|| panic!("no removal {section}/{key} in {}", plan["removals"]))
}

async fn item_id(app: &Router, pid: &str, title: &str) -> String {
    rows(app, pid, "items")
        .await
        .iter()
        .find(|i| i["title"] == title)
        .unwrap_or_else(|| panic!("no item {title}"))["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn an_export_records_a_revision_once() {
    let (app, pid) = my_saas().await;
    exported(&app, &pid).await;
    exported(&app, &pid).await;
    assert_eq!(revisions(&app, &pid).await.len(), 1);

    let (status, role) = send(
        &app,
        "POST",
        &format!("/api/projects/{pid}/roles"),
        json!({"name": "Soporte"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{role}");
    exported(&app, &pid).await;
    let list = revisions(&app, &pid).await;
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["number"], 2);
    assert_eq!(list[0]["origin"], "board");
}

#[tokio::test]
async fn a_plan_names_every_change_and_its_cost() {
    let (app, pid) = my_saas().await;
    exported(&app, &pid).await;
    let commented = item_id(&app, &pid, "Pipeline de PDFs en CI").await;
    let (status, comment) = send(
        &app,
        "POST",
        &format!("/api/items/{commented}/comments"),
        json!({"content": "Esto ya tiene historia"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{comment}");

    let (status, plan) = post(&app, &pid, "plan", "", &v2()).await;
    assert_eq!(status, StatusCode::OK, "{plan}");

    let renamed = change(&plan, "roles", "contenido", "update").expect("the role is renamed");
    assert_eq!(renamed["fields"].as_array().unwrap().len(), 1);
    assert_eq!(renamed["fields"][0]["path"], "name");
    assert!(change(&plan, "roles", "soporte", "create").is_some());
    let brief = change(
        &plan,
        "items",
        "vista-previa-de-la-factura-en-pdf",
        "update",
    )
    .expect("the brief is changed");
    assert!(
        brief["fields"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["path"] == "brief.acceptance"),
        "{brief}"
    );
    assert_eq!(
        removal(&plan, "fields", "plan")["cost"],
        json!(["20 values are deleted with it"])
    );
    assert_eq!(
        removal(&plan, "sprints", "semana-42")["cost"],
        json!(["3 cards leave it"])
    );
    removal(&plan, "items", "informe-mensual-de-métricas");
    let kept = change(&plan, "items", "pipeline-de-pdfs-en-ci", "keep")
        .expect("the item with a comment is kept");
    assert_eq!(kept["reason"], "it has 1 comment");
    assert_eq!(plan["applies"], true);
    // A folder never writes the board's keys, so the plan has nothing to say about them, even for
    // the item a removal keeps because it has history.
    assert!(
        !plan["warnings"].to_string().contains("the board's"),
        "{}",
        plan["warnings"]
    );
    assert_eq!(plan["conflicts"], json!([]));

    // Nothing was written.
    assert_eq!(revisions(&app, &pid).await.len(), 1);
    assert_eq!(rows(&app, &pid, "custom-fields").await.len(), 4);
    assert_eq!(rows(&app, &pid, "sprints").await.len(), 2);
}

#[tokio::test]
async fn an_apply_with_a_stale_hash_is_refused() {
    let (app, pid) = my_saas().await;
    exported(&app, &pid).await;
    let files = v2();
    let (_, plan) = post(&app, &pid, "plan", "", &files).await;
    let hash = plan["plan_hash"].as_str().unwrap();
    let (status, _) = send(
        &app,
        "POST",
        &format!("/api/projects/{pid}/roles"),
        json!({"name": "Otro"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, answer) = post(&app, &pid, "apply", &format!("?plan_hash={hash}"), &files).await;
    assert_eq!(status, StatusCode::CONFLICT, "{answer}");
    assert_eq!(
        answer["error"]["message"],
        "The project changed since the preview; preview again"
    );
    assert_eq!(rows(&app, &pid, "custom-fields").await.len(), 4);
    assert_eq!(rows(&app, &pid, "sprints").await.len(), 2);
}

#[tokio::test]
async fn an_apply_writes_everything_or_nothing_and_leaves_two_revisions() {
    let (app, pid) = my_saas().await;
    exported(&app, &pid).await;
    let files = v2();
    let (_, plan) = post(&app, &pid, "plan", "", &files).await;
    let hash = plan["plan_hash"].as_str().unwrap();
    let (status, applied) = post(&app, &pid, "apply", &format!("?plan_hash={hash}"), &files).await;
    assert_eq!(status, StatusCode::OK, "{applied}");

    let roles = rows(&app, &pid, "roles").await;
    let names: Vec<&str> = roles.iter().map(|r| r["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"Contenido editorial"), "{names:?}");
    assert!(names.contains(&"Soporte"), "{names:?}");
    assert_eq!(rows(&app, &pid, "custom-fields").await.len(), 3);
    assert_eq!(rows(&app, &pid, "sprints").await.len(), 1);
    let titles: Vec<String> = rows(&app, &pid, "items")
        .await
        .iter()
        .map(|i| i["title"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(titles.len(), 18);
    assert!(!titles.contains(&"Informe mensual de métricas".to_owned()));
    let previewed = item_id(&app, &pid, "Vista previa de la factura en PDF").await;
    let versions = get(&app, &format!("/api/items/{previewed}/brief/versions")).await;
    assert_eq!(versions.as_array().unwrap().len(), 2, "{versions}");

    let list = revisions(&app, &pid).await;
    assert_eq!(list[0]["origin"], "file");
    assert_eq!(applied["revision"]["number"], list[0]["number"]);
    assert_eq!(list[1]["origin"], "board");
    let status = get(&app, &format!("/api/projects/{pid}/folder/status")).await;
    assert_eq!(status["changes"], json!([]), "{status}");

    // The first revision, planned, is the inverse; applying it brings the role's name back.
    let board = list.last().unwrap()["number"].as_i64().unwrap();
    let uri = format!("/api/projects/{pid}/folder/plan?revision={board}");
    let (status, back) = send(&app, "POST", &uri, Value::Null, &[]).await;
    assert_eq!(status, StatusCode::OK, "{back}");
    assert_eq!(back["conflicts"], json!([]), "{back}");
    removal(&back, "roles", "soporte");
    let hash = back["plan_hash"].as_str().unwrap();
    let uri = format!("/api/projects/{pid}/folder/apply?revision={board}&plan_hash={hash}");
    let (status, undone) = send(&app, "POST", &uri, Value::Null, &[]).await;
    assert_eq!(status, StatusCode::OK, "{undone}");
    let roles = rows(&app, &pid, "roles").await;
    assert!(roles.iter().any(|r| r["name"] == "Contenido"));
    assert!(!roles.iter().any(|r| r["name"] == "Soporte"));
    assert_eq!(revisions(&app, &pid).await[0]["origin"], "revert");
}

#[tokio::test]
async fn what_the_board_changed_is_kept_and_a_double_change_is_a_conflict() {
    let (app, pid) = my_saas().await;
    let mut files = exported(&app, &pid).await;

    // The board changes a role's colour and an item's description since the revision.
    let roles = rows(&app, &pid, "roles").await;
    let contenido = roles.iter().find(|r| r["key"] == "contenido").unwrap();
    let (status, _) = send(
        &app,
        "PATCH",
        &format!("/api/roles/{}", contenido["id"].as_str().unwrap()),
        json!({"color": "#000001"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let item = item_id(&app, &pid, "Newsletter semanal").await;
    let (status, _) = send(
        &app,
        "PATCH",
        &format!("/api/items/{item}"),
        json!({"description": "Lo que dice el tablero"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // The folder changes the same description, and another role's colour.
    let path = "items/newsletter-semanal.md";
    let text = format!("{}Lo que dice la carpeta\n", files[path]);
    files.insert(path.into(), text);
    let project = files["project.yaml"].replace("color: \"#10b981\"", "color: \"#10b982\"");
    files.insert("project.yaml".into(), project);

    let (status, plan) = post(&app, &pid, "plan", "", &files).await;
    assert_eq!(status, StatusCode::OK, "{plan}");
    // The role's colour the board changed is kept: the folder never named it.
    assert!(
        change(&plan, "roles", "contenido", "update").is_none(),
        "{plan}"
    );
    assert!(
        change(&plan, "roles", "crecimiento", "update").is_some(),
        "{plan}"
    );
    let conflicts = plan["conflicts"].as_array().unwrap();
    assert_eq!(conflicts.len(), 1, "{plan}");
    assert_eq!(conflicts[0]["key"], "newsletter-semanal");
    assert_eq!(conflicts[0]["path"], "description");
    assert_eq!(conflicts[0]["board"], "Lo que dice el tablero");
    assert_eq!(conflicts[0]["file"], "Lo que dice la carpeta\n");
    assert_eq!(plan["applies"], false);

    let hash = plan["plan_hash"].as_str().unwrap();
    let (status, _) = post(&app, &pid, "apply", &format!("?plan_hash={hash}"), &files).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (_, settled) = post(&app, &pid, "plan", "?prefer=board", &files).await;
    assert_eq!(settled["applies"], true);
    assert_eq!(settled["conflicts"].as_array().unwrap().len(), 1);
    let hash = settled["plan_hash"].as_str().unwrap();
    let query = format!("?prefer=board&plan_hash={hash}");
    let (status, applied) = post(&app, &pid, "apply", &query, &files).await;
    assert_eq!(status, StatusCode::OK, "{applied}");
    let after = get(&app, &format!("/api/items/{item}")).await;
    let after = after.get("item").unwrap_or(&after);
    assert_eq!(after["description"], "Lo que dice el tablero");
    let roles = rows(&app, &pid, "roles").await;
    let colour = |key: &str| roles.iter().find(|r| r["key"] == key).unwrap()["color"].clone();
    assert_eq!(colour("contenido"), "#000001");
    assert_eq!(colour("crecimiento"), "#10b982");
}

#[tokio::test]
async fn an_agent_previews_and_a_person_applies() {
    let (app, _) = test_app_with_config(AppConfig {
        api_token: Some("operator-token".into()),
        ..AppConfig::default()
    })
    .await;
    let operator = [("authorization", "Bearer operator-token")];
    let (_, project) = send(
        &app,
        "POST",
        "/api/projects",
        json!({"name": "My SaaS", "project_type": "software"}),
        &operator,
    )
    .await;
    let pid = project["id"].as_str().unwrap();
    let (_, folder) = send(
        &app,
        "GET",
        &format!("/api/projects/{pid}/folder"),
        Value::Null,
        &operator,
    )
    .await;
    let (status, issued) = send(
        &app,
        "POST",
        "/api/agent-credentials",
        json!({"name": "Reader", "scopes": [{"all_projects": true, "capabilities": ["read"]}]}),
        &operator,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{issued}");
    let secret = format!("Bearer {}", issued["secret"].as_str().unwrap());
    let agent = [("authorization", secret.as_str())];

    let body = json!({ "files": folder["files"] });
    let uri = format!("/api/projects/{pid}/folder/plan");
    let (status, plan) = send(&app, "POST", &uri, body.clone(), &agent).await;
    assert_eq!(status, StatusCode::OK, "{plan}");
    let uri = format!(
        "/api/projects/{pid}/folder/apply?plan_hash={}",
        plan["plan_hash"].as_str().unwrap()
    );
    let (status, refused) = send(&app, "POST", &uri, body, &agent).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");
}

#[tokio::test]
async fn a_profile_another_project_uses_is_not_removed() {
    let (app, pid) = my_saas().await;
    let (status, other) = import(&app, "/api/projects/outline", MY_SAAS).await;
    assert_eq!(status, StatusCode::OK, "{other}");
    let files = exported(&app, &pid).await;

    // The folder hands the role Seguridad to the planner and drops the profile it had.
    let text = files["project.yaml"].clone();
    let start = text.find("- key: security-review").unwrap();
    let end = text[start..].find("  egress_mode: closed\n").unwrap()
        + start
        + "  egress_mode: closed\n".len();
    let text = format!("{}{}", &text[..start], &text[end..])
        .replace("profile: Security review", "profile: Planner");
    let mut changed = files.clone();
    changed.insert("project.yaml".into(), text);

    let (status, plan) = post(&app, &pid, "plan", "", &changed).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{plan}");
    let message = plan["error"]["message"].as_str().unwrap();
    assert!(message.contains("Security review"), "{message}");
    assert!(
        message.contains("1 role of other projects use it"),
        "{message}"
    );
}

#[tokio::test]
async fn a_first_plan_with_no_revision_removes_nothing() {
    let (app, pid) = my_saas().await;
    let (status, plan) = post(&app, &pid, "plan", "", &v2()).await;
    assert_eq!(status, StatusCode::OK, "{plan}");
    assert_eq!(plan["base"], Value::Null);
    assert_eq!(plan["removals"], json!([]));
    let kept = change(&plan, "sprints", "semana-42", "keep").expect("the sprint is kept");
    assert_eq!(
        kept["reason"],
        "no revision yet: export once to make the folder authoritative"
    );
}

#[tokio::test]
async fn the_project_block_is_applied_for_three_keys_only() {
    let (app, pid) = my_saas().await;
    let mut files = exported(&app, &pid).await;
    let text = files["project.yaml"]
        .replace(
            "name: My SaaS\n  type: web",
            "name: My SaaS 2\n  type: software",
        )
        .replace("Invoicing and appointment booking", "Invoices and bookings")
        .replace("sprint: Semana", "sprint: Etapa")
        .replace("wip_limit: 5", "wip_limit: 9");
    files.insert("project.yaml".into(), text);
    let (_, plan) = post(&app, &pid, "plan", "", &files).await;
    let hash = plan["plan_hash"].as_str().unwrap();
    let (status, applied) = post(&app, &pid, "apply", &format!("?plan_hash={hash}"), &files).await;
    assert_eq!(status, StatusCode::OK, "{applied}");

    let project = get(&app, &format!("/api/projects/{pid}")).await;
    assert_eq!(project["name"], "My SaaS 2");
    assert!(
        project["description"]
            .as_str()
            .unwrap()
            .starts_with("Invoices and bookings")
    );
    assert_eq!(project["vocabulary"]["sprint"], "Etapa");
    assert_eq!(project["project_type"], "web");
    assert_eq!(project["workflow"]["statuses"][2]["wip_limit"], 5);
}
