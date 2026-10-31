//! A write whose parent row is not there answers 404 from the check the store makes, never a 500
//! from the database's foreign key.

use crate::common;
use axum::http::StatusCode;
use serde_json::{Value, json};
use tack_api::config::AppConfig;
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
    check(cases).await;
}

/// The writes of the second sweep: briefs, boards, links, imports, outlines and the operator's
/// side of a run.
#[tokio::test]
async fn the_remaining_writes_to_a_missing_parent_answer_404() {
    let missing = Uuid::new_v4();
    let cases: Vec<(&str, String, Value)> = vec![
        (
            "PUT",
            format!("/api/items/{missing}/brief"),
            json!({"acceptance": [], "constraints": []}),
        ),
        (
            "POST",
            format!("/api/projects/{missing}/boards"),
            json!({"name": "B"}),
        ),
        (
            "PUT",
            format!("/api/items/{missing}/github-link"),
            json!({"repo": "owner/name", "issue_number": 1}),
        ),
        (
            "POST",
            format!("/api/items/{missing}/subtasks-from-plan"),
            json!({"artifact_id": "art_1", "subtasks": [{"title": "a", "acceptance": []}]}),
        ),
        (
            "PUT",
            format!("/api/projects/{missing}/autonomy"),
            json!({"level": "L3", "domains": [], "verify_require": [], "verify_loop_max_attempts": 3, "budgets": {"tokens_per_day": null, "auto_merges_per_day": null, "queue_items_max": null, "provisional_tokens_per_run": null}, "merge_on_done": false}),
        ),
        (
            "PATCH",
            format!("/api/projects/{missing}"),
            json!({"name": "x"}),
        ),
        (
            "POST",
            format!("/api/projects/{missing}/import-github"),
            json!({"repo": "owner/name"}),
        ),
        (
            "POST",
            format!("/api/projects/{missing}/import-linear"),
            json!({"api_key": "k", "team_id": "t"}),
        ),
        (
            "POST",
            format!("/api/projects/{missing}/outline"),
            json!({"items": [{"title": "t"}]}),
        ),
        (
            "POST",
            "/api/executions".to_owned(),
            json!({
                "item_id": missing,
                "idempotency_key": "k1",
                "permission_policy": {"tools": ["shell"], "network": false},
                "budgets": {},
                "environment": {},
                "metadata": {}
            }),
        ),
        (
            "POST",
            format!("/api/executions/req_{missing}/attempts/1/review"),
            json!({"verdict": "accepted"}),
        ),
        (
            "POST",
            format!("/api/executions/req_{missing}/attempts/1/mrp/review"),
            json!({"verdict": "accept", "reason": "fine"}),
        ),
        (
            "POST",
            format!("/api/executions/req_{missing}/attempts/1/mrp/viewed"),
            Value::Null,
        ),
    ];
    check(cases).await;
}

/// A CSV import and a decision's resolution need a content type and a token the JSON cases do
/// not send.
#[tokio::test]
async fn a_csv_import_and_a_decision_for_a_missing_parent_answer_404() {
    let missing = Uuid::new_v4();
    let config = AppConfig {
        execution_decision_token: Some("decision-token".into()),
        ..AppConfig::default()
    };
    let (app, _) = common::test_app_with_config(config).await;

    let csv = axum::http::Request::builder()
        .method("POST")
        .uri(format!("/api/projects/{missing}/import-csv"))
        .header("content-type", "text/csv")
        .body(axum::body::Body::from("title\nA\n"))
        .unwrap();
    let response = tower::ServiceExt::oneshot(app.clone(), csv).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND, "csv import");

    let (status, answer) = common::send(
        &app,
        "POST",
        &format!("/api/attempts/att_{missing}/decisions/dec_x/resolve"),
        json!({"answer": {"option_id": "a"}}),
        &[("x-tack-decision-token", "decision-token")],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{answer}");
}

/// An upload to an item that is not there answers 404 before the file is read.
#[tokio::test]
async fn an_attachment_uploaded_to_a_missing_item_answers_404() {
    let (app, _) = common::test_app().await;
    let boundary = "tackboundary";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.txt\"\r\n\
         Content-Type: text/plain\r\n\r\nhello\r\n--{boundary}--\r\n"
    );
    let request = axum::http::Request::builder()
        .method("POST")
        .uri(format!("/api/items/{}/attachments", Uuid::new_v4()))
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(axum::body::Body::from(body))
        .unwrap();
    let response = tower::ServiceExt::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

async fn check(cases: Vec<(&str, String, Value)>) {
    let (app, _) = common::test_app().await;
    let mut wrong = Vec::new();
    for (method, uri, body) in cases {
        let (status, answer) = common::send(&app, method, &uri, body, &[]).await;
        let from_the_check = answer["error"]["message"] != "There is no such route.";
        if status != StatusCode::NOT_FOUND || !from_the_check {
            wrong.push(format!("{method} {uri} -> {status} {answer}"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// A write whose body names a row that is not there (a parent item, a sprint, a role, the other
/// end of a dependency) answers a 4xx from the store's check, never a 500 from the foreign key.
#[tokio::test]
async fn a_write_whose_body_names_a_missing_row_never_answers_500() {
    let missing = Uuid::new_v4();
    let (app, _) = common::test_app().await;
    let project = common::create_project(&app, "P", "software").await;
    let item = common::create_item(&app, project, "T").await;
    let cases: Vec<(&str, String, Value)> = vec![
        (
            "POST",
            format!("/api/projects/{project}/items"),
            json!({"title": "t", "parent_id": missing}),
        ),
        (
            "POST",
            format!("/api/projects/{project}/items"),
            json!({"title": "t", "sprint_id": missing}),
        ),
        (
            "PATCH",
            format!("/api/items/{item}"),
            json!({"sprint_id": missing}),
        ),
        (
            "PUT",
            format!("/api/items/{item}/roles/{missing}"),
            json!({}),
        ),
        (
            "POST",
            format!("/api/items/{item}/dependencies"),
            json!({"target_item_id": missing, "dependency_type": "blocks"}),
        ),
        (
            "PUT",
            format!("/api/items/{item}/custom-fields/{missing}"),
            json!({"value": 1}),
        ),
    ];
    let mut wrong = Vec::new();
    for (method, uri, body) in cases {
        let (status, answer) = common::send(&app, method, &uri, body, &[]).await;
        if status.is_server_error() || status.is_success() {
            wrong.push(format!("{method} {uri} -> {status} {answer}"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
