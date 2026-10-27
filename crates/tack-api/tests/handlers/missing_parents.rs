//! A write whose parent row is not there answers 404 from the check the store makes, never a 500
//! from the database's foreign key.

use crate::common;
use axum::http::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

/// Every write that names a parent by id, driven to an id nothing has.
#[tokio::test]
async fn a_write_to_a_missing_parent_answers_404() {
    let missing = Uuid::new_v4();
    let other = Uuid::new_v4();
    let cases: Vec<(&str, String, Value)> = vec![
        (
            "POST",
            format!("/api/projects/{missing}/sprints"),
            json!({"name": "S1"}),
        ),
        (
            "POST",
            format!("/api/projects/{missing}/roles"),
            json!({"name": "Dev"}),
        ),
        (
            "POST",
            format!("/api/projects/{missing}/custom-fields"),
            json!({"name": "Score", "field_type": "number"}),
        ),
        (
            "POST",
            format!("/api/projects/{missing}/items"),
            json!({"title": "t"}),
        ),
        (
            "POST",
            format!("/api/items/{missing}/comments"),
            json!({"content": "hi"}),
        ),
        (
            "POST",
            format!("/api/items/{missing}/dependencies"),
            json!({"target_item_id": other, "dependency_type": "blocks"}),
        ),
        (
            "PUT",
            format!("/api/items/{missing}/roles/{other}"),
            json!({}),
        ),
        (
            "PUT",
            format!("/api/items/{missing}/custom-fields/{other}"),
            json!({"value": 1}),
        ),
        (
            "GET",
            format!("/api/items/{missing}/attachments"),
            Value::Null,
        ),
    ];
    let (app, _) = common::test_app().await;
    let mut wrong = Vec::new();
    for (method, uri, body) in cases {
        let (status, answer) = common::send(&app, method, &uri, body, &[]).await;
        if status != StatusCode::NOT_FOUND {
            wrong.push(format!("{method} {uri} -> {status} {answer}"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
