# Architecture Overview

This is the front door of the Developer Guide. It is for contributors who change Tack and integrators who build on it. After reading it you will know how the code is layered, where the other two guides stop and this one starts, and which page to read next.

## How the three guides fit

| Guide | Reader | Where it stops |
|---|---|---|
| [User Guide](../user-guide/quick-start.md) | Someone who works in Tack every day | It says what a control does and what you will see. It never names a crate, a file or a function |
| [Administrator Guide](../admin-guide/index.md) | Someone who installs, secures, runs and recovers Tack and its runners | It says how to set it up and fix it. It points here for why a rule exists |
| Developer Guide (this one) | A contributor or integrator | It says how the code is built, what the contract is, how to extend it and why it was decided |

A user page that needs a setting points to the Administrator Guide for how to set it. An administrator page that needs a rule points here for why. This guide never repeats step-by-step instructions for end users.

## A map of this guide

| Page | Read it to |
|---|---|
| This page | See the layering, the item model, a request's path and the live board channel |
| [Crate Tour](crate-tour.md) | Find the file that owns a behaviour, crate by crate |
| [`docs/ARCHITECTURE.md`](https://github.com/yielab/tack/blob/develop/docs/ARCHITECTURE.md) | Check a crate's boundary and the implementation rules that hold across crates |
| [Runs, Teams and Automation](runs-and-teams.md) | Follow an automatic run from a waiting task to a merge, and see how docket teams, the ceiling, usage and recurrence are built |
| [The Runner Contract](runner-contract.md) | Write or change a runner, or an adapter for an agent |
| [The Project File Format](project-file.md) | Produce, check or extend the file that carries a whole project |
| [Frontend & Design System](frontend.md) | Work on the SolidJS app and its design tokens |
| [Adding Features](adding-features.md) | Follow a recipe for a new entity, a workflow preset or a project-file key |
| [MCP Server](mcp.md) | Use or extend the stdio server for assistants |
| [API Reference](api-reference.md) | Look up a route (generated from `docs/openapi.json`) |
| [Testing](testing.md) | Run the suites and know where a new test belongs |
| [Architecture Decisions](decisions.md) | Find the ADR behind a behaviour |
| [Learning Path](learning/index.md) | Learn Rust, async, Axum, sqlx and SolidJS as you need them |

## Where to start

1. Read this page to the end.
2. Skim the [Crate Tour](crate-tour.md) for the crate you will touch.
3. Read [Runs, Teams and Automation](runs-and-teams.md) if you work on runs, agents or merging, or [The Project File Format](project-file.md) if you work on import and export.
4. Read the six records listed in [Architecture Decisions](decisions.md#reading-order-for-newcomers).
5. Before your first change, read [Testing](testing.md) and run `.githooks/pre-push`.

> `docs/ARCHITECTURE.md` states each crate's boundary and the rules that hold across crates, and is the authority when it and this guide disagree. This page is the narrative walkthrough.

## Why a board and a runner

> They are separate because they scale and fail differently. **One board, many runners:**
> a board on a small VPS dispatches to runners on ten developers' machines, each with its
> own agent, model and capacity. A runner that dies mid-run cannot corrupt the board: its
> lease expires and its fencing token stops writing. A board that restarts cannot lose a
> run: the runner's journal knows what it started. **One developer runs both in one
> process with one command**, on the same contract, with the same recovery.

The rest of this page explains why the code is structured the way it is: the reasoning behind each layer, how a request travels through the system, and where the interesting domain logic lives.

---

## 1. The Layering Rule

Tack enforces a strict one-way dependency graph across the workspace's crates:

```
tack-protocol     -> (none)
tack-core         -> tack-protocol
tack-orch         -> tack-protocol tack-core
tack-db           -> tack-protocol tack-core
tack-app          -> tack-protocol tack-core tack-orch tack-db
tack-api          -> tack-protocol tack-core tack-db tack-app
tack-runner       -> tack-protocol tack-core
tack-cli          -> tack-api tack-app tack-runner tack-db tack-protocol tack-core
tack-test-support -> tack-protocol tack-core tack-db
tack-evals        -> tack-test-support
```

Each line lists what a crate may depend on; `scripts/check-crate-graph.sh --print` prints exactly this, and the check fails on any other normal dependency. No reverse edges are allowed. `tack-test-support` holds test helpers and is a dependency of the others' tests only. `tack-evals` is a development tool that is never released, and nothing depends on it. An eleventh crate,
`tack-desktop`, is deliberately excluded from this workspace (its own `Cargo.toml`
and lockfile) because Tauri drags GTK/WebKit/glib into whatever workspace holds it —
see the [Crate Tour](crate-tour.md#tack-desktop) for how it supervises `tack` instead
of linking against it. `scripts/arch-facts.sh` prints the crate, route, migration and test counts, so this page quotes none of them.

**What this means in practice:**

- `tack-protocol`, `tack-core` and `tack-orch` do no I/O. `tack-core` holds the planning and runs policy as pure structs, enums and functions, so every test in it runs without a database.
- `tack-db` is the only crate that names `sqlx`. Commands are `Repository` methods, read models are functions in `queries/`, and every call fails with `StoreError`.
- `tack-app` holds the use cases of a run and every background tick. It reaches back into what still lives in `tack-api` through the traits in `tack_app::ports`.
- `tack-api` is the only crate that names `axum`. A handler extracts, calls a `tack-app` use case or a `tack-db` method, maps the error and responds.
- `tack-cli` is the single `tack` binary and the composition root: `serve.rs` builds the `Repository`, hands it to the server and spawns the ticks. Its client commands only talk to a running server over HTTP, so the CLI works whether the server is local or remote.
- `tack-runner` is a separate binary. It speaks runner-v1 to `tack-api` over loopback or the network, the same way a remote runner does, and never depends on `tack-db` or `tack-api`.

Each crate's boundary, in full, is in [`docs/ARCHITECTURE.md`](https://github.com/yielab/tack/blob/develop/docs/ARCHITECTURE.md); the file-by-file walk is the [Crate Tour](crate-tour.md).

**Why bother?** The layering prevents the kind of "everything knows about everything" entanglement that makes codebases brittle. It also means:

- Workflow rules are always consistent. Because all validation lives in `tack-core`, neither a direct DB call nor an API call nor a CLI command can bypass it.
- Tests are fast. Core tests are pure function calls — no test database, no async runtime, no cleanup.
- The domain model can be reused. If a future project needs the same workflow engine, it can depend on `tack-core` without dragging in SQLite or Axum.

---

## 2. The Universal Item Model

Every piece of work in Tack — whether it is called a task, an epic, a bug, a work order, or an assignment — is stored as an `Item`. There is one table, one struct, one set of repository functions.

The `Item` struct in `tack-core/src/models.rs` carries an `item_type` field that records what kind of thing it is: a **key of the project's type catalog**. The catalog (`tack-core/src/item_types.rs`, table `item_types`) is data of the project, not an enum. Each type has a key, a name, a colour and a **level**, `group` or `work`, and exactly one type is the project's default work type. A software project's catalog has Epic (a group) and Feature, Task, Bug and Requirement (work); a construction project's has Building and Section (groups) and Work Order, Activity, Defect and Specification (work). The level drives the tree rule (`tack-core/src/hierarchy.rs`): a type that holds work never goes under work, the tree has at most five levels and no loops, and an item can be moved under another parent at any time by that same rule. The **vocabulary** system translates the other ten words (`sprint`, `backlog`, …) into whatever label the project uses.

Think of it like a spreadsheet: every row has the same columns. The header labels change depending on who is looking at the sheet. A construction project calls its default work type `Work Order` and `sprint` `Phase`, but the underlying row structure is identical.

**Benefits:**

- One migration path. Adding a new field to items (say, a `story_points` column) means one migration, one struct update, one set of repository functions — not separate migrations per item type.
- Hierarchy is one table. Because every item can have a `parent_id` pointing to another item, the tree falls out of that column without a separate parent-type/child-type mapping. A type's level is what the tree reads: a type at level `group` holds groups and work, work holds work, and an item with children is a group whatever its type (ADR 0036, decisions 10 and 11). "Subtask" is not a type; it is work under work.
- Filtering is uniform. The same `ItemFilter` struct handles filtering by `item_type`, `status`, `priority`, `assignee`, or tag regardless of project type.

The tradeoff is that the `item_type` field carries less compile-time enforcement than an enum would. This is intentional — a project names its own types, and the store checks that an item's key is in its project's catalog inside the write's transaction.

---

## 3. How a Request Flows Through the System

Here is a concrete walk-through of `PATCH /api/items/{id}` — the endpoint that moves an item to a new status.

```
HTTP client
    │
    ▼
Axum router (tack-api/src/router.rs)
    │  Extracts: Path(item_id), State(AppState), Json(UpdateItem)
    ▼
Handler: update_item (tack-api/src/handlers/items.rs)
    │  1. Validates the body, then loads repo.get_item_snapshot(id) and checks If-Match
    │  2. Calls repo.get_project(project_id) to load the workflow
    │  3. Calls repo.update_item_atomically(id, input, &workflow, expected_version), which
    │     inside one transaction calls workflow.validate_transition(old, new) and
    │     workflow.check_wip_limit(new, count)  ← both pure, no I/O in tack-core
    │  4. after_update(...) then: state.events().emit(DomainEvent::item_changed(...))  ← the bus,
    │     and with it the WebSocket event
    │  5. create_next_recurrence(...)  ← a repeating task that reached Done makes its successor
    │  6. maybe_sync_github(...) and, for a task moved to Done, merge::on_done(...)
    ▼
Repository (tack-db/src/repo/items.rs)
    │  Runs parameterised SQL via sqlx
    ▼
SQLite
    │
    ◀── Result<Item, StoreError>
    │
Handler maps error → ApiError → HTTP response
    │
    ▼
HTTP client receives JSON
```

A few things worth noting:

- Step 3 calls pure functions in `tack-core`. No extra database round-trip is needed to validate the transition: the entire workflow config was loaded with the project in step 2, and it lives in memory as a `WorkflowConfig` struct. The repository runs them in the same transaction as the write, so a rejected move leaves the item's version untouched.
- The handler owns the orchestration. It decides the order of validation, persistence, and notification. The repository and the core crate do not know about each other.
- Errors propagate upward via Rust's `?` operator. The `ApiError` type (in `tack-api/src/error.rs`) implements `IntoResponse`, giving every failure a `StableErrorCode` (one per rule of `CoreError`), which decides the HTTP status and is in the body as `code`.

---

## 4. How Workflow Validation Works

The workflow for a project is a `WorkflowConfig` struct stored as JSON in the `projects` table. When the server loads a project, it deserialises that JSON back into the struct — no extra tables, no joins.

The `WorkflowConfig` holds:

- A list of `StatusDef` entries (name, category, optional WIP limit, sort order).
- An optional list of `Transition` pairs (`from`, `to`).

`validate_transition(from, to)` does two things:

1. Checks that both `from` and `to` exist in the status list.
2. If an explicit transition list exists, checks that the pair is in it.

If the transition list is absent (`None`), any move between two valid statuses is allowed. This is the Scrum and Kanban default. The construction workflow, by contrast, defines an explicit list — you cannot skip from `Permit` to `Handover`.

**Why is this in `tack-core` and not in the database layer?**

Because it is a business rule, not a storage rule. The database does not know what statuses are valid — that is configuration data stored in a JSON column. Only `tack-core` knows how to interpret that configuration and enforce the rules. Putting the validation in the repository would mean the repository would need to import workflow logic, violating the layer boundary. Putting it in the handler (without core) would scatter the logic and make it untestable without a running server.

By living in `tack-core`, the validation is:

- Testable with zero dependencies (its tests never touch a database).
- Reusable by any layer that loads a `WorkflowConfig`.
- Independent of how the config was persisted.

---

## 5. How Real-Time Updates Work

The real-time board feature uses a Tokio broadcast channel. Here is the architecture:

```
AppState
  └── broadcast_tx: broadcast::Sender<BoardEvent>   (capacity: 100)

On an item change:
  handler emits a DomainEvent through state.events()
    └── the adapter in tack-api/src/ports.rs calls broadcast_event(&state, BoardEvent::ItemUpdated { ... })
          └── state.broadcast_tx.send(event)  ← non-blocking, drops if no subscribers

On WebSocket connection (GET /api/projects/{id}/boards/live):
  handler upgrades the connection
  spawns two tasks:
    send_task:  rx.recv() in a loop → filter by project_id → send JSON to client
    recv_task:  reads messages from client (handles Close, Ping)
  tokio::select! on both tasks — whichever ends first, the other is aborted
```

The broadcast channel is the pub/sub backbone. Every handler that modifies an item emits a domain event, and the adapter sends the browser's share of it; a run or a decision has no browser message. Every WebSocket connection subscribes to the channel and filters events by `project_id` before forwarding them to the client.

**Important properties:**

- **Fire and forget.** `broadcast_tx.send()` returns immediately whether or not there are subscribers. If nobody is listening, the event is dropped. This keeps the write path fast.
- **Per-project filtering happens in the send task.** The channel carries events for all projects; each WebSocket connection only forwards events that match its `project_id`. This keeps the channel logic simple while allowing one channel to serve many concurrent connections.
- **No persistence.** Events are not stored. A client that disconnects and reconnects will not receive missed events — it will simply see the current board state on the next page load.
