# Architecture & Implementation Notes

Crate responsibilities, design patterns, and implementation details of record.
This file is the authority for design patterns, implementation notes, and
troubleshooting — CLAUDE.md keeps only the condensed map. It is for contributors; the
book's [Developer Guide](book/src/developer/index.md) is the narrative version, and its
[Runs, Teams and Automation](book/src/developer/runs-and-teams.md) and
[Project File Format](book/src/developer/project-file.md) pages cover the two newest
subsystems in depth. The mdBook's
[Crate Tour](book/src/developer/crate-tour.md) is a separate, deeper per-file
walkthrough (code snippets, one section per source file) rather than a copy of
this page; when the two disagree on a fact rather than depth, this file wins
and the tour should be corrected to match.

**Project structure:**
```
crates/
├── tack-protocol/ The runner-v1 contract the server and the runner share: execution types,
│                  lifecycle, capabilities, usage provenance, the merge-readiness pack
│                  (mrp), the prepare step, one Clock. No I/O
├── tack-core/     Pure business logic (no I/O): planning and runs policy
├── tack-orch/     The pure runs domain that is not protocol: the runner scheduler and
│                  the model-policy resolver, over plain values
├── tack-db/       SQLite persistence: the only crate that names sqlx. repo/ (commands),
│                  queries/ (read models), StoreError, migrations
├── tack-app/      The application layer: the runs use cases (runs/), every background
│                  tick, the scheduler's and model policy's live wiring, the embedded
│                  runner's module, AppError
├── tack-api/      Axum HTTP server + WebSocket (library; pub fn serve) — the only crate
│                  that names axum
├── tack-runner/   Pull-based execution runner — its own binary; owns local
│                  credentials, workspace, journal, the harness subprocess and the
│                  connection to the docket daemon
├── tack-cli/      The single `tack` binary — runs the server (tack serve, the
│                  composition root) and the CLI client
└── tack-test-support/  Test helpers shared by the other crates' tests: setup_test_db(),
                   seed, ControllableClock, fixtures

crates/tack-desktop/   Tauri shell that supervises `tack` as a bundled sidecar; excluded
                       from this workspace (own Cargo.toml/lockfile) so Tauri's GTK/WebKit
                       dependency never reaches a plain `cargo build --workspace`

frontend/
├── src/
│   ├── app/         Root App/Layout components and routes.tsx
│   ├── features/    One directory per feature (board, list, sprints, fleet,
│   │                agents, provisioning, economics, settings, …)
│   ├── shared/      Cross-feature code: api/ (generated client + schema),
│   │                realtime/, orch/, execution/, ui/, state/, vocab/, keyboard/
│   └── test/        Vitest setup
├── e2e/             Playwright specs
├── public/          Static assets
└── package.json     Frontend dependencies (SolidJS, Vite, Tailwind v4)

docs/                Documentation
├── API-REFERENCE.md Complete API documentation
├── TESTING.md       Testing guide
└── *.md            Various guides
```

## Crate Boundaries & Responsibilities

The edges between crates are the table `scripts/check-crate-graph.sh --print` prints, and the check
fails on any normal dependency it does not list. `scripts/arch-facts.sh` prints that table with the
current crate, route, migration and test counts; quote those counts from it, never from this page.

**tack-protocol** (the contract; depends on nothing in the workspace):
- The neutral runner-v1 types (`execution/`: request, attempt, event, artifact and decision types), the attempt lifecycle (`lifecycle::validate_transition` is the one table; `lifecycle::sources`, `lease_held` and `terminal` read the states a change may start from off it, and `tack-db`'s `Move` builds every `UPDATE` guard from those, so no query spells a state) and capability negotiation, both the server's handlers and `tack-runner` build on
- `execution/messages.rs`: every runner request and response (enrollment, refresh, claim, heartbeat, accept and start, events, decisions, artifact manifest, cancellation observation and completion; the recovery observation is in `types.rs`); the handlers in `tack-api` and the transport in `tack-runner` both serialize and deserialize these structs, and `tests/runner_contract/protocol.rs` round-trips every matching fixture through them. A runner request carries an `x-request-id` the runner makes per request; `runner_auth::with_request_id` scopes it around the handler so every error envelope names it
- `usage_provenance.rs`: compares an execution request's resolved model against what the attempt actually ran on, and keeps runner-observed wall-clock cost structurally separate from harness/vendor-reported token and dollar usage
- `mrp.rs`: the `mrp-v1` merge-readiness pack, its types, validation and pull-request rendering, as plain data; `prepare.rs`: the prepare step
- The one `Clock` trait and `SystemClock`; `utoipa` is behind an `openapi` feature
- Every dollar-valued field across the runs crates is named `*_usd_estimated` — token counts are the primary, trustworthy measure; absent usage is a typed "not measured", never a fabricated `0`

**tack-core** (pure, zero I/O; depends on `tack-protocol`):
- Domain models: `Project`, `Item`, `Sprint`, `Role`, `Comment`, `Dependency`, etc.
- Workflow engine: validates transitions, enforces WIP limits, provides presets
- Vocabulary system: customizable term mapping per project
- Dependency graph: DAG with cycle detection (DFS-based)
- Error types: typed domain errors (`CoreError`)
- Item briefs (`brief.rs`): types, validation and Markdown rendering, as plain data
- The merge policy (`autonomy.rs`): `Level`, `Facts`, `Hold` and the pure `evaluate`, including the team's ceiling (`Hold::HarnessCeiling`)
- Which profile a run gets (`run_defaults.rs`, `resolve`) and which waiting tasks a project starts (`auto_start.rs`, `pick`), both pure
- Recurrence (`recurrence.rs`: when the next copy of a repeating task is due) and the rate card (`rate_card.rs`)
- The project file (`outline.rs`, `outline/bundle.rs`): the types of every section, `resolve` for items, `check` for a whole file against a board and `to_bundle` for the export

**tack-orch** (pure; depends on `tack-protocol` and `tack-core`; no database, no clock, no I/O):
- `scheduler/`: a decision library over plain values — given a candidate pool of runners (health/capacity/labels/declared harness support) and a request (exact runner or fleet selector, required harness, optional model, priority), it returns a selected runner or a typed reason none qualify; it never grants the authoritative lease, only the repository's fenced claim does that. `select::select_runner` decides one request; `batch::schedule` orders several by priority then FIFO fairness. It matches the agent, not the model: a model string is the agent's own and is never compared with a list, and a request that names no model reaches an agent that requires one only to be told "<agent> needs a model named"
- `model_policy/`: a run's model — the request's own, otherwise the project's default model when the run uses the project's default agent, otherwise none (the agent chooses)
- The live callers that read rows and write the outcome are `tack-app`'s `scheduler_wiring` and `model_policy_wiring`

**tack-db** (the only crate that names `sqlx`; depends on `tack-protocol` and `tack-core`):
- SQLite via `sqlx` (async), behind `StoreError` (`NotFound { entity }`, `Conflict`, `Corrupt { table, column }`, `Sql`): no `sqlx` type crosses the crate's edge. A stored value that cannot be decoded is `Corrupt`, never a panic and never a default (`decode.rs` is the one place that reads ids, instants, enums and JSON back)
- Migrations (`migrations.rs`; `scripts/arch-facts.sh` prints how many, and the live count is `GET /api/health`'s `migrations_applied`) with FTS5 full-text search on items
- **Commands are `Repository` methods**, one directory or file per aggregate under `repo/` (`items.rs`, `projects.rs`, `execution/`, …). **Read models are functions in `queries/`**, one file per table, one `pub async fn` per question the rest of the program asks of it, each taking the `Repository` and returning a row struct under the columns' names
- An item is two types. `Item` is the aggregate: the columns of the `items` row, listed once (`ITEM_COLUMNS` in `repo/items.rs`) and read back through `ItemRow`. `ItemCard` (`tack-core`) is the `Item` plus what the run history says about it (`needs_review`, `merge_note`, `started_by`, `team`, `merged_by`), derived by subqueries on read and never written; `list_items`, `get_item_snapshot` and an update's answer return cards, every other read returns the `Item`. An item update is `update_item_atomically`, the only one: the store counts the target status inside its `BEGIN IMMEDIATE` transaction and asks the workflow (`tack-core`'s `StatusGate`) whether the move is allowed, so the workflow and WIP rules live in `tack-core` and the store holds none
- A state read from a row is converted to its typed enum at this crate's edge
- The runner-protocol half of the execution store (`repo/execution/`: claim, heartbeat, transitions, events, artifacts, decisions, completion, recovery, cancellation) takes and returns `tack-protocol`'s `RunnerId`, `AttemptId`, `FencingToken`, `Checkpoint`, `CompletionId`, `CancellationRequestId`, `RecoveryKey`, `WorkspaceId` and `ExecutionRequestId`, never a bare `&str` or `i64`; and the JSON it stores is the protocol's too: a completion's `ActualExecution` and `Usage`, an event's payload, a decision's `DecisionOption`s and `DecisionRecommendation`, a recovery's `RecoveryDetails` and a heartbeat's `RecoveryJournalState` cross the boundary as those values and are encoded inside the crate (a completion's `terminal_reason` stays one JSON object, because each harness adapter fills different keys). The conversion to a column happens inside the crate. The operator and read-model methods (runner administration, `AttemptListingRow`, MRP reviews) still take plain text
- `Repository::pool()` exists publicly only behind the `raw-pool` feature, which only `tack-test-support` enables; `scripts/check-crate-graph.sh` fails on any `.pool()` or `sqlx` use outside `tack-db` and `tack-test-support`, and the bans `scripts/gen-deny-toml.sh` writes into `deny.toml` name the same two crates
- Auto-runs migrations on startup; the database is created automatically if missing

**tack-app** (the application layer; depends on `tack-protocol`, `tack-core`, `tack-orch` and `tack-db`; names `reqwest`, never `axum` or `sqlx`):
- **Use cases** in `runs/`, one file per verb (`claim`, `heartbeat`, `start_run`, `complete`, `cancel_run`, `create_decision`, `resolve_decision`, …): `pub async fn <verb>(repo: &Repository, now: DateTime<Utc>, <args>) -> Result<T, AppError>`, where the caller read the clock once at the edge and the store takes that instant as a value. They hold the rules a handler used to hold (idempotency keys, fencing checks, `validate_transition`); a handler extracts, calls, maps the error and responds. A test of a rule calls the use case on `setup_test_db()`
- **Background work has one owner and stops.** Every background loop (`auto_start`, `github_sync`, `remote_backup`, `due_soon`, `retention`, `observability`) is a module with `tick()`, one pass that tests call with a fake clock so nothing sleeps, and `spawn()`, which hands that pass to `background::Loops`: the one helper that runs a named tick on an interval, stops on one shared `CancellationToken` (a pass in progress finishes first) and logs a panic with the loop's name. `tack-cli/src/serve.rs` owns the `Loops`, calls each module's `spawn` and stops them all, bounded by `background::STOP_GRACE`, when the server returns; the HTTP server starts only the artifact sweep (`tack-api`'s `ExecutionRuntime`, on the same `Loops`), because the blobs need its `ArtifactStorage`. Each table has one owner: `retention` is the only code that deletes from `execution_events`, and it also expires overdue decisions on the same flag; the artifact sweep deletes `execution_artifacts` and their blobs. A use case a handler starts after answering (the merge on Done, a GitHub push, the pack going to prill) goes on `AppState::tasks`, a `TaskTracker` the server closes and waits for, bounded, before it exits. `complete` writes everything a success sets going in the transaction that ends the attempt: it decides the board move and the rework follow-up from what is stored, hands them to `Repository::complete_execution_with_follow_ups`, and a crash leaves the attempt open for the runner's retry instead of ended with its move lost. The autonomy pass (`autonomy::tick`, with `merge.rs`) runs after each GitHub poll
- **`ports`** are the small traits through which `tack-app` calls back into things that still live in `tack-api`: `Verifier` (prill's attestation check and the outcome of a pull request, failing with `VerifierError`), `Artifacts` (the bytes of a stored file) and `BoardEvents` (an item moved). `Ports` bundles them with `github_sync::GitHub` (the API root, the token and the GitHub calls, which are `tack-app`'s own) and the embedded runner (`LocalRunnerControl`, which resolves secret references); `tack-api` builds it with `AppState::ports()`, and a `tack-app` test builds it from fakes (`tests/ports.rs`). **The process environment is read once, in each crate's config module** (`tack-api/src/config.rs`, `tack-cli/src/config.rs`, `tack-runner/src/config.rs`) and handed down as values: the runner's `ProcessEnv` reaches the harness locator, the readiness check, the verifier, the install and `env:` secret references, the CLI's `ClientEnvironment` reaches `Config::load_with`, and no test sets a process variable. The one read left outside them is the desktop shell's `WEBKIT_DISABLE_DMABUF_RENDERER`, set before the webview starts. A port is a trait only where its implementor names an HTTP type this crate may not
- `scheduler_wiring` and `model_policy_wiring` (the live callers of `tack-orch`), `local_runner` (the embedded runner as the rest of Tack sees it: the `LocalRunnerControl` seam the routes call, its preference, folder checks and the supervisor that keeps one runner process alive) and `AppError`

**tack-api** (library — does not build its own binary; the only crate that names `axum`; depends on `tack-protocol`, `tack-core`, `tack-db` and `tack-app`):
- Axum HTTP server. The documented paths are the `paths` of `docs/openapi.json` (`scripts/arch-facts.sh` prints how many), plus 1 WebSocket not in the spec — includes the operator execution/fleet surface and the `/api/runner/v1` runner-protocol paths. `docs/openapi.json` is generated and authoritative
- **Two authentication surfaces, separated structurally.** Operator routes live under `/api` behind `require_token`. Runner routes are nested as a _sibling_ of `/api` on the outer router, so they never traverse the operator auth layer at all — deliberately not an exemption-list entry, which a later edit could quietly widen. Each runner handler authenticates its own hashed bearer credential via `handlers/runner_protocol/runner_auth.rs`.
- `x-tack-principal` is **overwritten from server config** by `middleware::inject_operator_principal` and never read from the request. Operator idempotency is scoped by principal, so a trusted header would let one caller collide with another's requests.
- Server entry point exposed as `tack_api::serve()` (in `server.rs`)
- WebSocket support for real-time board updates
- Request handlers in `handlers/` (per entity): they extract, call a `tack-app` use case or a `tack-db` method, map the error and respond
- Config loading: TOML + env vars
- Error mapping: `CoreError`, `StoreError` and `AppError` each have a `code()`, a `StableErrorCode` from `tack-protocol` that knows its HTTP status; `ApiError` keeps the code (`ApiError::code()`) and every error body carries it, including the answer to a request no extractor could read (handlers use `extract::{Json, Path, Query, ...}`, never axum's, so a rejection is the same body) and a request id (`ApiError` wraps `StoreError`, which is why this crate names `tack-db`)
- `ports.rs` implements `tack_app::ports` (`Verifier`, `Artifacts`, `BoardEvents`) over `AppState` and builds `Ports` and the `GitHub` the board is configured with
- Debug endpoints: `/api/health`, `/api/debug/info`, `/api/debug/db-stats`
- File upload support: multipart/form-data (max 50MB)
- Export functionality: JSON, YAML and CSV formats
- Project file, outline and CSV import (`handlers/outline.rs`): the format and its checks are `tack_core::outline` and `outline::bundle::check`; items are written in one transaction (`Repository::create_outline_items`), the other sections one by one, and a failed section answers `ApiError::SectionFailed`. Export of the same file is `export?scope=bundle` (`handlers/export.rs`, `to_bundle`); templates are the same file without items (`handlers/templates.rs`)
- The automatic work (the auto-start tick, and the autonomy tick that merges and demotes by `tack_core::autonomy::evaluate`) lives in `tack-app` (`auto_start.rs`, `autonomy.rs`, `merge.rs`) and reads the team's ceiling from the runner's live capability at merge time; the auto-start tick enqueues through the `start_run` use case under the principal `auto_start`

**tack-runner** (the pull-based execution runner — its own binary, separate from `tack`):
- Polls the Tack API's `/api/runner/v1` surface: enroll, refresh capabilities, claim, heartbeat, accept, start, stream events, poll decisions, submit artifacts, complete, and report cancellation/recovery observations
- Owns everything the API must not: its own secret store, the isolated per-attempt workspace/worktree, the owner-only TOML journal written **before** spawn, and the harness subprocess itself
- `harness/` holds the adapter layer — `process.rs` (bounded output capture, timeouts, process-group cancellation), `event_sink.rs` (backpressure), `redact.rs`, `artifact.rs`, and `local_process.rs`, the one lifecycle every local CLI harness shares (locate, probe, request policy, environment and secrets, spawn, cancel, reconcile, log staging, outcome). A CLI harness is a `HarnessDescriptor` (data) plus a `HarnessGrammar` — its command line, how its output is read, what it supports, and optionally how it drives a stdio conversation so a run can pause and ask the operator: `codex.rs`, `claude_code.rs`, `opencode.rs`. Adding one is a module plus a line in `harness::DESCRIPTORS` and one in `harness::discover`; `tack runner doctor` and the Agents page read the descriptor. The harness vocabulary itself is open (`HarnessKind::Other(String)`) — a runner may report any kind string
- **docket is a daemon, not a process the runner spawns per run.** `harness/docket.rs` is a `DocketAdapter` that implements `HarnessAdapter` and `HarnessProbe` directly over docket's operator HTTP API (version 6, docket 0.2.0-beta.6): one attempt is one task posted to a team (`POST /tasks/<pod>`, idempotency key = attempt id), followed by its trace, cancelled over HTTP. Every approval and every parked question is an escalation in docket's one record (`GET /escalations?since=<seq>`), which becomes a decision and is answered over `POST /escalations/<id>`; a pending record of a kind Tack does not know is a timeline line telling the person to answer it in docket, never an approval. Every answer docket sends is read through a struct in `docket/wire.rs`, optional only where docket's spec allows the field to be absent, and `docket.rs` holds no lock across a call to docket, so one slow team delays only its own runs. The runner makes no checkout for a docket run: it reads the base commit and the task's branch from the project's repository, and evidence, the verifier and the push work from those two commits (`WorkspaceMode::Repository`). It is registered only when `[docket]` names a `binary` or `base_url`; the runner starts `docket start --dispatch` (`docket/daemon.rs`) only when `binary` is set. Its steps reach the timeline as `agent.hop_*` and `escalation.*` events, whose kinds are the constants of `tack_protocol::event_kind` that the metrics query shares, worded for a person (what was asked, the label of the option picked, "docket decided this on its own"); no other agent reports steps yet
- The runner runs up to `max_concurrent_attempts` attempts at once (default 1): `active.rs` (`ActiveAttempts`) holds the leases in flight, `transport.rs` runs the slots, and one ticker sends a single heartbeat for all of them and delivers a cancellation to the attempt it names. A folder worked in place stays one attempt at a time by the runner's lock on the folder
- Two traits: `client::engine::HarnessAdapter` (per-attempt lifecycle — `validate`/`start`/`cancel`/`wait`/`reconcile`) and `harness::HarnessProbe` (version/capability discovery, which needs no claimed attempt). `AdapterRegistry` implements `HarnessAdapter` by dispatching on the requested harness kind, so the engine takes exactly one adapter type
- **Capabilities are honest or the adapter is rejected.** `AdapterRegistry::register_probe` refuses any probe claiming `Supported` cancellation unless its harness announces the process groups its tools start, because every other harness's shell tool spawns its subprocess in a new session outside the runner's process group — verified with `ps` against real `claude`. docket stops its work over its own cancel route, so cancellation is `Supported` there and `Advisory` everywhere else; the scheduler must read the capability snapshot, never assume
- After a succeeded (or cancelled) attempt and before the workspace is deleted, the engine captures the attempt's change as evidence for every harness (`evidence.rs`), then, when the operator turned them on in the runner's TOML config, runs a verifier over it that writes a merge-readiness pack (`verify.rs`, `[verify]`) and pushes the work as a branch (`git.rs`, `[git]`). A failure of either is an event on the attempt, never a change to its outcome
- Runner events carry the attempt's next sequence, counted in the journal (`events_submitted`), since the server keeps one event per `(attempt, sequence)`; a harness's steps are batched as events every two seconds and flushed when the run ends
- docket's API version is probed at the first use and must be `5`; any other version is reported as "needs docket 0.2.0-beta.6" and no task is posted
- Live harness tests are opt-in (`#[ignore]` + a PATH check) and never required in CI; the shared fake binary at `harness/fixtures/fake_harness.sh` is the always-runnable path, driven by `TACK_FAKE_HARNESS_MODE`. docket is tested against an in-process fake of its API (`tests/fake_docket/`); no test runs a real docket

**tack-cli** (the single `tack` binary):
- `tack` with no subcommand (or `tack serve`) starts the server + web UI — the primary, UI-first entry point. `serve.rs` is the composition root: it opens the database, builds the `Repository` and the `AppState`, calls `tack_api::serve()` and hands `tack_app::loop_config::LoopConfig` (the loops' settings, owned by `tack-app` and flattened into `AppConfig`) the `Loops` to start
- CLI client using `clap` (`Commands` enum in `main.rs`): board/item basics — `init` (with `--template`), `export` (with `--bundle`), `import`, `projects`, `project`, `add`, `list`, `move`, `board`, `branch`, `start`, `open`, `search`, `config`, `completions`; entity subcommand groups — `sprint`, `template` (`list`, `show`, `create-from`, `save`), `role`, `comment`, `field`; backup — `backup`, `backups`, `restore`; and the execution/runner surface, each its own subcommand group — `execution` (create/list/get/cancel/reconcile requests), `fleet` (runner fleets), `runner` (enroll/revoke/start/doctor), `service` (manage `tack` as a background service — a systemd user unit on Linux, a launchd agent on macOS), `agent-profile`
- `tack mcp` — Model Context Protocol server over stdio (hand-rolled JSON-RPC 2.0 in `mcp.rs`); proxies tool calls to a running server over HTTP so workflow rules apply. See `docs/MCP.md`
- `tack branch <item-id>` — derives/creates a git branch from an item (`git.rs`)
- Client commands talk to the server over HTTP (blocking `reqwest`); never open the DB directly. Only `serve.rs` names `tack-db`, to build the `Repository` it hands to the server and the loops

**frontend** (SolidJS + TypeScript):
- Responsive SPA on a **two-axis design-token system**: mode (`.dark` class) × palette (`data-palette` attr) → Harbor (default)/Teal/Clay/Graphite × light/dark, switched in Settings → Display & accessibility, alongside text size (the root font size; all text is in rem), desktop zoom, a high-legibility font and reduced motion (`shared/state/display.ts`). All colors come from `--color-*` tokens in `index.css`; components never use raw hex. WCAG AA, axe-gated in CI. Fonts: Bricolage Grotesque (display) + Figtree (body) + JetBrains Mono, and Atkinson Hyperlegible Next as the high-legibility option (self-hosted via `@fontsource`). See `docs/book/src/developer/frontend.md`.
- **Board view** with HTML5 drag-and-drop (visual Kanban-style)
- **List view** with sortable table, filtering, and bulk operations
- WebSocket integration for real-time updates
- Optimistic UI updates (instant feedback)
- Keyboard shortcuts + command palette (Ctrl+K)
- Global search (Ctrl+/)
- Toast notifications
- Skeleton loading screens

## Key Design Patterns

**Universal Item Model**: All work units (epics, features, tasks, bugs, etc.) share the same `Item` struct. The `item_type` field and project vocabulary determine how they're labeled and displayed.

**Workflow Engine**: Each project has a `WorkflowConfig` defining:
- Status columns (name, category, WIP limit, order)
- Optional explicit transitions (e.g., construction workflow enforces linear progression)
- Presets: `scrum_workflow()`, `kanban_workflow()`, `simple_workflow()`, `construction_workflow()`

**Vocabulary Mapping**: Projects store a `VocabularyMap` to rename terms:
```rust
{
  "task": "Work Order",
  "sprint": "Phase",
  "epic": "Building"
}
```

**Dependency Graph**: DAG-based system prevents cycles. Uses adjacency lists for both forward (`edges`) and reverse (`reverse_edges`) lookups. Validates new edges before insertion.

**Project Types**: Each `ProjectType` (software, construction, personal, etc.) auto-selects default workflow and vocabulary. Users can fully customize after creation.

## Database Schema Highlights

- **Migrations** are tracked in the `_migrations` table — `scripts/arch-facts.sh` prints how many, and `GET /api/health`'s `migrations_applied` is the live count (039–048 added the ten neutral execution tables; 049–061 refine execution replay, recovery and attempt-start facts; 062 adds project-level default-model selection; 063 drops the unused `model_profiles` table; 064–073 drop the legacy Docket control-plane's ten tables, `control_planes` last since every other one referenced it; 075–077 add GitHub link sync state, mirrored comment ids and a project's GitHub token reference; 078–081 add item briefs, the decision pack columns, merge-readiness pack reviews and pull requests; 088 adds a role's profile and a profile's fleet; 089 a task's recurrence; 090 a project's auto-start; 091 the roles, profiles and automation a template carries; 092 drops dollars per day)
- Migrations are transactional with ordered-prefix and checksum enforcement; 037/038's copy/verify/swap rebuild and 064–073's table drops each run behind an automatic pre-upgrade `VACUUM INTO` snapshot
- **`BEGIN IMMEDIATE` is mandatory for read-then-write transactions.** A deferred transaction that reads then writes deadlocks under concurrency — two callers both upgrade from reader to writer and SQLite returns `SQLITE_LOCKED`. The sites in `repo/execution/` hit this (`grep -rn 'BEGIN IMMEDIATE' crates/tack-db/src`); each was stress-tested before and after the fix. Write-first methods are fine as-is and were deliberately left deferred. Note the shared in-memory test harness can _mask_ these races — prove any new concurrency test load-bearing against a file-backed DB by reverting the fix and watching it fail
- **FTS5 virtual table** (`items_fts`) for full-text search across titles, descriptions, tags
- **Triggers** maintain FTS index on INSERT/UPDATE/DELETE
- **Foreign keys** enforce referential integrity (e.g., items → projects, items → sprints)
- **Indexes** on common queries: project_id, status, priority, parent_id
- **Attachments table** with file metadata (filename, mime_type, storage_path, size)

## API Endpoint Structure

All routes follow RESTful conventions:

- `/api/projects` — CRUD for projects (5 endpoints)
- `/api/projects/{id}/boards` — Multiple boards per project (CRUD + view)
  - `GET /api/projects/{id}/boards/live` — **WebSocket** for real-time updates
- `/api/projects/{id}/export` — Export to JSON/YAML/CSV (1 endpoint)
- `/api/projects/import` — Import from JSON or YAML (1 endpoint)
- `/api/projects/outline`, `/api/projects/{id}/outline`, `/api/projects/import-csv`, `/api/projects/{id}/import-csv` — A project (with its fields, profiles, roles, sprints and automation) and its items, or items for a project, from a project file or a CSV (4 endpoints). `GET /api/projects/{id}/export?scope=bundle` writes the same file back
- `/api/templates`, `/api/projects/from-template/{id}`, `/api/projects/{project_id}/save-as-template` — templates, which are project files without items
- `/api/projects/{id}/autonomy` (GET/PUT) — a project's merge level, domains and budgets, and, read-only, its ceiling and the teams that cap it
- `/api/items/{id}/agent-context` — what the agent will read, and which profile a run would get and why
- `/api/projects/{id}/items` — Items scoped to project (3 endpoints)
- `/api/items/{id}` — Individual item operations (3 endpoints with WebSocket broadcasting)
- `/api/items/{id}/dependencies` — Dependency management (3 endpoints)
- `/api/items/{id}/attachments` — File attachments (2 endpoints)
- `/api/attachments/{id}` — Download/delete attachments (2 endpoints)
- `/api/projects/{id}/sprints` — Sprint management (4 endpoints)
- `/api/projects/{id}/roles` — Role/specialty management (5 endpoints)
- `/api/items/{id}/comments` — Comments on items (2 endpoints)
- `/api/projects/{id}/search` — Full-text search within project (1 endpoint)
- `/api/search` — **Global search** across all projects (1 endpoint)
- `/api/projects/{id}/import-github` — GitHub Issues import (1 endpoint; `owner/repo` or full URL, optional PAT, label filter, PR-skipping, cursor pagination). Imported items are linked in the `github_links` table; with `TACK_GITHUB_TOKEN` set, status and comments flow out to the issue on every change and back in on a poll (`TACK_GITHUB_POLL_SECONDS`). `PUT`/`DELETE /api/items/{id}/github-link` links or unlinks any other item by hand. See `docs/GITHUB-SYNC.md`
- `/api/projects/{id}/import-linear` — Linear import (1 endpoint; Linear API key, optional team/project filter, label filter, priority mapping, cursor pagination)
- `/api/backup`, `/api/restore` — Local DB backup download / staged restore (2 endpoints)
- `/api/backup/remote` (POST/GET), `/api/backup/remote/restore` — Cloud (S3-compatible) backup, list, and staged restore (3 endpoints)
- `/api/settings/backup` (GET/PUT) — Read/update the UI-editable cloud-backup config; secret key is write-only (returned as a `secret_key_set` boolean)
- `/api/executions`, `/api/runner-fleets`, `/api/runners/*`, `/api/agent-profiles` — **Operator** execution surface (create/list/get/cancel/requeue, fleet and profile management, runner enrollment and revocation). Under operator auth. Raw enrollment tokens are returned exactly once at issue time and only their SHA-256 hash is stored
- `/api/items/{id}/brief` (GET/PUT/DELETE) — an item's brief: acceptance criteria, constraints, definition of done, risk (3 endpoints). It travels in project export and import and is rendered into the instructions a harness receives
- `/api/executions/{request_id}/attempts/{n}/mrp` (GET), `.../mrp/viewed` and `.../mrp/review` — an attempt's merge-readiness pack, the mark that a person opened it, and the human verdict (the first verdict wins)
- `/api/projects/{id}/metrics/factory` — the project's factory metrics: every ratio with the counts behind it, and a reason when it cannot be measured; optional `since`
- `/api/runner/v1/*` — **Runner protocol**, the paths under a separate credential: `enroll`, `refresh`, `claim`, `heartbeat`, and per-attempt `accept`, `start`, `events`, `decisions`, `decisions/poll`, `artifacts`, `artifacts/{artifact_id}/content` (PUT — the content upload), `completion`, `cancellation-observation`, `recovery-observation`. Every attempt-scoped mutation validates runner identity + attempt id + current fencing token; a stale fence returns the stable `stale_lease` error and writes nothing

Query parameters support filtering, pagination, and search.

## Important Implementation Details

### Workflow Validation

When updating an item's status:
1. Check if both statuses exist in project workflow
2. If explicit transitions defined, validate the move is allowed
3. Check WIP limit for target column (before adding)
4. Update `started_at` when moving to in-progress category
5. Update `completed_at` when moving to done category
6. **Auto-propagate parent status** if item has a parent and all siblings are complete

**Example:** Construction projects have strict transitions (Permit → Procurement → Build → Inspect → Handover). Jumping from Permit to Handover is rejected.

### Auto-Status Propagation

When a child item is moved to a "done" status, the system automatically checks if all siblings are also complete. If so, the parent item is automatically updated to "done" as well. This cascades up the hierarchy.

**Implementation:**
- Repository method: `check_and_update_parent_status(parent_id, completed_status)`
- Triggered in `update_item` handler after status change
- Only activates when moving to a status with `StatusCategory::Done`
- Errors are silently ignored (best-effort feature)

**Example workflow:**
1. Epic "User Auth" has 3 child tasks
2. Complete task 1 → no parent update (still 2 incomplete)
3. Complete task 2 → no parent update (still 1 incomplete)
4. Complete task 3 → **parent epic auto-completes** ✓

### Dependency Cycle Detection

Before creating a dependency:
1. Check for self-reference (`source == target`)
2. Build adjacency graph from existing dependencies
3. Run DFS from target to see if it can reach source
4. Reject if cycle detected

The graph is reconstructed on each validation. For large dependency sets, consider caching.

### Sprint Status Lifecycle

Sprints have four states:
- `Planning` → `Active` → `Review` → `Closed`

Items can only be assigned to active or planning sprints (enforced in handlers).

### Tags & Search

- Tags are stored as JSON arrays in SQLite
- FTS5 index includes tags, titles, descriptions
- Search via `/api/projects/{id}/search?q=term` uses FTS5 `MATCH`

### File Attachments

- Upload via multipart/form-data to `/api/items/{id}/attachments`
- Max file size: 50MB (configurable in handler)
- Files stored in `TACK_STORAGE_DIR` organized by item ID
- Unique filenames prevent collisions (UUID-based)
- Download includes proper Content-Type and Content-Disposition headers
- Metadata stored in database: filename, mime_type, storage_path, size_bytes

### Export/Import

- **JSON Export**: Complete project snapshot — project, items, sprints, dependencies, metadata
  - `GET /api/projects/{id}/export?format=json`
- **YAML Export**: The same snapshot, serialized as YAML. YAML is a JSON superset, so both formats decode into the same intermediate value on import
  - `GET /api/projects/{id}/export?format=yaml`
- **CSV Export**: Simplified item list for spreadsheet import
  - `GET /api/projects/{id}/export?format=csv`
  - Includes: id, title, description, type, status, priority, assignee, estimate, due_date, tags, parent_id, created_at — the columns the CSV import reads, so an export re-imports with its hierarchy
- **Project file**: `GET /api/projects/{id}/export?scope=bundle&format=yaml|json` writes the whole project as the file that `POST /api/projects/outline` and `POST /api/projects/{id}/outline` read back (see [The Project File Format](book/src/developer/project-file.md)). It is deterministic, carries no ids or dates, and never deletes on import
- **Import**: `POST /api/projects/import` (`handlers/export.rs::import_project`) accepts the same JSON-or-YAML snapshot shape produced by export, dispatched on `Content-Type`. Creates a new project, imports sprints then items (two passes — create, then wire up `parent_id`) then dependencies, preserves each item's original `source` trust marker, and restores the source project's workflow and vocabulary. If any step fails, the created project is deleted and the error is returned — nothing is left half-imported.

### WebSocket Real-Time Updates

- **Endpoint**: `GET /api/projects/{id}/boards/live` (WebSocket upgrade)
- **Purpose**: Real-time board state updates for live collaboration
- **Implementation**:
  - Uses Tokio broadcast channel (100 message capacity)
  - AppState contains `broadcast_tx: broadcast::Sender<BoardEvent>`
  - Each WebSocket connection subscribes to the broadcast channel
  - Events are filtered by project_id before sending to client
- **Event Types**:
  - `ItemCreated` - New item added to board
  - `ItemUpdated` - Item status/details changed
  - `ItemDeleted` - Item removed
  - `BoardConfigUpdated` - WIP limits or columns changed
  - `SprintUpdated` - Sprint status changed
  - `Ping` - Keepalive (sent to all)
- **Usage**: Frontend connects via WebSocket, receives JSON events, updates UI reactively
- **Broadcasting**: Other handlers call `websocket::broadcast_event()` to notify all subscribers

### Testing Strategy

- **Unit tests** in `tack-core` for business logic (workflow, dependencies, vocabulary)
- **Integration tests** in `tack-db` for repository operations (require SQLite)
- **Use-case tests** in `tack-app` call a use case or a `tick()` directly on `setup_test_db()` and a fake clock, with no router and no sleeping
- **Pure policy tests** in `tack-orch` (scheduler, model policy) and the runner-v1 contract pins in `tack-protocol` need no database
- **Seeding and reading**: a test plants rows with `tack_test_support::seed` and reads them with `tack_db::queries::`, so a test that checks a route's answer never names SQL
- **Handler tests** in `tack-api` use in-memory databases
- **docket** is proved against `crates/tack-runner/tests/fake_docket/`, an in-process fake of its HTTP API
- **Frontend unit tests** in `frontend/src/**/*.test.tsx` (Vitest + jsdom)
- **End-to-end tests** in `frontend/e2e/` (Playwright): cross-browser smoke, user
  journeys, axe accessibility scans, and API wire-contract checks. Test setup
  talks to the API directly; the browser exercises the SPA via the proxy.
- **Security**: `cargo audit` + `npm audit` in CI; justified advisory exceptions
  in `.cargo/audit.toml`. **Performance**: k6 baseline in `tests/load/`.
- **Integration gates** in `crates/tack-api/tests/handlers/production_router.rs` — drive the real
  `build_router`,
  because one area's own green tests are not evidence that the integrated system works.

**A test that asserts a status code has usually not proved the claim.** The recurring failure
in this codebase has been tests that pass while proving something weaker than their name says:
a 413 asserted without checking the DB was unwritten; a concurrency test that caught `Err(_)`
and retried sequentially, so it silently stopped being concurrent; a capability reported
`Supported` with no implementation behind it. When adding a test for a "writes nothing" or
"rejects before X" claim, assert the absence directly — row counts, an untouched checkpoint,
empty bookkeeping — and prove the test is load-bearing by reverting the fix and watching it
fail.

Use `assert!(matches!(..))` for enum matching in tests. When changing an API
response shape, update both the Rust handler and the matching mock in the
frontend unit/E2E tests (e.g. the `GET /items/{id}` detail envelope).


## Common Patterns When Adding Features

### Adding a New Entity

1. Define model in `tack-core/src/models.rs`
2. Add migration in `tack-db/src/migrations.rs`
3. Create repository module in `tack-db/src/repo/` (commands) and, for reads, a file in `tack-db/src/queries/`
4. Put any rule that is more than extract-and-respond in a `tack-app` use case; add the handler in `tack-api/src/handlers/`
5. Register routes in `tack-api/src/router.rs`
6. Add DTOs for create/update operations

### Extending Workflow Logic

1. Add logic to `tack-core/src/workflow.rs`
2. Write unit tests in the same file
3. Update handlers to call new validation/logic
4. No database changes needed (workflow is JSON in DB)

### Adding a New Project Type

1. Add variant to `ProjectType` enum in `models.rs`
2. Create workflow preset in `workflow.rs` (e.g., `education_workflow()`)
3. Create vocabulary preset in `vocabulary.rs`
4. Update `workflow_for_type()` match statement

## Troubleshooting

**Migration errors on startup:**
- Check `_migrations` table: `SELECT * FROM _migrations`
- Manually delete failed migration record if needed
- Restart server to retry

**Database locked errors:**
- SQLite only supports one writer at a time
- Check if another process has the DB open
- Use `?mode=rwc` in connection string (already default)

**FTS5 not found:**
- SQLite must be compiled with FTS5 support
- Check with: `sqlite3 tack.db "PRAGMA compile_options;"`

