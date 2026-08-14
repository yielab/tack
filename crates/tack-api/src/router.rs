use std::sync::Arc;
use std::time::Duration;

use axum::extract::DefaultBodyLimit;
use axum::handler::Handler;
use axum::http::{HeaderValue, Method, header};
use axum::routing::{delete, get, patch, post, put};
use axum::{Router, middleware};
use tokio::sync::broadcast;
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

use tack_db::Repository;
use tack_db::repo::execution::{ExecutionClock, SystemExecutionClock};

use crate::config::AppConfig;
use crate::debug;
#[cfg(feature = "embed-spa")]
use crate::handlers::spa;
use crate::handlers::{
    alexa, attachments, backup, boards_multi, comments, custom_fields, decisions, dependencies,
    executions, export, import_github, import_linear, items, orch, projects, provisioning, roles,
    runner_admin, runner_protocol, settings, sprints, templates, websocket,
};
use crate::middleware::{inject_operator_principal, require_token};
use crate::orch_runtime::OrchRuntime;
use crate::webhook::WebhookClient;

/// Shared application state passed to all handlers.
#[derive(Clone)]
pub struct AppState {
    pub repo: Repository,
    pub config: AppConfig,
    pub workspace_id: Uuid,
    /// Broadcast channel for real-time WebSocket updates
    pub broadcast_tx: broadcast::Sender<websocket::BoardEvent>,
    /// Optional outbound webhook client (None when TACK_WEBHOOK_URL is unset)
    pub webhook: Option<WebhookClient>,
    /// Toggleable handle to the orchestration reconciler (card E1). Cheap to
    /// clone (`Arc` underneath) — every handler gets the same live runtime,
    /// so `PUT /api/settings/orchestration` starts/stops the exact tasks
    /// `server.rs` spawned (or didn't) at boot.
    pub orch_runtime: OrchRuntime,
}

impl AppState {
    pub fn pool(&self) -> &sqlx::SqlitePool {
        self.repo.pool()
    }
}

const ATTACH_LIMIT: usize = 50 * 1024 * 1024; // 50 MB for file uploads

fn content_security_policy(config: &AppConfig) -> HeaderValue {
    // `allowed_origins` is validated at startup. Keeping it in connect-src
    // permits an intentionally split frontend/API deployment without opening
    // the page to arbitrary script, frame, or object sources.
    let connect_sources = config.allowed_origins.join(" ");
    let policy = format!(
        "default-src 'self'; base-uri 'self'; object-src 'none'; frame-ancestors 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; connect-src 'self' {connect_sources}; form-action 'self'"
    );
    HeaderValue::from_str(&policy).expect("configured CSP must be a valid header value")
}

/// Agent-Factory Control Center routes (Phases 33–38). Every route this cycle
/// needs is batched into this one function (card A4 / TODO.md §2: `router.rs`
/// is a chokepoint file, touched once) — later-wave agents add their route to
/// the appropriate section below and update `crate::openapi::ApiDoc` rather
/// than restructuring this function or `build_router`.
///
/// The whole sub-router is gated behind the *effective* orchestration
/// setting (`app_meta`-stored value, falling back to `TACK_ORCH_ENABLE`) via
/// [`orch::require_orch_enabled`] — with orchestration disabled, every route
/// here returns `409 Conflict` with `error.code: "orchestration_disabled"`,
/// naming where to enable it (`PUT /api/settings/orchestration`), rather
/// than a bare 404 (TODO.md §0 rule 8, rewritten 2026-08-05 — card E1). The
/// auth token gate (`require_token`) is layered on top of this in
/// `build_router`, so it still applies as usual. `/api/settings/orchestration`
/// itself lives outside this sub-router (registered directly in
/// `build_router`, beside `/settings/backup`) precisely so it stays
/// reachable while the feature is off — see that route's own comment.
fn orch_routes(state: AppState) -> Router<AppState> {
    Router::new()
        // ─── Control planes (Wave 1 / A4, 33.5) ───────────────────────────
        .route(
            "/control-planes",
            post(orch::create_control_plane).get(orch::list_control_planes),
        )
        .route(
            "/control-planes/{id}",
            get(orch::get_control_plane)
                .patch(orch::update_control_plane)
                .delete(orch::delete_control_plane),
        )
        // ─── Project ↔ control-plane link (Wave 1 / A4, 33.5) ─────────────
        .route(
            "/projects/{id}/orch-link",
            get(orch::get_orch_link).put(orch::put_orch_link),
        )
        // ─── Fleet view aggregate (Wave 1 / A4, 33.5) ──────────────────────
        .route("/fleet", get(orch::get_fleet))
        // ─── Wave 2 (Phase 34) — metrics ────────────────────────────────────
        .route("/metrics", get(orch::get_metrics)) // B3, 34.3/34.7
        // ─── Wave 2 (Phase 34) — item/project agent activity ───────────────
        .route(
            "/items/{id}/agent-activity",
            get(orch::get_item_agent_activity),
        ) // B6, 34.8/34.9
        .route(
            "/projects/{id}/agent-activity",
            get(orch::get_project_agent_activity),
        ) // B6, 34.8/34.9
        // ─── Wave 3 (Phase 35) — dispatch ───────────────────────────────────
        .route("/items/{id}/dispatch", post(orch::dispatch_item)) // C1, 35.2/35.3/35.6
        .route("/sprints/{id}/dispatch", post(orch::dispatch_sprint)) // C3, 35.4
        .route(
            "/sprints/{id}/dispatch/dry-run",
            get(orch::dry_run_sprint_dispatch),
        ) // C3, 35.4
        // ─── Wave 4 (Phases 36–38) — approvals + provisioning ──────────────
        .route("/approvals", get(orch::list_pending_approvals)) // D1, 36.1 — fleet-wide inbox, read-only
        .route("/approvals/{token}", post(orch::decide_approval)) // D1, 36.1 — also gated on TACK_ORCH_APPROVAL_TOKEN (checked inside the handler, not this layer)
        .route("/projects/{id}/orch-budget", get(orch::get_orch_budget)) // D2, 36.3 — budget cap vs. mirrored spend
        .route("/projects/{id}/orch-policy", get(orch::get_orch_policy)) // D2, 36.4 — guardrail/tool-call/approval metrics (control-plane-wide)
        .route(
            "/templates/{id}/provision",
            post(provisioning::create_project_with_pod),
        ) // D4, 37.2 — provision a pod + create/link a Tack project from a template, rollback-on-failure (see handlers/provisioning.rs's module doc for why this is a separate route rather than a `provision_pod:true` extension of the existing endpoint)
        .merge(crate::handlers::economics::economics_routes()) // D5, 38.1-38.4 — unit economics summary + per-item export; see handlers/economics.rs
        .layer(middleware::from_fn_with_state(
            state,
            orch::require_orch_enabled,
        ))
}

/// Card C1's operator execution/fleet routes — `/api/executions`,
/// `/api/runner-fleets`, `/api/runners/*`, `/api/agent-profiles`,
/// `/api/model-profiles` (`crate::handlers::executions`,
/// `crate::handlers::runner_admin`) — plus two Wave 5 additions mounted the
/// same way: III-F1's decision resolution (`crate::handlers::decisions`) and
/// III-F2's operator artifact download
/// (`crate::handlers::runner_protocol::artifact_download`), both of which
/// shipped as deliberately unwired card-local modules with a suggested
/// integration snippet in their own doc comments — this function is the
/// Wave 5 integrator (III-F6) performing that integration. Every card-local
/// router already calls `with_state` internally (per its own `pub fn
/// routes(state) -> Router` signature), producing a fully-resolved
/// `Router<()>`; this re-labels that "no state missing" router's phantom
/// type parameter to `AppState` via a second `with_state` call — the
/// officially documented pattern for merging routers whose state types
/// differ (see `axum::Router::merge`'s own doc example) — so it can be
/// flat-`merge`d into `api` below at the same level as every other `/api/*`
/// route, rather than nested under an extra path segment neither card
/// chose.
///
/// Merged into `api` *before* `require_token` is layered on, so these
/// routes share the same operator authentication as the rest of `/api/*`
/// (`operator_session_or_api_token` per
/// `docs/contracts/runner-v1/protocol.json`) — never the runner router's
/// distinct bearer-credential check. `inject_operator_principal` (card C5,
/// `middleware.rs`) is layered directly on this sub-router so it runs for
/// every request these handlers see, strips any client-supplied
/// `x-tack-principal`, and replaces it with a value derived from the
/// request's own authenticated context — C1's handlers (and, as of this
/// integration, F1's/F2's) trust that header completely for idempotency/
/// audit scoping, so an external caller must never be able to set it.
///
/// **F1's decision-resolve route carries a second, independent gate on top**
/// (`TACK_EXECUTION_DECISION_TOKEN`, checked inside
/// `decisions::require_decision_token` — see that function's doc comment):
/// an integrator security decision (III-F6) that resolves the gap F1's own
/// handoff flagged and declined to decide unilaterally
/// (`docs/contracts/runner-v1/protocol.json` names decision resolution a
/// `"separately_scoped_operator_credential"`, distinct from the plain
/// operator gate every other route here uses). Mirrors
/// `handlers::orch::require_approval_token`/`TACK_ORCH_APPROVAL_TOKEN`
/// exactly, including its fail-closed-when-unset default.
///
/// F2's artifact-download route points at the same operator-configured
/// `TACK_STORAGE_DIR`-derived storage root as `runner_protocol_routes`'s own
/// artifact storage below, per F2's own recorded wiring request ("same
/// storage root as request 1, for consistency").
fn operator_execution_routes(state: &AppState) -> Router<AppState> {
    let clock: Arc<dyn ExecutionClock> = Arc::new(SystemExecutionClock);
    let operator_state = executions::OperatorExecutionState::with_clock(state.repo.clone(), clock);
    let decision_clock: Arc<dyn ExecutionClock> = Arc::new(SystemExecutionClock);
    let decision_state =
        decisions::DecisionOperatorState::with_clock(state.repo.clone(), decision_clock)
            .with_decision_token(state.config.execution_decision_token.clone());
    let artifact_download_state = runner_protocol::artifact_download::ArtifactDownloadState {
        repo: state.repo.clone(),
        artifact_storage: Arc::new(runner_protocol::artifact_storage::ArtifactStorage::new(
            format!("{}/execution-artifacts", state.config.storage_dir),
        )),
    };
    executions::routes(operator_state.clone())
        .merge(runner_admin::routes(operator_state))
        .merge(decisions::routes(decision_state))
        .merge(runner_protocol::artifact_download::routes(
            artifact_download_state,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            inject_operator_principal,
        ))
        .with_state::<AppState>(())
}

/// Card C2's runner-protocol v1 router
/// (`crate::handlers::runner_protocol`), mounted at
/// `docs/contracts/runner-v1/protocol.json`'s `base_path`
/// (`/api/runner/v1`). Nested as its own top-level branch in `build_router`
/// — a sibling of the `/api` nest, not a sub-path merged into it — so it
/// sits structurally **outside** the `require_token` layer applied to
/// `api`. That is the whole security property this function exists to
/// preserve: an operator Bearer token can never reach these routes, and a
/// runner credential can never reach the operator routes above, because the
/// two route families do not share a single gate that either could satisfy
/// — every runner-protocol write authenticates independently, per request,
/// against a hashed runner bearer credential
/// (`runner_protocol::runner_auth::authenticate`), matching
/// `protocol.json`'s `credentials_are_not_substitutable: true`. It still
/// inherits every layer applied to `outer` in `build_router` (CORS,
/// security headers, tracing) — only the operator-token check is skipped.
///
/// The global body limit is a partial exception, corrected by an
/// integrator-authorized cross-card amendment (see
/// `docs/agent-handoffs/part-iii/III-C2.md` and `III-C5.md`): this router
/// carries its own, more-specific `DefaultBodyLimit` layer (a fixed 4 MiB
/// protocol ceiling), and axum always applies whichever `DefaultBodyLimit`
/// is closest to the handler — so the plain global layer on `outer` alone
/// would never actually bind here. `state.config.max_body_size_bytes` is
/// threaded into `runner_protocol::routes` so its own layer enforces
/// `min(configured, 4 MiB)` instead: an operator who tightens the global
/// limit below 4 MiB gets a genuinely smaller runner-v1 surface, while a
/// loose or unset global limit can never widen it past the protocol
/// ceiling. Re-labelled to `Router<AppState>` via the same `with_state`
/// trick as `operator_execution_routes`, so it can be `nest`ed alongside
/// `api` without an extra `Service`-erasure layer.
///
/// Artifact content storage is rooted at the operator-configured
/// `TACK_STORAGE_DIR` (`state.config.storage_dir`), one level deeper than
/// attachments (`<storage_dir>/execution-artifacts`) so the two never
/// collide — fulfilling III-F2's recorded wiring request; without this,
/// `RunnerProtocolState::new` alone would fall back to a hardcoded,
/// process-CWD-relative default (see its own doc comment).
fn runner_protocol_routes(state: &AppState) -> Router<AppState> {
    let clock: Arc<dyn ExecutionClock> = Arc::new(SystemExecutionClock);
    let runner_state = runner_protocol::RunnerProtocolState::new(state.repo.clone(), clock)
        .with_artifact_storage_root(format!("{}/execution-artifacts", state.config.storage_dir));
    runner_protocol::routes(runner_state, state.config.max_body_size_bytes)
        .with_state::<AppState>(())
}

/// Build the full Axum router with all routes, middleware, and state.
pub fn build_router(state: AppState) -> Router {
    // ── CORS ─────────────────────────────────────────────────────────────────
    let allowed_origins: Vec<HeaderValue> = state
        .config
        .allowed_origins
        .iter()
        .filter_map(|o| o.parse().ok())
        .collect();

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(allowed_origins))
        .allow_methods(AllowMethods::list([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ]))
        .allow_headers(AllowHeaders::list([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            header::ACCEPT,
            // `If-Match` — card G3's optimistic-concurrency precondition on
            // items/orch-links/control-planes PATCH/PUT. Without this, any
            // cross-origin browser client (anything through
            // `TACK_ALLOWED_ORIGINS` that isn't same-origin `embed-spa`)
            // fails preflight on every conditional write and silently falls
            // back to unconditional last-write-wins.
            header::IF_MATCH,
            // `X-Tack-Approval-Token` — pre-existing bug, not something this
            // cycle introduced: `frontend/src/features/approvals/api.ts`
            // sends this on every grant/deny and it has only ever worked
            // because production is same-origin via `embed-spa`. Reusing
            // the handler's own constant (rather than a hand-copied
            // literal) so this list can't drift from the header
            // `handlers::orch::decide_approval` actually reads.
            header::HeaderName::from_static(orch::APPROVAL_TOKEN_HEADER),
        ]))
        // No `expose_headers` call existed before this card — a browser
        // could read zero non-safelisted response headers from this API.
        // `ETag` is the first one anything needs: cards G1/G3 add it to
        // `GET` responses so a client can send it back as `If-Match`, and
        // an unexposed response header is invisible to `fetch()`/`XHR`
        // regardless of what the server sends on the wire.
        .expose_headers([header::ETAG])
        .max_age(Duration::from_secs(3600));

    // ── API routes ───────────────────────────────────────────────────────────
    let api = Router::new()
        // ─── OpenAPI contract (public — no auth to read the schema) ──────
        .route("/openapi.json", get(crate::openapi::openapi_json))
        // ─── Health & Debug (always public) ──────────────────────────────
        .route("/health", get(debug::health))
        .route("/debug/info", get(debug::debug_info))
        .route("/debug/db-stats", get(debug::db_stats))
        // ─── Backup / restore ────────────────────────────────────────────
        .route("/backup", get(backup::get_backup))
        .route(
            "/restore",
            post(backup::post_restore.layer(DefaultBodyLimit::max(ATTACH_LIMIT))),
        )
        // ─── Remote cloud backup ──────────────────────────────────────────
        .route("/backup/remote", post(backup::post_remote_backup))
        .route("/backup/remote", get(backup::get_remote_backups))
        .route("/backup/remote/restore", post(backup::post_remote_restore))
        .route("/backup/remote/verify", post(backup::post_remote_verify))
        // ─── Cloud backup settings (UI-editable) ──────────────────────────
        .route(
            "/settings/backup",
            get(settings::get_backup_settings).put(settings::put_backup_settings),
        )
        // ─── Orchestration settings (UI-editable; card E1) ─────────────────
        // Deliberately **outside** `orch_routes`'/`require_orch_enabled`'s
        // gate: this is the one orchestration-adjacent endpoint that must
        // stay reachable while orchestration is off — it's how an operator
        // discovers the feature exists and turns it on. See this file's
        // `orch_routes` doc comment and TODO.md §0 rule 8.
        .route(
            "/settings/orchestration",
            get(settings::get_orch_settings).put(settings::put_orch_settings),
        )
        // ─── Projects ────────────────────────────────────────────────────
        .route("/projects", post(projects::create_project))
        .route("/projects", get(projects::list_projects))
        .route("/projects/{id}", get(projects::get_project))
        .route("/projects/{id}", patch(projects::update_project))
        .route("/projects/{id}", delete(projects::delete_project))
        // ─── Export/Import ───────────────────────────────────────────────
        .route("/projects/{id}/export", get(export::export_project))
        .route("/projects/import", post(export::import_project))
        .route("/projects/{id}/import-csv", post(export::import_csv))
        .route(
            "/projects/{id}/import-github",
            post(import_github::import_github),
        )
        .route(
            "/projects/{id}/import-linear",
            post(import_linear::import_linear),
        )
        // ─── Items ───────────────────────────────────────────────────────
        .route("/projects/{project_id}/items", post(items::create_item))
        .route("/projects/{project_id}/items", get(items::list_items))
        .route(
            "/projects/{project_id}/items/tree",
            get(items::get_item_tree),
        )
        .route("/projects/{project_id}/search", get(items::search_items))
        .route("/search", get(items::search_items_global))
        .route("/items/{id}", get(items::get_item))
        .route("/items/{id}", patch(items::update_item))
        .route("/items/{id}", delete(items::delete_item))
        // ─── Sprints ─────────────────────────────────────────────────────
        .route(
            "/projects/{project_id}/sprints",
            post(sprints::create_sprint),
        )
        .route("/projects/{project_id}/sprints", get(sprints::list_sprints))
        .route("/sprints/{id}", get(sprints::get_sprint))
        .route("/sprints/{id}/status", patch(sprints::update_sprint_status))
        // ─── Roles ───────────────────────────────────────────────────────
        .route("/projects/{project_id}/roles", post(roles::create_role))
        .route("/projects/{project_id}/roles", get(roles::list_roles))
        .route("/roles/{id}", delete(roles::delete_role))
        .route("/items/{item_id}/roles/{role_id}", put(roles::assign_role))
        .route(
            "/items/{item_id}/roles/{role_id}",
            delete(roles::remove_role),
        )
        // ─── Comments ────────────────────────────────────────────────────
        .route("/items/{item_id}/comments", post(comments::create_comment))
        .route("/items/{item_id}/comments", get(comments::list_comments))
        // ─── Dependencies ────────────────────────────────────────────────
        .route(
            "/items/{item_id}/dependencies",
            post(dependencies::create_dependency),
        )
        .route(
            "/items/{item_id}/dependencies",
            get(dependencies::list_dependencies),
        )
        .route(
            "/items/{item_id}/dependencies/{dep_id}",
            delete(dependencies::delete_dependency),
        )
        // ─── Attachments (upload has its own higher body limit) ──────────
        .route(
            "/items/{item_id}/attachments",
            post(attachments::upload_attachment.layer(DefaultBodyLimit::max(ATTACH_LIMIT)))
                .get(attachments::list_attachments),
        )
        .route("/attachments/{id}", get(attachments::download_attachment))
        .route("/attachments/{id}", delete(attachments::delete_attachment))
        // ─── Project Templates ───────────────────────────────────────────
        .route("/templates", post(templates::create_template))
        .route("/templates", get(templates::list_templates))
        .route("/templates/{id}", get(templates::get_template))
        .route("/templates/{id}", delete(templates::delete_template))
        .route(
            "/projects/from-template/{id}",
            post(templates::create_project_from_template),
        )
        .route(
            "/projects/{id}/save-as-template",
            post(templates::save_project_as_template),
        )
        // ─── Custom Fields ───────────────────────────────────────────────
        .route(
            "/projects/{project_id}/custom-fields",
            post(custom_fields::create_field),
        )
        .route(
            "/projects/{project_id}/custom-fields",
            get(custom_fields::list_fields),
        )
        .route("/custom-fields/{id}", get(custom_fields::get_field))
        .route("/custom-fields/{id}", patch(custom_fields::update_field))
        .route("/custom-fields/{id}", delete(custom_fields::delete_field))
        .route(
            "/items/{item_id}/custom-fields/{field_id}",
            put(custom_fields::set_field_value),
        )
        .route(
            "/items/{item_id}/custom-fields/{field_id}",
            get(custom_fields::get_field_value),
        )
        .route(
            "/items/{item_id}/custom-fields/{field_id}",
            delete(custom_fields::delete_field_value),
        )
        .route(
            "/items/{item_id}/custom-fields",
            get(custom_fields::get_all_field_values),
        )
        // ─── Multiple Boards ─────────────────────────────────────────────
        .route(
            "/projects/{project_id}/boards",
            post(boards_multi::create_board),
        )
        .route(
            "/projects/{project_id}/boards",
            get(boards_multi::list_boards),
        )
        .route("/projects/{id}/boards/live", get(websocket::board_live))
        .route("/boards/{id}", get(boards_multi::get_board))
        .route("/boards/{id}", patch(boards_multi::update_board))
        .route("/boards/{id}", delete(boards_multi::delete_board))
        .route("/boards/{id}/view", get(boards_multi::get_board_view))
        // ─── Alexa voice integration (skill-ID auth, exempt from token) ──
        .route("/alexa", post(alexa::handle_request))
        // ─── Agent-Factory Control Center (Phase 33+, gated) ──────────────
        // Every route this cycle needs is batched into this one sub-router so
        // router.rs is structurally touched once (TODO.md §2's chokepoint
        // note); later waves add their route to `orch_routes` below rather
        // than restructuring this file. `require_orch_enabled` returns a 409
        // with `error.code: "orchestration_disabled"` for every route here
        // while orchestration is off (TODO.md §0 rule 8, card E1).
        .merge(orch_routes(state.clone()))
        // ─── Operator execution/fleet API (Part III, card C1; wired here
        // by card C5) — `/executions`, `/runner-fleets`, `/runners/*`,
        // `/agent-profiles`, `/model-profiles`. Same operator auth as
        // everything else in this router: merged in *before*
        // `require_token` below. See `operator_execution_routes`'s doc
        // comment for why this is a `merge`, not a `nest`. ───────────────
        .merge(operator_execution_routes(&state))
        // ─── Auth token gate ──────────────────────────────────────
        .layer(middleware::from_fn_with_state(state.clone(), require_token));

    let outer = Router::new()
        .nest("/api", api)
        // ─── Runner protocol v1 (Part III, card C2; wired here by card
        // C5) — deliberately a *sibling* nest, not merged into `api`
        // above, so it never passes through that router's
        // `require_token` layer. See `runner_protocol_routes`'s doc
        // comment. ──────────────────────────────────────────────────────
        .nest("/api/runner/v1", runner_protocol_routes(&state));

    #[cfg(feature = "embed-spa")]
    let outer = outer.fallback(spa::serve_spa);

    outer
        // ── Global body limit (attachments route overrides above) ────────
        .layer(DefaultBodyLimit::max(state.config.max_body_size_bytes))
        // ── Minimal security response headers ────────────────────────────
        .layer(SetResponseHeaderLayer::overriding(
            header::HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::HeaderName::from_static("referrer-policy"),
            HeaderValue::from_static("same-origin"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::HeaderName::from_static("x-frame-options"),
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::CONTENT_SECURITY_POLICY,
            content_security_policy(&state.config),
        ))
        // ── CORS ─────────────────────────────────────────────────────────
        .layer(cors)
        // ── Request tracing ──────────────────────────────────────────────
        .layer(
            TraceLayer::new_for_http().make_span_with(|request: &axum::http::Request<_>| {
                tracing::info_span!(
                    "http_request",
                    method = %request.method(),
                    // Query strings can carry external callback credentials
                    // (notably Alexa's compatibility token). Never put them
                    // in a tracing field or span.
                    path = %request.uri().path(),
                )
            }),
        )
        .with_state(state)
}
