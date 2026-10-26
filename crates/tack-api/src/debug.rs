use axum::Json;
use axum::extract::State;
use axum::response::IntoResponse;
use serde_json::json;
use tack_db::queries;
use tracing::{info, instrument};

use crate::router::AppState;

/// GET /api/health — Liveness + readiness check
#[utoipa::path(
    get,
    path = "/api/health",
    tag = "system",
    responses((status = 200, description = "Service is live; reports version and applied migration count", body = serde_json::Value)),
)]
#[instrument(skip(state))]
pub async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let migrations_applied = queries::database::migrations_applied(&state.repo)
        .await
        .unwrap_or(0);

    // Echoed so a client attaching to an already-running server can confirm
    // that the server is the one it launched.
    let launch_nonce = std::env::var("TACK_LAUNCH_NONCE").ok();

    Json(health_body(migrations_applied, launch_nonce))
}

fn health_body(migrations_applied: i64, launch_nonce: Option<String>) -> serde_json::Value {
    let mut body = json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "migrations_applied": migrations_applied,
    });
    if let Some(nonce) = launch_nonce {
        body["launch_nonce"] = json!(nonce);
    }
    body
}

/// GET /api/debug/info — System info (only in debug builds)
#[utoipa::path(
    get,
    path = "/api/debug/info",
    tag = "system",
    responses((status = 200, description = "Build, version, database size, and non-sensitive config", body = serde_json::Value)),
)]
#[instrument(skip(state))]
pub async fn debug_info(State(state): State<AppState>) -> impl IntoResponse {
    info!("Debug info requested");

    let db_size = get_db_size(&state).await;

    Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "build": if cfg!(debug_assertions) { "debug" } else { "release" },
        "database": {
            "size_bytes": db_size,
            // `url` deliberately omitted — it can embed a filesystem path or
            // credentials and must not leak from an unauthenticated debug route.
        },
        "config": {
            "host": state.config.host,
            "port": state.config.port,
            "log_level": state.config.log_level,
        }
    }))
}

/// GET /api/debug/db-stats — Database statistics
#[utoipa::path(
    get,
    path = "/api/debug/db-stats",
    tag = "system",
    responses((status = 200, description = "Per-table row counts", body = serde_json::Value)),
)]
#[instrument(skip(state))]
pub async fn db_stats(State(state): State<AppState>) -> impl IntoResponse {
    let counts = get_table_counts(&state).await;

    Json(json!({
        "tables": counts,
    }))
}

async fn get_db_size(state: &AppState) -> i64 {
    queries::database::size_bytes(&state.repo)
        .await
        .unwrap_or(0)
}

async fn get_table_counts(state: &AppState) -> serde_json::Value {
    let tables = [
        "projects",
        "items",
        "sprints",
        "roles",
        "comments",
        "dependencies",
        "attachments",
    ];
    let mut counts = serde_json::Map::new();

    for table in tables {
        let count = queries::database::row_count(&state.repo, table)
            .await
            .unwrap_or(0);
        counts.insert(table.to_string(), json!(count));
    }

    serde_json::Value::Object(counts)
}

#[cfg(test)]
mod tests {
    use super::health_body;

    #[test]
    fn health_body_echoes_launch_nonce_only_when_set() {
        let set = health_body(3, Some("nonce-abc".to_string()));
        assert_eq!(set["launch_nonce"], "nonce-abc");

        let unset = health_body(3, None);
        assert!(
            unset.get("launch_nonce").is_none(),
            "launch_nonce must be absent when the variable is unset"
        );
    }
}
