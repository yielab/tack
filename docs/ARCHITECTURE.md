# Architecture & Implementation Notes

Crate responsibilities and the implementation rules that hold across them. This file is the
authority on crate boundaries; CLAUDE.md keeps only the condensed map. It is for contributors. The
book's [Developer Guide](book/src/developer/index.md) is the narrative version: its
[Crate Tour](book/src/developer/crate-tour.md) walks each source file, and
[Runs, Teams and Automation](book/src/developer/runs-and-teams.md) and
[The Project File Format](book/src/developer/project-file.md) cover those two subsystems in depth.
When this file and the tour disagree on a fact, this file wins and the tour should be corrected.
It does not list routes, tests or recipes: the routes are in the generated
[API reference](book/src/developer/api-reference.md), the test layout is in [TESTING.md](TESTING.md),
and the recipes for new code are in [Adding Features](book/src/developer/adding-features.md).

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
│   ├── features/    One directory per surface (board, list, table, calendar, timeline,
│   │                sprints, item-detail, dashboard, projects, settings, templates, agents)
│   ├── shared/      Cross-feature code: api/ (generated client + schema), realtime/,
│   │                runWithAgent/, execution/, agents/, ui/, state/, vocab/, keyboard/,
│   │                help/, types/
│   └── test/        Vitest setup
├── e2e/             Playwright specs
├── public/          Static assets
└── package.json     Frontend dependencies (SolidJS, Vite, Tailwind v4)

docs/                Documentation: this file, CONFIG.md, MCP.md, TESTING.md, the guides,
                     adr/ (decisions), contracts/ (language-neutral fixtures), book/ (mdBook)
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
- Migrations (`migrations.rs`; `scripts/arch-facts.sh` prints how many, and the live count is `GET /api/health`'s `migrations_applied`) with FTS5 full-text search on items. They are transactional, append-only, and checked at every startup by name and checksum; a migration that cannot be re-run safely (a table rebuild or a table drop) takes a `VACUUM INTO` snapshot first. See [MIGRATION-GUIDE.md](MIGRATION-GUIDE.md)
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
- docket's API version is probed at the first use and must be a number, 6 or later (`MIN_API_VERSION` in `harness/docket/probe.rs`); anything else is reported as "needs docket 0.2.0-beta.6" and no task is posted
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

## Implementation rules

**`BEGIN IMMEDIATE` is mandatory for read-then-write transactions.** A deferred transaction that
reads then writes deadlocks under concurrency: two callers both upgrade from reader to writer and
SQLite returns `SQLITE_LOCKED`. The sites in `repo/execution/` hit this (`grep -rn 'BEGIN IMMEDIATE'
crates/tack-db/src`). Write-first methods are fine as they are. The shared in-memory test harness
can mask these races, so prove a new concurrency test load-bearing against a file-backed database
by reverting the fix and watching it fail.

**Parent status follows its children.** When an item moves to a `Done`-category status, the
handler calls `propagate_parent_completion` (`tack-api/src/handlers/items.rs`), which calls
`Repository::check_and_update_parent_status`: if every sibling is done, the parent moves to done
too. The check does not repeat for the grandparent. It is best-effort: a failure there never fails the
move that triggered it.

**Search and tags.** Tags are JSON arrays in SQLite. The `items_fts` FTS5 table indexes titles,
descriptions and tags, and triggers keep it in step with `items`.

**Attachments.** Uploads are multipart to `/api/items/{id}/attachments`, at most 50 MB (the global
`TACK_MAX_BODY_SIZE` limit does not apply to that route). Files live under `TACK_STORAGE_DIR`, in
a directory per item, under a generated unique name; the row keeps the original name, MIME type
and size.

**Real-time updates.** `AppState.broadcast_tx` is one Tokio broadcast channel (capacity 100)
shared by every project. A handler that changes a board calls `broadcast_event`; each WebSocket
connection on `/api/projects/{id}/boards/live` forwards only the events of its own project. Events
are not stored. The walkthrough is in the [Architecture Overview](book/src/developer/index.md#5-how-real-time-updates-work)
and the wire format in [API-REFERENCE.md](API-REFERENCE.md#websocket-events).

## Troubleshooting

**Migration errors on startup:** the server refuses to start when the applied history differs from
the binary's list by name or checksum. See [MIGRATION-GUIDE.md](MIGRATION-GUIDE.md).

**Database locked errors:** SQLite allows one writer at a time. Check whether another process has
the database open.

**FTS5 not found:** SQLite must be compiled with FTS5. Check with
`sqlite3 tack.db "PRAGMA compile_options;"`.
