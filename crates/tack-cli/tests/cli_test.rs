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

// ── import (POST /api/projects/import-csv) ───────────────────────────────────

#[tokio::test]
async fn post_text_sends_content_type_query_and_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/projects/import-csv"))
        .and(header("Content-Type", "text/csv"))
        .and(query_param("name", "my plan"))
        .and(wiremock::matchers::body_string("title\nA\n"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "created": 1 })))
        .mount(&server)
        .await;

    let uri = server.uri();
    let resp = run_blocking(move || {
        TackClient::new(&make_config(&uri)).unwrap().post_text(
            "/projects/import-csv",
            &[("name", "my plan")],
            "text/csv",
            "title\nA\n".into(),
        )
    })
    .await;

    assert_eq!(resp.unwrap()["created"], 1);
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
    let guard = tempfile::tempdir().expect("temporary directory");
    let tmp = guard.path();
    let original_home = std::env::var("HOME").ok();
    // SAFETY: single-threaded test; no concurrent env reads in this process.
    unsafe { std::env::set_var("HOME", tmp) };

    config::save("http://test:9999", Some("tok123")).unwrap();

    let cfg = Config::load(None, None);
    assert_eq!(cfg.base_url, "http://test:9999");
    assert_eq!(cfg.token.as_deref(), Some("tok123"));

    // `~/.tackrc` carries a credential, so `config::save` writes it
    // owner-only — prove that end-to-end through the real save path.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(tmp.join(".tackrc"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "~/.tackrc must be owner-only (0600), got {:o}",
            mode & 0o777
        );
    }

    match original_home {
        Some(h) => unsafe { std::env::set_var("HOME", h) },
        None => unsafe { std::env::remove_var("HOME") },
    }
}

// ── the local server's token file is the last place the token is looked for ──

#[test]
fn token_falls_back_to_the_local_servers_file() {
    let guard = tempfile::tempdir().expect("temporary directory");
    // SAFETY: single-threaded test; no concurrent env reads in this process.
    unsafe {
        std::env::set_var("HOME", guard.path());
        std::env::remove_var("XDG_DATA_HOME");
        std::env::remove_var("TACK_API_TOKEN");
    }
    let data_root = dirs::data_dir().expect("data dir").join("tack");
    assert!(
        data_root.starts_with(guard.path()),
        "data root escaped HOME"
    );
    std::fs::create_dir_all(&data_root).unwrap();
    std::fs::write(data_root.join("api-token"), "from-the-server-file").unwrap();

    assert_eq!(
        Config::load(None, None).token.as_deref(),
        Some("from-the-server-file")
    );

    config::save("http://test:9999", Some("from-tackrc")).unwrap();
    assert_eq!(
        Config::load(None, None).token.as_deref(),
        Some("from-tackrc"),
        "~/.tackrc outranks the server's file"
    );
}

// ── start (GET project workflow, then PATCH item to the first in-progress status) ──

#[tokio::test]
async fn start_patches_item_to_first_in_progress_status() {
    let server = MockServer::start().await;
    let project_id = "ffffffff-0000-0000-0000-000000000000";
    let item_id = "11111111-0000-0000-0000-000000000000";

    Mock::given(method("GET"))
        .and(path(format!("/api/projects/{project_id}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": project_id,
            "workflow": {
                "workflow_type": "kanban",
                "statuses": [
                    {"name": "To Do", "category": "todo", "order": 0, "wip_limit": null},
                    {"name": "In Progress", "category": "in_progress", "order": 1, "wip_limit": null},
                    {"name": "Done", "category": "done", "order": 2, "wip_limit": null},
                ],
            },
        })))
        .mount(&server)
        .await;

    Mock::given(method("PATCH"))
        .and(path(format!("/api/items/{item_id}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": item_id,
            "status": "In Progress",
        })))
        .mount(&server)
        .await;

    let uri = server.uri();
    let (project, patched) = run_blocking(move || {
        let config = make_config(&uri);
        let client = TackClient::new(&config).unwrap();
        let project = client.get(&format!("/projects/{project_id}")).unwrap();
        let target_status = project["workflow"]["statuses"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["category"].as_str() == Some("in_progress"))
            .min_by_key(|s| s["order"].as_i64().unwrap_or(i64::MAX))
            .and_then(|s| s["name"].as_str())
            .unwrap()
            .to_string();
        let patched = client
            .patch(
                &format!("/items/{item_id}"),
                &serde_json::json!({"status": target_status}),
            )
            .unwrap();
        (project, patched)
    })
    .await;

    assert_eq!(
        project["workflow"]["statuses"][1]["category"],
        "in_progress"
    );
    assert_eq!(patched["status"], "In Progress");
}

// ── open (GET item, resolve project_id) ────────────────────────────────────────

#[tokio::test]
async fn open_gets_item_to_resolve_web_url() {
    let server = MockServer::start().await;
    let project_id = "22222222-0000-0000-0000-000000000000";
    let item_id = "33333333-0000-0000-0000-000000000000";

    Mock::given(method("GET"))
        .and(path(format!("/api/items/{item_id}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "item": {
                "id": item_id,
                "project_id": project_id,
                "title": "Fix login bug",
            },
        })))
        .mount(&server)
        .await;

    let uri = server.uri();
    let base_url = uri.clone();
    let resp = run_blocking(move || {
        let config = make_config(&uri);
        TackClient::new(&config)
            .unwrap()
            .get(&format!("/items/{item_id}"))
    })
    .await;

    assert!(resp.is_ok());
    let resp = resp.unwrap();
    let item = resp.get("item").unwrap_or(&resp);
    assert_eq!(item["project_id"], project_id);

    let url = format!(
        "{}/projects/{}/board?item={}",
        base_url.trim_end_matches('/'),
        item["project_id"].as_str().unwrap(),
        item["id"].as_str().unwrap(),
    );
    assert_eq!(
        url,
        format!("{base_url}/projects/{project_id}/board?item={item_id}")
    );
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

// ── export, init --template, template save (the `tack` binary against a mock) ─

/// Runs `tack --api-url <uri> <args>` with an empty HOME, so no `~/.tackrc`
/// or server token file of the operator's is read.
async fn run_tack(uri: String, args: &'static [&'static str]) -> std::process::Output {
    run_blocking(move || {
        let home = tempfile::tempdir().unwrap();
        std::process::Command::new(env!("CARGO_BIN_EXE_tack"))
            .env("HOME", home.path())
            .env_remove("TACK_API_URL")
            .env_remove("TACK_API_TOKEN")
            .arg("--api-url")
            .arg(&uri)
            .args(args)
            .output()
            .unwrap()
    })
    .await
}

#[tokio::test]
async fn export_bundle_asks_for_the_yaml_project_file() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/projects/p1/export"))
        .and(query_param("scope", "bundle"))
        .and(query_param("format", "yaml"))
        .respond_with(ResponseTemplate::new(200).set_body_string("project:\n  name: P\n"))
        .expect(1)
        .mount(&server)
        .await;

    let out = run_tack(server.uri(), &["export", "p1", "--bundle"]).await;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "project:\n  name: P\n"
    );
}

#[tokio::test]
async fn init_template_finds_the_template_by_name_then_creates_from_it() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/templates"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            { "id": "11111111-1111-1111-1111-111111111111", "name": "Library or CLI" },
            { "id": "22222222-2222-2222-2222-222222222222", "name": "Web product with content" },
        ])))
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(
            "/api/projects/from-template/22222222-2222-2222-2222-222222222222",
        ))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": "550e8400-e29b-41d4-a716-446655440000", "project_type": "web",
        })))
        .expect(1)
        .mount(&server)
        .await;

    let out = run_tack(
        server.uri(),
        &["init", "Site", "--template", "web-product-with-content"],
    )
    .await;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = run_tack(server.uri(), &["init", "Site", "--template", "nope"]).await;
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Library or CLI") && stderr.contains("Web product with content"),
        "{stderr}"
    );
}

#[tokio::test]
async fn template_save_posts_to_save_as_template() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/projects/p1/save-as-template"))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({ "name": "Mine" }),
        ))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": "550e8400-e29b-41d4-a716-446655440000", "name": "Mine",
        })))
        .expect(1)
        .mount(&server)
        .await;

    let out = run_tack(server.uri(), &["template", "save", "p1", "--name", "Mine"]).await;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
