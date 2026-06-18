use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use tack_cli::client::TackClient;
use tack_cli::config::{self, Config};
use tack_cli::vocab;

fn make_config(base_url: &str) -> Config {
    Config::load(Some(base_url.to_string()), None)
}

// Run a blocking closure in a thread that is allowed to block
async fn run_blocking<F, T>(f: F) -> T
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .expect("blocking task panicked")
}

// ── init (POST /api/projects) ─────────────────────────────────────────────────

#[tokio::test]
async fn init_sends_post_projects() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/projects"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "name": "My App",
            "project_type": "software",
        })))
        .mount(&server)
        .await;

    let uri = server.uri();
    let resp = run_blocking(move || {
        let config = make_config(&uri);
        TackClient::new(&config).unwrap().post(
            "/projects",
            &serde_json::json!({"name": "My App", "project_type": "software"}),
        )
    })
    .await;

    assert!(resp.is_ok());
    assert_eq!(resp.unwrap()["name"], "My App");
}

// ── list (GET /api/projects/:id/items) ───────────────────────────────────────

#[tokio::test]
async fn list_sends_get_items() {
    let server = MockServer::start().await;
    let project_id = "aaaaaaaa-0000-0000-0000-000000000000";

    Mock::given(method("GET"))
        .and(path(format!("/api/projects/{project_id}/items")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server)
        .await;

    let uri = server.uri();
    let path = format!("/projects/{project_id}/items");
    let resp = run_blocking(move || {
        let config = make_config(&uri);
        TackClient::new(&config).unwrap().get(&path)
    })
    .await;

    assert!(resp.is_ok());
    assert!(resp.unwrap().is_array());
}

// ── list with filter ──────────────────────────────────────────────────────────

#[tokio::test]
async fn list_with_status_filter() {
    let server = MockServer::start().await;
    let project_id = "bbbbbbbb-0000-0000-0000-000000000000";

    Mock::given(method("GET"))
        .and(path(format!("/api/projects/{project_id}/items")))
        .and(query_param("status", "In Progress"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server)
        .await;

    let uri = server.uri();
    let path = format!("/projects/{project_id}/items?status=In%20Progress");
    let resp = run_blocking(move || {
        let config = make_config(&uri);
        TackClient::new(&config).unwrap().get(&path)
    })
    .await;

    assert!(resp.is_ok());
}

// ── move (PATCH /api/items/:id) ───────────────────────────────────────────────

#[tokio::test]
async fn move_sends_patch_item() {
    let server = MockServer::start().await;
    let item_id = "cccccccc-0000-0000-0000-000000000000";

    Mock::given(method("PATCH"))
        .and(path(format!("/api/items/{item_id}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": item_id,
            "status": "Done",
        })))
        .mount(&server)
        .await;

    let uri = server.uri();
    let path = format!("/items/{item_id}");
    let resp = run_blocking(move || {
        let config = make_config(&uri);
        TackClient::new(&config)
            .unwrap()
            .patch(&path, &serde_json::json!({"status": "Done"}))
    })
    .await;

    assert!(resp.is_ok());
    assert_eq!(resp.unwrap()["status"], "Done");
}

// ── server error surfaces correctly ──────────────────────────────────────────

#[tokio::test]
async fn error_response_is_returned_as_err() {
    let server = MockServer::start().await;

    Mock::given(method("PATCH"))
        .and(path("/api/items/bad-id"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": "WIP limit reached for column 'In Progress'",
        })))
        .mount(&server)
        .await;

    let uri = server.uri();
    let result = run_blocking(move || {
        let config = make_config(&uri);
        TackClient::new(&config).unwrap().patch(
            "/items/bad-id",
            &serde_json::json!({"status": "In Progress"}),
        )
    })
    .await;

    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("WIP limit reached"), "got: {msg}");
}

// ── search (GET /api/search) ──────────────────────────────────────────────────

#[tokio::test]
async fn search_global_sends_correct_path() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/search"))
        .and(query_param("q", "login"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server)
        .await;

    let uri = server.uri();
    let resp = run_blocking(move || {
        let config = make_config(&uri);
        TackClient::new(&config).unwrap().get("/search?q=login")
    })
    .await;

    assert!(resp.is_ok());
    assert!(resp.unwrap().is_array());
}

// ── sprint create ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn sprint_create_sends_post() {
    let server = MockServer::start().await;
    let project_id = "dddddddd-0000-0000-0000-000000000000";

    Mock::given(method("POST"))
        .and(path(format!("/api/projects/{project_id}/sprints")))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": "eeeeeeee-0000-0000-0000-000000000000",
            "name": "Sprint 1",
            "status": "planning",
        })))
        .mount(&server)
        .await;

    let uri = server.uri();
    let path = format!("/projects/{project_id}/sprints");
    let resp = run_blocking(move || {
        let config = make_config(&uri);
        TackClient::new(&config).unwrap().post(
            &path,
            &serde_json::json!({"name": "Sprint 1", "goal": null}),
        )
    })
    .await;

    assert!(resp.is_ok());
    assert_eq!(resp.unwrap()["name"], "Sprint 1");
}

// ── bearer token is forwarded ─────────────────────────────────────────────────

#[tokio::test]
async fn bearer_token_is_forwarded() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/projects"))
        .and(header("authorization", "Bearer secret"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server)
        .await;

    let uri = server.uri();
    let resp = run_blocking(move || {
        let config = Config::load(Some(uri), Some("secret".to_string()));
        TackClient::new(&config).unwrap().get("/projects")
    })
    .await;

    assert!(resp.is_ok());
}

// ── config save / load round-trip ─────────────────────────────────────────────

#[test]
fn config_save_and_reload() {
    // Write to a temp file by temporarily overriding HOME
    let tmp = std::env::temp_dir().join(format!("tackrc_test_{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let original_home = std::env::var("HOME").ok();
    // SAFETY: single-threaded test; no concurrent env reads in this process.
    unsafe { std::env::set_var("HOME", &tmp) };

    config::save("http://test:9999", Some("tok123")).unwrap();

    let cfg = Config::load(None, None);
    assert_eq!(cfg.base_url, "http://test:9999");
    assert_eq!(cfg.token.as_deref(), Some("tok123"));

    // Restore HOME
    match original_home {
        Some(h) => unsafe { std::env::set_var("HOME", h) },
        None => unsafe { std::env::remove_var("HOME") },
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

// ── vocab fetch falls back gracefully when project 404s ───────────────────────

#[tokio::test]
async fn vocab_fetch_returns_empty_on_404() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/projects/missing-id"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let uri = server.uri();
    let map = run_blocking(move || {
        let config = make_config(&uri);
        let client = TackClient::new(&config).unwrap();
        vocab::fetch(&client, "missing-id")
    })
    .await;

    assert!(map.is_empty());
}

// ── vocab term falls back to key when missing ─────────────────────────────────

#[test]
fn vocab_term_fallback() {
    let map = std::collections::HashMap::from([("task".into(), "Work Order".into())]);
    assert_eq!(vocab::term(&map, "task"), "Work Order");
    assert_eq!(vocab::term(&map, "epic"), "epic"); // key not in map → returns key
}
