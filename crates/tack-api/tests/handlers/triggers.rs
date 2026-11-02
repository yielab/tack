//! Triggers over HTTP (ADR 0034, decision 8): the operator makes, lists, edits and deletes
//! them; an agent credential reaches none of the routes.

use crate::common::{send, test_app_with_config};
use axum::Router;
use axum::http::StatusCode;
use serde_json::{Value, json};
use tack_api::config::AppConfig;

const OPERATOR: &str = "Bearer operator-token";

async fn op(app: &Router, method: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send(app, method, uri, body, &[("authorization", OPERATOR)]).await
}

struct Setup {
    app: Router,
    project: String,
    profile: String,
}

async fn setup() -> Setup {
    let (app, _) = test_app_with_config(AppConfig {
        api_token: Some("operator-token".into()),
        ..AppConfig::default()
    })
    .await;
    let (_, project) = op(
        &app,
        "POST",
        "/api/projects",
        json!({"name": "My SaaS", "project_type": "software"}),
    )
    .await;
    let (status, profile) = op(
        &app,
        "POST",
        "/api/agent-profiles",
        json!({"name": "Manager profile", "instructions": "help"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{profile}");
    Setup {
        app,
        project: project["id"].as_str().unwrap().to_owned(),
        profile: profile["agent_profile_id"].as_str().unwrap().to_owned(),
    }
}

#[tokio::test]
async fn the_operator_makes_edits_lists_and_deletes_a_trigger() {
    let s = setup().await;
    let (status, refused) = op(
        &s.app,
        "POST",
        "/api/triggers",
        json!({"project_id": s.project, "profile": "no-such-profile", "name": "Failed runs",
               "source": {"kind": "event", "event": "run_failed"}}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");

    let (status, made) = op(
        &s.app,
        "POST",
        "/api/triggers",
        json!({"project_id": s.project, "profile": s.profile, "name": "Failed runs",
               "source": {"kind": "event", "event": "run_failed"}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    assert_eq!(
        (&made["quiet_seconds"], &made["daily_cap"]),
        (&json!(60), &json!(5))
    );
    assert_eq!(made["enabled"], true);
    let id = made["id"].as_str().unwrap().to_owned();

    let (status, stand_up) = op(
        &s.app,
        "POST",
        "/api/triggers",
        json!({"project_id": s.project, "profile": s.profile, "name": "Stand-up",
               "source": {"kind": "schedule", "days": [1,2,3,4,5], "minute_of_day": 540,
                          "utc_offset_minutes": 60}, "quiet_seconds": 0, "daily_cap": 1}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{stand_up}");

    let (_, listed) = op(
        &s.app,
        "GET",
        &format!("/api/triggers?project_id={}", s.project),
        json!(null),
    )
    .await;
    assert_eq!(listed.as_array().unwrap().len(), 2);

    let (status, edited) = op(
        &s.app,
        "PATCH",
        &format!("/api/triggers/{id}"),
        json!({"quiet_seconds": 300, "enabled": false}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{edited}");
    assert_eq!(
        (&edited["quiet_seconds"], &edited["enabled"]),
        (&json!(300), &json!(false))
    );
    let (status, _) = op(
        &s.app,
        "PATCH",
        &format!("/api/triggers/{id}"),
        json!({"daily_cap": 0}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = op(
        &s.app,
        "DELETE",
        &format!("/api/triggers/{id}"),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = op(&s.app, "GET", &format!("/api/triggers/{id}"), json!(null)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_agent_credential_reaches_no_trigger_route() {
    let s = setup().await;
    let (_, issued) = op(
        &s.app,
        "POST",
        "/api/agent-credentials",
        json!({"name": "Manager", "scopes": [{"projects": [s.project],
               "capabilities": ["read", "propose", "converse"]}]}),
    )
    .await;
    let secret = issued["secret"].as_str().unwrap();
    for (method, uri) in [("GET", "/api/triggers"), ("POST", "/api/triggers")] {
        let (status, body) = send(
            &s.app,
            method,
            uri,
            json!({}),
            &[("authorization", &format!("Bearer {secret}"))],
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}: {body}");
    }
}
