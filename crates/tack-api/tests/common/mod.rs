use axum::Router;
use tack_api::handlers::websocket::BoardEvent;
use tack_api::orch_runtime::OrchRuntime;
use tack_api::{AppState, config::AppConfig, router::build_router};
use tack_db::{Repository, init_pool, migrations};
use tokio::sync::broadcast;
use uuid::Uuid;

/// Build a fully wired Axum router backed by an in-memory SQLite database.
/// Returns the router and the test workspace ID.
#[allow(dead_code)] // not every test binary calls the no-config constructor directly
pub async fn test_app() -> (Router, Uuid) {
    test_app_with_config(AppConfig::default()).await
}

/// Same as `test_app` but with a custom config (e.g. to set an api_token).
pub async fn test_app_with_config(config: AppConfig) -> (Router, Uuid) {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_all(&pool).await.expect("migrations");

    let workspace_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'CI Workspace', '{}')",
    )
    .bind(workspace_id.to_string())
    .execute(&pool)
    .await
    .expect("insert workspace");

    let (tx, _rx) = broadcast::channel::<BoardEvent>(16);
    // Keep config.database_url in sync with the actual pool so backup/restore
    // handlers see the right path (or correctly detect in-memory).
    let config = AppConfig {
        database_url: "sqlite::memory:".to_string(),
        ..config
    };
    let state = AppState {
        repo: Repository::new(pool),
        config,
        workspace_id,
        broadcast_tx: tx,
        webhook: None,
        orch_runtime: OrchRuntime::new(),
    };

    (build_router(state), workspace_id)
}

/// Build a test app backed by a file-based SQLite database.
/// The caller supplies the full SQLite URL (e.g. `"sqlite:/tmp/test.db?mode=rwc"`).
/// Used for tests that require file-level operations (backup/restore).
#[allow(dead_code)] // not every test binary exercises file-backed databases
pub async fn test_app_with_file_db(db_url: &str) -> (Router, Uuid) {
    let pool = init_pool(db_url).await.expect("file-based pool");
    migrations::run_all(&pool).await.expect("migrations");

    let workspace_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'File Workspace', '{}')",
    )
    .bind(workspace_id.to_string())
    .execute(&pool)
    .await
    .expect("insert workspace");

    let config = AppConfig {
        database_url: db_url.to_string(),
        ..AppConfig::default()
    };

    let (tx, _rx) = broadcast::channel::<BoardEvent>(16);
    let state = AppState {
        repo: Repository::new(pool),
        config,
        workspace_id,
        broadcast_tx: tx,
        webhook: None,
        orch_runtime: OrchRuntime::new(),
    };

    (build_router(state), workspace_id)
}
