mod common;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tower::ServiceExt;

const SKILL_ID: &str = "amzn1.ask.skill.test-0000";

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn alexa_config() -> AppConfig {
    AppConfig {
        alexa_skill_id: Some(SKILL_ID.into()),
        ..AppConfig::default()
    }
}

/// Build an Alexa request envelope with the given skill ID and request body.
fn envelope(skill_id: &str, timestamp: chrono::DateTime<chrono::Utc>, request: Value) -> Value {
    let mut request = request;
    request["requestId"] = json!("EdwRequestId.test");
    request["timestamp"] = json!(timestamp.to_rfc3339());
    json!({
        "version": "1.0",
        "session": { "application": { "applicationId": skill_id } },
        "context": { "System": { "application": { "applicationId": skill_id } } },
        "request": request,
    })
}

fn intent(name: &str, slots: Value) -> Value {
    json!({ "type": "IntentRequest", "intent": { "name": name, "slots": slots } })
}

/// Stamp a locale onto an existing envelope's request.
fn with_locale(mut env: Value, locale: &str) -> Value {
    env["request"]["locale"] = json!(locale);
    env
}

fn slot(name: &str, value: &str) -> Value {
    json!({ name: { "name": name, "value": value } })
}

async fn post_alexa(app: &Router, body: &Value) -> (StatusCode, Value) {
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/alexa")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), 64 * 1024)
        .await
        .unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

/// Speech text from a standard Alexa response.
fn spoken(body: &Value) -> &str {
    body["response"]["outputSpeech"]["text"]
        .as_str()
        .unwrap_or("")
}

async fn create_project(app: &Router, name: &str) -> Value {
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/projects")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "name": name, "project_type": "software" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 64 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn list_project_items(app: &Router, project_id: &str) -> Vec<Value> {
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/projects/{project_id}/items"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 64 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

// ─── Verification ────────────────────────────────────────────────────────────

#[tokio::test]
async fn alexa_disabled_returns_404() {
    let (app, _) = common::test_app().await; // no skill ID configured
    let body = envelope(
        SKILL_ID,
        chrono::Utc::now(),
        json!({ "type": "LaunchRequest" }),
    );
    let (status, _) = post_alexa(&app, &body).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn wrong_skill_id_is_rejected() {
    let (app, _) = common::test_app_with_config(alexa_config()).await;
    let body = envelope(
        "amzn1.ask.skill.attacker",
        chrono::Utc::now(),
        json!({ "type": "LaunchRequest" }),
    );
    let (status, _) = post_alexa(&app, &body).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn stale_timestamp_is_rejected() {
    let (app, _) = common::test_app_with_config(alexa_config()).await;
    let stale = chrono::Utc::now() - chrono::Duration::minutes(10);
    let body = envelope(SKILL_ID, stale, json!({ "type": "LaunchRequest" }));
    let (status, _) = post_alexa(&app, &body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn alexa_bypasses_bearer_token_gate() {
    // With an API token configured, /api/alexa must still work without an
    // Authorization header — Alexa cannot send one.
    let config = AppConfig {
        api_token: Some("secret-token".into()),
        ..alexa_config()
    };
    let (app, _) = common::test_app_with_config(config).await;
    let body = envelope(
        SKILL_ID,
        chrono::Utc::now(),
        json!({ "type": "LaunchRequest" }),
    );
    let (status, _) = post_alexa(&app, &body).await;
    assert_eq!(status, StatusCode::OK);
}

// ─── Intents ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn launch_request_speaks_welcome_and_keeps_session_open() {
    let (app, _) = common::test_app_with_config(alexa_config()).await;
    let body = envelope(
        SKILL_ID,
        chrono::Utc::now(),
        json!({ "type": "LaunchRequest" }),
    );
    let (status, res) = post_alexa(&app, &body).await;
    assert_eq!(status, StatusCode::OK);
    assert!(spoken(&res).contains("Welcome"));
    assert_eq!(res["response"]["shouldEndSession"], json!(false));
}

#[tokio::test]
async fn add_task_creates_item_in_project() {
    let (app, _) = common::test_app_with_config(alexa_config()).await;
    let project = create_project(&app, "Voice Project").await;

    let body = envelope(
        SKILL_ID,
        chrono::Utc::now(),
        intent("AddTaskIntent", slot("title", "buy cement")),
    );
    let (status, res) = post_alexa(&app, &body).await;
    assert_eq!(status, StatusCode::OK);
    assert!(spoken(&res).contains("buy cement"));
    assert!(spoken(&res).contains("Voice Project"));

    let items = list_project_items(&app, project["id"].as_str().unwrap()).await;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["title"], "buy cement");
}

#[tokio::test]
async fn add_task_targets_named_project() {
    let (app, _) = common::test_app_with_config(alexa_config()).await;
    let first = create_project(&app, "Casa").await;
    let _second = create_project(&app, "Garden").await;

    let mut slots = slot("title", "pour foundation");
    slots["project"] = json!({ "name": "project", "value": "casa" });
    let body = envelope(SKILL_ID, chrono::Utc::now(), intent("AddTaskIntent", slots));
    let (status, res) = post_alexa(&app, &body).await;
    assert_eq!(status, StatusCode::OK);
    assert!(spoken(&res).contains("Casa"));

    let items = list_project_items(&app, first["id"].as_str().unwrap()).await;
    assert_eq!(items.len(), 1);
}

#[tokio::test]
async fn add_task_without_title_prompts_for_one() {
    let (app, _) = common::test_app_with_config(alexa_config()).await;
    create_project(&app, "Voice Project").await;

    let body = envelope(
        SKILL_ID,
        chrono::Utc::now(),
        intent("AddTaskIntent", json!({})),
    );
    let (status, res) = post_alexa(&app, &body).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(res["response"]["shouldEndSession"], json!(false));
}

#[tokio::test]
async fn add_task_with_no_projects_explains() {
    let (app, _) = common::test_app_with_config(alexa_config()).await;
    let body = envelope(
        SKILL_ID,
        chrono::Utc::now(),
        intent("AddTaskIntent", slot("title", "buy cement")),
    );
    let (status, res) = post_alexa(&app, &body).await;
    assert_eq!(status, StatusCode::OK);
    assert!(spoken(&res).contains("any projects"));
}

#[tokio::test]
async fn list_tasks_counts_open_items() {
    let (app, _) = common::test_app_with_config(alexa_config()).await;
    create_project(&app, "Voice Project").await;

    for title in ["task one", "task two"] {
        let body = envelope(
            SKILL_ID,
            chrono::Utc::now(),
            intent("AddTaskIntent", slot("title", title)),
        );
        let (status, _) = post_alexa(&app, &body).await;
        assert_eq!(status, StatusCode::OK);
    }

    let body = envelope(
        SKILL_ID,
        chrono::Utc::now(),
        intent("ListTasksIntent", json!({})),
    );
    let (status, res) = post_alexa(&app, &body).await;
    assert_eq!(status, StatusCode::OK);
    assert!(spoken(&res).contains("2 open"));
    assert!(spoken(&res).contains("task one"));
}

#[tokio::test]
async fn complete_task_moves_item_to_done() {
    let (app, _) = common::test_app_with_config(alexa_config()).await;
    let project = create_project(&app, "Voice Project").await;

    let body = envelope(
        SKILL_ID,
        chrono::Utc::now(),
        intent("AddTaskIntent", slot("title", "buy cement")),
    );
    post_alexa(&app, &body).await;

    let body = envelope(
        SKILL_ID,
        chrono::Utc::now(),
        intent("CompleteTaskIntent", slot("title", "buy cement")),
    );
    let (status, res) = post_alexa(&app, &body).await;
    assert_eq!(status, StatusCode::OK);
    assert!(spoken(&res).contains("Marked buy cement"));

    let items = list_project_items(&app, project["id"].as_str().unwrap()).await;
    assert_eq!(items[0]["status"], "Done");
}

#[tokio::test]
async fn complete_unknown_task_speaks_not_found() {
    let (app, _) = common::test_app_with_config(alexa_config()).await;
    create_project(&app, "Voice Project").await;

    let body = envelope(
        SKILL_ID,
        chrono::Utc::now(),
        intent("CompleteTaskIntent", slot("title", "nonexistent")),
    );
    let (status, res) = post_alexa(&app, &body).await;
    assert_eq!(status, StatusCode::OK);
    assert!(spoken(&res).contains("couldn't find"));
}

// ─── Localisation ────────────────────────────────────────────────────────────

#[tokio::test]
async fn spanish_locale_gets_spanish_welcome() {
    let (app, _) = common::test_app_with_config(alexa_config()).await;
    let body = with_locale(
        envelope(
            SKILL_ID,
            chrono::Utc::now(),
            json!({ "type": "LaunchRequest" }),
        ),
        "es-MX",
    );
    let (status, res) = post_alexa(&app, &body).await;
    assert_eq!(status, StatusCode::OK);
    assert!(spoken(&res).contains("Bienvenido"));
}

#[tokio::test]
async fn spanish_locale_add_task_responds_in_spanish() {
    let (app, _) = common::test_app_with_config(alexa_config()).await;
    create_project(&app, "Casa").await;

    let body = with_locale(
        envelope(
            SKILL_ID,
            chrono::Utc::now(),
            intent("AddTaskIntent", slot("title", "comprar cemento")),
        ),
        "es-MX",
    );
    let (status, res) = post_alexa(&app, &body).await;
    assert_eq!(status, StatusCode::OK);
    assert!(spoken(&res).contains("Agregué"));
    assert!(spoken(&res).contains("comprar cemento"));
    assert!(spoken(&res).contains("Casa"));
}

#[tokio::test]
async fn spanish_locale_complete_task_responds_in_spanish() {
    let (app, _) = common::test_app_with_config(alexa_config()).await;
    create_project(&app, "Casa").await;

    let body = envelope(
        SKILL_ID,
        chrono::Utc::now(),
        intent("AddTaskIntent", slot("title", "comprar cemento")),
    );
    post_alexa(&app, &body).await;

    let body = with_locale(
        envelope(
            SKILL_ID,
            chrono::Utc::now(),
            intent("CompleteTaskIntent", slot("title", "comprar cemento")),
        ),
        "es-MX",
    );
    let (status, res) = post_alexa(&app, &body).await;
    assert_eq!(status, StatusCode::OK);
    assert!(spoken(&res).contains("Marqué"));
}

#[tokio::test]
async fn english_locale_still_responds_in_english() {
    let (app, _) = common::test_app_with_config(alexa_config()).await;
    let body = with_locale(
        envelope(
            SKILL_ID,
            chrono::Utc::now(),
            json!({ "type": "LaunchRequest" }),
        ),
        "en-US",
    );
    let (status, res) = post_alexa(&app, &body).await;
    assert_eq!(status, StatusCode::OK);
    assert!(spoken(&res).contains("Welcome"));
}

#[tokio::test]
async fn stop_intent_ends_session() {
    let (app, _) = common::test_app_with_config(alexa_config()).await;
    let body = envelope(
        SKILL_ID,
        chrono::Utc::now(),
        intent("AMAZON.StopIntent", json!({})),
    );
    let (status, res) = post_alexa(&app, &body).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(res["response"]["shouldEndSession"], json!(true));
}
