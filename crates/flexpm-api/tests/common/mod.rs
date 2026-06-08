use axum::Router;
use flexpm_api::{config::AppConfig, router::build_router, AppState};
use flexpm_api::handlers::websocket::BoardEvent;
use flexpm_db::{init_pool, migrations, Repository};
use tokio::sync::broadcast;
use uuid::Uuid;

/// Build a fully wired Axum router backed by an in-memory SQLite database.
/// Returns the router and the test workspace ID.
pub async fn test_app() -> (Router, Uuid) {
    test_app_with_config(AppConfig::default()).await
}

/// Same as `test_app` but with a custom config (e.g. to set an api_token).
pub async fn test_app_with_config(config: AppConfig) -> (Router, Uuid) {
    let pool = init_pool("sqlite::memory:")
        .await
        .expect("in-memory pool");
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
    let state = AppState {
        repo: Repository::new(pool),
        config,
        workspace_id,
        broadcast_tx: tx,
    };

    (build_router(state), workspace_id)
}
