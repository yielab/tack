//! The four type routes, and what the catalog does to the routes that name a type: the project
//! serves it, an item's type must be in it, and a used type is merged before it goes.

use crate::common;
use axum::http::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

async fn types_of(app: &axum::Router, pid: Uuid) -> Vec<Value> {
    let (status, list) = common::send(
        app,
        "GET",
        &format!("/api/projects/{pid}/types"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{list}");
    list.as_array().unwrap().clone()
}

fn find<'a>(types: &'a [Value], key: &str) -> &'a Value {
    types
        .iter()
        .find(|t| t["key"] == key)
        .unwrap_or_else(|| panic!("no type {key}"))
}

#[tokio::test]
async fn a_project_serves_its_catalog_with_how_many_items_use_each_type() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "construction").await;
    let (_, project) = common::send(
        &app,
        "GET",
        &format!("/api/projects/{pid}"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(project["item_types"][0]["name"], "Building");
    assert_eq!(project["item_types"][0]["level"], "group");

    common::create_item(&app, pid, "A work order").await;
    let types = types_of(&app, pid).await;
    assert_eq!(types.len(), 6);
    assert_eq!(find(&types, "task")["items"], 1);
    assert_eq!(find(&types, "task")["name"], "Work Order");
    assert_eq!(find(&types, "task")["default_work"], true);
    assert_eq!(find(&types, "epic")["items"], 0);

    let (status, _) = common::send(
        &app,
        "GET",
        &format!("/api/projects/{}/types", Uuid::new_v4()),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_type_is_created_renamed_and_kept_to_one_default() {
    let (app, pid) = {
        let (app, _) = common::test_app().await;
        let pid = common::create_project(&app, "P", "software").await;
        (app, pid)
    };
    let uri = format!("/api/projects/{pid}/types");
    let (status, created) = common::send(
        &app,
        "POST",
        &uri,
        json!({"key": "spike", "name": "Spike", "level": "work", "color": "accent"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(created["key"], "spike");
    assert_eq!(created["default_work"], false);

    let (status, again) = common::send(
        &app,
        "POST",
        &uri,
        json!({"key": "spike", "name": "Again", "level": "work", "color": "accent"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{again}");
    let (status, _) = common::send(
        &app,
        "POST",
        &uri,
        json!({"key": "Not A Key", "name": "X", "level": "work", "color": "accent"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, patched) = common::send(
        &app,
        "PATCH",
        &format!("{uri}/spike"),
        json!({"name": "Research spike", "default_work": true, "color": "warning"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{patched}");
    assert_eq!(patched["name"], "Research spike");
    assert_eq!(patched["default_work"], true);
    let types = types_of(&app, pid).await;
    assert_eq!(
        types.iter().filter(|t| t["default_work"] == true).count(),
        1
    );
    assert_eq!(find(&types, "task")["default_work"], false);

    let (status, _) = common::send(
        &app,
        "PATCH",
        &format!("{uri}/ghost"),
        json!({"name": "X"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_without_merge_into_is_type_in_use_while_items_use_the_type() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    let (status, item) = common::send(
        &app,
        "POST",
        &format!("/api/projects/{pid}/items"),
        json!({"title": "Crash", "item_type": "bug"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{item}");
    let uri = format!("/api/projects/{pid}/types/bug");

    let (status, refused) = common::send(&app, "DELETE", &uri, Value::Null, &[]).await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    assert_eq!(refused["error"]["code"], "type_in_use");

    // A level cannot change while the type is used, either.
    let (status, refused) = common::send(&app, "PATCH", &uri, json!({"level": "group"}), &[]).await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");

    let (status, _) = common::send(
        &app,
        "DELETE",
        &format!("{uri}?merge_into=feature"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let types = types_of(&app, pid).await;
    assert!(types.iter().all(|t| t["key"] != "bug"));
    assert_eq!(find(&types, "feature")["items"], 1);

    let (_, moved) = common::send(
        &app,
        "GET",
        &format!("/api/items/{}", item["id"].as_str().unwrap()),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(moved["item"]["item_type"], "feature");
}

#[tokio::test]
async fn an_item_of_a_type_the_project_lacks_is_not_found_and_none_takes_the_default() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    let uri = format!("/api/projects/{pid}/items");
    let (status, refused) = common::send(
        &app,
        "POST",
        &uri,
        json!({"title": "Odd", "item_type": "saga"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{refused}");
    let (status, plain) = common::send(&app, "POST", &uri, json!({"title": "Plain"}), &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(plain["item_type"], "task");
}

#[tokio::test]
async fn an_automation_domain_names_only_types_the_project_has() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    let uri = format!("/api/projects/{pid}/autonomy");
    let (_, mut settings) = common::send(&app, "GET", &uri, Value::Null, &[]).await;
    settings["domains"] =
        json!([{"name": "docs", "path_globs": ["docs/**"], "item_types": ["saga"], "level": "L3"}]);
    let (status, refused) = common::send(&app, "PUT", &uri, settings.clone(), &[]).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
    settings["domains"][0]["item_types"] = json!(["bug"]);
    let (status, saved) = common::send(&app, "PUT", &uri, settings, &[]).await;
    assert_eq!(status, StatusCode::OK, "{saved}");
}
