//! HTTP-level proof for `handlers::local_runner` (ADR 0061 decisions 2 and
//! 6): the routes exist only on a loopback bind with a control actually
//! wired in, they are a genuine 404 otherwise (never present-and-refusing),
//! and the on/off preference persists in `app_meta` and nowhere else. A fake
//! [`LocalRunnerControl`] stands in for the real embedded-runner composition
//! (`crates/tack-cli/src/local_runner.rs`), unreachable from this crate —
//! this proves the HTTP contract those routes must uphold regardless of
//! which control answers them.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::Utc;
use tack_api::{
    LocalRunnerControl, LocalRunnerControlError, RuntimeState, RuntimeStatus, config::AppConfig,
};
use tower::ServiceExt;

use crate::common::test_app_with_local_runner;

/// Records every call it receives so a test can assert on which methods
/// actually ran, without needing a real runner process. `pub(crate)` so the
/// sibling `crud` module can seed it as the project-token source for
/// `push_uses_the_project_token_before_the_environment_token`.
#[derive(Default)]
pub(crate) struct FakeControl {
    running: AtomicBool,
    start_calls: std::sync::atomic::AtomicUsize,
    /// Name → value, filled by [`FakeControl::set_secret`] — what `resolve_secret` reads
    /// back, so a test can seed `store:<name>` to resolve to a chosen value.
    secret_values: std::sync::Mutex<std::collections::HashMap<String, String>>,
    /// The runner id to return from `runner_id()`. Defaults to Some("runner-test").
    pub(crate) runner_id_override: std::sync::Mutex<Option<Option<String>>>,
}

#[async_trait::async_trait]
impl LocalRunnerControl for FakeControl {
    async fn status(&self) -> RuntimeStatus {
        if self.running.load(Ordering::SeqCst) {
            RuntimeStatus {
                state: RuntimeState::Running,
                since: Some(Utc::now()),
                max_concurrent_attempts: 2,
                active_attempts: 1,
            }
        } else {
            RuntimeStatus {
                state: RuntimeState::Stopped,
                since: None,
                max_concurrent_attempts: 2,
                active_attempts: 0,
            }
        }
    }

    async fn start(&self) -> Result<(), LocalRunnerControlError> {
        self.start_calls.fetch_add(1, Ordering::SeqCst);
        self.running.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    async fn runner_id(&self) -> Option<String> {
        self.runner_id_override
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| Some("runner-test".to_string()))
    }

    async fn resolve_secret(&self, reference: &str) -> Result<String, LocalRunnerControlError> {
        let name = reference.strip_prefix("store:").unwrap_or(reference);
        self.secret_values
            .lock()
            .unwrap()
            .get(name)
            .cloned()
            .ok_or_else(|| {
                LocalRunnerControlError::SecretStore(format!("no secret named {name:?}"))
            })
    }
}

impl FakeControl {
    /// Seeds the value `resolve_secret` answers for `name`.
    pub(crate) async fn set_secret(
        &self,
        name: &str,
        value: &str,
    ) -> Result<(), LocalRunnerControlError> {
        self.secret_values
            .lock()
            .unwrap()
            .insert(name.to_owned(), value.to_owned());
        Ok(())
    }
}

fn loopback_config() -> AppConfig {
    AppConfig {
        host: "127.0.0.1".to_owned(),
        ..AppConfig::default()
    }
}

fn non_loopback_config() -> AppConfig {
    AppConfig {
        host: "0.0.0.0".to_owned(),
        allow_unauthenticated_nonloopback: true,
        ..AppConfig::default()
    }
}

type MaybeControl = Option<Arc<dyn LocalRunnerControl>>;

#[tokio::test]
async fn routes_are_absent_without_both_loopback_and_a_control() {
    let control: Arc<dyn LocalRunnerControl> = Arc::new(FakeControl::default());
    let cases: Vec<(&str, AppConfig, MaybeControl)> = vec![
        (
            "non-loopback bind, control wired in",
            non_loopback_config(),
            Some(control.clone()),
        ),
        (
            "loopback bind, no control wired in",
            loopback_config(),
            None,
        ),
    ];
    for (name, config, control) in cases {
        let (app, _workspace_id) = test_app_with_local_runner(config, control).await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/local-runner")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // A genuine 404 — not a 409/403 "disabled" envelope. `build_router`
        // never merges `local_runner_routes` at all unless both hold, so
        // this is axum's own fallback for an unmatched path, proving the
        // routes are absent rather than present-and-refusing.
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{name}");
    }
}

#[tokio::test]
async fn loopback_bind_with_control_mounts_routes_and_starts_it() {
    let control = Arc::new(FakeControl::default());
    let control_trait_object: Arc<dyn LocalRunnerControl> = control.clone();
    let (app, _workspace_id) =
        test_app_with_local_runner(loopback_config(), Some(control_trait_object)).await;

    let get_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/local-runner")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(get_response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["enabled"], false);
    assert_eq!(body["state"], "stopped");
    assert!(body["since"].is_null());
    assert_eq!(body["max_concurrent_attempts"], 2);
    assert_eq!(body["active_attempts"], 0);
    assert!(
        body["config_file"].is_null(),
        "a runner on defaults names no settings file"
    );

    let put_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/local-runner")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"enabled": true}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(put_response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        control.start_calls.load(Ordering::SeqCst),
        1,
        "PUT {{enabled:true}} must call the exact same start() the auto-start check would"
    );

    let get_after = app
        .oneshot(
            Request::builder()
                .uri("/api/local-runner")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(get_after.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["enabled"], true);
    assert_eq!(body["state"], "running");
}

async fn post_json(
    app: &axum::Router,
    uri: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or_default())
}

fn git(dir: &std::path::Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

#[tokio::test]
async fn check_folder_reports_git_facts() {
    let control: Arc<dyn LocalRunnerControl> = Arc::new(FakeControl::default());
    let (app, _workspace_id) = test_app_with_local_runner(loopback_config(), Some(control)).await;
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-b", "trunk"]);
    std::fs::write(repo.join("a.txt"), "a").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "first"]);
    git(
        &repo,
        &["remote", "add", "origin", "https://example.com/r.git"],
    );
    std::fs::write(repo.join("b.txt"), "b").unwrap();
    let plain = tmp.path().join("plain");
    std::fs::create_dir(&plain).unwrap();
    let file = tmp.path().join("file.txt");
    std::fs::write(&file, "x").unwrap();
    let missing = tmp.path().join("missing");

    let check = |p: &std::path::Path| {
        post_json(
            &app,
            "/api/local-runner/check-folder",
            serde_json::json!({ "path": p.to_str().unwrap() }),
        )
    };
    let (status, body) = check(&repo).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        serde_json::json!({"exists": true, "is_dir": true, "is_git": true, "branch": "trunk",
            "remote_url": "https://example.com/r.git", "dirty_files": 1})
    );
    let (_, body) = check(&plain).await;
    assert_eq!(
        body,
        serde_json::json!({"exists": true, "is_dir": true, "is_git": false, "branch": null,
            "remote_url": null, "dirty_files": 0})
    );
    let (_, body) = check(&file).await;
    assert_eq!(body["exists"], true);
    assert_eq!(body["is_dir"], false);
    assert_eq!(body["is_git"], false);
    let (_, body) = check(&missing).await;
    assert_eq!(body["exists"], false);

    let (status, body) = post_json(
        &app,
        "/api/local-runner/check-folder",
        serde_json::json!({ "path": "relative/dir" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["details"]["field"], "path");
}

#[tokio::test]
async fn init_folder_creates_a_repo_once() {
    let control: Arc<dyn LocalRunnerControl> = Arc::new(FakeControl::default());
    let (app, _workspace_id) = test_app_with_local_runner(loopback_config(), Some(control)).await;
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("new").join("project");
    let body = serde_json::json!({ "path": target.to_str().unwrap() });

    let (status, _) = post_json(&app, "/api/local-runner/init-folder", body.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(target.join(".git").is_dir());

    let (status, _) = post_json(&app, "/api/local-runner/init-folder", body).await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_run_creates_a_hidden_project_and_a_request() {
    let repo = tack_test_support::setup_test_db().await;
    let workspace_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'W', '{}')")
        .bind(workspace_id.to_string())
        .execute(repo.pool())
        .await
        .unwrap();
    tack_db::repo::execution::seed_builtin_profiles(repo.pool())
        .await
        .unwrap();
    let t = "2026-09-01T00:00:00Z";
    sqlx::query(
        "INSERT INTO agent_runners (id, name, credential_hash, protocol_version, created_at, updated_at)
         VALUES ('runner-test', 'local-1', 'h', 1, ?, ?)",
    )
    .bind(t)
    .bind(t)
    .execute(repo.pool())
    .await
    .unwrap();
    let pool = repo.pool().clone();
    let control = Arc::new(FakeControl::default());
    let (tx, _rx) = tokio::sync::broadcast::channel(16);
    let app =
        crate::common::with_default_host(tack_api::router::build_router(tack_api::AppState {
            repo,
            config: loopback_config(),
            workspace_id,
            broadcast_tx: tx,
            webhook: None,
            local_runner: Some(control.clone()),
        }));
    let body = serde_json::json!({ "harness_kind": "claude-code" });

    // The embedded runner is stopped: nothing to target.
    let (status, error) = post_json(&app, "/api/local-runner/test-run", body.clone()).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["error"]["code"], "local_runner_unavailable");

    control.start().await.unwrap();
    let (status, first) = post_json(&app, "/api/local-runner/test-run", body.clone()).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    let request_id = first["request_id"].as_str().unwrap().to_owned();

    let row: (String, String, Option<String>, String, String, i64) = sqlx::query_as(
        "SELECT selector_id, repository_snapshot, requested_model, permission_policy,
                agent_profile_id, timeout_seconds FROM execution_requests WHERE id = ?",
    )
    .bind(&request_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, "runner-test");
    let repository: serde_json::Value = serde_json::from_str(&row.1).unwrap();
    assert_eq!(repository["kind"], "scratch");
    assert_eq!(repository["remote"], "");
    assert_eq!(row.2, None);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&row.3).unwrap(),
        // The Implementer's own tools for this harness: none would leave the agent unable to act.
        serde_json::json!({
            "tools": ["Read", "Edit", "Write", "Bash", "Agent", "Grep", "Glob"],
            "network": false
        })
    );
    let implementer: String =
        sqlx::query_scalar("SELECT id FROM agent_profiles WHERE kind = 'implementer'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row.4, implementer);
    assert_eq!(row.5, 300);

    let listing = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/projects")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let listing: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(listing.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(listing, serde_json::json!([]), "the test project is hidden");

    let (status, second) = post_json(&app, "/api/local-runner/test-run", body).await;
    assert_eq!(status, StatusCode::OK, "{second}");
    assert_ne!(second["request_id"], first["request_id"]);
    let projects: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM projects WHERE name = 'Agent tests' AND archived = 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(projects, 1, "the second call reuses the project");
    let items: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM items")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(items, 2);
}

#[tokio::test]
async fn test_run_is_refused_when_the_runner_is_not_enrolled() {
    let repo = tack_test_support::setup_test_db().await;
    let workspace_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'W', '{}')")
        .bind(workspace_id.to_string())
        .execute(repo.pool())
        .await
        .unwrap();
    tack_db::repo::execution::seed_builtin_profiles(repo.pool())
        .await
        .unwrap();
    let control = Arc::new(FakeControl::default());
    // Override runner_id to return None
    *control.runner_id_override.lock().unwrap() = Some(None);

    let (tx, _rx) = tokio::sync::broadcast::channel(16);
    let app =
        crate::common::with_default_host(tack_api::router::build_router(tack_api::AppState {
            repo,
            config: loopback_config(),
            workspace_id,
            broadcast_tx: tx,
            webhook: None,
            local_runner: Some(control.clone()),
        }));
    let body = serde_json::json!({ "harness_kind": "claude-code" });

    // Start the runner but with runner_id returning None
    control.start().await.unwrap();
    let (status, error) = post_json(&app, "/api/local-runner/test-run", body).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["error"]["code"], "local_runner_unavailable");
    assert_eq!(
        error["error"]["message"],
        "The embedded runner is not enrolled"
    );
}
