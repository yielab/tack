# Testing

Tack's test suite is structured as a pyramid: fast pure-function tests at the base, integration tests in the middle, handler tests and CLI tests at the top. Every Rust test runs with a single command — `cargo nextest run --workspace` — and needs no external service; the summary line carries the count. `tack-desktop` is a workspace of its own (`cargo nextest run --manifest-path crates/tack-desktop/Cargo.toml`).

> **Scope of this page.** This chapter explains how to *write and run* the Rust tests, crate by crate. For the full cross-cutting test strategy — including the Playwright end-to-end suite, the k6 load baseline, the security audits, and the CI gate matrix — see [`docs/TESTING.md`](../../../TESTING.md), the authoritative testing reference. A count appears here only next to the command that produces it.

---

## Test Pyramid

| Crate | Kind | Run it alone |
|---|---|---|
| `tack-core` | Unit — pure functions, zero I/O | `cargo nextest run --workspace -E 'package(tack-core)'` |
| `tack-db` | Integration — in-memory SQLite (+ one `#[ignore]`d perf test) | `… -E 'package(tack-db)'` |
| `tack-orch` | Unit + contract — byte-pinned runner-v1 fixtures, golden files | `… -E 'package(tack-orch)'` |
| `tack-api` | Handler + unit — in-memory SQLite + Axum, no port | `… -E 'package(tack-api)'` |
| `tack-runner` | Unit + fake-harness crash matrix (live harnesses `#[ignore]`d, billed) | `… -E 'package(tack-runner)'` |
| `tack-cli` | Contract — `wiremock`, plus a real-process scheduler E2E | `… -E 'package(tack-cli)'` |

Always `--workspace`, selecting with `-E`: `cargo test -p <crate>` resolves dependency features differently and builds a second copy of every crate. The frontend adds the Vitest suite (`cd frontend && npm test`) plus a cross-browser **Playwright** end-to-end suite (`make e2e`) that boots an isolated API and the SPA.

---

## Running Tests

```sh
# Everything — failures and a one-line summary are the whole output
cargo nextest run --workspace

# Show println!/tracing output (runs the selected tests serially)
cargo nextest run --workspace --no-capture -E 'test(workflow_transition)'

# One test, by name (regex)
cargo nextest run --workspace -E 'test(/^workflow/)'

# One crate
cargo nextest run --workspace -E 'package(tack-core)'

# Performance test — ignored by default, needs ~5 s
cargo nextest run --workspace --run-ignored ignored-only -E 'test(list_items_p95)'

# Handler tests that require the bundled SPA (needs frontend/dist/ first;
# -p is right here — a feature build is its own resolution anyway)
npm run build --prefix frontend
cargo nextest run -p tack-api --features embed-spa
```

`cargo nextest` is a separate install: `cargo install cargo-nextest --locked`, or the prebuilt from <https://get.nexte.st>. Filtersets are documented at <https://nexte.st/docs/filtersets/>.

---

## `tack-core` — Unit Tests

All tests live alongside their source code in `#[cfg(test)]` modules inside `workflow.rs`, `vocabulary.rs`, and `dependency.rs`.

Because `tack-core` has no I/O, tests are plain synchronous functions:

```rust
#[test]
fn construction_rejects_skipping_stages() {
    let wf = construction_workflow();
    assert!(wf.validate_transition("Permit", "Handover").is_err());
    assert!(wf.validate_transition("Permit", "Build").is_err());
}
```

Use `assert_matches!` (from the standard library's `matches!` macro or the `assert_matches` crate) when you want to check which enum variant was returned without needing to exhaustively match all fields:

```rust
assert_matches!(
    wf.validate_transition("Permit", "Handover"),
    Err(CoreError::InvalidTransition { from, to })
        if from == "Permit" && to == "Handover"
);
```

**What to test in core:**

- Valid and invalid transitions for each preset workflow.
- WIP limits: under limit (ok), at the limit (error), over the limit (error), no limit set (always ok).
- Cycle detection: no cycle (ok), direct cycle (error), transitive cycle (error), self-reference (error).
- Vocabulary: overridden key, fallback to default, unknown key rejected.
- `initial_status`: correct for each preset, error for empty workflow.
- `is_done_status`, `find_first_done_status`: correct category detection.

---

## `tack-db` — Integration Tests

Tests live in `crates/tack-db/tests/`.

Each test gets a fresh in-memory database via the `setup_test_db()` helper:

```rust
pub async fn setup_test_db() -> Repository {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_all(&pool).await.expect("migrations");
    Repository::new(pool)
}
```

Because each `sqlite::memory:` URL creates a brand-new database, there is no shared state between tests even when they run in parallel.

A typical test:

```rust
#[tokio::test]
async fn test_create_and_get_project() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    let project = repo.create_project(ws_id, CreateProject {
        name: "Test Project".into(),
        description: None,
        project_type: ProjectType::Software,
        template: None,
    }).await.unwrap();

    let fetched = repo.get_project(project.id).await.unwrap().unwrap();
    assert_eq!(fetched.name, "Test Project");
}
```

Helper functions in `tests/common/mod.rs` reduce boilerplate:

- `setup_test_db()` — returns a `Repository` backed by a fresh in-memory pool with all migrations applied.
- `create_test_workspace(repo)` — inserts a bare workspace row and returns its `Uuid`.
- `make_project(repo, workspace_id)` — creates a software project.
- `make_item(repo, project)` — creates a task item in the project's initial workflow status.

**What to test at the DB layer:**

- CRUD round-trips: create → get → update → delete.
- List operations with filters (by status, by item type, by sprint, etc.).
- Cascading deletes (deleting a project should delete its items, sprints, roles, etc.).
- The parent-status propagation logic (`check_and_update_parent_status`).
- FTS search returning expected results.

### Performance test

`crates/tack-db/tests/perf_test.rs` contains one test marked `#[ignore]`:

```
list_items_p95_under_100ms_at_50k
```

This test inserts 50,000 items and measures the P95 latency of `list_items`. It is excluded from normal CI runs because it takes several seconds. Run it manually when you change query structure or add indexes:

```sh
cargo nextest run --workspace --run-ignored ignored-only -E 'test(list_items_p95)'
```

---

## `tack-api` — Handler Tests

Tests live in `crates/tack-api/tests/`. Add to the file whose subject fits rather than creating a new one: each file is its own binary and its own full link.

The `test_app()` helper in `tests/common/mod.rs` builds a fully wired Axum router backed by an in-memory SQLite database. It returns both the router and the test workspace ID:

```rust
pub async fn test_app() -> (Router, Uuid) {
    test_app_with_config(AppConfig::default()).await
}

pub async fn test_app_with_config(config: AppConfig) -> (Router, Uuid) {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_all(&pool).await.expect("migrations");
    // insert workspace, wire AppState, call build_router
}
```

Requests are sent via `tower::ServiceExt::oneshot`, which processes a single request through the router without starting a TCP server:

```rust
#[tokio::test]
async fn health_returns_ok() {
    let (app, _) = common::test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}
```

To send a JSON body, set the `Content-Type` header and provide a serialised body:

```rust
let body = serde_json::to_vec(&json!({
    "name": "My Project",
    "project_type": "software",
})).unwrap();

let res = app
    .oneshot(
        Request::builder()
            .method(Method::POST)
            .uri("/api/projects")
            .header("Content-Type", "application/json")
            .body(Body::from(body))
            .unwrap(),
    )
    .await
    .unwrap();

assert_eq!(res.status(), StatusCode::CREATED);
```

To test token authentication, use `test_app_with_config`:

```rust
let config = AppConfig {
    api_token: Some("secret".into()),
    ..AppConfig::default()
};
let (app, _) = common::test_app_with_config(config).await;
// Request without token → 401
// Request with correct token → 200
```

A variant helper, `test_app_with_file_db(db_url)`, creates a router backed by a file-based SQLite database. This is used for tests that exercise the backup and restore endpoints, which require a real file path.

**What to test at the handler layer:**

- HTTP status codes for success cases (200, 201, 204).
- HTTP status codes for error cases (400 for bad input, 404 for missing resources, 409 for conflicts, 401 for missing/wrong token).
- Response body shape (correct JSON keys and values).
- Workflow validation rejection (try to create an invalid transition and assert 400).
- Auth token gate (without token configured → open; with token configured → requires header).

---

## `tack-cli` — Contract Tests

Tests live in `crates/tack-cli/tests/cli_test.rs`. They use `wiremock` to spin up a mock HTTP server in the test process.

```rust
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
        TackClient::new(&config).unwrap()
            .post("/projects", &serde_json::json!({
                "name": "My App",
                "project_type": "software"
            }))
    }).await;

    assert!(resp.is_ok());
    assert_eq!(resp.unwrap()["name"], "My App");
}
```

Because `TackClient` uses `reqwest::blocking`, tests wrap the call in `tokio::task::spawn_blocking` (via the `run_blocking` helper) to avoid blocking the async test executor.

**What to test in the CLI layer:**

- That the correct HTTP method and path are called.
- That query parameters are correctly appended.
- That the correct request body is sent (use `wiremock`'s `.and(body_json(...))` matcher).
- That errors from the API are surfaced correctly (mock a 4xx/5xx response and check the `Err` return).

---

## CI

GitHub Actions runs the following on every push to `main`, `develop` and `claude/**`, and on every pull request:

1. **`cargo nextest run --workspace --profile ci`** — every Rust test, once, each in its own process; the JUnit report carries each test's status. The `#[ignore]`d tests (the perf test and the live-harness runner tests, which bill a real agent account) stay skipped.
2. **regenerate-and-diff gates** — the OpenAPI spec and tack-orch's golden files are regenerated from the code and must match what is committed.
3. **frontend job** — `npm run type-check`, `npm run build`, the design-token lint and a bundle-size budget.
4. **quality gates** — `scripts/check-comments.sh`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, the Playwright accessibility scan (axe, WCAG AA).

Two jobs run on pull requests and pushes to `main` only, because each costs as much as the whole test job: **coverage** (`cargo llvm-cov` floors per crate) and **embed-spa** (the size-optimised release build with the SPA embedded, and its binary-size budget).

The performance test (`list_items_p95`) is not run in CI. Run it locally when profiling query performance. See [`docs/TESTING.md`](../../../TESTING.md) for the complete CI gate matrix and the load/security suites.
