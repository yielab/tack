//! The outline and CSV import routes: a new project or an existing one, a
//! dry run, a refusal that writes nothing, and the provenance each records.

use crate::common;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

async fn post_raw(app: &Router, uri: &str, content_type: &str, body: &str) -> (StatusCode, Value) {
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(uri)
                .header("Content-Type", content_type)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let bytes = to_bytes(res.into_body(), 1 << 20).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn items_of(app: &Router, pid: &str) -> Vec<Value> {
    let (_, items) = common::send(
        app,
        "GET",
        &format!("/api/projects/{pid}/items"),
        Value::Null,
        &[],
    )
    .await;
    items["data"].as_array().cloned().unwrap_or_default()
}

async fn project_count(app: &Router) -> usize {
    let (_, projects) = common::send(app, "GET", "/api/projects", Value::Null, &[]).await;
    projects.as_array().map_or(0, Vec::len)
}

fn by_title<'a>(items: &'a [Value], title: &str) -> &'a Value {
    items
        .iter()
        .find(|i| i["title"] == title)
        .unwrap_or_else(|| panic!("no item titled {title}"))
}

fn plan() -> Value {
    json!({
        "project": { "name": "Relaunch", "project_type": "Software" },
        "items": [
            { "title": "Wireframes", "parent": "design", "depends_on": ["Research"], "status": "in progress" },
            { "ref": "design", "title": "Design", "type": "epic", "due_date": "2026-11-01" },
            { "title": "Research", "priority": "high" }
        ]
    })
}

#[tokio::test]
async fn outline_creates_project_with_hierarchy_and_dependencies() {
    let (app, _) = common::test_app().await;
    let (status, out) = common::send(&app, "POST", "/api/projects/outline", plan(), &[]).await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert_eq!(out["created"], 3);
    assert_eq!(out["dependencies"], 1);
    assert_eq!(out["project"]["project_type"], "software");

    let pid = out["project"]["id"].as_str().unwrap();
    let items = items_of(&app, pid).await;
    let design = by_title(&items, "Design");
    let wireframes = by_title(&items, "Wireframes");
    assert_eq!(wireframes["parent_id"], design["id"]);
    assert_eq!(wireframes["status"], "In Progress");
    assert_eq!(design["item_type"], "epic");
    assert!(items.iter().all(|i| i["source"] == "outline"));

    let wid = wireframes["id"].as_str().unwrap();
    let (_, deps) = common::send(
        &app,
        "GET",
        &format!("/api/items/{wid}/dependencies"),
        Value::Null,
        &[],
    )
    .await;
    let research = by_title(&items, "Research")["id"].clone();
    assert!(
        deps.to_string().contains(research.as_str().unwrap()),
        "Wireframes must wait on Research: {deps}"
    );
}

#[tokio::test]
async fn outline_accepted_as_yaml() {
    let (app, _) = common::test_app().await;
    let yaml = "project:\n  name: From YAML\nitems:\n  - ref: 1\n    title: One\n  - title: Two\n    parent: 1\n";
    let (status, out) = post_raw(&app, "/api/projects/outline", "application/x-yaml", yaml).await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert_eq!(out["created"], 2);
    assert_eq!(out["items"][1]["parent_id"], out["items"][0]["id"]);
}

#[tokio::test]
async fn refused_outline_writes_nothing_and_lists_every_problem() {
    let (app, _) = common::test_app().await;
    let before = project_count(&app).await;
    let body = json!({
        "project": { "name": "Never" },
        "items": [{ "title": "" }, { "title": "Orphan", "parent": "nowhere" }]
    });
    let (status, out) = common::send(&app, "POST", "/api/projects/outline", body, &[]).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let message = out.to_string();
    assert!(message.contains("2 problems"), "{message}");
    assert_eq!(project_count(&app).await, before);
}

#[tokio::test]
async fn dry_run_reports_without_writing() {
    let (app, _) = common::test_app().await;
    let before = project_count(&app).await;
    let (status, out) = common::send(
        &app,
        "POST",
        "/api/projects/outline?dry_run=true",
        plan(),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert_eq!(out["dry_run"], true);
    assert_eq!(out["created"], 3);
    assert_eq!(out["items"][0]["title"], "Design", "parents come first");
    assert!(out["items"][0]["id"].is_null());
    assert_eq!(project_count(&app).await, before);
}

#[tokio::test]
async fn new_project_needs_a_name() {
    let (app, _) = common::test_app().await;
    let body = json!({ "items": [{ "title": "A" }] });
    let (status, out) = common::send(&app, "POST", "/api/projects/outline", body, &[]).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(out.to_string().contains("?name="), "{out}");
}

#[tokio::test]
async fn outline_adds_under_an_existing_item_of_the_same_project_only() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "Mine", "software").await;
    let other = common::create_project(&app, "Other", "software").await;
    let epic = common::create_item(&app, pid, "Epic").await;
    let foreign = common::create_item(&app, other, "Foreign").await;

    let under =
        |parent: Uuid| json!({ "items": [{ "title": "Child", "parent": parent.to_string() }] });
    let uri = format!("/api/projects/{pid}/outline");

    let (status, out) = common::send(&app, "POST", &uri, under(foreign), &[]).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{out}");
    assert!(out.to_string().contains("not an item of this project"));

    let (status, out) = common::send(&app, "POST", &uri, under(epic), &[]).await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert_eq!(out["items"][0]["parent_id"], epic.to_string());
}

#[tokio::test]
async fn csv_creates_a_named_project() {
    let (app, _) = common::test_app().await;
    let csv = "ref,title,parent,status\ne,Epic,,\nt,\"Task, with comma\",e,Nope\n";
    let (status, out) = post_raw(
        &app,
        "/api/projects/import-csv?name=From%20CSV&project_type=web",
        "text/csv",
        csv,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert_eq!(out["project"]["name"], "From CSV");
    assert_eq!(out["sections"], json!([]), "a CSV has no sections");
    assert_eq!(out["items"][1]["title"], "Task, with comma");
    assert_eq!(out["items"][1]["parent_id"], out["items"][0]["id"]);
    assert_eq!(
        out["warnings"].as_array().unwrap().len(),
        1,
        "unknown status"
    );

    let pid = out["project"]["id"].as_str().unwrap();
    assert!(
        items_of(&app, pid)
            .await
            .iter()
            .all(|i| i["source"] == "csv_import")
    );
}

#[tokio::test]
async fn csv_into_existing_project_marks_rows_csv_import() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    let csv = "title,description\nFrom a spreadsheet,could be anyone's data\n,\n";
    let (status, out) = post_raw(
        &app,
        &format!("/api/projects/{pid}/import-csv"),
        "text/csv",
        csv,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert_eq!(out["created"], 1);
    let items = items_of(&app, &pid.to_string()).await;
    assert_eq!(
        items[0]["source"], "csv_import",
        "a CSV-imported row must be recorded with source: csv_import (untrusted for dispatch)"
    );
}

#[tokio::test]
async fn the_csv_export_round_trips_recurrence() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    common::send(
        &app,
        "POST",
        &format!("/api/projects/{pid}/items"),
        json!({"title": "Weekly review", "recurrence": "weekly"}),
        &[],
    )
    .await;
    let (_, _, csv) = common::send_with_raw(
        &app,
        "GET",
        &format!("/api/projects/{pid}/export?format=csv"),
        Value::Null,
        &[],
    )
    .await;
    assert!(csv.lines().next().unwrap().contains("recurrence"), "{csv}");

    let (status, out) = post_raw(
        &app,
        "/api/projects/import-csv?name=Again&project_type=web",
        "text/csv",
        &csv,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{out}");
    let items = items_of(&app, out["project"]["id"].as_str().unwrap()).await;
    assert_eq!(by_title(&items, "Weekly review")["recurrence"], "weekly");
}
