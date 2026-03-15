use std::time::Duration;

use axum::routing::{delete, get, patch, post, put};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

use flexpm_db::Repository;

use crate::config::AppConfig;
use crate::debug;
use crate::handlers::{comments, dependencies, items, projects, roles, sprints};

/// Shared application state passed to all handlers.
#[derive(Clone)]
pub struct AppState {
    pub repo: Repository,
    pub config: AppConfig,
    pub workspace_id: Uuid,
}

/// Build the full Axum router with all routes, middleware, and state.
pub fn build_router(state: AppState) -> Router {
    let api = Router::new()
        // ─── Health & Debug ──────────────────────────
        .route("/health", get(debug::health))
        .route("/debug/info", get(debug::debug_info))
        .route("/debug/db-stats", get(debug::db_stats))
        // ─── Projects ────────────────────────────────
        .route("/projects", post(projects::create_project))
        .route("/projects", get(projects::list_projects))
        .route("/projects/{id}", get(projects::get_project))
        .route("/projects/{id}", patch(projects::update_project))
        .route("/projects/{id}", delete(projects::delete_project))
        // ─── Items ───────────────────────────────────
        .route("/projects/{project_id}/items", post(items::create_item))
        .route("/projects/{project_id}/items", get(items::list_items))
        .route("/projects/{project_id}/items/tree", get(items::get_item_tree))
        .route("/projects/{project_id}/search", get(items::search_items))
        .route("/items/{id}", get(items::get_item))
        .route("/items/{id}", patch(items::update_item))
        .route("/items/{id}", delete(items::delete_item))
        // ─── Sprints ─────────────────────────────────
        .route("/projects/{project_id}/sprints", post(sprints::create_sprint))
        .route("/projects/{project_id}/sprints", get(sprints::list_sprints))
        .route("/sprints/{id}", get(sprints::get_sprint))
        .route("/sprints/{id}/status", patch(sprints::update_sprint_status))
        // ─── Roles ───────────────────────────────────
        .route("/projects/{project_id}/roles", post(roles::create_role))
        .route("/projects/{project_id}/roles", get(roles::list_roles))
        .route("/roles/{id}", delete(roles::delete_role))
        .route("/items/{item_id}/roles/{role_id}", put(roles::assign_role))
        .route("/items/{item_id}/roles/{role_id}", delete(roles::remove_role))
        // ─── Comments ────────────────────────────────
        .route("/items/{item_id}/comments", post(comments::create_comment))
        .route("/items/{item_id}/comments", get(comments::list_comments))
        // ─── Dependencies ────────────────────────────
        .route("/items/{item_id}/dependencies", post(dependencies::create_dependency))
        .route("/items/{item_id}/dependencies", get(dependencies::list_dependencies))
        .route("/items/{item_id}/dependencies/{dep_id}", delete(dependencies::delete_dependency));

    Router::new()
        .nest("/api", api)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &axum::http::Request<_>| {
                    tracing::info_span!(
                        "http_request",
                        method = %request.method(),
                        uri = %request.uri(),
                    )
                })
        )
        .layer(
            CorsLayer::permissive()
                .max_age(Duration::from_secs(3600))
        )
        .with_state(state)
}
