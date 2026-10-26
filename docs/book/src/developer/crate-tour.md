# Crate Tour

This chapter is for contributors who need to find the file that owns a behaviour. It walks through each Rust crate in the main workspace: what it owns, what it deliberately does not own, the key files, and the patterns worth understanding. The workspace has nine crates: `tack-protocol` (the contract the server and the runner share), `tack-core` (pure domain logic), `tack-orch` (pure runs policy), `tack-db` (the store), `tack-app` (the use cases and the background ticks), `tack-api` (HTTP), `tack-runner`, `tack-cli`, and `tack-test-support`, which holds test helpers. A tenth, `tack-desktop`, sits outside that workspace by design; its own section at the end of this chapter says why. `scripts/check-crate-graph.sh --print` prints which crate may depend on which, `scripts/arch-facts.sh` prints that table with the current counts, and the check fails on any edge outside it.

> The SolidJS web UI in `frontend/` is covered separately in
> [Frontend & Design System](frontend.md) — structure, the design-token system,
> and the `shared/ui` component kit.
>
> This chapter is a per-file walkthrough, not a reference. How an automatic run, a docket
> team and the merge ceiling are built is in [Runs, Teams and Automation](runs-and-teams.md);
> the project file is in [The Project File Format](project-file.md). For crate boundaries stated
> as a condensed table, design patterns, the DB schema, the API structure, and
> troubleshooting, see [`docs/ARCHITECTURE.md`](https://github.com/yielab/tack/blob/develop/docs/ARCHITECTURE.md) — the
> authority when the two disagree on a fact rather than depth.

---

## `tack-protocol`

**Lives in:** `crates/tack-protocol/src/`

**Owns:** the runner-v1 contract, the one thing the server and `tack-runner` both build on: the execution types (`execution/`), the attempt lifecycle, capability negotiation, usage provenance, the merge-readiness pack (`mrp.rs`: the `mrp-v1` types, their validation and their pull-request rendering), the prepare step (`prepare.rs`) and the one `Clock` trait with `SystemClock` (`clock.rs`). `utoipa` is behind an `openapi` feature.

**Does not own:** I/O of any kind, or a dependency on any other workspace crate. This is the trust boundary: what the board and a runner exchange is defined here and nowhere else, and `docs/contracts/runner-v1/` fixtures outrank any Rust or TypeScript type. `tests/runner_contract` byte-pins those fixtures.

---

### `execution/lifecycle.rs`

`validate_transition(from, to, actor)` is the single authority for every legal
execution-attempt state change. States: `queued | leased | preparing | running |
waiting_decision | succeeded | failed | cancelled | lost | needs_operator`. Each
transition is validated against **both** the state pair and which `TransitionActor`
(`Scheduler`, `Operator`, `LeaseOwner`, `RecoveryService`) is allowed to request it —
for example, only `RecoveryService` may move an attempt into `lost` or
`needs_operator`; a lease owner reporting the same crash cannot self-authorize it. See
the [Recovery Runbook](../admin-guide/recovery-runbook.md) for what drives those
transitions operationally.

### `execution/types.rs`

Wire-shape-adjacent types shared by both the server and the runner:
`AgentProfileSnapshot { name, instructions, tool_policy, timeout_seconds, budgets }`,
`RepositorySnapshot { kind, remote, base_revision }`, `PermissionPolicy { tools,
network }`, and `EnvironmentValue { value, secret_reference }` — the last one is why
execution requests never store a raw secret: every environment entry is either a
literal non-secret value or an opaque reference the runner resolves locally.

### `usage_provenance.rs`

Two independent pure concerns, neither performing I/O. `model_provenance`
checks the request's resolved model (or "no model requested") against the attempt's
actual, observed execution — visible as a mismatch, never silently reconciled.
`build_usage_economics` keeps runner-observed wall-clock time cost structurally
separate from the harness/vendor's own self-reported token or dollar usage, never
summed into one opaque number. Every dollar-valued field in the runs crates is named
`*_usd_estimated`, never `*_usd` alone, and absent usage is a `Measurement` with
`source: NotMeasured`, never a fabricated `0`.

---

## `tack-core`

**Lives in:** `crates/tack-core/src/`

**Owns:** the planning and runs policy, as pure logic. Domain models, workflow engine, vocabulary system, dependency DAG, typed error enum, the merge policy, the rules that choose a run's profile and the tasks to start, and the contracts Tack reads and writes as plain data: an item's brief (`brief.rs`: the types, their validation and a Markdown rendering for the harness), and the project file (`outline.rs` and `outline/bundle.rs`).

**Does not own:** anything that performs I/O. No `sqlx`, no `reqwest`, no file operations, no `tokio`. This is enforced by the `Cargo.toml` — the crate has no async runtime dependency at all.

---

### `models.rs`

The single source of truth for every domain struct. Notable types:

- `Item` — the universal work unit. Fields include `item_type: ItemType`, `status: String`, `parent_id: Option<Uuid>`, `tags: Vec<String>`. The `status` field is a plain string rather than an enum because valid statuses are project-specific configuration, not compile-time constants.
- `Project` — carries the `workflow: WorkflowConfig` and `vocabulary: VocabularyMap` inline. Both are serialised to JSON when stored in SQLite.
- `ItemType` — an enum with `Epic`, `Feature`, `Task`, `Subtask`, `Bug`, `Requirement`, and `Custom(String)`. The `Custom` variant allows ad-hoc item types without a code change.
- `CreateItem`, `UpdateItem`, `CreateProject`, `UpdateProject`, etc. — all DTOs used for both API deserialization and repository function parameters. Keeping them in `tack-core` means the API and CLI reference the same validated shapes.

Validation constraints (length, range) are expressed via the `validator` crate's derive macros directly on the DTO structs. The API handlers call `.validate()?` before doing anything with the data.

---

### `workflow.rs`

Defines `WorkflowConfig`, which is what gets stored as JSON per project.

```rust
pub struct WorkflowConfig {
    pub workflow_type: WorkflowType,
    pub statuses: Vec<StatusDef>,
    pub transitions: Option<Vec<Transition>>,
}
```

Each `StatusDef` has a `name`, a `category` (`Todo`, `InProgress`, or `Done`), an optional `wip_limit`, and an `order` integer for display sorting.

**`validate_transition(from, to)`** is the central enforcement function. It:
1. Checks that both names exist in the status list.
2. If `transitions` is `Some(list)`, checks that the pair appears in the list.
3. Returns `Ok(())` or `Err(CoreError::InvalidTransition { from, to })`.

If `transitions` is `None`, any move between two known statuses is allowed. This is the default for Scrum and Kanban workflows.

**`check_wip_limit(status, current_count)`** looks up the `StatusDef` for the target column and returns `Err(CoreError::WipLimitExceeded { ... })` if `current_count >= limit`.

**Preset functions** produce ready-made configs for each domain:

| Function | Type | Statuses | Transitions |
|---|---|---|---|
| `scrum_workflow()` | `Scrum` | Backlog, To Do, In Progress (WIP 5), In Review (WIP 3), Done | None (open) |
| `kanban_workflow()` | `Kanban` | Queue, In Progress (WIP 3), Review (WIP 2), Done | None (open) |
| `simple_workflow()` | `Simple` | To Do, Doing, Done | None (open) |
| `construction_workflow()` | `Construction` | Permit, Procurement, Build, Inspect, Handover | Explicit linear list |

`workflow_for_type(project_type)` maps each `ProjectType` to the right preset. Adding a new project type means adding a variant to the `ProjectType` enum, a preset function, and a match arm here.

The test suite in this file covers initial status selection, transition validation for open and constrained workflows, WIP limit edge cases, done-status detection, and parent-completion logic — all without any database or async runtime.

---

### `vocabulary.rs`

`VocabularyMap` is a type alias for `HashMap<String, String>`. It maps canonical keys (like `"task"`, `"sprint"`, `"epic"`) to their display labels for a given project.

**`resolve(vocab, key)`** looks up a key in the project's vocabulary, falls back to the default vocabulary, and finally falls back to the key itself. This means partial vocabularies work fine — a construction project only needs to override the terms it cares about.

**`vocabulary_for_type(project_type)`** provides preset vocabularies. The construction preset, for example, maps:

- `"task"` → `"Work Order"`
- `"sprint"` → `"Phase"`
- `"epic"` → `"Building"`
- `"bug"` → `"Defect"`

**`validate(vocab)`** ensures that all keys in a user-supplied vocabulary are from the recognised list (`VOCABULARY_KEYS`). Unknown keys return `Err(CoreError::InvalidVocabularyKey(...))` — this prevents typos from silently creating orphaned entries.

---

### `dependency.rs`

`DependencyGraph` is an adjacency-list representation of item dependencies:

```rust
pub struct DependencyGraph {
    edges: HashMap<Uuid, Vec<(Uuid, DependencyType)>>,   // item → items it blocks
    reverse_edges: HashMap<Uuid, Vec<(Uuid, DependencyType)>>,  // item → items that block it
}
```

`DependencyGraph::from_edges(edges)` builds the graph from a slice of `DependencyEdge` values. The API handler loads all existing dependencies for the involved items, builds the graph, then calls `validate_new_edge(source, target)` before inserting.

**`would_create_cycle(source, target)`** runs a depth-first search starting from `target`, following the `edges` adjacency list. If it ever reaches `source`, adding `source → target` would close a cycle and the function returns `true`. The check is O(V + E) over the existing graph.

`validate_new_edge` wraps the check in a `Result`, also catching the self-reference case (`source == target`).

---

### `error.rs`

`CoreError` is a `thiserror`-derived enum that covers every domain-level failure:

- `ItemNotFound(Uuid)`, `ProjectNotFound(Uuid)`, `SprintNotFound(Uuid)`, `RoleNotFound(Uuid)` — map to HTTP 404.
- `InvalidTransition { from, to }`, `WipLimitExceeded { column, limit, current }`, `DependencyCycle(Uuid)`, `DuplicateDependency { ... }`, `InvalidVocabularyKey(String)`, `EmptyWorkflow`, `HasChildren(Uuid, usize)`, `Validation(String)` — map to HTTP 400.

The mapping from `CoreError` to HTTP status codes lives in `tack-api/src/error.rs`, keeping the core crate free of HTTP knowledge.

---

### `autonomy.rs`

The merge policy as one pure function. `Level` (`L3 < L4 < L5`), `Facts` (the trigger, the level, the verifier's decision, the checks, CI and the team's `ceiling`) go in; `evaluate(&Facts)` returns `Ok` or the first `Hold` that stops the merge. `level_for` picks the level that governs a change from the project's level and its domains. Nothing here reads a clock, a database or GitHub; `tack-api` gathers the facts. See [the ceiling](runs-and-teams.md#the-ceiling).

---

### `run_defaults.rs` and `auto_start.rs`

`run_defaults::resolve` returns the profile a run gets when the caller names none, and where it came from (`Source`): the task's saved settings, its role, the project default, the built-in Implementer. `auto_start::pick` takes the waiting tasks (`Candidate`) and the project's `Limits` and returns the ones to start now and a `Skip` reason for each other. Both are tables of cases, tested without a database. The tick that feeds them is `tack-app/src/auto_start.rs`.

---

### `recurrence.rs` and `rate_card.rs`

`recurrence::next_due` computes when the next copy of a repeating task is due, and `clone_for_next` builds it. `rate_card` prices a model's tokens against the operator's card, returning nothing for a model the card does not list.

---

### `outline.rs` and `outline/bundle.rs`

The project file. `outline.rs` holds the serde types of every section (`Outline`, `OutlineItem`, `Bundle*`) and `resolve`, which checks the items. `outline/bundle.rs` holds `check`, which checks a whole file against a description of the board, and `to_bundle`, which writes a project back as the same file. Both are pure. See [The Project File Format](project-file.md).

---

## `tack-db`

**Lives in:** `crates/tack-db/src/`

**Owns:** SQLite connection pool initialisation, the migration runner, every `sqlx` line in the server, and `StoreError`. Commands are `Repository` methods in `repo/`; read models are functions in `queries/`.

**Does not own:** HTTP concerns, config loading, or business rule enforcement. The repository functions are thin: they translate between Rust structs and SQL rows. It is the only crate allowed to name `sqlx`, and no `sqlx` type crosses its edge: every call returns `StoreError` (`NotFound`, `Conflict` or `Sql`), which `AppError` and `ApiError` both wrap. `Repository::pool()` is public only behind the `raw-pool` feature, which `tack-test-support` alone enables; `scripts/check-crate-graph.sh` and the bans in `deny.toml` keep `sqlx` and the raw pool in those two crates.

---

### `lib.rs`

`init_pool(database_url)` creates a `SqlitePool` using `SqlitePoolOptions` with a max of 5 connections, then immediately runs two `PRAGMA` statements:

- `PRAGMA journal_mode=WAL` — enables Write-Ahead Logging for better concurrent read performance.
- `PRAGMA foreign_keys=ON` — SQLite does not enforce foreign keys by default; this enables them.

---

### `migrations.rs`

Contains every migration (check `GET /api/health`'s `migrations_applied` field for
the current count rather than trusting a hand-written number here) as `const` arrays
of SQL strings. Each entry is `(&str name, &[&str] statements)`. The runner:

1. Creates `_migrations` table if absent.
2. For each migration, checks if the name is already recorded.
3. Executes each SQL statement in order.
4. Records the migration name on success.

Migrations are idempotent — running them on an existing database is safe. Notable migrations:

- `004_items` — creates the `items` table with indexes on `project_id`, `status`, `priority`, `parent_id`, and `sprint_id`.
- `010_fts` — creates the FTS5 virtual table `items_fts` and three triggers (`after_item_insert`, `after_item_update`, `after_item_delete`) that keep the FTS index in sync with the `items` table.
- `012_custom_fields` — `custom_field_definitions` and `custom_field_values` tables.
- `016_perf_indexes` — additional composite indexes added after profiling.
- `039`–`048` — the ten neutral runner-v1 execution-domain tables (execution
  requests, attempts, events, decisions and artifacts; agent profiles; runner fleets
  and their members; model profiles). `049`+ refine execution replay, recovery and
  attempt-start facts.
- `078`–`081` — item briefs, the decision pack columns (options' details and risks,
  the recommendation, when a decision was first seen), merge-readiness pack reviews and
  pull requests.
- `088`–`091` — a role's profile and a profile's fleet, a task's recurrence, a project's
  auto-start, and the roles, profiles and automation a template carries. See
  [Runs, Teams and Automation](runs-and-teams.md#the-migrations).

Each ordinary migration runs in its own transaction with the `_migrations` record
inserted at commit; a failing statement rolls the whole migration back. Applied
migrations are checked at every startup against the binary's own ordered list by name
**and** a deterministic checksum — an edited or reordered history refuses to boot
rather than running silently. A small subset (037/038, a table
copy/verify/swap rebuild predating Part III) additionally takes an automatic
`VACUUM INTO` snapshot before its first attempt. See `docs/MIGRATION-GUIDE.md` for the
operator-facing version of this and `docs/adr/0008-transactional-migration-rebuild-recovery.md`
for the design rationale.

---

### `repo.rs` and `repo/`

`repo.rs` declares the `Repository` struct, which holds a `SqlitePool`, and re-exports a method for each database operation by delegating to the appropriate submodule:

```rust
pub struct Repository {
    pool: SqlitePool,
}

impl Repository {
    pub async fn create_item(&self, ...) -> Result<Item, StoreError> {
        items::create_item(self.pool(), ...).await
    }
    // ...
}
```

This design gives callers a single `repo` value to pass around while keeping each entity's SQL in its own file.

**Per-entity submodules** (`items.rs`, `projects.rs`, `sprints.rs`, `roles.rs`, `comments.rs`, `dependencies.rs`, `attachments.rs`, `boards.rs`, `templates.rs`, `custom_fields.rs`, `briefs.rs`, `pull_requests.rs`, `metrics.rs`, `github_links.rs`, `execution/` (one file per concern: `enqueue`, `claim`, `events`, `decisions`, `artifacts`, `completion`, `recovery`, `review`, `admin`, `state`), `autonomy.rs`, `budgets.rs`, `rate_card.rs`):

`rate_card.rs` holds the one SQL that prices usage: `spend_by_model` returns tokens per model, repository and team, and the Factory page and the dollar budget both call it.

- Functions take `&SqlitePool` (or `&self` for the struct-based submodules) and return `Result<T, StoreError>` or `Result<T, DependencyError>`.
- Queries use `sqlx::query` / `sqlx::query_as` with positional `?` parameters.
- UUIDs are stored as `TEXT` — bound as `.bind(id.to_string())` and parsed back from the row.
- JSON fields (`workflow`, `vocabulary`, `tags`) are serialised to/from strings with `serde_json`.
- Timestamps are stored as RFC 3339 strings and parsed via `chrono::DateTime<Utc>`.

**`queries/`** is the read side: one file per table (`items.rs`, `execution_requests.rs`, `execution_attempts.rs`, `agent_runners.rs`, `pull_requests.rs`, …), one `pub async fn` per question the rest of the program asks of it. Each takes the `Repository`, returns `Result<_, StoreError>` and answers with a row struct whose fields are the columns, under the columns' names. A test reads what a route wrote through these functions instead of writing SQL.

**Notable function: `check_and_update_parent_status`** (in `items.rs`). After an item is moved to a `Done`-category status, the handler calls this function with the item's `parent_id`. It queries whether all sibling items are also in a done status, and if so, updates the parent. The `WorkflowConfig::should_complete_parent(all_siblings_done)` call in `tack-core` provides the decision logic — the repository only handles the data queries.

---

## `tack-orch`

**Lives in:** `crates/tack-orch/src/`

**Owns:** the pure part of the runs domain that is not protocol: the runner scheduler (`scheduler/`) and the model-policy resolver (`model_policy/`), over plain values.

**Does not own:** HTTP, SQL, a clock, or any I/O. It depends only on `tack-protocol` and `tack-core`, so a test of a scheduling or model decision builds its inputs by hand and needs no database. The code that reads the rows and writes the outcome is in `tack-app` (`scheduler_wiring`, `model_policy_wiring`).

---

### `scheduler/`

The deterministic fleet scheduler: given a candidate set of runners (health,
capacity, labels, declared harnesses and whether each needs a model named) and a request
(exact runner or fleet selector, required harness, optional model, priority), it decides
which runner gets the work, or a typed reason none qualify. Two entry points:
`select::select_runner` for one request against a candidate pool, and `batch::schedule`
for several requests sharing one pool, ordered by priority then FIFO fairness. Both
are pure and synchronous — no database, no network client — and neither grants the
authoritative lease; only the repository's fenced claim
(`docs/contracts/runner-v1/`) can do that. The live bridge is not in this crate: `tack-app`'s `scheduler_wiring::choose_request_for_runner` loads real `agent_runners` / `agent_fleet_members` /
`execution_requests` rows through `tack-db` and calls into the pure core above, ahead of the
naive `ORDER BY created_at LIMIT 1` match in the claim query. The only
production caller is the `claim` use case. The same wiring fails a queued request no runner can take after `SchedulingPolicy.unclaimable_fail_after` (30 minutes by default) with the code `no_runner_can_claim`, and answers whether a project's daily budget is spent (`budget_reached`).

### `model_policy/`

Which model a run asks for: the request's own, otherwise the project's default model when
the run uses the project's default agent, otherwise none, which leaves the choice to the
agent's own default. A model is one string in the agent's own spelling; Tack never
checks it against a list, and the agent refuses one it does not know when the run starts.
`tack-app`'s `model_policy_wiring` is the `tack-db`-backed caller that reads the project's default.

---

## `tack-app`

**Lives in:** `crates/tack-app/src/`

**Owns:** what Tack does, written over the store and a clock and nothing else: the runs use cases (`runs/`), every background tick, the live wiring of the scheduler and the model policy, the embedded runner as the rest of Tack sees it (`local_runner/`) and `AppError`. It depends on `tack-protocol`, `tack-core`, `tack-orch` and `tack-db`.

**Does not own:** HTTP types or SQL. It may name `reqwest` (its ticks talk to GitHub and the backup store); it never names `axum` or `sqlx`.

---

### `runs/`

One file per verb: `claim`, `heartbeat`, `start_run`, `complete`, `cancel_run`, `create_decision`, `resolve_decision`, `submit_artifacts`, `report_events` and the rest of the runner-v1 operations. A use case is `pub async fn <verb>(repo: &Repository, clock: &dyn Clock, <args>) -> Result<T, AppError>`: it takes plain values and holds the rule a handler used to hold (idempotency keys, fencing checks, `validate_transition`). A handler in `tack-api` extracts, calls the use case, maps the error and responds. A test of a rule calls the use case directly on `setup_test_db()`, with no router.

### `AppError`

`NotFound`, `Conflict`, `InvalidTransition` and `StaleLease` are raised by a use case where it checks the rule; `Store(StoreError)` and `Core(CoreError)` come from the layers below through `?`. `AppError::code()` is the runner-v1 error code the failure carries on the wire. `tack-api` turns it into an `ApiError`.

### Ticks

Every timer-driven loop (`auto_start`, `github_sync`, `remote_backup`, `due_soon`, `retention`, `observability`) is a module with two functions. `tick()` is one pass, and tests call it with a fake clock so nothing sleeps. `run()` is the loop: wait, `tick()`, log a failed pass, repeat. `tack-cli/src/serve.rs` spawns each `run()`; the HTTP server starts none of them. The autonomy pass (`autonomy::tick`, which merges and demotes by `tack_core::autonomy::evaluate`, with the merge itself in `merge.rs`) runs after each GitHub poll.

### `retention.rs` and `observability.rs`

Two sibling background loops, each a `tick()` plus a `run()`. Retention
sweeps stale terminal-attempt event rows out of `execution_events` on an injectable
clock (so tests never depend on wall time), and `tack serve` spawns it only when the operator turned it on; there is no daily roll-up table for
`execution_events` yet, so this purges rows outright rather than aggregating them,
and says so rather than calling itself a "roll up". Observability computes a periodic,
id-free snapshot of runner/queue/lease/
event counts and logs alerts from it — keyed only by the domain's two small, closed
state vocabularies (`agent_runners.state`, `execution_requests.state`), never by
attempt/request/runner id, so the label set stays bounded regardless of fleet size.

### `host.rs`

`Host` is the one trait through which `tack-app` calls back into things that still live in `tack-api`: `github_token`, `ci_state`, `resolve_secret`, `pull_request_observed`, `merge_pull_request`, `open_pull_request`, `apply_mrp_accepted`, `verify_attestation`, `read_artifact` and `item_moved`. `tack-api` implements it over its `AppState` (`host.rs` there). A method goes away when the thing it reaches moves into this crate. Tests of the ticks use a fake `Host`.

### `scheduler_wiring.rs` and `model_policy_wiring.rs`

The `tack-db`-backed callers of `tack-orch`: they read the rows, call the pure decision and write the outcome.

### `local_runner/`

The embedded runner, as the rest of Tack sees it: the `LocalRunnerControl` seam the routes call, the preference that says whether it should run, the folder checks and the test run the Settings screens ask of it, and the supervisor that keeps one runner process alive. This crate never depends on `tack-runner`; the few things only the binary that links the runner can answer come in through `RunnerHost`.

---

## `tack-api`

**Lives in:** `crates/tack-api/src/`

**Owns:** HTTP server startup, route registration, request/response handling, configuration, WebSocket management, error mapping.

**Does not own:** SQL (that is in `tack-db`, which it names only because `AppState.repo` is the `Repository` and `ApiError` wraps `StoreError`) or the rules of a run (those are `tack-app` use cases and `tack-core` policy). A handler extracts, calls, maps the error and responds. It is the only crate allowed to name `axum`.

This crate is a **library only** — it does not produce its own binary. The single
`tack` binary (in `tack-cli`) calls `tack_api::serve()` to start the server.

---

### `server.rs`

Exposes two functions the composing binary calls in order. `prepare(&mut config)` does what must happen before the database opens: initialises the `tracing` subscriber (plain text or JSON depending on config), runs the security preflight, ensures the API and decision tokens and applies any staged database restore (rename `.restore` file into place).

`serve(state, ready_tx)` takes the finished `AppState`: it starts the execution sweep, builds the router, binds the listener, brings the embedded runner up when its preference says so and answers requests until graceful shutdown on `CTRL+C`. It opens no database and spawns none of the tick loops; that is `tack-cli`'s `serve.rs`.

`tack-cli`'s `serve.rs` calls them when you run `tack` with no
subcommand (or `tack serve`).

---

### `router.rs`

`AppState` is the shared state cloned into every handler:

```rust
pub struct AppState {
    pub repo: Repository,
    pub config: AppConfig,
    pub workspace_id: Uuid,
    pub broadcast_tx: broadcast::Sender<BoardEvent>,
}
```

`build_router(state)` assembles the Axum `Router`. Routes are grouped by entity and nested under `/api`. The file also wires up:

- **CORS** — reads `config.allowed_origins`, constructs a `tower_http::cors::CorsLayer`.
- **Body limit** — `DefaultBodyLimit::max(config.max_body_size_bytes)` globally; the attachment upload route overrides this to 50 MB.
- **Security headers** — `X-Content-Type-Options: nosniff`, `Referrer-Policy: same-origin`, `X-Frame-Options: DENY` via `SetResponseHeaderLayer`.
- **Request tracing** — `TraceLayer` logs every request with method and URI.
- **Token gate** — `middleware::from_fn_with_state(state, require_token)` wraps all `/api` routes.
- **embed-spa feature** — when compiled with `--features embed-spa`, a fallback handler serves the bundled SPA.

---

### `handlers/`

One file per entity group. The agent-work ones: `executions.rs` and `attempt_lists.rs` (requests, and the attempts with their pull request; `executions.rs` also holds `resolve_profile`, `default_selector` and `create_as`, which a run started by the board itself goes through), `decisions.rs`, `briefs.rs` (`/items/{id}/brief`), `mrp.rs` (an attempt's merge-readiness pack, its viewed mark and its review), `metrics.rs` (`/projects/{id}/metrics/factory`), `autonomy.rs` (a project's levels, ceiling and teams), `local_runner.rs` (the embedded runner) and `runner_protocol/` (the runner's own surface). The project file goes through `outline.rs` (import), `export.rs` (export) and `templates.rs`.

The board's own timed work (the auto-start tick, the autonomy tick that merges and demotes, the merge itself, the GitHub poll and the remote backup) is in `tack-app`; a handler that needs one of them calls it. What `tack-app` needs back from this crate is `host.rs`, the implementation of `tack_app::host::Host` over `AppState`.

 A typical handler follows this shape:

```rust
pub async fn update_item(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateItem>,
) -> ApiResult<Json<Item>> {
    input.validate().map_err(|e| ApiError::BadRequest(e.to_string()))?;
    // load, validate, persist, broadcast
}
```

Handlers return `ApiResult<Json<T>>`, which is `Result<Json<T>, ApiError>`. The `ApiError` type implements `IntoResponse`, so Axum converts errors to JSON automatically.

**`websocket.rs`** is somewhat different from other handler files — see the Architecture Overview for a full walkthrough of the connection lifecycle. The key public API it exposes to other handlers is:

```rust
pub fn broadcast_event(state: &AppState, event: BoardEvent) { ... }
```

Any handler that mutates data calls this after a successful write.

---

### `config.rs`

`AppConfig::load()` tries to read `tack.toml` from the current directory. If that fails, it reads environment variables (`TACK_HOST`, `TACK_PORT`, `TACK_DATABASE_URL`, etc.) over a `Default::default()` base. There is no `figment` or other config framework — the logic is a straightforward chain of `if let Ok(v) = std::env::var(...)` assignments.

The API token is never logged. The only place it appears in logs is a boolean "token configured: true/false" in the startup message.

---

### `error.rs`

`ApiError` is the unified error type for all handlers. It implements `IntoResponse` with this mapping:

| `ApiError` variant | HTTP status |
|---|---|
| `NotFound` | 404 |
| `BadRequest` | 400 |
| `Conflict` | 409 |
| `SectionFailed` | 500, with a message that names the project-file section and says whether what came before stayed |
| `Core(CoreError::*NotFound*)` | 404 |
| `Core(CoreError::InvalidTransition \| WipLimitExceeded \| …)` | 400 |
| `Store(StoreError::NotFound)` | 404 |
| `Store(StoreError::Conflict)` | 409 |
| `Store(StoreError::Sql)` | 500 |
| `Internal` | 500 |

The response body is always `{ "error": { "status": <code>, "message": "<text>" } }`.

---

## `tack-runner`

**Lives in:** `crates/tack-runner/src/`

**Owns:** its own binary (`tack-runner`, entirely separate from `tack`) — local
enrollment/credential handling, the isolated per-attempt workspace, the owner-only
attempt journal, the harness adapter layer, and the steps that follow a succeeded attempt (`evidence.rs`, `verify.rs` and the branch push in `git.rs`). **Does not own** anything the API
must not touch: vendor credentials, workspace contents, and the harness subprocess
never leave this crate. See [Installing and Operating Runners](../admin-guide/runners.md)
for the operator-facing view of everything below.

### `config.rs`

`RunnerConfig::from_sources` layers defaults → TOML file → environment
(`TACK_RUNNER_API_URL`, `TACK_RUNNER_ID`, `TACK_RUNNER_STATE_DIR`,
`TACK_RUNNER_ENROLLMENT_TOKEN`, `TACK_RUNNER_MAX_CONCURRENT_ATTEMPTS`) → CLI flags. It also
holds `max_concurrent_attempts` (default 1) and the `[docket]` table (`DocketConfig`:
`binary`, `base_url`, `home`, `port`, `token_file`). `EnrollmentCredential`'s `Debug`/
`Display` are hardcoded to print `[REDACTED]` — redaction here is structural, not a
convention a future `println!` could accidentally bypass.

### `active.rs`

`ActiveAttempts` is the registry of the leases in flight, shared by the attempts that hold them and by the one ticker that renews them all. `register` records a lease and hands back a `watch::Receiver<bool>` that turns `true` when the server asks for a cancellation; `snapshot` and `free` feed the heartbeat; `apply` reads the `lease_results` the server answers. `transport.rs` runs up to `max_concurrent_attempts` claims at once in a `JoinSet`.

### `journal.rs` and `workspace.rs`

A `WorkspaceJournal` record is written to `TACK_RUNNER_STATE_DIR` **before** any
harness process is allowed to spawn — this ordering is what makes crash recovery
possible (see the [Recovery Runbook](../admin-guide/recovery-runbook.md)).
`JournalState` tracks `Prepared → ProcessObservedRunning → ... → Reported`; a restart
finds this file and reports what it actually observed rather than guessing.
`WorktreeProvisioner` is the sole trait allowed to create a workspace's git worktree —
tests inject a fake so unit tests never touch a real checkout.

### `client.rs` — `RunnerProtocolClient`

Defines the trait the runner's runtime loop drives: `enroll`, `refresh`, `claim`,
`heartbeat`, and per-attempt `accept`/`start`/`events`/`decisions`/`artifacts`/
`completion`/`cancellation`/`recovery-observation`. The HTTP-backed implementation is
`transport::HttpPullProtocol`, which `bootstrap::build_runtime` wires for both the
standalone binary and the embedded runner. `UnavailableProtocolClient` remains only as
the typed `RunnerError::ProtocolUnavailable` fallback for a runtime built without a
client — see
[What actually runs today](runner-contract.md#what-actually-runs-today).

### `evidence.rs`, `verify.rs` and the push in `git.rs`

What the engine does between a terminal outcome and deleting the workspace. `evidence.rs`
reads the attempt's change out of the workspace for every harness (`changes.patch`,
`files.json`, `evidence.json`, plus `brief.json` when the request carried one; the shape is
`docs/contracts/evidence-v1/`). `verify.rs` runs the operator's `[verify]` program over that
evidence and stages the merge-readiness pack it writes. `git.rs` also holds the branch push
(`[git] push_branches`). None of the three can change the attempt's outcome: a failure is an
event (`attempt.verify_failed`, `attempt.push_failed`) and the attempt still completes.

### `harness/`

The adapter layer: `process.rs` (bounded output capture, timeouts, process-group
cancellation), `event_sink.rs` (backpressure), `redact.rs`, `artifact.rs`, and
`local_process.rs` — the one lifecycle every local CLI harness shares: locating the
binary, the version probe, the request policy, environment and secrets, spawn, cancel, reconcile, log staging and the outcome. A CLI harness adds a
`HarnessDescriptor` (data) and a `HarnessGrammar` (its command line, how its
output is read, what it supports, and optionally how it drives a conversation so a run can pause and ask): `codex.rs`, `claude_code.rs`, `opencode.rs`. Adding one is a module
plus a line in `harness::DESCRIPTORS` and one in `harness::discover`. The harness
vocabulary itself stays open (`HarnessKind::Other(String)`). Every runner event takes the attempt's next sequence, counted in the journal (`events_submitted`), because the server keeps one event per sequence.

**docket is not a CLI harness.** It is a daemon the runner talks to over HTTP, so `docket.rs` implements the two engine-facing traits directly, with no grammar: `DocketAdapter`, helped by `docket/client.rs` (one method per route), `docket/probe.rs` (API version and teams), `docket/task.rs` (the task body) and `docket/daemon.rs` (supervising `docket start --dispatch`). It is registered by `harness::discover` only when `[docket]` names a binary or a `base_url`. [Runs, Teams and Automation](runs-and-teams.md#the-docket-adapter) has every call.

The two engine-facing traits are
`HarnessAdapter` (per-attempt lifecycle: `validate`/`start`/`cancel`/`wait`/
`reconcile`) and `HarnessProbe` (version/capability discovery). `AdapterRegistry`
implements `HarnessAdapter` by dispatching on harness kind and **refuses to register
any probe claiming `cancel: supported`** unless its harness announces the process groups
its tools start, or stops them over its own API (only docket does) — every other harness's own shell tool
spawns its subprocess in a new session outside the runner's process group, confirmed
against the real binaries with `ps`. Live harness
tests are `#[ignore]`d and never required in CI; `harness/fixtures/fake_harness.sh`,
driven by `TACK_FAKE_HARNESS_MODE`, is the always-runnable path every required test
uses instead. docket is tested against an in-process fake in `tests/fake_docket/`.

---

## `tack-cli`

**Lives in:** `crates/tack-cli/src/`

**Owns:** the single `tack` binary — both starting the server and the command-line client (parsing, human-readable output, HTTP calls to the API).

**Does not own:** any `tack-core` or `tack-db` types in its client commands. They work entirely through the HTTP API — they serialise to JSON for requests and deserialise from `serde_json::Value` for responses (no strongly-typed response structs). This keeps the CLI decoupled from internal model changes that do not affect the API contract. The one exception is `serve.rs`, below.

---

### `serve.rs`

The composition root of `tack serve`, and the only place in this crate that names `tack-db`. It loads the config, calls `tack_api::server::prepare`, opens the database, runs the migrations, builds the `Repository` and the `AppState`, spawns one `tack_app::<module>::run(..)` per background loop and hands the state to `tack_api::serve()`.

---

### `main.rs`

Uses `clap`'s derive API. The top-level `Cli` struct has two global flags (`--api-url`, `--token`) and an **optional** `Commands` enum. Run `tack --help` for the authoritative, current list; as of this writing it is:

- Board basics: `serve`, `init` (with `--template`), `export` (with `--bundle`), `import`, `projects`, `project`, `add`, `list`, `move`, `board`, `branch`, `start`, `open`, `search`, `sprint`, `config`, `completions`
- Backup/restore: `backup`, `backups`, `restore`
- Project setup: `template` (`list`, `show`, `create-from`, `save`), `role`, `comment`, `field`
- Agent onboarding and the runner-fleet surface: `mcp` (MCP server over stdio), `execution` (create/list/cancel/reconcile execution requests), `fleet` (runner fleets), `runner` (enroll/revoke/start/doctor), `service` (run `tack` as a systemd/launchd background service), `agent-profile` (instructions, tool policy, limits, fleet)

Running `tack` with **no subcommand** — or `tack serve` — starts the server + web UI: `run_server()` builds a Tokio runtime and starts the server (`serve.rs`). This is the primary, UI-first entry point. Everything else is the CLI client.

Three cases (`serve`, `config`, and `completions`) are handled before the `TackClient` is constructed, since they do not need a live connection.

All other commands instantiate a `TackClient`, call the appropriate method, and then either print raw JSON (with `--json`) or format a human-readable table. The table formatter (`print_table_row`) pads and truncates columns to fixed widths, which keeps the output readable in standard terminals.

The `add` and `list` commands fetch the project's vocabulary via `vocab::fetch()` and translate `item_type` strings through it before printing — so a construction project shows `Work Order` rather than `task` in the output.

---

### `client.rs`

`TackClient` wraps `reqwest::blocking::Client`. All methods prepend `/api` to the supplied path and attach the `Authorization: Bearer <token>` header when a token is configured.

Public methods:

- `get(path)` → `serde_json::Value`
- `post(path, body)` → `serde_json::Value`
- `patch(path, body)` → `serde_json::Value`
- `delete(path)` → `()`
- `get_bytes(path)` → `Vec<u8>` — used for backup download
- `post_bytes(path, data)` → `serde_json::Value` — used for restore upload

Error handling: the `extract()` helper parses the response body regardless of status code, then returns the body on success or extracts the `error.message` field and surfaces it as an `anyhow::Error` on failure.

---

### `config.rs`

`Config::load(base_url_override, token_override)` applies a precedence chain:

1. CLI flag value (passed as `Option<String>`)
2. Environment variable (`TACK_API_URL`, `TACK_API_TOKEN`)
3. `~/.tackrc` — a TOML file with `base_url` and optional `token` fields
4. Default: `http://127.0.0.1:3210`

`config::save(base_url, token)` writes `~/.tackrc`. This is what `tack config --url <url>` does.

---

## `tack-test-support`

**Lives in:** `crates/tack-test-support/src/`

**Owns:** what the other crates' tests share: `setup_test_db()` (a migrated in-memory `Repository`), the `seed` module (one function per table, which plants the rows a test needs, including a `created_at` or a stored state, below the store's own commands), `ControllableClock` (a `Clock` a test sets and advances), `poll_until` and fixture loading. It depends on `tack-protocol`, `tack-core` and `tack-db`, and is the one crate that enables `tack-db`'s `raw-pool` feature.

A test plants rows with `tack_test_support::seed` and reads them back with `tack_db::queries::`, so a test of a route or a use case never names SQL. Every crate may use it as a dev-dependency; `tack-api`'s `tests/common` keeps its own test-app helpers because they need the router.

---

## `tack-desktop`

**Lives in:** `crates/tack-desktop/src/`

**Owns:** the Tauri shell — a window, a system tray icon, and the supervisor that either attaches to a `tack serve` already answering on the configured port or starts one itself as a bundled sidecar. Built with `make desktop`; excluded from the root workspace (see the crate map in the top-level `CLAUDE.md`) so a contributor without Tauri's system dependencies (GTK, WebKit) still builds every other crate with a plain `cargo build --workspace`. Its own `Cargo.lock`, CI job, and Dependabot entry follow from that same exclusion.

**Does not own:** the server. It never opens the database or reimplements anything `tack-api` already does — only starts, attaches to, and supervises the `tack` binary as a child process, and only ever stops a server it started itself.

---

### `main.rs`

Builds the Tauri app and calls `attach_or_start` before creating the window — the window exists only on `Ok`. Of the ways that call can fail, two render a dialog naming the reason before exiting (a port already held by something that isn't Tack; an attached server older than the bundle), and a catch-all arm covers everything else. The window opens at `1200x800` (`WebviewWindowBuilder::inner_size`).

### `supervisor.rs`

`attach_or_start` probes the configured port for a Tack server already answering at a compatible version and attaches to it instead of spawning a second one; otherwise it spawns the bundled sidecar binary and waits up to a fixed health timeout for `/api/health` to answer. A server this process did not start is never signalled to stop, on any exit path.

### `first_run.rs`

`ensure_settings` runs before the supervisor does: on an empty data root it shows a first-run dialog (database path, port) and writes the answer to `settings.json` before anything tries to reach a server. The dialog call itself runs off the main thread — Tauri's dialog plugin deadlocks the event loop if it doesn't.

### `tray.rs`

Builds the tray icon and its menu: **Open Tack** (show or refocus the window), a disabled **agent execution status line**, a checked **Launch at login** toggle (on by default the first time), and **Quit**. The status line polls `GET /api/local-runner` every three seconds — at the port this app's own `settings.json` names, falling back to the default — and renders one of six typed labels: *on*, *off*, *turning on…*, *turning off…* (the persisted preference and the runtime state disagreeing while a toggle takes effect), *waiting for the server…* (nothing answering yet), and *status unavailable (request failed)*. It is a status line and not a switch: it never writes, and a server that has not answered is never rendered as *off*. Closing the window only hides it — the server keeps running; **Quit** is the action that actually stops it.
