# Phase 93: an item is a group or work

**Status: PROPOSED 2026-11-05.** Executes ADR 0036's amendment of 2026-11-05, decisions 10 to 13
(`docs/adr/0036-the-board-orchestrates-and-a-team-declares-what-it-accepts.md`, section "Amendment,
2026-11-05"). Read that table first: it says what is decided, and this file says only how. Eight
cards in five rounds: **A**; **B**; **C ∥ D**; **E**; **F ∥ G ∥ H**. The cards are grouped by the
files they own, so that two cards of one round never edit the same file except `CHANGELOG.md` and
the generated files.

**Nothing old stays.** The `ItemType` enum, the type words of the vocabulary, the `subtask` type, the
saved boards and the web's own board derivation are deleted by the card that replaces them, in the
same change. No compatibility reader, no alias, no `#[serde(alias)]` for the old words, no "legacy"
branch, no feature flag. A database from `develop` is migrated once, and an old file that names a
type the catalog lacks is read with the rule of card A (the word is added as work), not with a
second parser.

## How a card is worked

- **Read first:** ADR 0036's amendment, then this file's "Shapes shared by every card", then
  `docs/ARCHITECTURE.md` §"Crate Boundaries", then only the files your card names. Line numbers
  move: find each place with `grep -n` before editing it.
- **Where each thing lives** (the crate boundaries of `docs/ARCHITECTURE.md`, checked by
  `scripts/check-crate-graph.sh`):
  - Every rule is pure and lives in `tack-core`: the tree in `hierarchy.rs` (new), the catalog's
    types and presets in `item_types.rs` (new), progress in `children.rs`, routing in
    `run_defaults.rs`, and auto-start's pick in `auto_start.rs`.
  - Only `tack-db` names SQL. Commands are `Repository` methods in `repo/<aggregate>.rs`; read
    models are functions in `queries/<table>.rs`. A rule that must hold inside a write transaction
    is asked of `tack-core` through a trait the store calls with the facts only it can read
    atomically, as `StatusGate` is today (`tack-core/src/workflow.rs`, used by
    `update_item_atomically`).
  - A run's use cases live in `tack-app/src/runs/`, and the background ticks also live in
    `tack-app`. `tack-api` handlers only extract, call, map the error and respond.
  - Every new refusal is a `CoreError` variant with its `StableErrorCode`, added in `tack-protocol`
    (`execution/types.rs`, beside `HasChildren`) with its HTTP status, and in the code list of
    `execution/types/tests.rs`.
  - In the web, `features/*` never imports another `features/*`, and `shared/*` never imports
    `features/*` (`frontend/src/architecture.test.ts`). Anything two views share goes in `shared/`.
- **A screen never decides the tree.** Whether an item is a group, and how far a group is, come from
  the server (`node`, `progress`). The web may derive one thing, an item's ancestors, and only in
  `shared/tree.ts` (card E), because every view already holds the project's items.
- **Migrations** are appended to `crates/tack-db/src/migrations.rs` after the last one (106 today):
  A writes `107_item_types` and D writes `108_drop_boards`. A migration is SQL only: `ordinary` for
  statements, or `rebuild` for a table rebuild. There is no Rust step, so a JSON column is rewritten
  with SQLite's `json_remove` and `json_group_array`. A statement with `DROP TABLE` takes the
  `VACUUM INTO` snapshot by itself (`migrations.rs`, "needs snapshot"); `ALTER TABLE … DROP COLUMN`
  is ordinary, as in 103. Never edit a shipped migration (`docs/MIGRATION-GUIDE.md`).
  `tests/migrations/upgrade_from_beta9.rs` must still pass.
- **Generated files:** after any route or schema change, run `./scripts/regen-generated.sh` (it
  needs `npm ci` in `frontend/` for `schema.gen.ts`). Never edit `docs/openapi.json` or
  `frontend/src/shared/api/schema.gen.ts` by hand. After a rebase, run the script again rather
  than resolving their conflicts.
- **Tests:** run only the tests of what your card touched:
  `cargo nextest run -p <crate> -E 'binary_id(<crate>::it) & test(/^<module>::/)'` for integration
  tests, `-E 'test(/<module>::/)'` for unit tests, and `cd frontend && npx vitest run <path>` for
  the web. Put each new test at the lowest layer that can state it (ADR 0010; `docs/TESTING.md`):
  a rule in `tack-core`, a query in `tack-db`, a route in `tack-api` with `common::test_app()`. A new
  integration-test subject is a new `mod` line in its crate's `tests/it.rs`. The full suites and
  `.githooks/pre-push` run once per round, by the integrator.
- **Build limits:** use `PATH=$HOME/.cargo/bin:$PATH`, `--build-jobs 4 --test-threads 4` (in
  `cargo nextest`, `-j` means `--test-threads`), the `CARGO_TARGET_DIR` your dispatcher names, and
  `CARGO_INCREMENTAL=0`. No GUI: no `tack-desktop`, no `make desktop`, no headed Playwright.
- **Docs travel with the change:**
  - Each card writes its lines in `CHANGELOG.md` under `[Unreleased]`.
  - Each card updates the book pages its "Docs" list names. The User and Administrator guides carry no
    source paths, test names or phase numbers, and every number they quote is measured again first.
  - When a fact about a crate changes, the card updates `docs/ARCHITECTURE.md` and
    `developer/crate-tour.md`.
- **The person's words** come from ADR 0036 decision 13 and the "Words" section below. Never show
  "node", "level" or "leaf" on a screen.
- **Git:** one worktree per card, cut from `develop`; commit as the history does (`Feature: …`,
  `Docs: …`, a sentence about what changed for the person). **No `Co-Authored-By` trailer.** Agents
  never merge; the integrator fast-forwards one card at a time.
- **Model:** Sonnet for every card. Card A is the largest; split it across two sessions on the same
  branch if it does not fit one.

## Shapes shared by every card

```rust
// tack-core/src/item_types.rs — card A
pub enum Level { Group, Work }                     // serde "group" | "work"
pub struct ItemTypeDef {
    pub key: String,          // lowercase, [a-z0-9_-]{1,40}; stable, never renamed
    pub name: String,         // what the project calls it ("Epic", "Building", "Course")
    pub level: Level,
    pub color: TypeColor,     // a token name, never a hex: Accent | Accent2 | Danger | Warning | Success | Neutral
    pub order: u32,
    pub default_work: bool,   // exactly one per project, and it is Work
}
pub fn preset(project_type: &ProjectType) -> Vec<ItemTypeDef>;    // replaces the type half of vocabulary_for_type
pub fn check_catalog(types: &[ItemTypeDef]) -> Result<(), CoreError>; // one default work type, unique keys, valid keys
// Software, Web, Mobile, Maintenance, Personal, Custom: Epic (group); Feature, Task, Bug, Requirement (work, Task default)
// Construction: Building, Section (group); Work Order (default), Activity, Defect, Specification (work)
// Homework: Course, Module (group); Assignment (default), Question, Correction, Rubric Item (work)
// Legal, Research, Event: today's vocabulary_for_type words, with the epic word as the group and the task word as the default
```

```rust
// tack-core/src/hierarchy.rs — card B
pub enum Node { Group, Work }
pub fn node(level: Level, has_children: bool) -> Node;            // Group when level is Group or has_children
pub const MAX_DEPTH: usize = 5;
pub struct Placement { pub item: Uuid, pub level: Level, pub project: Uuid, pub subtree_height: usize, pub in_sprint: bool }
pub struct Target { pub id: Uuid, pub level: Level, pub project: Uuid, pub ancestors: Vec<Uuid>, pub depth: usize }
pub fn admit_parent(item: &Placement, parent: Option<&Target>) -> Result<(), CoreError>;
//   GroupUnderWork | ParentLoop | TreeTooDeep | ParentInOtherProject
pub fn admit_sprint(node: Node) -> Result<(), CoreError>;          // GroupInSprint
pub trait ParentGate: Sync { fn admit(&self, item: &Placement, parent: Option<&Target>) -> Result<(), CoreError>; }
//   the store reads Placement and Target inside update_item_in's BEGIN IMMEDIATE and asks the gate, as it asks StatusGate

// card C
pub fn runnable(node: Node, profile_kind: &str) -> Result<(), CoreError>;   // GroupNotRunnable unless kind is planner | researcher
```

```rust
// tack-core/src/models.rs — card A changes Item, card D changes ItemCard
pub struct Item { /* … */ pub item_type: String /* a key of the project's catalog */, /* … */ }
pub struct ItemCard { /* … */ pub node: Node, pub progress: Option<Progress> }   // `children_progress` is deleted
pub struct Progress {                     // over every work item under the group, at any depth, hidden ones excluded
    pub total: u32, pub done: u32,                         // done = a status in the workflow's done category
    pub estimate_total: Option<f64>, pub estimate_done: Option<f64>,   // None when no work under it has an estimate
    pub starts: Option<DateTime<Utc>>, pub ends: Option<DateTime<Utc>>, // earliest start, latest due
    pub teams: Vec<String>, pub open_runs: u32, pub needs_you: u32,
}
pub struct Project { /* … */ pub item_types: Vec<ItemTypeDef> }   // served with the project, so no screen fetches it twice
```

```rust
// tack-core/src/run_defaults.rs — card C
pub struct ChainLink { pub item: Uuid, pub title: String, pub run_settings: Option<String>, pub roles: Vec<(String, Option<String>)> }
pub fn resolve(explicit: Option<&str>, chain: &[ChainLink], project_default: Option<&str>,
               implementer: Option<&str>, profiles: &[Candidate]) -> Option<Resolved>;
//   chain = the item, then its parent, … up to the top; the first link that names a profile wins
//   (its run settings, else its roles when they agree); roles that disagree at a link stop the climb
//   and give the project default, as today. Resolved gains `from: Option<(Uuid, String)>`, the ancestor
//   the answer came from, None when it is the item's own.
```

```text
HTTP — cards A, B, C, D
GET    /api/projects/{id}/types                      -> Vec<{ ..ItemTypeDef, items: u32 }>   how many items use each type
POST   /api/projects/{id}/types                      {key, name, level, color}
PATCH  /api/projects/{id}/types/{key}                {name?, color?, order?, default_work?}   level changes only while no item uses the type
DELETE /api/projects/{id}/types/{key}?merge_into=k   refused with TypeInUse without merge_into while items use it
PATCH  /api/items/{id}                               UpdateItem gains parent_id: Option<Option<Uuid>> (null = the top)
GET    /api/projects/{id}/parents?item=<id>|type=<key> -> Vec<{ id, title, type }>   every parent B admits (B)
GET    /api/items/{id}/run-options                   -> Vec<{ profile_id, allowed, reason? }>   (C)
GET    /api/projects/{id}/items?node=work|group&under=<id>
```

```yaml
# project file and template — card A (export writes it; import reads it)
types:
  - {key: epic, name: Epic, level: group, color: accent}
  - {key: task, name: Task, level: work, color: neutral, default: true}
vocabulary: {sprint: Iteration}    # only the words that are not types
```

## Words

| Where | Words |
|---|---|
| A group's type, anywhere | its own name from the catalog (Epic, Building, Course); never "group" alone |
| Settings → Types, the level column | **Holds work** (like an epic) · **Work** (like a task) |
| A refused run on a group | "Checkout is an epic: it holds work and is not run itself. Run its items, or plan it." |
| A refused move | "An epic cannot go under a task." · "That would put Checkout inside itself." · "Five levels is the most." |
| A refused sprint | "An epic does not go in a sprint: its items do." |
| Progress | "3 of 7 done" · "12 of 20 points" (the project's word for points) |
| Inherited routing | "Saas-build, from Checkout" |
| The Board's grouping | "Lanes: By parent · None · Role · Priority · Assignee"; the lane of work with no parent: "No parent" |
| Work split by a plan | "Split into 3 items" |

## The visual language every view shares (card E builds it; F, G, H only use it)

- **`TypeTile`** (`shared/ui/TypeTile.tsx`, replacing `TypeBadge.tsx`) draws the catalog's name and
  colour. A type that holds work is drawn as a stacked tile, and work as a flat one, so the level
  reads before the word.
- **`ProgressBar`** (`shared/ui/ProgressBar.tsx`) shows `Progress`, in points when it has estimates
  and in items otherwise, with "n need you" beside it when `needs_you > 0`.
- **`GroupHeader`** (`shared/ui/GroupHeader.tsx`) holds the ancestors (`ItemPath`), the `TypeTile`,
  the title as a link to the item page, a status chip that opens the status menu, the
  `ProgressBar`, the teams, a fold (`shared/ui/Fold.tsx`), **Add item** and **Plan with agent**. It
  has two densities, lane and heading, and it is the only way a group is drawn above its work.
- **`ItemPath`** (`shared/ui/ItemPath.tsx`) shows the ancestors as links, from `shared/tree.ts`'s
  `ancestorsOf`. `shared/ui/Breadcrumb.tsx` is the app's navigation and is not reused for this.
- **Every drag has a keyboard twin.** Moving under a parent is also the item menu's **Move under…**,
  a combobox (`shared/ui/ParentPicker.tsx`) filled from `GET /api/projects/{id}/parents`, so the web
  offers exactly the parents the server admits and never works the rule out itself.
- **Disabled, never hidden:** a control a group cannot use stays visible, disabled, with the reason
  from "Words".
- **Progressive disclosure:** a project with no group and no item with children looks as it does
  today: no lanes, no level filter, no Groups panel.

## Cards

### A — a project's types are a catalog

**Model:** Sonnet, the largest card. **Crates:** `tack-core`, `tack-db`, `tack-app`, `tack-api`,
`tack-cli`, `tack-runner` (records only), `tack-test-support`, `frontend` (only what keeps it
compiling).

**Builds:**

- **Core.** `tack-core/src/item_types.rs` holds `ItemTypeDef`, `Level`, `TypeColor`, `preset` and
  `check_catalog`. `vocabulary.rs` keeps only the words that are not types.
- **Migration `107_item_types`:**
  - It creates `item_types(project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE, key
    TEXT NOT NULL, name TEXT NOT NULL, level TEXT NOT NULL CHECK (level IN ('group','work')), color
    TEXT NOT NULL, sort_order INTEGER NOT NULL, default_work INTEGER NOT NULL DEFAULT 0, PRIMARY
    KEY (project_id, key))`, and a partial unique index on `(project_id) WHERE default_work = 1`.
  - For each project, it inserts the preset of its `project_type`, with each type's name taken from
    the project's vocabulary when the vocabulary renamed it.
  - It inserts every other `item_type` its items use, as work with the key as its name.
  - It rewrites `item_type = 'subtask'` to the project's default work key; those items keep
    `parent_id`.
  - It removes the six type keys (`epic`, `feature`, `task`, `subtask`, `bug`, `requirement`) from
    `projects.vocabulary` and `templates.vocabulary` with `json_remove`, and adds
    `templates.item_types TEXT NOT NULL DEFAULT '[]'`, filled from each template's project type with
    `json_group_array`.
  - All of it is SQL. The presets are a `VALUES` list of `(project_type, key, name, level, color,
    order, default_work)` joined to `projects`; the name is `COALESCE(json_extract(vocabulary, '$.'
    || key), name)`. This list is a frozen copy of `preset()` as of this migration, and a `tack-db`
    test compares the two, so a later change to `preset()` is a decision and not drift.
- **Store.** `repo/item_types.rs` holds the commands: insert, update, merge-then-delete in one
  transaction, and `seed(project, preset)`, called by `repo/projects.rs` where `vocabulary_for_type`
  is called today. `queries/item_types.rs` holds `of_project`. `Parent::ItemType` is added to
  `require_row` (`tack-db/src/repo.rs`): every item insert and type change checks that the key is in
  the catalog, inside its transaction. There is no foreign key on `items`, so the table is not
  rebuilt.
- **Default work type.** `hidden` items made by `repo/threads.rs` and `repo/triggers.rs` take the
  default work key.
- **Every reader of a type reads the catalog:**
  - `outline.rs`: `parse_item_type` is deleted, and a key is looked up in the board's catalog; a
    word it lacks is added as work and listed in the preview under `new_types[]`.
  - `outline/csv.rs` and `outline/bundle.rs`: `types:` is read and written; the vocabulary only
    carries terms.
  - `change_set.rs`: `anchor_type` becomes the default work type when the anchor is a group.
  - `recurrence.rs`.
  - `autonomy.rs` domains: an unknown key is refused when the project's automation is saved.
  - `tack-app`: `change_sets.rs` and `runs/start_run.rs` (`kind_of` reads the catalog's name).
  - `tack-api`: `items.rs`, `outline.rs`, `export.rs`, `templates.rs`, `import_github.rs` and
    `import_linear.rs` (the default work key), and `autonomy.rs`.
  - `tack-cli`: `cmd/board.rs`, `git.rs` (`type_prefix` reads the key), `mcp/items.rs`,
    `mcp/projection.rs`, `mcp/changes.rs` and `tools_list.json` (the `item_type` descriptions say
    "a key of the project's types").
  - `tack-runner/src/evidence.rs` and `harness/docket/records.rs`: an entry with no `type` sends
    none, and the board gives it the default work type, not its parent's.
- **Routes.** `handlers/item_types.rs` serves the four type routes of the shared shapes; the list
  carries how many items use each type (`queries/item_types.rs`, `in_use`). `Project` carries
  `item_types`. A new CLI command, `tack types <project>`, lists the catalog.
- **Web, only to compile and to read the catalog:**
  - The `ItemType` TS type is a string.
  - `useItemTypes()` in `shared/vocab/` reads `project.item_types`.
  - `TypeBadge` reads its name and colour from the catalog; E replaces it with `TypeTile`.
  - The create dialog's type select lists the catalog.
  - `VocabularyPanel.tsx` stops listing the six type keys.
  - `computeStats.ts`, `listRows.ts`, `SearchBar.tsx`, `Token.tsx` and `PlanTree.tsx` read the key
    as a string.

**Removes:** the `ItemType` enum and its `Custom` variant; `parse_item_type` (both copies:
`tack-core/src/outline.rs` and `tack-db/src/repo/items.rs`); the six type keys in `VOCABULARY_KEYS`,
`default_vocabulary` and `vocabulary_for_type`; in the web, `ITEM_TYPE_KEYS`, `ITEM_TYPE_META`,
`getItemTypeList`, `getItemTypeMap`, `typeKey` and `typeBadgeTone`'s switch.

**Docs:**

- `user-guide/vocabulary.md` becomes "Types and words": the catalog first, the other words after.
- `user-guide/import-export.md` and `developer/project-file.md` gain `types:`.
- In `developer/index.md`, the section "Hierarchy is free" is rewritten: one table, a type catalog
  with a level, and the tree rule of B.
- In `developer/crate-tour.md`, the `ItemType` and `VocabularyMap` sections are rewritten.
- In `developer/runner-contract.md`, plan-v1's `type` default becomes the default work type.
- `docs/MCP.md`, `user-guide/cli.md` (`tack types`; `-t` takes any key of the catalog), and
  `docs/ARCHITECTURE.md`'s `tack-core` bullet ("Item types and vocabulary").

**Tests:**

- `tack-core`: `preset` for every `ProjectType` passes `check_catalog`; `check_catalog` refuses two
  default types, a default that is a group, and a bad key.
- `tack-db`:
  - the migration on a database with an epic, a subtask with a parent, a custom type and a renamed
    vocabulary (the subtask is now `task` and keeps its parent; the custom type is work; the renamed
    word is the type's name);
  - `upgrade_from_beta9` passes;
  - inserting an item whose key is not in the catalog is `NotFound { entity: item_type }`;
  - merge-then-delete moves every item.
- `tack-api`: the four type routes; delete without `merge_into` while the type is used answers
  `TypeInUse`.
- The bundle round-trips `types:`, an outline with an unknown word lists it in `new_types`, and the
  CSV import does the same.
- Vitest: the create dialog lists a Construction project's types.

**Done when:**

- `git grep -nE 'ItemType::|enum ItemType|parse_item_type|ITEM_TYPE_META|typeKey\(' crates frontend/src`
  prints nothing.
- `git grep -n '"subtask"' crates frontend/src` prints only the migration and its test.
- The generated files are regenerated, and `.githooks/pre-push` passes.

### B — one rule for the tree, and moving an item

**Model:** Sonnet. **Crates:** `tack-core`, `tack-protocol` (error codes), `tack-db`, `tack-app`,
`tack-api`, `tack-cli`.

**Builds:**

- **Core.** `tack-core/src/hierarchy.rs` holds `Node`, `node`, `MAX_DEPTH`, `Placement`, `Target`,
  `admit_parent`, `admit_sprint` and `ParentGate` (shapes above). `impl ParentGate for Hierarchy` is
  a unit struct that calls `admit_parent`. The new `CoreError` variants are `GroupUnderWork`,
  `ParentLoop`, `TreeTooDeep`, `ParentInOtherProject` and `GroupInSprint`, each with a
  `StableErrorCode` (422) and the sentence from "Words".
- **Store:**
  - `UpdateItem` gains `parent_id: Option<Option<Uuid>>`.
  - `update_item_in` (`repo/items.rs`) takes a `parent_gate: &dyn ParentGate` beside `gate`. When
    `parent_id` changes, it reads `Placement` (the item's level, its project, the height of its
    subtree by a recursive CTE, its sprint) and `Target` (the parent's level, project, ancestors and
    depth) inside its `BEGIN IMMEDIATE`, and asks the gate.
  - When `sprint_id` is set, it asks `admit_sprint` with the item's node.
  - `insert_item` asks the same gate for a new item's parent; a new item has no subtree.
  - `create_outline_items` and `apply_change_set` go through these same functions; their callers
    pass `&Hierarchy`.
  - `set_item_parent` is deleted, and its one caller uses the gate path.
- **Change sets.** `tack-core/src/change_set.rs` gains `Operation::Parent { item, parent:
  Option<ItemRef> }`. Its simulation applies `admit_parent` over the `BoardView`, so a set that
  would break the tree is refused with every reason before anything is kept.
- **MCP:** `update_item` gains `parent_id`, and `propose` documents `parent`
  (`tools_list.json`, `mcp/items.rs`, `mcp/changes.rs`).
- **Which parents are allowed.** `GET /api/projects/{id}/parents?item=<id>` (for a move) or
  `?type=<key>` (for a new item) answers every item that `admit_parent` admits, in tree order. It
  is one read in `queries/items.rs` (each candidate's level, depth and ancestors in one recursive
  CTE) filtered by the core rule. It lives in `handlers/items.rs`; every picker in the web reads
  it.
- **CLI:** `tack move <item> [status] --under <item>` or `--top` (`main.rs`'s `Move`,
  `cmd/board.rs`'s `cmd_move`).

**Removes:** `Repository::set_item_parent`; the comment in `frontend/src/shared/types/index.ts`
that says re-parenting is not supported (only the comment; F removes the toast).

**Docs:**

- `user-guide/items.md` gains a short paragraph "Moving an item under another", with the rule in a
  person's words.
- `docs/MCP.md` and `user-guide/cli.md` gain the new parameters.
- `developer/crate-tour.md` gains `hierarchy.rs`, and `docs/ARCHITECTURE.md`'s `tack-db` bullet on
  `update_item_atomically` names the second gate.

**Tests:**

- `tack-core`: one test per pair of levels (group under group, work under group, work under work,
  group under work refused); a loop of one, a loop of three, depth 5 allowed and 6 refused, another
  project refused, a group in a sprint refused, and work with children counts as a group.
- `tack-db`: moving under a parent and back to the top; a move refused inside the transaction leaves
  the row and its `version` unchanged.
- `tack-api`: `PATCH parent_id` answers each code; a change set with a loop is refused whole;
  `/parents` for a task leaves out the task's own subtree, and `/parents?type=epic` lists only
  groups.
- `tack-cli`: `--under` shapes the request.

**Done when:** every refusal of the "Words" table has a code and a test, and `git grep -n
set_item_parent crates` prints nothing.

### C — work runs; a group is planned

**Model:** Sonnet. **Crates:** `tack-core`, `tack-db` (queries), `tack-app`, `tack-api`, `tack-cli`
(MCP runs), `frontend` (`shared/runWithAgent/` only). **Runs beside D**; it does not touch
`models.rs`'s `ItemCard`, `repo/items.rs`, `queries/children.rs` or `handlers/items.rs`. Both C and D
touch `router.rs`: C adds one route, and D removes the `/boards` routes; the second to merge keeps
both edits.

**Builds:**

- **Core.** `hierarchy::runnable` and `CoreError::GroupNotRunnable`, with its code (409).
  `run_defaults::resolve` takes `chain: &[ChainLink]`, and `Resolved` gains `from` (shapes above).
  Every caller passes the chain.
- **Store.** In `queries/items.rs`:
  - `routing_chain(repo, item_id)` returns the item and its ancestors with `run_settings` and roles,
    in one recursive CTE.
  - `routing_facts(repo, project_id)` returns every item's `parent_id`, `run_settings` and roles,
    for the project-wide router; `hierarchy::chain(item, by_id)` builds the chains in core from
    them.
  - `waiting_with_someone_to_do_it` leaves out groups in SQL: its type's level is `group`, or a
    non-hidden child exists. Its "someone to do it" test climbs the ancestors through the same CTE.
- **App:**
  - `runs/start_run.rs`: before a request is created, `resolve_profile` reads the chain, and the
    item's node is checked with `runnable(node, profile.kind)`. A refusal creates nothing and
    answers `GroupNotRunnable`.
  - `auto_start.rs` keeps using `pick`; it never sees a group, because the query left groups out.
  - `change_sets.rs`'s `who_does_it` on a group is allowed: it names who does the group's work.
- **API.** `handlers/routing.rs` serves `from` for each item (`{ id, title }`), and answers
  `GET /api/items/{id}/run-options` (new): each profile, and whether it may run this item, with the
  reason. Both read `runnable` and `resolve`; neither copies them.
- **Web:**
  - `shared/runWithAgent/ProfilePicker.tsx` and `RunWithAgentModal.tsx` read `run-options`: on a
    group, only Planner and Researcher profiles can be picked, the others are disabled with the
    reason, and the primary button reads **Plan with agent**.
  - `WhoDoesIt.tsx` shows "…, from" and the ancestor's title when `from` is set.
- **MCP.** `create_execution` answers the same refusal.

**Removes:** nothing outside this card's files; F, G and H delete their own copies.

**Docs:**

- `user-guide/running-agents.md` gets "An epic is planned, its items are run", and
  `user-guide/automation.md` gets "auto-start starts work, never a group" and the inherited "who
  does it".
- `developer/runs-and-teams.md` and `crate-tour.md` (`run_defaults`) are updated.

**Tests:**

- `tack-core`: `runnable` for each node and kind; `resolve` with a chain where the parent names a
  profile, where the item overrides it, and where the grandparent's roles disagree (the project
  default).
- `tack-app`:
  - auto-start on a project whose epic has a role and sits in the start status starts its two tasks
    and never the epic;
  - `start_run` with an Implementer on an epic answers `GroupNotRunnable` and creates no request;
  - a Planner on an epic creates one, and accepting its plan puts the children under the epic.
- `tack-api`: `run-options` on a group and on work; `/routing` names `from`.
- Vitest: the Run dialog on a group.

**Done when:** a task with no role of its own, under an epic whose role names a profile, is routed,
auto-started and drawn on Flow in that profile's lane.

### D — the facts every screen reads, and one board

**Model:** Sonnet. **Crates:** `tack-core` (`models.rs`, `children.rs`), `tack-db`, `tack-api`,
`tack-cli`. **Runs beside C**; it does not touch `run_defaults.rs`, `start_run.rs` or
`auto_start.rs`.

**Builds:**

- **Core.** `ItemCard` gains `node` and `progress`, and `children_progress` and `ChildrenProgress`
  are replaced by `Progress` (shapes above). `children::progress(work: &[Work], workflow) ->
  Option<Progress>` counts over the work under a group at any depth.
- **Store:**
  - `queries/children.rs` reads, in one recursive CTE per project, for every group, the facts of
    each work item under it: status, estimate, start (`started_at`, else `created_at`), due, team of
    the latest run, open run, and needs-you. `of_parent` does the same for one subtree.
  - `list_items` and `get_item_snapshot` fill `node` with `hierarchy::node(level, has_children)`;
    the level comes from a join on `item_types`, and `has_children` from `EXISTS` over non-hidden
    children.
  - `ItemFilter` gains `node` and `under` (the subtree, by the same CTE).
- **API:**
  - `handlers/items.rs`: `progress_by_parent` becomes `progress_by_group`, and `ItemNode`'s
    `children_progress` becomes `progress`.
  - `GET /api/projects/{id}/items?node=&under=`.
  - The saved boards are deleted: a migration `108_drop_boards` runs `DROP TABLE boards`, which
    takes the snapshot by itself, and `ALTER TABLE templates DROP COLUMN default_boards`.
- **CLI.** `tack board` reads `GET /api/projects/{id}/items?node=work` and prints one column per
  workflow status; `--by parent` prints a block per group with its progress line.

**Removes:** in `tack-core/src/models.rs`, `Board`, `CreateBoard`, `UpdateBoard`, `BoardGrouping`,
`BoardView` and `BoardColumn`; `tack-db/src/repo/boards.rs`; `tack-api/src/handlers/boards_multi.rs`
with its `BoardViewResponse` and `BoardColumnWithItems`, and their entries in `openapi.rs`; the routes `/projects/{id}/boards`, `/boards/{id}` and
`/boards/{id}/view`; `default_boards` in templates (`repo/templates.rs`, `handlers/templates.rs`,
`tack-cli/src/cmd/entities.rs`); `ChildrenProgress`. The live socket
`/projects/{id}/boards/live` stays: it carries every event of the project.

**Docs:**

- `docs/ARCHITECTURE.md`'s "An item is two types" paragraph names `node` and `progress`.
- `developer/crate-tour.md`.
- `user-guide/cli.md` (`tack board --by parent`).
- `developer/api-reference.md` is regenerated.

**Tests:**

- `tack-core`: `progress` with points, without points, with nested work, and with a hidden child.
- `tack-db`:
  - `node` for a group type with no children, for work with children, and for work alone;
  - `under` returns the whole subtree;
  - `perf_test` seeds its 50 000 items as 1 000 groups of 49 work items, three levels deep, and
    `list_items` stays within its current p95 budget with `node` and `progress` filled.
- `tack-api`: `GET /api/boards/{id}` answers 404, and `?node=work` leaves the epics out.
- `tack-cli`: `tack board` against a `wiremock` list.

**Done when:** `git grep -nE 'BoardGrouping|boards_multi|default_boards|children_progress' crates
frontend/src docs` prints only `CHANGELOG.md` and closed plans.

### E — the Board shows work in lanes, and the pieces every screen shares

**Model:** Sonnet. **Crates:** `frontend` (`shared/ui`, `shared/tree.ts`, `shared/api/boards.ts`,
`features/board/Board.tsx` and its tests). It runs alone in its round, because F, G and H build on
what it adds to `shared/`.

**Builds:**

- **The shared pieces** of "The visual language" above: `TypeTile`, `ProgressBar`, `GroupHeader`
  and `ItemPath`, plus `shared/tree.ts` (`ancestorsOf(item, byId)` and `itemsUnder(group, items)`,
  both pure and tested).
- **The Board:**
  - Its cards are `node === 'work'`. The column counts and WIP limits count work only.
  - **Lanes** are the default whenever an item with `node === 'group'` exists. The toolbar choice
    ("Lanes: By parent · None · Role · Priority · Assignee") is remembered per project in this
    browser, through `shared/state/display.ts`.
  - A lane per parent shows its `GroupHeader` (lane density) over a row of columns.
  - Lanes are ordered as columns are, needs-you first (`needs_you`, then open runs, then the rest by
    the parent's `sort_order`). A lane whose group is in a done-category status starts folded. Work
    with no parent goes last, in "No parent".
  - An empty group shows one row with **Add item** and **Plan with agent**.
  - Dragging a card into another lane PATCHes `parent_id`; a refusal shows the server's sentence
    inline on the lane, and the card returns to where it was. The card menu gains **Move under…**.
  - A card shows its `ItemPath` only when the board is not in lanes.
- The Board keeps reading the project's items once (`shared/state/projectItemsContext.tsx`) and
  splits them by `node`: work becomes cards, groups become lane headers. It sends no second request.
- **Move under…** on a card and on a lane uses `ParentPicker` (`shared/ui/ParentPicker.tsx`, new
  here) over B's `/parents`.

**Removes:** `deriveBoard`, `BoardState`, `BoardColumn` and the `boards` export in
`shared/api/boards.ts` (the file goes); `Board.tsx`'s `parentTitleOf` and its parent line under the
card; `TypeBadge.tsx` (every caller moves to `TypeTile`).

**Docs:** `user-guide/views.md` §Board is rewritten for lanes, the grouping choice, moving a card
under another group, and the empty group. `developer/frontend.md` names the shared pieces (`TypeTile`, `ProgressBar`, `GroupHeader`,
`ItemPath`, `ParentPicker`, `shared/tree.ts`).

**Tests (Vitest):** lanes render from a fixture with two epics and loose work; the grouping choice
persists; a drag across lanes sends the PATCH and rolls back on a 422; an empty group shows its two
actions; a project with no groups renders no lane header; `ancestorsOf` and `itemsUnder` are tested
in `shared/tree.test.ts`. The live spec's board locators (`onboarding-lab/e2e/docket-prill.spec.ts`,
git-ignored, so the integrator updates them) are listed in the card's note.

**Done when:** `git grep -nE 'deriveBoard|TypeBadge|parentTitleOf' frontend/src` prints nothing.

### F — the List is the tree; the item page says what the item is

**Model:** Sonnet. **Crates:** `frontend` (`features/list/`, `features/item-detail/`,
`features/board/CreateItemModal.tsx`).

**Builds:**

- **The List:**
  - A group's row has a stacked `TypeTile`, its title in semibold, and a `ProgressBar` in the Work
    column instead of a run chip.
  - Dropping a row onto another moves it under that row, and the row menu has **Move under…**; both
    PATCH `parent_id`, and a refusal shows its sentence.
  - "Show: Everything · Work · Groups" appears when the project has groups.
  - **+** on a row adds a child of the default work type.
- **The item page** (`ItemHeader.tsx`, `ItemPage.tsx`, `ItemBody.tsx`):
  - `ItemPath` is at the top.
  - A group's page leads with a **Progress** fold that is open by default: its `ProgressBar`, its
    work grouped by Work state as the List shows it (from `GET …/items?under=`), **Add item** and
    **Plan with agent**.
  - Its run section is C's dialog.
  - Work with children shows "Split into n items" above its runs, and the run section reads like a
    group's.
  - The header's menu has **Move under…**.
- **The create dialog** (`CreateItemModal.tsx`) lists the types grouped as "Holds work" and "Work",
  always has a **Parent** field (`ParentPicker` with `?type=` set to the chosen type), and is
  opened with the parent preset from a lane's or a row's **Add item**.

**Removes:** List's `handleMove` toast ("Re-parenting items isn't supported yet"); `ItemHeader.tsx`'s
`children()` filter and `parent()` lookup (from `progress` and `ItemPath`); `CreateItemModal.tsx`'s
own child list, and its two `item_type: 'subtask'` creations (children take the default work type).

**Docs:** `user-guide/views.md` §List, and `user-guide/items.md` (the item page of a group, "Split
into", **Move under…**).

**Tests (Vitest):** a group row and a work row are drawn differently; a drop and **Move under…**
both PATCH, and a 422 shows the sentence; a group's page shows Progress first and its run choices
from a `run-options` fixture; the create dialog offers only allowed parents.

**Done when:** `git grep -nE "Re-parenting items isn't supported|'subtask'" frontend/src` prints
nothing.

### G — the views of time and effort

**Model:** Sonnet. **Crates:** `frontend` (`features/timeline/`, `features/calendar/`,
`features/sprints/`, `features/dashboard/`).

**Builds:**

- **Timeline:**
  - A group is a summary bar from `progress.starts` to `progress.ends`, drawn with its `TypeTile`,
    with its work nested under it and folding.
  - The group's own `due_date` is a diamond on the bar, and the bar turns the danger colour when
    `ends` is after that date.
  - A summary bar cannot be dragged, and its cursor says so; work bars drag as today.
  - Rows follow the tree order of `shared/tree.ts`.
- **Calendar:** work items by due date as today; a group's due date is a milestone chip with its
  `TypeTile`, which cannot be dragged.
- **Sprints:**
  - The backlog and every sprint list only work.
  - The backlog is set out under `GroupHeader`s at heading density, then "No parent".
  - Dropping a group on a sprint is not offered, because groups are not draggable; a server refusal
    (`GroupInSprint`) is shown if it comes back.
- **Dashboard** (`computeStats.ts`, `Dashboard.tsx`): counts, points and the breakdown by type cover
  work only, and a **Groups** panel lists each top-level group with its `ProgressBar`, ordered by
  `needs_you` and then by how little is left.

**Removes:** the Timeline's flat sort of every item, and `computeStats`' count of groups as work.

**Docs:** `user-guide/views.md` §Timeline, §Calendar, §Sprints and §Overview.

**Tests (Vitest):** a summary bar spans its work and is red past its due; Calendar renders a
milestone for a group; Sprints' backlog headings and no group card; `computeStats` over a fixture
with one epic and three tasks counts three.

**Done when:** each view's test file pins its level.

### H — orchestration, search and Settings → Types

**Model:** Sonnet. **Crates:** `frontend` (`features/orchestration/`, `features/settings/`,
`features/templates/`, `features/projects/CreateProjectModal.tsx`, `shared/ui/SearchBar.tsx`,
`shared/ui/CommandPalette.tsx`).

**Builds:**

- **Flow and Map** (`flow.ts`, `map.ts`, `Token.tsx`, `Lane.tsx`):
  - A token's item is work, read from `node`.
  - A group's planning run is a token in its lane, marked **Planning**, with its `TypeTile`.
  - A token's subtitle is its `ItemPath`.
- **Search and the command palette** show each result's `TypeTile` and `ItemPath`.
- **Settings → Types** (`settings/panels/TypesPanel.tsx`, in the settings navigation beside
  Vocabulary):
  - one row per type: name, the level as **Holds work** or **Work** (the choice is disabled with its
    reason while the list's `items` count is above 0), how many items use it, a colour token, the
    default work type as a radio, and an order handle;
  - **Add type**;
  - **Remove**, which opens "Move its n items to…" before it removes.
- **The Vocabulary panel** lists only the other words.
- **The autonomy domains'** "Item types" become a multi-select over the catalog
  (`automation/Merge.tsx`).
- **Templates and the create-project dialog** show the template's or preset's types under its name.

**Removes:** `flow.ts`'s `hasChildren` and `parents` sets, and the `startable` check that used them;
`Token.tsx`'s emoji; `Merge.tsx`'s free-text list of types.

**Docs:** `user-guide/vocabulary.md` (the panel; A wrote the page), `user-guide/views.md`
§Orchestration, and `user-guide/command-palette.md`.

**Tests (Vitest):** Flow draws no token for a group's own item except its planning run; the Types
panel adds a type, refuses changing the level of a used type with its reason, and removes by
merging; the domains picker lists the catalog.

**Done when:** `git grep -nE 'hasChildren|parents\.has' frontend/src/features` prints nothing, and
no screen reads `parent_id` to decide whether something is a group (`git grep -n "parent_id" frontend/src/features`
matches only moves, creations and `shared/tree.ts` callers).

## Rounds and integration

| Round | Cards | Integrator checks after the round |
|---|---|---|
| 1 | A | full suites; pre-push; A's two greps |
| 2 | B | full suites; pre-push; `set_item_parent` grep |
| 3 | C ∥ D | merge C then D (D rebases, keeps both `CHANGELOG.md` lines, reruns `regen-generated.sh`); full suites; pre-push; D's grep |
| 4 | E | Vitest; pre-push; E's grep |
| 5 | F ∥ G ∥ H | merge in any order; Vitest; pre-push; F's and H's greps |

Each round runs `cargo nextest run --workspace`, `cd frontend && npm test`, and
`.githooks/pre-push`. After round 5, the integrator:

- updates the git-ignored live suite (`onboarding-lab/e2e/docket-prill.spec.ts`) and the
  `onboard-project` skill's references where they describe epics on the board (the skill already
  creates epics with one task each; they now read as lanes, and a role set on an epic routes its
  tasks);
- runs `onboarding-lab/e2e/docket-prill-e2e.sh` and its `--check`.

## Acceptance, end to end

On a fresh board, with a Software project and a runner reporting `claude-code`:

1. **Creating items.** Create an epic "Checkout" with role Build, whose profile is an Implementer on
   `claude-code`, and three tasks under it with no role. Create one loose bug.
   - The Board shows a lane "Checkout" with "0 of 3 done" over the three task cards, and "No
     parent" with the bug.
   - The epic is not a card, and the column counts are 4, not 5.
2. **Auto-start.** Switch auto-start on, at the start status.
   - The three tasks start, routed "Build, from Checkout"; the epic never does.
   - Flow draws three tokens in Build's lane.
3. **Runs on the epic.** Press Run on the epic.
   - Only Planner and Researcher can be picked, and the button reads **Plan with agent**.
   - A Planner run proposes two items; accepting them puts them under the epic, and the lane reads
     "0 of 5 done".
4. **Moves and refusals.**
   - Drag a task into "No parent"; it leaves the lane.
   - **Move under…** the bug into one of the tasks: the bug is now under the task, and the task reads
     "Split into 1 item". Its Run offers only planners, and the bug's card stands in a lane whose
     header reads "Checkout ›" followed by that task's title.
   - Try to move the epic under the bug: refused with "An epic cannot go under a task."
5. **Other views.**
   - On the Timeline, Checkout is a summary bar over its work.
   - The Dashboard counts five work items (the task that holds the bug is no longer work) and shows
     Checkout's bar.
   - Settings → Types renames Epic to "Initiative", and every screen says Initiative.
6. **Files and the CLI.**
   - The project file exports `types:`, and importing it into a new project rebuilds the same tree.
   - `tack board --by parent` prints the lanes.
7. **The live suite** passes with its locators updated.

## Not in this phase

- Status rules: a group still does not move by itself (ADR 0036 decision 7, Phase 86).
- A list of allowed children per type.
- Saved views with filters.
- Roll-up of custom fields.
- Cross-project parents.
- A Roadmap view apart from the Timeline.
- Phase 76's sync of the project file: `types:` is imported additively, like the rest of the file.
