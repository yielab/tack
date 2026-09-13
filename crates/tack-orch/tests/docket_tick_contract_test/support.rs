//! Fixture setup, the `TestRepoStore` fake control plane, wire-body
//! builders, the one-tick driver, and the golden-file harness (request list
//! and resulting rows) shared by every scenario in `docket_tick_contract_test`.
//! No `#[test]` here: see the parent file for the five scenarios and their
//! own per-scenario mount helpers, which stay there rather than here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use sqlx::sqlite::SqlitePool;
use sqlx::{Column, Row};
use uuid::Uuid;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use tack_core::models::{CreateProject, Project, ProjectType};
use tack_core::vocabulary;
use tack_db::repo::orch::{
    CreateControlPlane, NewOrchApproval, NewOrchEvent, NewOrchMetric, NewOrchRun, UpsertOrchLink,
};
use tack_db::{Repository, init_pool, migrations};
use tack_orch::adapters::docket::DocketAdapter;
use tack_orch::reconciler::{
    ControlPlaneStore, HealthRecord, ReconcilerConfig, RegisteredPlane, spawn_reconcilers,
};
use tack_orch::{ControlPlane, OrchError};

// ---------------------------------------------------------------------------
// Fixture setup — mirrors ingestion/support.rs
// ---------------------------------------------------------------------------

pub(crate) async fn setup_repo() -> Repository {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_all(&pool).await.expect("migrations");
    Repository::new(pool)
}

pub(crate) async fn seed_workspace(repo: &Repository) -> Uuid {
    let id = Uuid::new_v4();
    let vocab = serde_json::to_string(&vocabulary::default_vocabulary()).unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'Test Workspace', ?)",
    )
    .bind(id.to_string())
    .bind(&vocab)
    .execute(repo.pool())
    .await
    .expect("insert workspace");
    id
}

pub(crate) async fn seed_project(repo: &Repository, workspace_id: Uuid) -> Project {
    repo.create_project(
        workspace_id,
        CreateProject {
            name: "Test Project".into(),
            description: None,
            project_type: ProjectType::Software,
            template: None,
        },
    )
    .await
    .expect("create project")
}

/// Bearer token every control plane below is configured with, so the
/// authenticated/unauthenticated route split (`docket.rs`'s
/// `get_authed`/`get_unauthed`) has something real to prove: the header NAME
/// `authorization` appears in the golden for `/runs`, `/approvals`,
/// `/traces/*` and never for `/health`, `/status.json`, `/metrics`.
/// [`assert_never_leaks_token`] checks the literal string never reaches a
/// golden file, not just that headers serialise as names only.
const PLANE_TOKEN: &str = "docket-secret-do-not-leak-9f3a";

pub(crate) async fn seed_plane(repo: &Repository, base_url: &str) -> Uuid {
    repo.create_control_plane(CreateControlPlane {
        name: "Test Docket".into(),
        kind: None,
        base_url: base_url.to_string(),
        token: Some(PLANE_TOKEN.to_string()),
    })
    .await
    .expect("create control plane")
    .id
}

pub(crate) async fn link_project(
    repo: &Repository,
    project_id: Uuid,
    plane_id: Uuid,
    remote_project: &str,
) {
    repo.upsert_orch_link(
        project_id,
        UpsertOrchLink {
            control_plane_id: plane_id,
            remote_project: remote_project.to_string(),
            pipeline_file: None,
            blueprint: None,
            auto_dispatch: false,
            budget_usd: None,
            status_map: serde_json::json!({}),
        },
    )
    .await
    .expect("create orch link");
}

/// A [`ControlPlaneStore`] backed directly by a real `Repository` — the
/// test-only stand-in for `tack-api::orch_store::RepoControlPlaneStore`
/// (`tack-orch` must never depend on `tack-api`, so there is no single real
/// implementation to import). The fetch-plus-persist-through-a-real-
/// `spawn_reconcilers`-loop shape, and this mechanical thin impl, are
/// copied from `tests/ingestion/runs.rs`/`traces.rs`'s own
/// `ingestion/support.rs` copy rather than shared, so each test binary
/// stays scoped to its own concerns. Unlike those two files, this one never
/// seeds an `orch_tasks` row to correlate against — every run/approval/event
/// below lands uncorrelated (`item_id: null`) on purpose, since correlation
/// is already covered there.
struct TestRepoStore {
    repo: Repository,
}

#[async_trait::async_trait]
impl ControlPlaneStore for TestRepoStore {
    async fn list_registered(&self) -> Result<Vec<RegisteredPlane>, OrchError> {
        let rows = self
            .repo
            .list_control_planes()
            .await
            .map_err(|e| OrchError::Unavailable(e.to_string()))?;

        let mut planes = Vec::new();
        for row in rows {
            if row.kind != "docket" {
                continue;
            }
            let token = self
                .repo
                .get_control_plane_token(row.id)
                .await
                .map_err(|e| OrchError::Unavailable(e.to_string()))?;
            let adapter = DocketAdapter::new(row.base_url.clone(), token)
                .map_err(|e| OrchError::Unavailable(e.to_string()))?;
            planes.push(RegisteredPlane {
                id: row.id,
                control_plane: Arc::new(adapter) as Arc<dyn ControlPlane>,
            });
        }
        Ok(planes)
    }

    async fn record_health(
        &self,
        control_plane_id: Uuid,
        record: &HealthRecord,
    ) -> Result<(), OrchError> {
        self.repo
            .update_control_plane_health(
                control_plane_id,
                record.health.as_str(),
                record.last_seen_at,
                record.consecutive_failures,
                record.api_version.as_deref(),
            )
            .await
            .map_err(|e| OrchError::Unavailable(e.to_string()))
    }

    async fn list_linked_projects(&self, control_plane_id: Uuid) -> Result<Vec<String>, OrchError> {
        let links = self
            .repo
            .list_orch_links_for_plane(control_plane_id)
            .await
            .map_err(|e| OrchError::Unavailable(e.to_string()))?;
        Ok(links.into_iter().map(|l| l.remote_project).collect())
    }

    async fn find_item_for_remote_task(
        &self,
        remote_task_id: &str,
    ) -> Result<Option<Uuid>, OrchError> {
        let task = self
            .repo
            .find_orch_task_by_remote_task_id(remote_task_id)
            .await
            .map_err(|e| OrchError::Unavailable(e.to_string()))?;
        Ok(task.map(|t| t.item_id))
    }

    async fn upsert_runs(
        &self,
        control_plane_id: Uuid,
        runs: &[NewOrchRun],
    ) -> Result<(), OrchError> {
        self.repo
            .upsert_orch_runs(control_plane_id, runs)
            .await
            .map_err(|e| OrchError::Unavailable(e.to_string()))
    }

    async fn upsert_approvals(
        &self,
        control_plane_id: Uuid,
        approvals: &[NewOrchApproval],
    ) -> Result<(), OrchError> {
        self.repo
            .upsert_orch_approvals(control_plane_id, approvals)
            .await
            .map_err(|e| OrchError::Unavailable(e.to_string()))
    }

    async fn upsert_metrics(
        &self,
        control_plane_id: Uuid,
        metrics: &[NewOrchMetric],
    ) -> Result<(), OrchError> {
        self.repo
            .upsert_orch_metrics(control_plane_id, metrics)
            .await
            .map_err(|e| OrchError::Unavailable(e.to_string()))
    }

    async fn list_trace_cursors(
        &self,
        control_plane_id: Uuid,
    ) -> Result<std::collections::HashMap<String, String>, OrchError> {
        let cursors = self
            .repo
            .list_trace_cursors(control_plane_id)
            .await
            .map_err(|e| OrchError::Unavailable(e.to_string()))?;
        Ok(cursors
            .into_iter()
            .map(|c| (c.remote_project, c.cursor))
            .collect())
    }

    async fn set_trace_cursor(
        &self,
        control_plane_id: Uuid,
        remote_project: &str,
        cursor: &str,
    ) -> Result<(), OrchError> {
        self.repo
            .set_trace_cursor(control_plane_id, remote_project, cursor)
            .await
            .map_err(|e| OrchError::Unavailable(e.to_string()))
    }

    async fn upsert_events(
        &self,
        control_plane_id: Uuid,
        events: &[NewOrchEvent],
    ) -> Result<(), OrchError> {
        self.repo
            .upsert_orch_events(control_plane_id, events)
            .await
            .map_err(|e| OrchError::Unavailable(e.to_string()))
    }
}

// ---------------------------------------------------------------------------
// docket wire-body builders
// ---------------------------------------------------------------------------

const HEALTH_BODY: &str = r#"{"status":"ok","gateway":0}"#;
const STATUS_BODY: &str = r#"{"apiVersion":"2","timestamp":"2026-08-05T00:00:00Z","gateway":"inactive","channels":[],"agents":[],"totalCostUsd":0.0}"#;

pub(crate) async fn mount_health_status(server: &MockServer) {
    mount_get(server, "/health", HEALTH_BODY).await;
    mount_get(server, "/status.json", STATUS_BODY).await;
}

/// Mounts a `GET <route>` returning `body` verbatim — the shape most
/// single-response mocks in this file share, without a query-param match.
pub(crate) async fn mount_get(server: &MockServer, route: &str, body: impl Into<String>) {
    Mock::given(method("GET"))
        .and(path(route))
        .respond_with(ResponseTemplate::new(200).set_body_string(body.into()))
        .mount(server)
        .await;
}

/// Mounts `GET /runs?project=<project>` returning `body` verbatim.
pub(crate) async fn mount_runs(server: &MockServer, project: &str, body: impl Into<String>) {
    Mock::given(method("GET"))
        .and(path("/runs"))
        .and(query_param("project", project))
        .respond_with(ResponseTemplate::new(200).set_body_string(body.into()))
        .mount(server)
        .await;
}

/// Mounts `GET /traces/<project>`, optionally matching `since`, returning
/// `body` verbatim.
pub(crate) async fn mount_traces(
    server: &MockServer,
    project: &str,
    since: Option<&str>,
    body: String,
) {
    let mock = Mock::given(method("GET")).and(path(format!("/traces/{project}")));
    let mock = match since {
        Some(since) => mock.and(query_param("since", since)),
        None => mock,
    };
    mock.respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(server)
        .await;
}

/// One `GET /runs` list entry. `finished_at: None` renders as JSON `null`,
/// matching a still-running docket run.
pub(crate) fn run_json(
    id: &str,
    source: &str,
    state: &str,
    created: &str,
    finished_at: Option<&str>,
) -> String {
    let finished = match finished_at {
        Some(f) => format!("\"{f}\""),
        None => "null".to_string(),
    };
    format!(
        r#"{{"id":"{id}","source":"{source}","project":"demo","state":"{state}","taskIds":[],
        "error":"","created":"{created}","startedAt":"{created}","finishedAt":{finished},
        "pids":[],"variables":{{}}}}"#
    )
}

pub(crate) fn approval_json(token: &str, action: &str, created: &str) -> String {
    format!(
        r#"{{"token":"{token}","project":"demo","role":"implementer","action":"{action}","state":"pending","created":"{created}","context":{{}}}}"#
    )
}

/// docket's *real* `GET /traces/{{project}}` wire shape, verified against
/// `serve.py`'s `_traces_page`/`do_GET` (see `adapters/docket.rs`'s module
/// doc): `events` is an array of raw JSON **strings**, each independently
/// encoding one event object, not an array of objects. Copied verbatim from
/// `ingestion/traces.rs`'s own helper of the same name — see the module
/// doc's "Pattern copied, not invented".
pub(crate) fn traces_body(events: &[serde_json::Value], next: &str) -> String {
    let encoded: Vec<String> = events
        .iter()
        .map(|e| serde_json::to_string(e).unwrap())
        .collect();
    serde_json::json!({ "events": encoded, "next": next }).to_string()
}

pub(crate) fn trace_event_json(
    session_id: &str,
    ts: &str,
    event_type: &str,
    payload: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "ts": ts,
        "project": "demo",
        "session_id": session_id,
        "agent_role": "lead",
        "event_type": event_type,
        "payload": payload,
        "cost_usd": 0.0021,
        "duration_ms": 842
    })
}

// ---------------------------------------------------------------------------
// Driving exactly one tick
// ---------------------------------------------------------------------------

/// Base poll interval for every scenario: large enough (even after ±20%
/// jitter — see `reconciler.rs`'s `jittered_secs`) that a second tick cannot
/// start inside this test's wait window. `spawn_one`'s loop
/// (`reconciler.rs`) fires tick 1 immediately with no up-front sleep, so
/// exactly one tick runs deterministically within that window.
const TICK_POLL_SECS: u64 = 100_000;

/// Upper bound on how long [`run_one_tick`] waits for the expected request
/// count to land before giving up and snapshotting whatever arrived anyway.
/// Deliberately a cap, not a hard-panic timeout: a deliberately-broken store
/// must be able to reach the golden comparison and fail there with a full
/// diff, rather than die on an opaque timeout assertion first. Verified by
/// hand once: comment out `three_linked_projects_issues_three_per_project_calls_each`'s
/// `link_project` calls (link zero projects instead of three) while leaving
/// its golden-file names pointed at the real goldens, run just that test,
/// confirm `assert_eq!` prints the full committed JSON (naming `demo-b`'s/
/// `demo-c`'s requests) against a shorter actual JSON missing them, then revert.
const REQUEST_WAIT_CAP: Duration = Duration::from_millis(1_500);

/// Upper bound on the post-request-count settle poll below: how long to
/// wait for `orch_*` writes to stop changing before giving up and
/// snapshotting whatever landed anyway — same "let a broken store reach
/// the golden diff" rationale as [`REQUEST_WAIT_CAP`].
const ROWS_SETTLE_CAP: Duration = Duration::from_millis(1_000);

/// Registers the plane behind `repo` with `spawn_reconcilers`, lets exactly
/// one tick run, then aborts and returns every request the mock server
/// observed (matched or not — an unmatched request still belongs in the
/// wire golden, since it proves the code tried to call a route this
/// scenario didn't expect).
pub(crate) async fn run_one_tick(
    server: &MockServer,
    repo: Repository,
    expected_requests: usize,
) -> Vec<Request> {
    // Kept for the settle-poll below, taken before `repo` moves into the
    // store — `Repository` is a cheap handle onto the same pool.
    let query_repo = repo.clone();
    let store: Arc<dyn ControlPlaneStore> = Arc::new(TestRepoStore { repo });
    let handles = spawn_reconcilers(
        true,
        store,
        ReconcilerConfig {
            poll_secs: TICK_POLL_SECS,
            ..Default::default()
        },
    )
    .await;
    assert_eq!(
        handles.len(),
        1,
        "exactly one control plane must be registered for a single observed tick"
    );

    let start = tokio::time::Instant::now();
    loop {
        let seen = server.received_requests().await.unwrap_or_default().len();
        if seen >= expected_requests || start.elapsed() >= REQUEST_WAIT_CAP {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    settle_orch_rows(&query_repo).await;

    for h in handles {
        h.abort();
    }

    server.received_requests().await.unwrap_or_default()
}

/// Polls `orch_*` rows for this scenario's one control plane until two
/// reads 20ms apart agree, or `ROWS_SETTLE_CAP` runs out. The persist phase
/// (`record_health`, then `persist_runs`/`persist_approvals`/
/// `persist_metrics`/`persist_events`) runs strictly after the fetch
/// phase's last HTTP `.await` resolves, inside the same spawned tick (see
/// `reconciler.rs`'s module doc, "three-phase shape") — but the pool has 5
/// connections, so its writes can land out of submission order, and a
/// single row appearing proves nothing about the rest. Comparing full
/// snapshots is the generic signal every scenario in this file can share.
async fn settle_orch_rows(repo: &Repository) {
    let Ok(planes) = repo.list_control_planes().await else {
        return;
    };
    let Some(control_plane_id) = planes.first().map(|p| p.id) else {
        return;
    };

    let start = tokio::time::Instant::now();
    let mut previous = to_golden_json(&snapshot_rows(repo.pool(), control_plane_id).await);
    loop {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let current = to_golden_json(&snapshot_rows(repo.pool(), control_plane_id).await);
        if current == previous || start.elapsed() >= ROWS_SETTLE_CAP {
            return;
        }
        previous = current;
    }
}

// ---------------------------------------------------------------------------
// Golden artifact (A): the ordered request list
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct GoldenRequest {
    method: String,
    path: String,
    /// Sorted by key via `BTreeMap`'s own iteration order — deterministic
    /// regardless of whether some future dependency enables `serde_json`'s
    /// `preserve_order` feature elsewhere in the build (this crate itself
    /// never does — see `reconciler.rs`'s `derive_event_id` doc comment).
    query: BTreeMap<String, String>,
    /// Header NAMES only, lower-cased, sorted, deduplicated — never values.
    /// See [`PLANE_TOKEN`]'s doc comment for why that split is load-bearing.
    headers: Vec<String>,
    body: Option<serde_json::Value>,
}

/// Recursively rebuilds a `serde_json::Value` so every object's keys are
/// sorted, independent of whatever `Map` implementation `serde_json` is
/// compiled with — see [`GoldenRequest::query`]'s doc comment for why this
/// crate does not rely on that being invariant forever.
fn canonical_json(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let sorted: BTreeMap<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), canonical_json(v)))
                .collect();
            serde_json::to_value(sorted).expect("re-serialize a sorted object")
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(canonical_json).collect())
        }
        other => other.clone(),
    }
}

/// The body canonicalised: parse JSON and re-serialise with sorted keys;
/// non-JSON bodies as-is. Every request this
/// file's scenarios capture is a bodyless `GET`, so this always returns
/// `None` in practice; the JSON/non-JSON branches exist so the golden format
/// does not have to change the day a future scenario captures a `POST`.
fn canonicalize_body(bytes: &[u8]) -> Option<serde_json::Value> {
    if bytes.is_empty() {
        return None;
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => match serde_json::from_str::<serde_json::Value>(text) {
            Ok(value) => Some(canonical_json(&value)),
            Err(_) => Some(serde_json::Value::String(text.to_string())),
        },
        Err(_) => Some(serde_json::Value::String(format!(
            "<{} non-utf8 bytes>",
            bytes.len()
        ))),
    }
}

fn to_golden_request(req: &Request) -> GoldenRequest {
    let query: BTreeMap<String, String> = req
        .url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let mut headers: Vec<String> = req
        .headers
        .keys()
        .map(|k| k.as_str().to_ascii_lowercase())
        .collect();
    headers.sort();
    headers.dedup();
    GoldenRequest {
        method: req.method.to_string(),
        path: req.url.path().to_string(),
        query,
        headers,
        body: canonicalize_body(&req.body),
    }
}

fn to_golden_requests(reqs: &[Request]) -> Vec<GoldenRequest> {
    reqs.iter().map(to_golden_request).collect()
}

// ---------------------------------------------------------------------------
// Golden artifact (B): the resulting rows
// ---------------------------------------------------------------------------

/// Only two things vary between runs of the same scenario, and only these
/// are normalised: [`WALL_CLOCK_COLUMNS`] (`datetime('now')` at insert —
/// `tack-db/src/repo/orch.rs`'s upsert functions) collapse to
/// [`NOW_PLACEHOLDER`], and `control_planes.id` (freshly minted every test
/// run) collapses to [`CONTROL_PLANE_PLACEHOLDER`] wherever it appears as a
/// foreign key. Everything else — `run_id`/`token`, `state`, `payload`,
/// timestamps parsed from fixture fields — comes FROM the fixture and is
/// asserted literally: normalising a value that came off the wire would
/// blind this oracle to the exact class of change it exists to catch.
const NOW_PLACEHOLDER: &str = "<NOW>";
const CONTROL_PLANE_PLACEHOLDER: &str = "<CONTROL_PLANE_ID>";
const WALL_CLOCK_COLUMNS: [&str; 3] = ["created_at", "updated_at", "scraped_at"];

#[derive(Serialize)]
struct GoldenRows {
    orch_runs: Vec<BTreeMap<String, serde_json::Value>>,
    orch_approvals: Vec<BTreeMap<String, serde_json::Value>>,
    orch_events: Vec<BTreeMap<String, serde_json::Value>>,
    orch_metrics: Vec<BTreeMap<String, serde_json::Value>>,
    orch_trace_cursors: Vec<BTreeMap<String, serde_json::Value>>,
}

/// Dynamic `SELECT *` row -> sorted-key map, generic across all five tables.
/// Two columns need type-aware decoding rather than the `TEXT` every other
/// column in these five tables uses: `orch_metrics.value` (`REAL`, nullable
/// — see migration 025's comment on why it can't be `NOT NULL`) and
/// `orch_runs.run_attempt` (`INTEGER NOT NULL DEFAULT 1`, added by migration
/// 037's rebuild — sqlx's SQLite driver rejects decoding a
/// declared-`INTEGER` column as `Option<String>` outright rather than
/// coercing, so without this branch every scenario that touches `orch_runs`
/// panics on the first row).
async fn fetch_table(
    pool: &SqlitePool,
    sql: &'static str,
) -> Vec<BTreeMap<String, serde_json::Value>> {
    let rows = sqlx::query(sql)
        .fetch_all(pool)
        .await
        .expect("query golden snapshot table");
    rows.into_iter()
        .map(|row| {
            let mut map = BTreeMap::new();
            for col in row.columns() {
                let name = col.name();
                let value = if name == "value" {
                    row.try_get::<Option<f64>, _>(name)
                        .expect("decode REAL column")
                        .map(|v| serde_json::json!(v))
                        .unwrap_or(serde_json::Value::Null)
                } else if name == "run_attempt" {
                    row.try_get::<Option<i64>, _>(name)
                        .expect("decode INTEGER column")
                        .map(|v| serde_json::json!(v))
                        .unwrap_or(serde_json::Value::Null)
                } else {
                    row.try_get::<Option<String>, _>(name)
                        .expect("decode TEXT column")
                        .map(serde_json::Value::String)
                        .unwrap_or(serde_json::Value::Null)
                };
                map.insert(name.to_string(), value);
            }
            map
        })
        .collect()
}

fn normalize_common(row: &mut BTreeMap<String, serde_json::Value>, control_plane_id: &str) {
    for col in WALL_CLOCK_COLUMNS {
        if row.contains_key(col) {
            row.insert(
                col.to_string(),
                serde_json::Value::String(NOW_PLACEHOLDER.to_string()),
            );
        }
    }
    for value in row.values_mut() {
        if let serde_json::Value::String(s) = value
            && s.as_str() == control_plane_id
        {
            *s = CONTROL_PLANE_PLACEHOLDER.to_string();
        }
    }
}

/// Re-parses a `TEXT`-stored JSON column (`payload`, `labels`) into a nested
/// `Value`, canonicalised — for readability, and so the "body canonicalised"
/// discipline applies here too, not only to request bodies.
fn parse_json_field(row: &mut BTreeMap<String, serde_json::Value>, field: &str) {
    let parsed = match row.get(field) {
        Some(serde_json::Value::String(raw)) => serde_json::from_str::<serde_json::Value>(raw).ok(),
        _ => None,
    };
    if let Some(value) = parsed {
        row.insert(field.to_string(), canonical_json(&value));
    }
}

/// Replaces a Tack-generated id column (`orch_events.id` — content-derived
/// via `derive_event_id`, but its input includes the random
/// `control_plane_id` so it's exactly as volatile across runs;
/// `orch_metrics.id` — `Uuid::new_v4()` at insert, no natural key) with an
/// ordinal placeholder. `ordinal` is the row's position after sorting, so
/// two runs of the same scenario always assign the same placeholder to the
/// same (by every OTHER column) row.
fn normalize_generated_id(
    row: &mut BTreeMap<String, serde_json::Value>,
    field: &str,
    label: &str,
    ordinal: usize,
) {
    row.insert(
        field.to_string(),
        serde_json::Value::String(format!("<{label}_{}>", ordinal + 1)),
    );
}

async fn snapshot_rows(pool: &SqlitePool, control_plane_id: Uuid) -> GoldenRows {
    let cp = control_plane_id.to_string();

    // `external_run_id`, not `run_id` — migration 037 rebuilt
    // `orch_runs` around the widened primary key `(control_plane_id,
    // external_run_id, run_attempt)` and renamed the physical column; the
    // repo layer (`tack-db/src/repo/orch.rs`) aliases it back to `run_id` in
    // its own `SELECT`s so `OrchRun`'s Rust-level shape is unchanged, but
    // this is a raw `SELECT *` against the table itself, which sees the
    // physical name. Ordering by it (rather than the full new PK) is still
    // sufficient here: every scenario in this file uses a single
    // `control_plane_id`, and every run it seeds has `run_attempt` 1 (no
    // scenario exercises retries).
    let mut orch_runs = fetch_table(pool, "SELECT * FROM orch_runs ORDER BY external_run_id").await;
    let mut orch_approvals = fetch_table(pool, "SELECT * FROM orch_approvals ORDER BY token").await;
    let mut orch_events = fetch_table(
        pool,
        "SELECT * FROM orch_events ORDER BY occurred_at, event_type",
    )
    .await;
    let mut orch_metrics =
        fetch_table(pool, "SELECT * FROM orch_metrics ORDER BY name, labels").await;
    let mut orch_trace_cursors = fetch_table(
        pool,
        "SELECT * FROM orch_trace_cursors ORDER BY remote_project",
    )
    .await;

    for row in orch_runs
        .iter_mut()
        .chain(orch_approvals.iter_mut())
        .chain(orch_events.iter_mut())
        .chain(orch_metrics.iter_mut())
        .chain(orch_trace_cursors.iter_mut())
    {
        normalize_common(row, &cp);
    }

    for (i, row) in orch_events.iter_mut().enumerate() {
        parse_json_field(row, "payload");
        normalize_generated_id(row, "id", "EVENT_ID", i);
    }
    for (i, row) in orch_metrics.iter_mut().enumerate() {
        parse_json_field(row, "labels");
        normalize_generated_id(row, "id", "METRIC_ID", i);
    }

    GoldenRows {
        orch_runs,
        orch_approvals,
        orch_events,
        orch_metrics,
        orch_trace_cursors,
    }
}

// ---------------------------------------------------------------------------
// Golden-file harness — mirrors crates/tack-api/tests/openapi_contract.rs's
// UPDATE_OPENAPI=1 gate exactly, renamed here to UPDATE_GOLDEN=1.
// ---------------------------------------------------------------------------

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/tick")
}

fn to_golden_json<T: Serialize>(value: &T) -> String {
    let mut s = serde_json::to_string_pretty(value).expect("serialize golden json");
    s.push('\n');
    s
}

fn assert_matches_golden(file_name: &str, actual: &str) {
    let path = golden_dir().join(file_name);

    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(golden_dir()).expect("create tests/golden/tick");
        std::fs::write(&path, actual).expect("write golden file");
        eprintln!("Regenerated {}", path.display());
        return;
    }

    let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "could not read {} ({e}).\nGenerate it with: UPDATE_GOLDEN=1 cargo nextest run --workspace -E 'binary(docket_tick_contract_test)'",
            path.display()
        )
    });

    assert_eq!(
        committed,
        actual,
        "\n\n{} is out of date with the reconciler's observed tick behaviour.\n\
         Regenerate it with:\n    UPDATE_GOLDEN=1 cargo nextest run --workspace -E 'binary(docket_tick_contract_test)'\n",
        path.display()
    );
}

fn assert_never_leaks_token(golden_text: &str) {
    assert!(
        !golden_text.contains(PLANE_TOKEN),
        "a golden file must never contain the raw bearer token"
    );
}

/// The ordered-request golden half of every scenario's tail, checked for a
/// leaked token before the diff. `prefix` names the committed
/// `<prefix>.requests.json`.
pub(crate) fn assert_requests_golden(prefix: &str, requests: &[Request]) {
    let requests_golden = to_golden_json(&to_golden_requests(requests));
    assert_never_leaks_token(&requests_golden);
    assert_matches_golden(&format!("{prefix}.requests.json"), &requests_golden);
}

/// The resulting-rows golden half of every scenario's tail. `prefix` names
/// the committed `<prefix>.rows.json`.
pub(crate) async fn assert_rows_golden(prefix: &str, pool: &SqlitePool, plane_id: Uuid) {
    let rows_golden = to_golden_json(&snapshot_rows(pool, plane_id).await);
    assert_never_leaks_token(&rows_golden);
    assert_matches_golden(&format!("{prefix}.rows.json"), &rows_golden);
}
