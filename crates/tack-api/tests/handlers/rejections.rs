//! A request axum's extractors refuse (a body that is not JSON, a path or query parameter that is
//! not valid, a wrong content type, a route that does not exist) gets the standard error body
//! with a `code`, on the operator routes and on the runner routes, never axum's plain text.

use crate::common;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

async fn raw(
    app: &Router,
    method: &str,
    uri: &str,
    content_type: Option<&str>,
    body: &str,
) -> (StatusCode, Option<String>, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(content_type) = content_type {
        builder = builder.header("content-type", content_type);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(body.to_owned())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let request_id = response
        .headers()
        .get("x-request-id")
        .map(|value| value.to_str().unwrap().to_owned());
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| panic!("{method} {uri} answered a body that is not JSON: {text}"));
    (status, request_id, value)
}

struct Expect {
    method: &'static str,
    uri: String,
    content_type: Option<&'static str>,
    body: &'static str,
    status: StatusCode,
    code: &'static str,
    field: Option<&'static str>,
}

fn case(
    method: &'static str,
    uri: impl Into<String>,
    content_type: Option<&'static str>,
    body: &'static str,
    status: StatusCode,
    code: &'static str,
    field: Option<&'static str>,
) -> Expect {
    Expect {
        method,
        uri: uri.into(),
        content_type,
        body,
        status,
        code,
        field,
    }
}

#[tokio::test]
async fn every_refused_request_answers_the_standard_error_body() {
    let (app, _) = common::test_app().await;
    let project = common::create_project(&app, "P", "software").await;
    let json = Some("application/json");
    let cases = vec![
        // A body that is not JSON, one that has the wrong shape, one that lacks a field.
        case(
            "POST",
            format!("/api/projects/{project}/items"),
            json,
            "{not json",
            StatusCode::BAD_REQUEST,
            "invalid_request",
            None,
        ),
        case(
            "POST",
            format!("/api/projects/{project}/items"),
            json,
            r#"{"title": 5}"#,
            StatusCode::BAD_REQUEST,
            "invalid_request",
            None,
        ),
        case(
            "POST",
            format!("/api/projects/{project}/items"),
            json,
            "{}",
            StatusCode::BAD_REQUEST,
            "invalid_request",
            Some("title"),
        ),
        case(
            "POST",
            "/api/projects",
            json,
            r#"{"name": "x"}"#,
            StatusCode::BAD_REQUEST,
            "invalid_request",
            Some("project_type"),
        ),
        // A body sent as something other than JSON.
        case(
            "POST",
            format!("/api/projects/{project}/items"),
            Some("text/plain"),
            r#"{"title": "t"}"#,
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            None,
        ),
        case(
            "POST",
            format!("/api/projects/{project}/items"),
            None,
            r#"{"title": "t"}"#,
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            None,
        ),
        // A path parameter that is not an id.
        case(
            "GET",
            "/api/items/not-a-uuid",
            None,
            "",
            StatusCode::BAD_REQUEST,
            "invalid_request",
            Some("id"),
        ),
        case(
            "GET",
            "/api/projects/not-a-uuid/boards",
            None,
            "",
            StatusCode::BAD_REQUEST,
            "invalid_request",
            Some("project_id"),
        ),
        // A query parameter that is not a number.
        case(
            "GET",
            format!("/api/projects/{project}/items?page=abc"),
            None,
            "",
            StatusCode::BAD_REQUEST,
            "invalid_request",
            None,
        ),
        // An operator execution route.
        case(
            "POST",
            "/api/executions",
            json,
            "[1, 2",
            StatusCode::BAD_REQUEST,
            "invalid_request",
            None,
        ),
        case(
            "POST",
            "/api/runner-fleets",
            json,
            "{}",
            StatusCode::BAD_REQUEST,
            "invalid_request",
            Some("name"),
        ),
        // The runner routes: a path parameter that is not an attempt id, and a body that is not
        // JSON, answered with the protocol envelope.
        case(
            "POST",
            "/api/runner/v1/attempts/%FF/completion",
            json,
            "{}",
            StatusCode::BAD_REQUEST,
            "invalid_request",
            Some("attempt_id"),
        ),
        // A path that is nowhere, and one that is there for another method.
        case(
            "GET",
            "/api/no-such-route",
            None,
            "",
            StatusCode::NOT_FOUND,
            "not_found",
            None,
        ),
        case(
            "GET",
            "/api/runner/v1/no-such-route",
            None,
            "",
            StatusCode::NOT_FOUND,
            "not_found",
            None,
        ),
        case(
            "PUT",
            "/api/projects",
            None,
            "",
            StatusCode::METHOD_NOT_ALLOWED,
            "method_not_allowed",
            None,
        ),
        case(
            "GET",
            "/api/runner/v1/enroll",
            None,
            "",
            StatusCode::METHOD_NOT_ALLOWED,
            "method_not_allowed",
            None,
        ),
    ];
    let mut wrong = Vec::new();
    for case in cases {
        let (status, request_id, body) =
            raw(&app, case.method, &case.uri, case.content_type, case.body).await;
        let error = &body["error"];
        let field = error["details"]["field"].as_str();
        let named = error["request_id"].as_str();
        let ok = status == case.status
            && error["code"] == case.code
            && error["status"] == status.as_u16()
            && error["message"].as_str().is_some_and(|m| !m.is_empty())
            && field == case.field
            && named.is_some()
            && named == request_id.as_deref();
        if !ok {
            wrong.push(format!("{} {} -> {status} {body}", case.method, case.uri));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[tokio::test]
async fn a_request_id_the_client_sent_comes_back_in_the_error_body() {
    let (app, _) = common::test_app().await;
    let request = Request::builder()
        .method("GET")
        .uri(format!("/api/items/{}", Uuid::new_v4()))
        .header("x-request-id", "req_from_the_client")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["error"]["request_id"], "req_from_the_client");
}

#[tokio::test]
async fn a_missing_field_is_said_in_plain_words_naming_the_field() {
    let (app, _) = common::test_app().await;
    let project = common::create_project(&app, "P", "software").await;
    let (_, _, body) = raw(
        &app,
        "POST",
        &format!("/api/projects/{project}/items"),
        Some("application/json"),
        "{}",
    )
    .await;
    assert_eq!(body["error"]["message"], "The field `title` is required.");
}
