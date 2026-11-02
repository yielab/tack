# Crate Tour

This chapter is for contributors who need to find the file that owns a behaviour. It walks through each Rust crate in the main workspace: what it owns, what it deliberately does not own, the key files, and the patterns worth understanding. The workspace has nine crates: `tack-protocol` (the contract the server and the runner share), `tack-core` (pure domain logic), `tack-orch` (pure runs policy), `tack-db` (the store), `tack-app` (the use cases and the background ticks), `tack-api` (HTTP), `tack-runner`, `tack-cli`, and `tack-test-support`, which holds test helpers. A tenth, `tack-desktop`, sits outside that workspace by design; its own section at the end of this chapter says why. `scripts/check-crate-graph.sh --print` prints which crate may depend on which, `scripts/arch-facts.sh` prints that table with the current counts, and the check fails on any edge outside it.

> The SolidJS web UI in `frontend/` is covered separately in
> [Frontend & Design System](frontend.md) — structure, the design-token system,
> and the `shared/ui` component kit.
>
> This chapter is a per-file walkthrough, not a reference. How an automatic run, a docket
> team and the merge ceiling are built is in [Runs, Teams and Automation](runs-and-teams.md);
> the project file is in [The Project File Format](project-file.md). Crate boundaries and the
> rules that hold across crates are in [`docs/ARCHITECTURE.md`](https://github.com/yielab/tack/blob/develop/docs/ARCHITECTURE.md),
> the authority when the two disagree on a fact. Modules a section does not name are small
> and carry a doc comment on their first lines; `ls` the crate's `src/` for the full list.

---

## `tack-protocol`

**Lives in:** `crates/tack-protocol/src/`

**Owns:** the runner-v1 contract, the one thing the server and `tack-runner` both build on: the execution types (`execution/`), the attempt lifecycle, capability negotiation, usage provenance, the merge-readiness pack (`mrp.rs`: the `mrp-v1` types, their validation and their pull-request rendering), the prepare step (`prepare.rs`) and the one `Clock` trait with `SystemClock` (`clock.rs`). `utoipa` is behind an `openapi` feature.

It also holds `conversation.rs` (what a thread's turn and the harness running it say to each other), `event_kind.rs` (the names of the timeline events a runner reports) and `execution/activity.rs` (what a team is doing, in Tack's own words).

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

Smaller modules: `decision.rs` (who settled a decision), `principal.rs` (who is calling, and their scopes), `thread.rs` (a conversation with a profile), `run_outcome.rs` (whether a run was turned away before the agent did anything), `rich_text.rs` (an item's description as an agent reads it) and `outline/csv.rs`.

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

The file also holds presets for the legal, research and event project types and three construction variants (wood frame, steel frame, SIP panel). `workflow_for_type(project_type)` maps each `ProjectType` to its preset, and `preset_workflow(name)` finds one by name for the project file. Adding a project type is a recipe in [Adding Features](adding-features.md#adding-a-new-workflow-preset). The tests need no database or async runtime.

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

`CoreError` is a `thiserror`-derived enum that covers every domain-level failure: the four not-found cases (`ItemNotFound`, `ProjectNotFound`, `SprintNotFound`, `RoleNotFound`) and the rule violations (`InvalidTransition`, `WipLimitExceeded`, `DependencyCycle`, `DuplicateDependency`, `InvalidVocabularyKey`, `EmptyWorkflow`, `InvalidWorkflow`, `HasChildren`, `Validation`).

Each variant has a `code()`, a `StableErrorCode` from `tack-protocol`, and that code decides the HTTP status. The table is under `tack-api`'s `error.rs` below, so the core crate stays free of HTTP knowledge.

---

### `autonomy.rs`

The merge policy as one pure function. `Level` (`L3 < L4 < L5`), `Facts` (the trigger, the level, the verifier's decision, the checks, CI and the team's `ceiling`) go in; `evaluate(&Facts)` returns `Ok` or the first `Hold` that stops the merge. `level_for` picks the level that governs a change from the project's level and its domains. Nothing here reads a clock, a database or GitHub; `tack-api` gathers the facts. See [the ceiling](runs-and-teams.md#the-ceiling).

---

### `run_defaults.rs` and `auto_start.rs`

`run_defaults::resolve` returns the profile a run gets when the caller names none, and where it came from (`Source`): the task's saved settings, its role, the project default, the built-in Implementer. `auto_start::pick` takes the waiting tasks (`Candidate`) and the project's `Limits` and returns the ones to start now and a `Skip` reason for each other. Both are tables of cases, tested without a database. The tick that feeds them is `tack-app/src/auto_start.rs`.

---

### `change_set.rs`

Change sets, the one way an agent changes a board (ADR 0034). `ChangeSet` is a list of `Operation`s (`create`, `who_does_it`, `priority`, `move`, `depend`, `comment`, `due`), each with its own `OpState` and, once applied, an `OpResult`. An operation names an item by an `ItemRef`: the position of a `create` earlier in the same set (a number in the JSON) or an id the board already has. `validate` walks the operations in the order they would apply (creates, then the rest, then the dependencies the creates ask for) against a `BoardView` the caller read from the store, with the workflow's own `admit_move` for statuses, transitions and WIP limits and the dependency graph for loops, and answers either how each operation resolves or every `Problem` at once. `decide` takes the project's `AgentsOnBoard` rung and the `ceiling_of` the credential's scopes and answers `Apply` or `Wait` per operation; an operation that needs a create that waits waits with it. `status_of` derives a set's status from its operations. All of it is pure.

---

### `digest.rs`

The digest (ADR 0034, decision 6): what a reader should know about some projects before it asks for detail. `digest(&DigestInput)` is pure, bounded and byte-stable. The caller hands in plain rows (`ProjectFacts` per project, `TeamTaskFact`s, and the instant they were read at, since nothing in it reads a clock) and gets a `Digest`: per project the items per status, what waits on a person and since when (`WaitKind`: a run's decision, a run that needs the operator, finished work nobody reviewed), the blocked chains (each open item that blocks others and is not blocked itself, with how many items wait for it and how deep the chain goes), the tasks no team will pick up and why (auto-start's own explanation), the stale items (in progress and unchanged for 14 days), the runs that failed since `since`, the tokens against the day's limit and the change sets with operations still waiting; and, for the whole answer, each team's running work and queue. Every list is a `Section` (`total`, at most 8 `rows`, `more`), at most 20 projects and 20 teams are described, and every text is clipped, so a very large board still answers in well under 512 KiB. Every list is sorted by a total order and the maps are ordered ones, so the same rows give the same bytes; `digest/my-saas.expected.json` holds the answer for the fixture board and a test compares to it byte for byte, also with every input list reversed.

`tack_db::queries::digest` holds the narrow questions behind it (open items, runs waiting, items awaiting review, failed runs since, titles, open team tasks), `tack_app::digest::read` assembles them for the projects it is given, and `GET /api/digest?projects=<ids>&since=<rfc3339>` in `tack-api` chooses the projects from the caller's scopes: an omitted `projects` is every project the caller may read, naming one it may not read is a `403` that names the `read` scope, and the tasks teams hold that the board did not start are shown only to a caller that may read every project and did not narrow the list. `since` defaults to the last 24 hours.

---

### `trigger.rs`

What wakes a profile (ADR 0034, decision 8), as pure values and rules. A `Trigger` is a `Source` (an `EventKind` with a `Filter`, or a `Schedule` of ISO weekdays and a minute of the day in a stated zone), a profile, a quiet period, a daily cap and a `ThreadScope` (the home project, and optionally the credential whose projects count). `fires(trigger, happened, own, reaches)` says whether a `Happened` belongs in the trigger's next run; `own` is `profile_labels(...)`, the principal labels the profile works under (the credentials its triggers name, its triggers' `trigger:<id>` and its threads' `thread:<id>`), and `is_echo` makes an event a principal of that set caused never fire it. `burst_ready` waits for quiet but not for more than an hour, `cap_reached` counts the last 24 hours, `schedule_due` compares the latest scheduled moment with the last run (or the trigger's creation), and `run_input` writes what the run is told: why it started, up to 50 events and the digest.

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

**Does not own:** HTTP concerns, config loading, or business rule enforcement. The repository functions are thin: they translate between Rust structs and SQL rows. It is the only crate allowed to name `sqlx`, and no `sqlx` type crosses its edge: every call returns `StoreError` (`NotFound { entity }`, `Conflict`, `Corrupt { table, column }` or `Sql`), which `AppError` and `ApiError` both wrap. `Repository::pool()` is public only behind the `raw-pool` feature, which `tack-test-support` alone enables; `scripts/check-crate-graph.sh` and the bans in `deny.toml` keep `sqlx` and the raw pool in those two crates.

---

### `lib.rs`

`init_pool(database_url)` creates a `SqlitePool` with a maximum of 5 connections. Foreign keys are switched on in the connect options, because SQLite enforces them per connection and a one-off `PRAGMA` would arm only the first. It then sets `PRAGMA journal_mode=WAL`, which is stored in the database file and so applies to every connection.

---

### `migrations.rs`

Holds every migration in one ordered list (`all_migrations()`); `GET /api/health`'s `migrations_applied` gives the current count. An entry is `(name, statements)`, the statements being `const` arrays of SQL strings. At startup the runner:

1. Creates the `_migrations` table if absent.
2. Checks every applied migration against the binary's own list by name **and** a deterministic checksum. An edited or reordered history refuses to boot instead of running silently.
3. Takes a `VACUUM INTO` snapshot before any pending migration that cannot be re-run safely: a table rebuild (`037_orch_runs_rebuild`, `038_orch_approvals_rebuild`) or one that runs `DROP TABLE`.
4. Applies each pending migration in order. An ordinary migration runs all its statements and records its name in **one transaction**, so a failing statement rolls the whole migration back. A rebuild also proves the copy (row counts and projections) and runs `foreign_key_check` before it commits.

Migrations are append-only: never edit a shipped one. For example, `004_items` creates the `items` table and its indexes, `010_fts` creates the FTS5 table `items_fts` with the triggers that keep it in step, and `012_custom_fields` adds the custom-field tables. The operator-facing version is `docs/MIGRATION-GUIDE.md`; the design is `docs/adr/0001-transactional-migration-rebuild-recovery.md`. How to add one is in [Adding Features](adding-features.md#adding-a-migration).

---

### `repo.rs` and `repo/`

`repo.rs` declares the `Repository` struct, which holds a `SqlitePool`, and each submodule of `repo/` adds an `impl Repository` block with the commands of one aggregate (`items`, `projects`, `sprints`, `roles`, `comments`, `change_sets`, `threads`, `triggers`, `agent_credentials`, `templates/`, …). `repo/execution/` has one file per concern (`enqueue`, `claim`, `events`, `decisions`, `artifacts`, `completion`, `recovery`, `review`, `admin`, `state`). A caller holds one `Repository` and calls `repo.create_item(..)`; each entity's SQL stays in its own file. `canonical.rs` is the one canonical form of a JSON value that snapshots and replay fingerprints are compared in.

`rate_card.rs` holds the one SQL that prices usage: `spend_by_model` returns tokens per model, repository and team, and the Factory page and the token budget both call it.

- Methods return `Result<T, StoreError>` (or `Result<T, DependencyError>` for the dependency graph).
- Queries use `sqlx::query` / `sqlx::query_as` with positional `?` parameters.
- A value read back from a row is decoded through `decode.rs` (`decode::uuid`, `json`, `instant`, `variant`), which returns `StoreError::Corrupt` rather than panicking or defaulting.
- UUIDs are stored as `TEXT` and bound as `.bind(id.to_string())`.
- JSON fields (`workflow`, `vocabulary`, `tags`) are serialised to and from strings with `serde_json`.
- Timestamps are stored as RFC 3339 strings and parsed via `chrono::DateTime<Utc>`.

**`queries/`** is the read side: one file per table or subject (`items.rs`, `execution_requests.rs`, `execution_attempts.rs`, `agent_runners.rs`, `team_activity.rs`, `digest.rs`, …), one `pub async fn` per question the rest of the program asks of it. Each takes the `Repository`, returns `Result<_, StoreError>` and answers with a row struct whose fields are the columns, under the columns' names. A test reads what a route wrote through these functions instead of writing SQL.

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

One file per verb: `claim`, `heartbeat`, `start_run`, `complete`, `cancel_run`, `create_decision`, `resolve_decision`, `submit_artifacts`, `report_events` and the rest of the runner-v1 operations. A use case is `pub async fn <verb>(repo: &Repository, now: DateTime<Utc>, <args>) -> Result<T, AppError>`: the caller reads the clock once at the edge, and the use case takes plain values and holds the rule of the run (idempotency keys, fencing checks, `validate_transition`). A handler in `tack-api` extracts, calls the use case, maps the error and responds. A test of a rule calls the use case directly on `setup_test_db()`, with no router.

### `change_sets.rs`

`propose` reads the board into a `BoardView`, validates the whole set (a set that fails is refused with every reason and kept nowhere), keeps it under its idempotency key (the same key on a project answers the first change set again), lets `decide` pick the operations that apply now and applies them with `Repository::apply_change_set`: one `BEGIN IMMEDIATE` transaction that re-reads the set, writes every operation through the same row-level functions the person's routes use (`insert_item`, `update_item_in` with the workflow as gate, `insert_dependency`) and records each operation's state and result, so a failure anywhere leaves nothing. `apply` (a person, all or some operations, with edits), `reject` and `edit_operation` finish the lifecycle. `Mode::Now` is a person's own plan: everything applies at once and the rung does not count it. A plan accepted from a run (`POST /api/items/{id}/subtasks-from-plan`) is a set of creates proposed in that mode, so the plan flow and an agent's proposal share one write path. The handlers in `tack-api` announce what changed to the WebSocket and run `after_update` for a move, as `update_item` does.

### `threads.rs`

Opens a thread (a conversation between a person and a profile, ADR 0034 decision 7), writes in it and starts the turn each message causes: a run of the thread's profile against the thread's hidden item, under the principal `thread:<id>`, whose input is the window of messages and the digest. The rules that bound a thread are `tack_core::thread`'s; the routes are `/api/threads` in `tack-api`.

### `triggers.rs`

`observe` turns a `DomainEvent` into a `Happened` (a run or decision of a thread's or trigger's hidden item is the profile's own work and is skipped) and keeps it, per trigger, in `trigger_events`. The loop (`spawn`) drains a subscription to the event bus (`DomainEvents::subscribe`) and calls `tick` every five seconds: `tick` notices each change set that has waited longer than a `change_set_waiting` trigger's `after_seconds` once (a dedupe key, cause `system:tack`), then for every enabled trigger whose last run has ended and whose burst is quiet or whose schedule has come, and whose cap is not reached, starts one run against the trigger's hidden item under the principal `trigger:<id>` and marks the events it took. A run that cannot start is logged and counts toward the cap. The HTTP routes (`/api/triggers`) are the operator's alone.

### `AppError`

`NotFound`, `Conflict`, `InvalidTransition`, `StaleLease` and `Refused(Refusal)` (a run that was not started, with its code and structured details) are raised by a use case where it checks the rule; `Store(StoreError)` and `Core(CoreError)` come from the layers below through `?`. `AppError::code()` is the `StableErrorCode` the failure carries on the wire, and `StableErrorCode::status()` is the HTTP status of every answer with that code: one table in `tack-protocol`. `tack-api` turns it into an `ApiError` and keeps the code.

### Ticks

Every timer-driven loop (`auto_start`, `github_sync`, `remote_backup`, `due_soon`, `retention`, `observability`, `triggers`) is a module with `tick()`, one pass that tests call with a fake clock so nothing sleeps, and `spawn()`, which hands that pass to `background::Loops`. `tack-cli/src/serve.rs` owns the `Loops` and stops them all when the server returns. The rules for a loop (one owner per table, shared cancellation, a use case started after answering goes on `AppState::tasks`) are in [`docs/ARCHITECTURE.md`](https://github.com/yielab/tack/blob/develop/docs/ARCHITECTURE.md). `loop_config.rs` holds the loops' settings, which `tack-api`'s `AppConfig` flattens into `tack.toml`. The autonomy pass (`autonomy::tick`, with `merge.rs`) runs after each GitHub poll; `pull_request.rs` opens the pull request an accepted attempt asks for.

### `retention.rs` and `observability.rs`

Two sibling background loops, each a `tick()` plus a `spawn()`. Retention purges stale terminal-attempt event rows from `execution_events` outright (there is no roll-up table) on an injectable clock, so tests never depend on wall time; `tack serve` spawns it only when the operator turned it on. Observability computes a periodic, id-free snapshot of runner, queue, lease and event counts and logs alerts from it. It is keyed only by the domain's two small, closed state vocabularies (`agent_runners.state`, `execution_requests.state`), never by attempt, request or runner id, so the label set stays bounded regardless of fleet size.

### `ports.rs`

The small traits through which `tack-app` calls back into things that still live in `tack-api`: `Verifier`, `Artifacts` and `DomainEvents`. `Ports` bundles them with `github_sync::GitHub` and the embedded runner. `tack-api`'s `ports.rs` builds a `Ports` over its `AppState`, and a `tack-app` test builds one from fakes. See [`docs/ARCHITECTURE.md`](https://github.com/yielab/tack/blob/develop/docs/ARCHITECTURE.md) for why a port is a trait only where its implementor names an HTTP type.

### `events.rs`

What happened on a board, as values: a `DomainEvent` is a `Fact` (an item created, updated, moved or deleted; a run started, finished, failed or waiting on a person; a decision opened or resolved), the project it happened in, and the `Principal` that caused it (a runner and Tack itself are principals too). The place where a fact becomes true emits it once: the item routes and the change-set announcement in `tack-api`, and `runs/announce.rs` for what a runner reports, which the handler calls with the outcome of the use case so that a replayed report, an already-asked question or an already-given answer announces nothing. `EventBus` is the in-process implementation every consumer subscribes to, and `RecordedEvents` is the one a test reads. `tack-api`'s `ports.rs` emits each event to the bus and, for the item events, to the WebSocket as the message the open boards understand; a run or a decision has no such message and is not sent to the browser.

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
    pub broadcast_tx: broadcast::Sender<BoardEvent>,          // the browser's share of the events
    pub event_bus: tack_app::events::EventBus,                // every consumer of a domain event
    pub webhook: Option<WebhookClient>,                       // None when TACK_WEBHOOK_URL is unset
    pub local_runner: Option<Arc<dyn LocalRunnerControl>>,    // the embedded runner's seam
    pub tasks: TaskTracker,                                   // work started after answering
}
```

`build_router(state)` assembles the Axum `Router`. Routes are grouped by entity and nested under `/api`. The file also wires up:

- **CORS** — reads `config.allowed_origins`, constructs a `tower_http::cors::CorsLayer`.
- **Body limit** — `DefaultBodyLimit::max(config.max_body_size_bytes)` globally; the attachment upload route overrides this to 50 MB.
- **Security headers** — `X-Content-Type-Options: nosniff`, `Referrer-Policy: same-origin`, `X-Frame-Options: DENY` and a Content-Security-Policy built from the config, via `SetResponseHeaderLayer`.
- **Request tracing** — `TraceLayer` logs every request with method and URI.
- **Token gate** — `middleware::from_fn_with_state(state, require_token)` wraps all `/api` routes.
- **embed-spa feature** — when compiled with `--features embed-spa`, a fallback handler serves the bundled SPA.

---

### `handlers/`

One file per entity group. The agent-work ones: `executions.rs` and `attempt_lists.rs` (requests, and the attempts with their pull request; `executions.rs` also holds `create_as`, which a run started by the board itself goes through, and a `resolve_profile` that calls the one in `tack-app`'s `runs/start_run.rs`, beside `default_selector`), `decisions.rs`, `briefs.rs` (`/items/{id}/brief`), `mrp.rs` (an attempt's merge-readiness pack, its viewed mark and its review), `metrics.rs` (`/projects/{id}/metrics/factory`), `autonomy.rs` (a project's levels, ceiling and teams), `teams.rs` (the teams the agents report), `local_runner.rs` (the embedded runner) and `runner_protocol/` (the runner's own surface). The manager's surface (ADR 0034) is `agent_credentials.rs`, `change_sets.rs`, `digest.rs`, `threads.rs` and `triggers.rs`, and `agent_access.rs` is the closed table of routes an agent credential may call. The project file goes through `outline.rs` (import), `export.rs` (export) and `templates.rs`.

The board's own timed work (the auto-start tick, the autonomy tick that merges and demotes, the merge itself, the GitHub poll and the remote backup) is in `tack-app`; a handler that needs one of them calls it. What `tack-app` needs back from this crate is `ports.rs`, the implementation of `tack_app::ports` over `AppState`.

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

**`websocket.rs`** is somewhat different from other handler files — see the Architecture Overview for a full walkthrough of the connection lifecycle. The key public API it exposes is:

```rust
pub fn broadcast_event(state: &AppState, event: BoardEvent) { ... }
```

A handler that mutates an item does not call it: it emits a `DomainEvent` through `state.events()`, and the adapter in `ports.rs` maps the item events onto `BoardEvent`. `broadcast_event` is called directly only for a refresh nudge that is not a domain fact.

---

### `config.rs`

`AppConfig::load()` reads `tack.toml` from the current directory, or starts from `Default::default()` when the file is missing or does not parse, and then lets environment variables (`TACK_HOST`, `TACK_PORT`, `TACK_DATABASE_URL`, etc.) override each field. There is no `figment` or other config framework — the logic is a chain of `if let Ok(v) = std::env::var(...)` assignments. Every variable is in [`docs/CONFIG.md`](https://github.com/yielab/tack/blob/develop/docs/CONFIG.md).

The API token is never logged. The only place it appears in logs is a boolean "token configured: true/false" in the startup message.

---

### `error.rs`

`ApiError` is the unified error type for all handlers. Every variant has a `code()`, a `StableErrorCode`, and the code decides the status (`StableErrorCode::status()`):

| `ApiError` variant | Code | HTTP status |
|---|---|---|
| `NotFound` | `not_found` | 404 |
| `BadRequest` | `invalid_request` | 400 |
| `Unprocessable` | `unprocessable` | 422 |
| `Forbidden` | `forbidden` | 403 |
| `Conflict` | `conflict` | 409 |
| `MisdirectedHost`, `CrossSiteRequest`, `UnsupportedMediaType` | `misdirected_host`, `cross_site_request`, `unsupported_media_type` | 421, 403, 415 |
| `Coded { code: method_not_allowed }` | `method_not_allowed` (a path that exists for another method) | 405 |
| `UnreadableArchive` | `unreadable_archive` (a restore whose archive or manifest cannot be read) | 422 |
| `SectionFailed` | `section_failed`, with a message that names the project-file section and says whether what came before stayed | 500 |
| `Coded { code, message, field }` | the one a use case raised (`invalid_transition`, `stale_lease`, a refused run's code, `precondition_failed`) | the code's |
| `Core(CoreError::*NotFound*)` | `not_found` | 404 |
| `Core(CoreError::…)` | one code per rule: `invalid_status_transition`, `wip_limit_exceeded`, `dependency_cycle`, `duplicate_dependency`, `invalid_vocabulary_key`, `empty_workflow`, `has_children`, `invalid_workflow`, `validation_failed` (`CoreError::code()`) | 400 |
| `Store(StoreError::NotFound { entity })` | `not_found`, "`<entity>` not found" | 404 |
| `Store(StoreError::Conflict)` | `conflict` | 409 |
| `Store(StoreError::Corrupt { table, column })` | `internal_error`; logged with the table, the column and why the decode failed (never the value); the client sees "Internal server error" | 500 |
| `Store(StoreError::Sql)`, `Internal` | `internal_error` | 500 |

The response body is always `{ "error": { "status", "code", "message", "retryable", "request_id", "details" } }`, on the `/api` routes and the runner-v1 routes alike (`details.field` names the field of the request the error blames, when one); the OpenAPI schema documents `code` as the `StableErrorCode` enum. A client matches `code`; `message` is plain words for people.

A request an extractor cannot read gets that body too. Handlers take `tack_api::extract::{Json, Path, Query, RawBody, Multipart, Upgrade}` instead of axum's, which refuse with `ApiError` (`invalid_request`, `unsupported_media_type`, `payload_too_large`), and the router answers an unmatched path with `not_found` and a wrong method with `method_not_allowed`, so no answer of the API is plain text. Every request carries an id (`request_id.rs`): the `x-request-id` it sent when that is short and safe, one the board makes otherwise.

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

**docket is not a CLI harness.** It is a daemon the runner talks to over HTTP, so `docket.rs` implements the two engine-facing traits directly, with no grammar: `DocketAdapter`, helped by `docket/client.rs` (one method per route), `docket/probe.rs` (API version and teams), `docket/task.rs` (the task body), `docket/steps.rs` and `docket/records.rs` (what each hop was made of, and the rows a team wrote) and `docket/daemon.rs` (supervising `docket start --dispatch`). It is registered by `harness::discover` only when `[docket]` names a binary or a `base_url`. [Runs, Teams and Automation](runs-and-teams.md#the-docket-adapter) has every call.

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

The composition root of `tack serve`, and the only place in this crate that names `tack-db`. It loads the config, calls `tack_api::server::prepare`, opens the database, runs the migrations, builds the `Repository` and the `AppState`, starts one `tack_app::<module>::spawn(..)` per background loop into one `background::Loops`, hands the state to `tack_api::serve()` and stops the loops when it returns.

---

### `main.rs`

Uses `clap`'s derive API. The top-level `Cli` struct has two global flags (`--api-url`, `--token`) and an **optional** `Commands` enum; the command bodies are in `cmd/`. Run `tack --help` for the authoritative list. The groups are:

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

The methods are one per verb and body kind: `get`, `post`, `patch`, `delete`, `put_json`, `put_empty` and `delete_json` answer a `serde_json::Value` (`delete` answers `()`); `get_bytes` and `post_bytes` carry the backup download and the restore upload; `post_text` sends a raw text body; and `get_with_etag` and `patch_if_match` read an item's `ETag` and send it back as `If-Match`, which is how `update_item` and `move_item` avoid overwriting a concurrent change.

Error handling: the `extract()` helper parses the response body regardless of status code, then returns the body on success or extracts the `error.message` field and surfaces it as an `anyhow::Error` on failure.

---

### `config.rs`

`Config::load(base_url_override, token_override)` applies a precedence chain (`load_with` does the same over an environment the caller already read, in a `ClientEnvironment`):

1. CLI flag value (passed as `Option<String>`)
2. Environment variable (`TACK_API_URL`, `TACK_API_TOKEN`)
3. `~/.tackrc` — a TOML file with `base_url` and optional `token` fields
4. For the token, the `api-token` file of a server running on this machine
5. Default URL: `http://127.0.0.1:3210`

`config::save(base_url, token)` writes `~/.tackrc`. This is what `tack config --url <url>` does.

---

## `tack-test-support`

**Lives in:** `crates/tack-test-support/src/`

**Owns:** what the other crates' tests share: `setup_test_db()` (a migrated in-memory `Repository`), the `seed` module (one function per table, which plants the rows a test needs, including a `created_at` or a stored state, below the store's own commands), `ControllableClock` (a `Clock` a test sets and advances), `poll_until` and fixture loading. It depends on `tack-protocol`, `tack-core` and `tack-db`, and is the one crate that enables `tack-db`'s `raw-pool` feature.

A test plants rows with `tack_test_support::seed` and reads them back with `tack_db::queries::`, so a test of a route or a use case never names SQL. Every crate may use it as a dev-dependency; `tack-api`'s `tests/common` keeps its own test-app helpers because they need the router.

---

## `tack-desktop`

**Lives in:** `crates/tack-desktop/src/`

**Owns:** the Tauri shell — a window, a system tray icon, and the supervisor that either attaches to a `tack serve` already answering on the configured port or starts one itself as a bundled sidecar. Built with `make desktop`; excluded from the root workspace (the `exclude` comment in the root `Cargo.toml` says why) so a contributor without Tauri's system dependencies (GTK, WebKit) still builds every other crate with a plain `cargo build --workspace`. Its own `Cargo.lock`, CI job, and Dependabot entry follow from that same exclusion.

**Does not own:** the server. It never opens the database or reimplements anything `tack-api` already does — only starts, attaches to, and supervises the `tack` binary as a child process, and only ever stops a server it started itself.

---

### `main.rs`

Builds the Tauri app and calls `attach_or_start` before creating the window — the window exists only on `Ok`. Of the ways that call can fail, two render a dialog naming the reason before exiting (a port already held by something that isn't Tack; an attached server older than the bundle), and a catch-all arm covers everything else. The window opens at `1200x800` (`WebviewWindowBuilder::inner_size`).

### `supervisor.rs`

`attach_or_start` probes the configured port for a Tack server already answering at a compatible version and attaches to it instead of spawning a second one; otherwise it spawns the bundled sidecar binary and waits up to a fixed health timeout for `/api/health` to answer. A server this process did not start is never signalled to stop, on any exit path.

### `paths.rs` and `lifecycle.rs`

`paths.rs` computes the per-user data root (`dirs::data_dir()` + `tack`) and the folders under it; only the database path can move, through `settings.json`. `lifecycle.rs` makes closing the window hide it, *Open Tack* show it again, and *Quit* the only way to stop the app, warning first when an agent attempt is in flight.

### `first_run.rs`

`ensure_settings` runs before the supervisor does: on an empty data root it shows a first-run dialog (database path, port) and writes the answer to `settings.json` before anything tries to reach a server. The dialog call itself runs off the main thread — Tauri's dialog plugin deadlocks the event loop if it doesn't.

### `tray.rs`

Builds the tray icon and its menu: **Open Tack** (show or refocus the window), a disabled **agent execution status line**, a **Launch at login** toggle (unchecked until you turn it on), and **Quit**. The status line polls `GET /api/local-runner` every three seconds — at the port this app's own `settings.json` names, falling back to the default — and renders one of six typed labels: *on*, *off*, *turning on…*, *turning off…* (the persisted preference and the runtime state disagreeing while a toggle takes effect), *waiting for the server…* (nothing answering yet), and *status unavailable (request failed)*. It is a status line and not a switch: it never writes, and a server that has not answered is never rendered as *off*. Closing the window only hides it — the server keeps running; **Quit** is the action that actually stops it.
