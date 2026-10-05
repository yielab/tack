//! HTTP tests for `GET`/`PUT`/`DELETE /api/items/{id}/brief` and for the brief
//! surviving a project export → import.

use crate::common;
use axum::http::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

/// `docs/contracts/brief-v1/example.json` as a `PUT` body: the server owns the
/// item id and both timestamps, so those three keys are dropped.
fn example_body() -> Value {
    let mut v: Value = serde_json::from_str(include_str!(
        "../../../../docs/contracts/brief-v1/example.json"
    ))
    .unwrap();
    for key in ["item_id", "created_at", "updated_at"] {
        v.as_object_mut().unwrap().remove(key);
    }
    v
}

/// The same shape with the server-owned keys removed from a response.
fn normalised(mut v: Value) -> Value {
    for key in ["item_id", "created_at", "updated_at"] {
        v.as_object_mut().unwrap().remove(key);
    }
    v
}

#[tokio::test]
async fn put_then_get_round_trips_the_example() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    let iid = common::create_item(&app, pid, "Item").await;
    let uri = format!("/api/items/{iid}/brief");

    let (status, _) = common::send(&app, "GET", &uri, Value::Null, &[]).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "no brief yet");

    let (status, put) = common::send(&app, "PUT", &uri, example_body(), &[]).await;
    assert_eq!(status, StatusCode::OK, "{put}");
    let (status, got) = common::send(&app, "GET", &uri, Value::Null, &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got["item_id"], iid.to_string());
    assert_eq!(normalised(got), example_body());
}

#[tokio::test]
async fn brief_rejects_a_duplicate_criterion_id_and_a_missing_item() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    let iid = common::create_item(&app, pid, "Item").await;
    let dup = json!({
        "acceptance": [
            {"kind": "manual", "id": "a", "title": "x", "text": "x"},
            {"kind": "manual", "id": "a", "title": "y", "text": "y"},
        ],
        "constraints": [],
    });
    let (status, _) = common::send(&app, "PUT", &format!("/api/items/{iid}/brief"), dup, &[]).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let missing = format!("/api/items/{}/brief", Uuid::new_v4());
    for method in ["GET", "PUT", "DELETE"] {
        let (status, _) = common::send(&app, method, &missing, example_body(), &[]).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} on a missing item");
    }
}

#[tokio::test]
async fn deleting_the_item_or_the_brief_removes_the_brief() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    let iid = common::create_item(&app, pid, "Item").await;
    let uri = format!("/api/items/{iid}/brief");
    common::send(&app, "PUT", &uri, example_body(), &[]).await;

    let (status, _) = common::send(&app, "DELETE", &uri, Value::Null, &[]).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = common::send(&app, "GET", &uri, Value::Null, &[]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    common::send(&app, "PUT", &uri, example_body(), &[]).await;
    let (status, _) = common::send(
        &app,
        "DELETE",
        &format!("/api/items/{iid}"),
        Value::Null,
        &[],
    )
    .await;
    assert!(status.is_success(), "item delete: {status}");
    let (status, _) = common::send(&app, "GET", &uri, Value::Null, &[]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn export_then_import_restores_the_brief() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    let iid = common::create_item(&app, pid, "Item").await;
    common::send(
        &app,
        "PUT",
        &format!("/api/items/{iid}/brief"),
        example_body(),
        &[],
    )
    .await;

    let (status, export) = common::send(
        &app,
        "GET",
        &format!("/api/projects/{pid}/export?format=json"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(export["briefs"].as_array().unwrap().len(), 1);

    // A fresh database: the import must rebuild the brief against the new item id.
    let (fresh, _) = common::test_app().await;
    let (status, out) = common::send(&fresh, "POST", "/api/projects/import", export, &[]).await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert_eq!(out["stats"]["briefs_imported"], 1);
    let new_pid = out["project"]["id"].as_str().unwrap();
    let (_, items) = common::send(
        &fresh,
        "GET",
        &format!("/api/projects/{new_pid}/items"),
        Value::Null,
        &[],
    )
    .await;
    let new_iid = items["data"][0]["id"].as_str().unwrap();
    let (status, got) = common::send(
        &fresh,
        "GET",
        &format!("/api/items/{new_iid}/brief"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(normalised(got), example_body());
}
