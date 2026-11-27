# Plan: Phase 76 — the plan lives in the repository

**Status: open 2026-11-26** (the maintainer: "abre la fase 76"). Serves ADR 0025 and its amendment
of 2026-11-26 (`docs/adr/0025-the-project-file-is-kept-in-sync-not-only-imported.md`), decisions 1
to 16 with the amendment's four defaults, and ADR 0033's amendment of the same day (decisions 6 to
12), which the screens' asks below serve. The maintainer accepted both and put the whole phase in
the prepared release, `v0.2.0-beta.1` (2026-11-26: "todo esto va en la release programada"). Written against `develop` at `b62c8d00`; every path and
line below was read that day. Re-run `rg -n` before trusting a line number. The earlier version of
this plan (2026-10-18, configuration only) is in the history of this file.

What this phase does, in one paragraph. A project's plan (its configuration and, for each item,
what the item is and what makes it done) is kept in a `.tack/` folder beside the code:
`project.yaml` plus one `items/<key>.md` per item. A person exports the folder, edits it or lets an
assistant edit it, commits it, and imports it again; the board follows. Every role, field, sprint,
profile and item has a key that survives a rename. An import compares the folder, the board and the
last revision, so what the folder removed goes (when nothing happened to it), what the board changed
is kept, and a field both changed is a conflict. Every export and import leaves a revision, the
preview's hash travels to the apply, and every import of a project file is written in one
transaction. Where an item is, who answers for it and when, and everything a run leaves, stay on the
board.

**Two sessions, one phase.** This plan's tasks are the API and everything under `crates/`; this
session integrates them. The screens are the frontend session's (section "The screens" at the end):
the Project folder card, and the views redefined around the board as the place that says where each
thing is and what needs a person. Neither session edits the other's files. A screen that needs a
fact the API does not serve asks for it (see "When the screens need the API").

## The rule for every task: one owner per fact

| Fact | Its one owner after this phase | Callers that stop computing it |
|---|---|---|
| A thing's key from its name or title | `tack_core::outline::key::key_of` | — (new) |
| The three-way comparison | `tack_core::outline::bundle::sync::merge`; the two-way `diff` beside it | — (new); the status route reads `diff` |
| Which board row a file entry is | `bundle::match_rows` (today `match_names`, `bundle.rs` line 220): by key, then by name | — |
| The folder as files | `tack_core::outline::folder::{read, write}` | — (new); the CLI only reads and writes the files it is given |
| What a project's plan is as a file | `handlers::export::bundle_of(state, project, items)` | `bundle_response` and `templates::save_project_as_template` (its own `BundleAutonomy` mapping, `templates.rs` line 399, goes) |
| What a file is checked against | `handlers::outline::board_of(state, project)` | `into_existing_project` builds it inline today |
| Writing a checked plan | `Repository::write_project_file(&ProjectWrite)` in a new `crates/tack-db/src/repo/project_file.rs`, one `BEGIN IMMEDIATE` (the precedent is `apply_change_set`, `change_sets.rs` line 372) | `handlers/outline.rs` `write`, `write_profiles`, `write_automation` stop calling the repository one row at a time; `create_new`'s compensating `delete_project` (line 419) and `SectionFailed.kept` (`error.rs` line 64) go |
| A row's SQL on a caller's connection | a `pub(crate) async fn <name>_in(conn: &mut SqliteConnection, …)` in its repository module, the pool method a one-line wrapper (the precedent is `insert_item`, `items.rs` line 678) | — |
| What a removal costs, and whether an item has history | `tack_db::repo::project_file::usage_of(conn, …)` | — (new) |
| The revisions | `tack_db::repo::project_file::{last_revision, record_revision_in, list_revisions, revision}` | — (new) |
| The plan's hash | `handlers::folder::plan_hash` | — (new) |
| The level of a change that touches `.tack/` | `tack_core::autonomy::level_for` (line 75) | — |
| Whether a task can be verified ("Not ready" and why) | `tack_core::brief::readiness` | — (new); the screens read the answer, never the rule |
| Whether auto-start may start one more run | `tack_core::auto_start` (its `Limits` and `Skip`, line 29 and 121) | — |
| What waits for a person in a project | the inbox's open `needs_you` notifications (`tack_core::notification`, ADR 0035) | — |

No task adds a layer, a trait, a loop, a table or a setting the task does not name. A new number
is a named constant beside the code that reads it, never a config key.

## Decided — not re-opened by any task

Every row of ADR 0025's two tables and the amendment's four defaults, plus:

| Question | Decision |
|---|---|
| Migrations | Three, in the style of the several-statement migrations (`apply_migrations` runs one name in one transaction, `migrations.rs` line 385). `115_keys`: for each of `roles`, `custom_field_definitions`, `sprints`, `agent_profiles`, `items`: `ADD COLUMN key TEXT NOT NULL DEFAULT ''`, then a partial unique index `CREATE UNIQUE INDEX idx_<table>_key ON <table>(project_id, key) WHERE key <> ''` (`agent_profiles(key)`, a profile is the board's). `116_waiting_max` (task N): `ALTER TABLE project_budgets ADD COLUMN waiting_max INTEGER` (null is no limit), beside `queue_items_max`. `117_plan_revisions` (task S): the revisions table below. No key is computed in SQL: SQLite's `lower` is ASCII-only and it cannot apply `key_of`. |
| Filling the keys | `migrations::run_all` (line 184), after `apply_migrations`, calls `crate::repo::project_file::fill_missing_keys(pool)`: every row of the five tables whose `key` is `''`, except hidden items, gets `key_of(name or title)`, made unique within its project with `-2`, `-3`, …, in creation order, in one transaction. It is idempotent, so a second start finds nothing to do. Every create path writes a key: `create_role`, `create_field`, `create_sprint`, `write_agent_profile` (a new profile), and `insert_item` (`items.rs` line 678, the one insert of a visible item). The hidden items of threads (`threads.rs` line 128) and triggers (`triggers.rs` line 124) keep `''` and are never in a folder. |
| The key | `key_of(text)`: trim; lowercase (Unicode); every run of characters that are not alphanumeric (`char::is_alphanumeric`) becomes one `-`; leading and trailing `-` removed; cut to 64 chars at a char boundary, then trailing `-` removed again; empty → `unnamed`. `is_key(text)`: non-empty, at most 64 chars, lowercase, only alphanumerics and `-`, no leading or trailing `-`. A key in a file that **creates** a row must pass `is_key` or the file is refused naming the entry; a key that names an existing row is matched as written. |
| Matching | One rule in `match_rows`, for the five keyed kinds. File entry has `key` → the board row with that key; none → create with that key, unless a row of another name holds it (refused: `roles: key "dev" is already the key of "Dev Team"`); a row with the same name but another key → refused (`roles: "Dev" is "dev-team" on the board; write that key or drop the name`). File entry has no `key` → the row with that name (roles, fields, sprints, profiles) as today, else create with `key_of(name)`, refused if taken. **Items** without a key are always created, as ADR 0017 says; an item with a key the board has is **updated** in its plan fields (decision 9), never moved or removed by `tack import`. So a file exported and imported twice no longer doubles its items. |
| What an item's plan is | `title`, `description`, `type`, `parent` (a key), `depends_on` (keys, compared as a sorted set), `roles` (sorted), `tags`, `estimate`, `estimate_unit`, `recurrence`, `fields`, `brief`. The board's keys are `status`, `priority`, `assignee`, `sprint`, `due_date`: read for a new item, ignored for one that exists (a `Keep` with the reason "the board's"), never written by `folder::write`. `ref` stays for hand-written single files and is never written by an export, which names parents and blockers by key. |
| The brief in a file | `OutlineItem.brief: Option<OutlineBrief>` with `definition_of_done`, `risk`, `acceptance`, `constraints` in brief-v1's shape (`docs/contracts/brief-v1/`), without `item_id`. Checked with `tack_core::brief::validate` (`brief.rs`) as the Brief tab is. Written through the brief's own write on the caller's connection (`upsert_item_brief_in`, from `briefs.rs` line 93), so every change is a version with its hash. A brief whose four parts are empty is no brief. |
| The folder | `.tack/project.yaml`: the `Outline` with `items` empty, as the single file writes it. `.tack/items/<key>.md`: `---\n` + the item's plan fields but `description` and `key`, as YAML + `---\n` + the description as Markdown (nothing when it has none). The key is the file name without `.md`, and a file whose name fails `is_key` while creating is refused naming the file. `read(files: &BTreeMap<String, String>) -> Result<Outline, Vec<String>>` takes paths relative to `.tack/`; any other path is a problem; items come back in key order and `check` orders them parents first. `write(&Outline) -> BTreeMap<String, String>` writes every visible item, in key order, without the board's keys. `serde-saphyr` (already the workspace's YAML) becomes a dependency of `tack-core`. |
| The revisions table | `project_plan_revisions (project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE, number INTEGER NOT NULL, origin TEXT NOT NULL CHECK (origin IN ('board','file','revert')), hash TEXT NOT NULL, body TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT (datetime('now')), PRIMARY KEY (project_id, number))`. `body` is the outline the folder carries (the configuration plus each item's plan, no board keys) as canonical JSON (maps sorted, items in key order); `hash` is its SHA-256 in hex. |
| What `merge` compares | `project` (one entry, fields `name`, `description`, `vocabulary`; `type` and `workflow` reported kept with their reason), `types`, `fields`, `profiles`, `roles`, `sprints`, `automation` (one entry), `items` (their plan fields). `teams` is never compared (decision 16). Objects are compared by dotted path (`autonomy.budgets.tokens_per_day`, `brief.risk`); arrays and scalars are leaves, except `depends_on` and `roles`, compared sorted. |
| The three-way rule, per field at path `p` of an entry on both sides | `board == file` → that; `base == board` → the file's (a change); `base == file` → the board's (kept); else a `Conflict`, listed whether or not `prefer` settles it (so the plan shows what was chosen), and the merged outline takes `prefer`'s side, or the board's when there is none. `applies` is `conflicts.is_empty() || prefer.is_some()`. With no base entry: `board == file` → that, else a conflict. |
| The three-way rule, per entry | In the file, not on the board: in base and `file == base` → kept absent ("removed on the board since the last revision"); in base and `file != base` → a conflict (`prefer=file` creates, `prefer=board` keeps absent); not in base → create. On the board, not in the file: in base and `board == base` → a removal candidate; in base and `board != base` → a conflict (`prefer=file` removes, `prefer=board` keeps); not in base → kept ("created on the board since the last revision", or "no revision yet: export once to make the folder authoritative"). `merge` is pure: it proposes removals; the route turns each into a removal or a kept row with the reason, from `usage_of`. |
| What a removal costs, and what is refused | A role: the cards that lose it (`item_roles`). A field: the values deleted with it (`custom_field_values`). A sprint: the cards that leave it (`items.sprint_id`). A type: refused when in use, by the catalogue (`item_types.rs` `set_item_types`). A profile: refused when a role of another project or another project's default names it, or a run of it is not terminal. An item: **kept** with the reason when it has a row in `execution_requests`, `comments`, `attachments`, `change_sets` (`anchor_item_id`), `github_links`, or visible children (`items.parent_id`); otherwise removed, and its cost is its blockers (`dependencies`). A built-in profile is never a candidate (`to_bundle` leaves built-ins out). |
| The project's own settings | `name`, `description` and `vocabulary` through `update_project_in`; `type` and `workflow` never written, reported kept. |
| The plan's hash | `sha256(the merged body ‖ "\n" ‖ the board's body hash ‖ "\n" ‖ the removals as "section/key" lines ‖ "\n" ‖ prefer)`, hex. The apply recomputes it from the request and the live board; a mismatch is 409 `{"error": "The project changed since the preview; preview again"}`. |
| The apply, in order, in one `write_project_file` transaction | record a `board` revision when the live body's hash differs from the last revision's; the removals (profiles, then items, roles, fields, sprints; the foreign keys already cascade or set null); the creates and updates of the merged plan, sections in the file's order, items parents first; the briefs; the project's three keys; a `file` (or `revert`) revision whose body is the merged plan; commit; then `BoardConfigUpdated { project_id }` (`websocket.rs` line 39) and one `ItemCreated`, `ItemUpdated` or `ItemDeleted` per item touched. |
| An export records a revision | `GET …/folder` and `GET …/export?scope=bundle` record a `board` revision when the hash differs from the last. `scope=snapshot` and `format=csv` do not. |
| Routes | `GET /api/projects/{id}/folder` → `{ files: {path: text} }` (the export). `GET …/folder/status` → `{ last_revision: RevisionSummary \| null, board_hash, changes: [Change] }` (`diff(last.body, board)`). `POST …/folder/plan` and `POST …/folder/apply`: body `{ files: {path: text} }` or `?revision=<n>` with no body; `prefer=file\|board` optional; apply also needs `plan_hash=<hex>`. `GET …/folder/revisions` → `[RevisionSummary]` newest first; `GET …/folder/revisions/{n}` → `{ files }`. |
| Who may call them | Reading routes and `plan`: any principal that may read the project. `apply`: `PrincipalKind::Operator` only (`tack_core::principal`); an agent's credential answers 403 `{"error": "An agent may preview the project folder; a person applies it"}`. Handlers read `Extension<Principal>` as `handlers/change_sets.rs` line 207 does. |
| The plan's answer | `FolderPlan { plan_hash, base: Option<RevisionSummary>, changes: Vec<Change>, conflicts: Vec<Conflict>, removals: Vec<Removal { section, key, name, cost: Vec<String> }>, warnings: Vec<String>, applies: bool }`; `applies` is false while a conflict has no `prefer`. Anything `check` refuses is a 400 with every problem, as today. The apply answers `FolderApplied { revision: RevisionSummary, changes, removals }`. |
| A key on the wire | `Role`, `Sprint`, `CustomFieldDefinition`, the profile response and `Item` gain `key: String`. `CreateRole`, `CreateSprint`, `CreateCustomField`, the profile create body and `CreateItem` gain `key: Option<String>` (checked with `is_key`; default `key_of`, made unique; 409 when a given key is taken). No update route changes a key. |
| A change under `.tack/` | `level_for`: when any path starts with `.tack/`, the answer is `(L3, Some("project folder"))`, before the domains are read. The name is the constant `PROJECT_FOLDER_DOMAIN`. And `.tack` is one of `tack_protocol::checked_files`'s directories, so the files are listed under "The agent changed how it is checked" and the Merge button asks for a reason (built with task A). |
| The CLI | `tack folder <status \| export \| import \| history \| revert> <project> [dir]`, `dir` defaulting to `.tack` in the current directory. `export` writes the files the route answers and removes `items/*.md` files the answer does not have; `import` and `revert` print the plan and apply unless `--dry-run`; a plan with removals applies only with `--yes` (else exit 1, "add --yes to apply the N removals"); conflicts without `--prefer` exit 1; `--json` everywhere. `tack import` and `tack export --bundle` keep their meaning. `tack config` stays the CLI's own settings. |
| The MCP tool | `plan_folder { project_id*, files (object) \| revision (number), prefer }` → the plan, never an apply. `create_outline`'s description gains: "It never removes anything; a person keeps the plan in the repository with `tack folder`." |
| Words on screen and in messages | key, revision, project folder, plan, kept, removed, conflict, "the board's", "prefer the folder / prefer the board". Never sync, merge, base, upsert, prune. |
| Generated files | A task that changes an API type runs `./scripts/regen-generated.sh` and stages the result. When two branches conflict only there, the integrator regenerates after the merge. |
| `CHANGELOG.md` | Written by task Z, by hand, in `[Unreleased]`. |

## Rounds

The maintainer asked for several Sonnet agents (2026-11-26), so up to three build cargo at once,
every one of them Sonnet, all under the external `renice` keeper (`/var/tmp/tack-agent-keeper/keeper.sh`,
`feedback_agent_load_caps`). Worktrees under `/var/tmp/tack-worktrees/p76-<id>` cut from `develop`,
`CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/p76-<id>`, `CARGO_INCREMENTAL=0`. A target is
deleted as soon as its branch merges. The frontend session works beside every round and never
builds cargo.

| Round | Tasks | Needs |
|---|---|---|
| 1 | M (the key and the comparison, pure), B (one write, one transaction), Q (the figures by sprint, domain and level) | — |
| 2 | K (keys on the five tables), A (the `.tack/` level), N (what waits for a person, counted and limited) | B (K and N write through B's `_in` functions) and M (`key_of`) |
| 3 | I (items: matching by key, the brief in the file and its summary), F (the folder format, pure), T (a thread's question reaches Needs you) | K; F only M; T only N |
| 4 | S (routes, revisions, plan, apply) | everything above |
| 5 | C (CLI and MCP), then Z | S |

Ceilings: Sonnet one layer 100 calls, two layers 150. One that reaches it hands off and is
replaced fresh with a shorter brief.

## How a task is handed to an agent

One agent, one task, one branch from `develop` named `phase76/<id>`; the brief is the task's
section of this file plus these lines, and nothing else:

```text
Task <id> from docs/plans/phase-76.md. Read that section, its "The rule for every task" and "Decided" tables, and the files it lists, nothing wider.
The ADR is docs/adr/0025-the-project-file-is-kept-in-sync-not-only-imported.md with its amendment; its tables and this plan's "Decided" table are the decision, do not re-open them.
Touch only the files the task lists. Never touch frontend/: another session owns it. A change needed outside your files is a finding: stop and report it.
Fix the fact in its owner and make the other callers read it; never add a second copy of a rule.
Add only the tests the task names, RED first. Prefer a row in an existing table test over a new function.
Do not add a type, trait, option, flag, loop or helper the task does not name.
Never call a real model endpoint; never run docket or any CLI against the operator's own HOME, ~/.docket, ~/Sites or ~/.local/share/tack.
Words in messages: key, revision, project folder, plan, kept, removed, conflict; never sync, merge, base, upsert, prune.
Work only inside your worktree <given path>. Build and test there with CARGO_TARGET_DIR=<given>, CARGO_INCREMENTAL=0, PATH=$HOME/.cargo/bin:$PATH, --build-jobs 4, --test-threads 4; only the crates you change while working. Confirm rustc --version prints the version rust-toolchain.toml pins. Never start a GUI, tack-desktop or a browser in headed mode.
Finish by running PATH=$HOME/.cargo/bin:$PATH .githooks/pre-push once and reporting its real output. Do not commit.
Report: what changed, per file, in one line each; what you measured; what you could not do; your tool-call count.
```

## The tasks

### M — the key, and the comparison (Sonnet, one layer, pure)

**Files:** a new `crates/tack-core/src/outline/key.rs` (+ `pub mod key;` in `outline.rs`); a new
`crates/tack-core/src/outline/bundle/sync.rs` (+ `pub mod sync;` in `bundle.rs`) with its tests in
`crates/tack-core/src/outline/bundle/sync/tests.rs`, in the `mod tests;` style `bundle.rs` uses.
Nothing else; no I/O; no new dependency.

**Do:**

- `key.rs`: `pub fn key_of(text: &str) -> String` and `pub fn is_key(text: &str) -> bool`, exactly
  as "Decided" says, with a two-line doc comment saying what a key is for (ADR 0025 decision 2).
- `sync.rs`, this public surface and nothing more:

  ```rust
  pub struct Three<'a> { pub base: Option<&'a Outline>, pub board: &'a Outline, pub file: &'a Outline }
  #[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)] #[serde(rename_all = "lowercase")]
  pub enum Prefer { File, Board }
  #[derive(Serialize)] #[serde(rename_all = "snake_case")]
  pub enum Verb { Create, Update, Remove, Keep }
  #[derive(Serialize)] pub struct FieldChange { pub path: String, pub from: Value, pub to: Value }
  #[derive(Serialize)] pub struct Change { pub section: &'static str, pub key: String, pub name: String, pub verb: Verb, pub fields: Vec<FieldChange>, pub reason: Option<String> }
  #[derive(Serialize)] pub struct Conflict { pub section: &'static str, pub key: String, pub name: String, pub path: String, pub base: Value, pub board: Value, pub file: Value }
  pub struct Merged { pub outline: Outline, pub changes: Vec<Change>, pub conflicts: Vec<Conflict> }
  pub fn merge(three: &Three<'_>, prefer: Option<Prefer>) -> Merged
  pub fn diff(from: &Outline, to: &Outline) -> Vec<Change>
  ```

  Sections in the order of "What `merge` compares". An entry's identity is its `key`, else
  `key_of(name)` (else `key_of(title)` for an item): write `entry_key(key: Option<&str>, name: &str)`
  now; the `key` fields land in tasks K and I, and until then they are absent. Each entry is
  `serde_json::to_value` of its struct with `key` removed (and, for an item, the board's keys
  removed). `diff_values(base, board, file, path, &mut fields, &mut conflicts)` recurses into
  objects; arrays and scalars are leaves, but `depends_on` and `roles` are sorted first. The merged
  outline is built back per entry with `serde_json::from_value`, removals left out, `project.type`
  and `project.workflow` and `teams` copied from the board's outline. `Keep` carries its reason from
  "Decided". `changes` come in the file's order, then the board-only entries by name. `diff(from,
  to)` is `merge(&Three { base: Some(from), board: to, file: from }, None)`'s changes seen from `to`'s
  side: a board-only entry is `Create`, a board-changed field `Update`, a board-removed entry `Remove`.

**Tests (RED first), one table test with a row per rule, plus three scenarios:**

- `key_of`: `"Contenido"` → `contenido`; `"En revisión"` → `en-revisión`; `"  Dev  Team!! "` →
  `dev-team`; `"!!!"` → `unnamed`; 70 letters → 64; a 64-char cut that ends on `-` drops it.
  `is_key("Dev")` false, `is_key("dev-2")` true, `is_key("-dev")` false.
- The per-field and per-entry rules of "Decided", each a row `(base, board, file, prefer) →
  (verb, fields, conflicts, reason)`, with `base: None` rows.
- Scenario one, My SaaS's fixture (`crates/tack-api/tests/fixtures/bundles/my-saas.yaml`, read with
  `include_str!` as `bundle/tests.rs` does) as base and board: the file renames role `Contenido` to
  `Content` under its key, drops a sprint and changes `automation.on_finish_status`; the board
  changed the role's colour and added a role `Ops`. Expect one `Update` with `fields = [name]` and
  the board's colour in the merged outline, one `Remove`, one `Update` at `on_finish_status`, one
  `Keep` for `Ops` with its reason, no conflicts, and the board's `type`, `workflow` and `teams`.
- Scenario two, items: the file changes an item's title and its `brief.risk` under its key, gives a
  `status` for an existing item, and drops an item; the board changed another item's description.
  Expect an `Update` with `[title, brief.risk]`, a `Keep` with "the board's" for the status, a
  `Remove` for the dropped item, the board's description kept.
- Scenario three: both change `automation.autonomy.budgets.tokens_per_day` to different values →
  one conflict at that path; with `Prefer::File` the file's value; with `Prefer::Board` the board's.

**Done when:** `cargo nextest run -p tack-core` and the gate are green.

### B — one write, one transaction (Sonnet, two layers)

**Files:** a new `crates/tack-db/src/repo/project_file.rs` (+ `pub mod project_file;` in
`crates/tack-db/src/repo.rs`); `crates/tack-db/src/repo/{projects, roles, sprints, custom_fields,
item_types, autonomy, briefs, items, dependencies}.rs`, `crates/tack-db/src/repo/execution/claim.rs`
(`write_agent_profile`) and `crates/tack-db/src/repo/execution/admin.rs` (`delete_agent_profile`),
each only for the `_in` functions below;
`crates/tack-api/src/handlers/outline.rs`; `crates/tack-api/src/error.rs` (`SectionFailed`); a new
`crates/tack-db/tests/repository/project_file.rs` registered in `crates/tack-db/tests/repository.rs`;
`crates/tack-api/tests/handlers/bundle.rs`.

**The pattern, already in the tree:** `apply_change_set` (`change_sets.rs` line 372) takes a plan
the API built, opens `BEGIN IMMEDIATE` inside the repository and writes through functions that take
the connection; `insert_item(conn, …)` (`items.rs` line 678) is such a function. Since Phase 79
`tack-api` has no pool (`repo.rs` line 64, the `raw-pool` feature is the test crate's), so the
transaction must open inside `tack-db`.

**Do:**

- `_in` twins (SQL moves into the twin; the pool method becomes `let mut conn =
  self.pool().acquire().await?; x_in(&mut conn, …).await`; public signatures do not change) for:
  `create_project`, `update_project`; `create_role`, `update_role`, `delete_role`,
  `assign_role_to_item`; `create_sprint`, `update_sprint`, `update_sprint_status`, and a new
  `delete_sprint` with its pool method (`DELETE FROM sprints WHERE id = ?`, returns whether a row
  went); `create_field`, `update_field`, `delete_field`, `set_field_value`; `set_item_types`;
  `put_autonomy_settings` (`autonomy.rs` line 92; its own `begin` moves into the pool wrapper);
  `write_agent_profile` and `delete_agent_profile`; `upsert_item_brief` (its transaction moves into
  the wrapper); `update_item` and `delete_item`; the dependency insert `create_outline_items` uses. `create_outline_items` keeps its
  transaction around a `create_outline_items_in`.
- `project_file.rs`: `pub struct ProjectWrite` holding, in order, what `write` does today: an
  optional new project (a `CreateProject`) or the project id; the project update; the types; the
  field, profile, role and sprint creates and updates as the repositories' own input structs; the
  automation settings; the outline items and their blocks as `create_outline_items` takes them.
  `Repository::write_project_file(&self, &ProjectWrite) -> Result<Written, WriteError>` writes all
  of it on one `BEGIN IMMEDIATE`, and `WriteError::Section { section: &'static str, source:
  StoreError }` names the section that failed. `Written` carries what the handler answers today
  (the created project, the item ids, the counts).
- `handlers/outline.rs`: `write`, `write_profiles` and `write_automation` keep their mapping and
  build one `ProjectWrite` (a pure `fn to_write(plan, board, …) -> ApiResult<ProjectWrite>`), then
  make one call. `create_new` no longer creates the project first, and its `delete_project`
  compensation and comment go. The inline `Board` building of `into_existing_project` becomes
  `pub(crate) async fn board_of(state, project: &Project) -> ApiResult<OwnedBoard>`, where
  `OwnedBoard` owns what `Board` borrows and has `fn borrow(&self) -> Board<'_>`.
- `ApiError::SectionFailed { section, source }`: `kept` goes; the message is "The {section}
  section could not be written; nothing was changed."

**Tests (RED first):**

- `repository/project_file.rs`: a `ProjectWrite` whose last role names a project that does not
  exist fails, and afterwards the project's fields, sprints and roles are what they were before.
  `delete_sprint` clears `items.sprint_id` of its items; `delete_agent_profile_in` refuses a
  built-in.
- `bundle.rs`: `a_file_that_cannot_be_written_is_refused_whole` now proves that nothing is kept:
  every section's count is what it was, and the message names the section.

**Done when:** `cargo nextest run --workspace -E 'package(tack-db) + package(tack-api)'` and the gate
are green; `rg -n "kept" crates/tack-api/src/error.rs` and `rg -n "delete_project" crates/tack-api/src/handlers/outline.rs`
find nothing.

### K — a key on the five kinds (Sonnet, two layers)

**Files:** `crates/tack-db/src/migrations.rs` (`MIGRATION_115` + its line in `all_migrations()` and
the fill call in `run_all`); `crates/tack-db/src/repo/project_file.rs` (`fill_missing_keys`); a new
`crates/tack-db/tests/migrations/keys.rs` registered in `crates/tack-db/tests/migrations.rs`;
`crates/tack-core/src/models.rs` (`Role`, `Sprint`, `CustomFieldDefinition`, `Item`, `CreateRole`,
`CreateSprint`, `CreateCustomField`, `CreateItem`); `crates/tack-core/src/outline.rs`
(`BundleField`, `BundleProfile`, `BundleRole`, `BundleSprint`); `crates/tack-core/src/outline/bundle.rs`
(`Board`, `match_names` → `match_rows`, `check`, `to_bundle`) and its `tests.rs`; the row mappings
and create `_in` functions of `roles.rs`, `sprints.rs`, `custom_fields.rs`, `items.rs`
(`insert_item`), `execution/claim.rs` (`list_agent_profile_rows` line 1007, `write_agent_profile_in`);
`crates/tack-api/src/handlers/runner_admin.rs` (`create_profile` and the profile response);
`crates/tack-api/src/handlers/outline.rs` (`board_of`, `to_write`'s maps);
`crates/tack-api/tests/handlers/bundle.rs`; `crates/tack-api/tests/handlers/crud.rs` (one row); the
generated files.

**Do:**

- The migration, the fill and the create paths as "Decided" says. The fill reads every row with an
  empty key, computes `key_of`, and suffixes within the project in `created_at` order.
- Models: `pub key: String` on `Role`, `Sprint`, `CustomFieldDefinition`, `Item`;
  `pub key: Option<String>` (serde default) on the four create bodies; a given key passes
  `is_key` (else 400 naming it); a taken one is a `StoreError::Conflict`, so 409 (sqlx's `is_unique_violation()`, as
  `agent_credentials.rs` line 195 maps it). `ProfileRow` gains `key`; the profile response carries it.
- The file: `pub key: Option<String>` as the **first** field of `BundleField`, `BundleProfile`,
  `BundleRole`, `BundleSprint`, serde default, `skip_serializing_if = "Option::is_none"`.
- `bundle.rs`: `pub struct Named { pub id: String, pub key: String, pub name: String }`;
  `Board.roles` and `Board.sprints` become `&[Named]`, and `check`'s fields and profiles become
  `Vec<Named>`. `match_names` becomes `match_rows` with "Decided"'s rule; `Upsert::Create` carries
  the key it will be created with. `to_bundle` writes every key.
- `to_write` keeps `role_ids`, `sprint_ids`, `field_ids`, `profile_ids` keyed by **name**, because
  items name them by name (ADR 0022 decision 1); the create inputs get the key.

**Tests (RED first):**

- `migrations/keys.rs`: at `114_reviewer_verdicts` insert roles `Dev Team` and `dev team` in one
  project, a profile `Writer`, an item `Login: GitHub`, a hidden item; run all; the keys are
  `dev-team`, `dev-team-2`, `writer`, `login-github`, and `''` for the hidden item; a second
  `run_all` changes nothing; the partial indexes exist (`PRAGMA index_list`).
- `bundle/tests.rs`, rows in the existing check table: `key: Dev` creating a role (refused); a key
  another row holds (refused, names both); the board row's name under another key (refused); a
  board row's key with a new name (an update, the rename). A new `the_export_writes_every_key`. In
  `sync/tests.rs`, scenario one gains the rename task M could not write before keys existed: the
  file renames role `Contenido` to `Content` under `key: contenido`, and the plan is one `update`
  with `fields = [name]`.
- `bundle.rs` (API): `the_export_is_the_import` now carries `key:` under every role, field, profile
  and sprint, and stays the same text after a round trip; a second import that renames `Contenido`
  to `Content` under `key: contenido` updates the same row.
- `crud.rs`: `POST /api/projects/{id}/roles` `{ "name": "Dev Team" }` answers `key: "dev-team"`; a
  second with `"key": "dev-team"` is 409.

**Done when:** `cargo nextest run --workspace -E 'package(tack-core) + package(tack-db) + package(tack-api)'`,
a clean `./scripts/regen-generated.sh` after staging, and the gate are green.

### A — a change under `.tack/` is merged by a person (Sonnet, one layer, pure)

**Files:** `crates/tack-core/src/autonomy.rs` (`level_for`, the constant) and its tests.

**Do:** as "Decided" says, with the doc comment of `level_for` gaining one sentence: a change to
the project folder is a change to the plan, so a person merges it (ADR 0025 decision 14).

**Tests (RED first):** rows in `level_for`'s table: an L5 project whose change is
`[".tack/items/a.md", "src/x.rs"]` → `(L3, Some("project folder"))`; a domain at L4 covering `**`
and the path `.tack/project.yaml` → L3; `src/.tack-notes` → the domains as before.

**Done when:** `cargo nextest run -p tack-core` and the gate are green.

### I — items in the file: matched by key, with their brief (Sonnet, two layers)

**Files:** `crates/tack-core/src/brief.rs` (`BriefSummary`, `readiness`) and its `brief/tests.rs`;
`crates/tack-api/src/handlers/items.rs` (`list_items`, the summary);
`crates/tack-core/src/outline.rs` (`OutlineItem.key`, `OutlineItem.brief`, a new
`OutlineBrief`); `crates/tack-core/src/outline/bundle.rs` (item matching in `check`, the item side
of `to_bundle`, `Parts` gains the briefs) and its `tests.rs`; `crates/tack-db/src/repo/project_file.rs`
(`ProjectWrite` gains item updates and briefs); `crates/tack-api/src/handlers/outline.rs` (`to_write`);
`crates/tack-api/src/handlers/export.rs` (`bundle_of` reads the briefs with
`list_item_briefs_for_project`, `briefs.rs` line 72); `crates/tack-api/tests/fixtures/bundles/my-saas.yaml`
(two items gain a brief); `crates/tack-api/tests/handlers/bundle.rs`; the generated files.

**Do:**

- `OutlineItem` gains `key: Option<String>` and `brief: Option<OutlineBrief>`; `OutlineBrief` is
  brief-v1's four parts, `deny_unknown_fields` like the rest of the file.
- `check`: an item with a key the board has is an update of its plan fields (decision 9's list),
  checked as a new item's would be; its board keys give a warning ("status is the board's: kept");
  `parent` and `depends_on` may name a key (of the file or of the board) or, as today, a `ref` or an
  exact title. A brief is checked with `brief::validate`; a wrong one is a problem naming the item.
- `to_write`: item updates go through B's `update_item_in` and the briefs through
  `upsert_item_brief_in`, in the same `ProjectWrite`.
- `to_bundle`: items carry `key`, name parents and blockers by key, carry their brief, and never
  write `ref`.
- **The brief's summary on the item list** (the screens' first ask). `tack_core::brief` gains
  `pub struct BriefSummary { done_when: bool, checks: u32, risk: Option<Risk>, not_ready: Option<String> }`
  and `pub fn readiness(brief: Option<&ItemBrief>, project_done: Option<&str>) -> BriefSummary`.
  `done_when` is the item's definition of done, or the project's when the item has none (a run
  receives the project's, `executions.rs` line 2062). `checks` counts the criteria that are not
  `manual`. `not_ready` is `None` when there is a Done when or at least one machine check, else
  `"No Done when and no check a machine runs"`. The items the project's list route answers
  (`GET /api/projects/{project_id}/items`, `router.rs` line 388), and the one item `GET /api/items/{id}`
  answers (the run dialog reads it there), carry `brief_summary` (`null` when the
  item has no brief and the project no Done when), read with
  one `list_item_briefs_for_project` per request, never one call per item.

**Tests (RED first):** `sync/tests.rs`, scenario two gains what task M could not write before
item keys and briefs existed: the file changes an item's `title` and `brief.risk` under its key,
and the plan is one `update` with `[title, brief.risk]`. `brief/tests.rs`: `readiness` rows for no brief, a manual criterion only,
one command criterion, the project's Done when, and a risk. `bundle/tests.rs`: an item whose key the board has is an update with a
warning for its `status`; a brief with two criteria of the same id is refused naming the item.
`bundle.rs` (API): importing `my-saas.yaml` creates its two briefs (two `item_brief_versions`
rows); exporting and importing the export into the same project adds no item and changes nothing;
editing one item's title in the export and importing it renames the card. A handler test: the
project's item list carries `brief_summary` for an item with a brief and `not_ready` for one
without.

**Done when:** the three crates' tests, a clean regen, and the gate are green.

### F — the folder format (Sonnet, one layer, pure)

**Files:** a new `crates/tack-core/src/outline/folder.rs` (+ `pub mod folder;`) with its tests in
`crates/tack-core/src/outline/folder/tests.rs`; `crates/tack-core/Cargo.toml` (`serde-saphyr` from
the workspace).

**Do:** `read` and `write` exactly as "Decided" says. `read` collects every problem (an unknown
path, a file without its two `---` lines, a key that fails `is_key`, YAML that does not parse,
naming the file) and returns them together, as `check` does. It treats the item's YAML as an
`OutlineItem` without `key` and `description`, so the fields task I adds come through with no
change here.

**Tests (RED first):** My SaaS's fixture → `write` → `read` gives the same outline minus the
board's keys; a file `items/Login.md` is refused; a file with no closing `---` is refused naming
it; `notes.txt` is refused; an item with no description writes no body; `write` never writes
`status`.

**Done when:** `cargo nextest run -p tack-core` and the gate are green.

### S — the routes, the revisions, the plan and the apply (Sonnet, two layers)

**Files:** a new `crates/tack-api/src/handlers/folder.rs` (+ `pub mod folder;` in `handlers.rs`);
`crates/tack-api/src/router.rs` (the six routes beside `/projects/{id}/export`, line 364);
`crates/tack-api/src/openapi.rs` (the paths and `FolderStatus`, `FolderPlan`, `FolderApplied`,
`RevisionSummary`, `Removal`, and `sync::{Change, Conflict, FieldChange, Verb, Prefer}`, which gain
`#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]` in `sync.rs`, the one line this task
may change there); `crates/tack-db/src/migrations.rs` (`MIGRATION_117`); `crates/tack-db/src/repo/project_file.rs`
(revisions, `usage_of`, removals in `ProjectWrite`); `crates/tack-api/src/handlers/export.rs`
(`bundle_of`, the revision on export); `crates/tack-api/src/handlers/templates.rs`
(`save_project_as_template` reads `bundle_of`); a new `crates/tack-api/tests/handlers/folder.rs`
registered in `crates/tack-api/tests/handlers.rs`; a new fixture
`crates/tack-api/tests/fixtures/bundles/my-saas-folder-v2/` (the folder of My SaaS, changed); the
generated files.

**Do:**

- `repo/project_file.rs`: `record_revision_in(conn, project_id, origin, body) -> Result<Option<RevisionRow>>`
  (`None` when the last revision has the same hash; number = last + 1), `last_revision`,
  `list_revisions` (no body), `revision(project_id, n)`, and `usage_of(conn, …)` with one function
  per kind returning "Decided"'s costs and refusals. `ProjectWrite` gains the removals and the two
  revisions, written in "Decided"'s order.
- `export.rs`: `bundle_of(state, project, items)` holds what `bundle_response` builds today (the
  `BundleAutonomy` mapping at line 200 included); `bundle_response` calls it and records a `board`
  revision for `scope=bundle`. `templates::save_project_as_template` calls `bundle_of(…, &[])` and
  drops its copy of the mapping.
- `folder.rs`: the six handlers. `plan` and `apply` share one `async fn planned(state, project,
  body, query)`: read the files with `folder::read` (or a revision's body), `bundle_of` as the
  board, `last_revision` as the base, `merge`, `check(&merged.outline, &board.borrow())` (400 on
  problems), the removals through `usage_of` (an item with history becomes a `Keep` with its
  reason; a refused profile or type is a problem), then `plan_hash`. `apply` checks the principal,
  compares the hash (409), and makes one `write_project_file` call. `status` is
  `diff(last.body, board)`. `GET …/folder` answers `folder::write(bundle_of(…))` and records the
  revision.
- After the commit: `BoardConfigUpdated` and the item events, as "Decided" says. `Written` gains
  the ids of the items it updated, so each gets its `ItemUpdated`.
- **What tasks I and C left for this task** (read their notes in "How it went"):
  - An updated item takes the file's roles, field values and blockers **exactly**: the ones the
    file drops are removed. Add the `_in` twins this needs beside the ones B made
    (`remove_role_from_item_in` in `roles.rs`, the dependency delete in `dependencies.rs`,
    `delete_field_value_in` in `custom_fields.rs`) and call them from `write_in`.
  - `bundle_of` reads **every** visible item. The default `ItemFilter` stops at 100, so today's
    `GET …/export?scope=bundle` writes a truncated file for a bigger project; page through all of
    them as `board_of` does.
  - The agent-access classification of the six routes is already in `agent_access.rs` (plan is a
    read, apply is a person's); the handler still checks the principal itself.
  - `ProjectWrite` already has `updates` and `workflow`, and `write_in` takes the transaction.

**Tests (RED first), on `bundle::app()`:**

- `an_export_records_a_revision_once`: two `GET …/folder` in a row leave one revision; a role made
  through `POST …/roles` and a third export leave two.
- `a_plan_names_every_change_and_its_cost`: import My SaaS, export (the base), set a `Plan` value on
  two items, then plan the v2 folder (role `Contenido` renamed under its key, field `Plan` removed,
  sprint `Semana 42` removed, one item's brief changed, one item without history removed, one item
  with a comment removed, a new role): the rename is an `update` with `[name]`; the removals carry
  "2 values are deleted with it" and "3 cards leave it"; the item with a comment is `keep` with its
  reason; the brief change is an `update` at `brief.acceptance`; `applies: true`; nothing is written.
- `an_apply_with_a_stale_hash_is_refused`: plan, create a role, apply with the old hash → 409 and
  nothing changed.
- `an_apply_writes_everything_or_nothing_and_leaves_two_revisions`: apply v2 → 200; every change of
  the plan is on the board, the changed brief has a new version, and `…/folder/revisions` shows
  `board` then `file` on top; `…/folder/status` answers `changes: []`. Planning `?revision=<the
  board one>` shows the inverse, and applying it brings the role's name back (a `revert` revision).
- `what_the_board_changed_is_kept_and_a_double_change_is_a_conflict`, as the earlier plan had it,
  on a role's colour and an item's description.
- `an_agent_previews_and_a_person_applies`: with an agent credential, `plan` answers and `apply` is
  403 with the decided message.
- `a_profile_another_project_uses_is_not_removed`; `a_first_plan_with_no_revision_removes_nothing`;
  `the_project_block_is_applied_for_three_keys_only`.

**Done when:** `cargo nextest run --workspace -E 'package(tack-api) + package(tack-db)'`, a clean
regen, and the gate are green.

### N — what waits for a person, counted and limited (Sonnet, two layers)

Serves ADR 0033 decisions 6 and 9 (the screens' second and third asks).

**Files:** `crates/tack-core/src/notification.rs` (a new `Kind::Review`, its key, `notice` and the
resolving facts) and its tests; `crates/tack-core/src/event.rs` (a new `Fact::RunReviewed { run }`);
the places that accept, turn down or merge an attempt, which publish it (read
`crates/tack-api/src/handlers/executions.rs` and `crates/tack-api/src/handlers/mrp.rs` first and
touch only the publishing lines); `crates/tack-core/src/auto_start.rs` (`Limits`, `Skip`, the text,
line 29 and 121) and its tests; `crates/tack-app/src/auto_start.rs` (reads the count);
`crates/tack-api/src/handlers/auto_start.rs` (`WaitingReason` gains the new reason);
`crates/tack-db/src/migrations.rs` (`MIGRATION_116`); `crates/tack-db/src/repo/budgets.rs` and
`crates/tack-db/src/queries/budgets.rs` (`waiting_max` beside `queue_items_max`);
`crates/tack-db/src/repo/notifications.rs` (a count of a project's open `needs_you`);
`crates/tack-api/src/handlers/autonomy.rs` (the budgets on `GET`/`PUT /projects/{id}/autonomy`);
`crates/tack-core/src/outline.rs` (`BundleBudgets.waiting_max`, line 329), and the budgets mapping
in `crates/tack-api/src/handlers/outline.rs` (B's `to_write`), `export.rs` and `templates.rs` (both
places until S makes them one); `crates/tack-cli/src/cmd/board.rs` (the budgets line, 286); the
tests beside each; the generated files.

**Do:**

- A run that succeeded waits for a person until it is accepted, turned down or merged, or a new run
  of its item starts: `Fact::RunFinished` opens a `review` notification at `needs_you` (key
  `review:<request>`), replacing today's `info` one, and `RunReviewed` or `RunStarted` resolves it.
  A merge at Level 4 or 5 publishes `RunReviewed` too, so the factory's own merges never wait.
- Read whether a manager's thread turn that waits for a person reaches the inbox as a `decision`.
  If it does, nothing changes; if it does not, stop and report it as a finding.
- `GET /notifications?project=<uuid>` returns only that project's notifications, and its `open`
  count is the project's: the shared list keeps the newest 50, so a project's own page asks for
  its own (the frontend session's finding, 2026-11-26).
- `waiting_max` (null: no limit) in the project's budgets, on the autonomy settings route and in the
  project file (`automation.autonomy.budgets.waiting_max`). `Limits` gains `waiting_max:
  Option<usize>` and `waiting_now: usize` (the project's open `needs_you` count); at the limit
  auto-start starts nothing, and every task `GET /auto-start` lists as waiting carries the reason
  `waiting_max` with the text "{n} of {max} waiting for you: auto-start is paused until one is
  decided". A file may set the limit, since it only holds work back.

**Tests (RED first):** `notification` rows: `RunFinished` opens `review` at `needs_you`;
`RunReviewed` and `RunStarted` resolve it. `auto_start` rows: at `waiting_now == waiting_max`
nothing starts and the reason says why; `waiting_max: None` starts as today. A handler test: the
autonomy route round-trips `waiting_max`. A bundle row: `budgets.waiting_max: 2` round-trips.

**Done when:** the crates' tests, a clean regen, and the gate are green.

### Q — the figures by sprint, by domain and by level (Sonnet, two layers)

Serves ADR 0033 decisions 8 and 11 (the screens' fourth and fifth asks).

**Files:** `crates/tack-api/src/handlers/metrics.rs` (the answer `FactoryMetrics`);
`crates/tack-db/src/repo/metrics.rs` and `crates/tack-db/src/queries/metrics.rs` (the decisions'
minutes grouped by the item's sprint);
`crates/tack-api/src/handlers/metrics_quality.rs` (two more cuts beside `profile`, line 420); the
query that loads the attempts for the quality figures, which joins the merge rows (`MergeRow`
carries `domain` and `level`, `repo/autonomy.rs` line 38); the tests beside each; the generated
files.

**Do:**

- `FactoryMetrics` gains `human_minutes_by_sprint: Vec<{ sprint_id, minutes: HumanMinutes }>`: the
  same human-minutes figure the answer already has, over the decisions of each sprint's items, one
  row per sprint of the project that has any. The waits per sprint the screens count from what they
  already read per item.
- `work_quality.cuts` gains `domain` and `level`: an attempt that merged is cut by the domain and
  level its merge row recorded; one that did not merge is "not merged". The revert rate of a row is
  then the rate at which that level got it wrong in that domain.

**Tests (RED first):** two sprints whose items had decisions of 10 and 30 minutes: each row of
`human_minutes_by_sprint` carries its own median and an item with no sprint counts in none. A quality test: two merges at L4 in domain
`docs`, one reverted, give the row `docs` and the row `L4` a revert rate of one half.

**Done when:** `cargo nextest run --workspace -E 'package(tack-api) + package(tack-db)'`, a clean
regen, and the gate are green.

### T — a question asked in a thread reaches Needs you (Sonnet, two layers)

Found by task N: a manager's thread turn runs on the thread's hidden anchor item, and the inbox
skips every notice about a hidden item, so a question asked in a thread opens no `decision`
(ADR 0033 decision 6 promises it). The same holds for a trigger's hidden item.

**Files:** `crates/tack-app/src/notifications/subscriber.rs` (`apply`, line 57, and the reconcile's
use of `is_hidden`, line 164); `crates/tack-db/src/repo/notifications.rs` (`pending_decisions`,
line 420, and the list queries); `crates/tack-core/src/notification.rs` (`Notification`, line 163);
`crates/tack-api/src/handlers/notifications.rs` only if the answer is built there; the tests beside
each; the generated files.

**Do:**

- A `decision` notice is never skipped for a hidden item: a question always waits for a person.
  Every other kind keeps the skip (a thread's runs are not reviews, failures or news).
  `pending_decisions` drops `i.hidden = 0` for the same reason.
- `Notification` gains `thread_id: Option<Uuid>`: the thread whose anchor is the notification's
  item, read with a `LEFT JOIN threads t ON t.anchor_item_id = n.item_id` in the list queries. No
  column and no migration; a notification about an ordinary item has none.

**Tests (RED first):** in `crates/tack-app/tests/notifications.rs`, a question in a thread's turn
opens a `decision` whose `thread_id` is the thread's; a run that finishes on the same anchor opens
no `review`; the reconcile keeps the thread's question open.

**Done when:** the crates' tests, a clean regen, and the gate are green.

### C — `tack folder`, and the MCP tool (Sonnet, one layer)

**Files:** `crates/tack-cli/src/main.rs` (`Commands::Folder { action: FolderAction }` beside
`Commands::Template`); a new `crates/tack-cli/src/cmd/folder.rs` (+ its line in `cmd/mod.rs`), beside
`cmd_export` and `cmd_import` in `cmd/board.rs` line 108; `crates/tack-cli/src/client.rs` only if a
method it needs is missing; `crates/tack-cli/src/mcp/projects.rs` and `crates/tack-cli/src/mcp/tools_list.json`
(`plan_folder`, and `create_outline`'s sentence); `crates/tack-cli/tests/cli_test.rs` (rows beside
the import tests, same wiremock pattern).

**Do:** the verbs as "Decided" says, with one `print_plan(&Value)` for `import`, `revert` and
`status`: changes as `<verb> <section> <name>: <path> <from> → <to>`, conflicts with the three
values, removals with their cost lines, kept rows with their reason, warnings. `export` writes the
files under `dir` (creating `items/`) and removes the `items/*.md` the answer lacks. `import` reads
every file under `dir`, paths relative to it, and sends `{ files }`.

**Tests (RED first):** `folder import --dry-run` posts the folder's files to `/folder/plan` and
prints a removal's cost; `folder import` with a removal and no `--yes` exits 1 and never calls
`/folder/apply`; with `--yes` it calls apply with the plan's hash; `folder export` into a dir that
holds a stale `items/old.md` leaves only the answer's files; `folder revert 3` posts
`/folder/plan?revision=3`. MCP: `plan_folder` with both `files` and `revision` is an error, and it
never calls `/folder/apply`.

**Done when:** `cargo nextest run -p tack-cli` and the gate are green.

### Z — the integrator: docs, changelog, the gate, a run on a copy of the real database

**Files:** `docs/book/src/user-guide/import-export.md` (a section "Keeping the plan in the
repository": the folder, export, status, import, what is removed and what it costs, what is the
board's, kept and conflicts, history and revert, keys; "If a section fails to write" becomes
"nothing is written"; "Nothing is deleted" becomes "`tack import` removes nothing; `tack folder
import` removes what the folder removed, when nothing happened to it, and shows the cost first"),
`docs/book/src/user-guide/cli.md` (`tack folder`), `docs/book/src/user-guide/merging.md` (a change
to the project folder is merged by a person), `docs/book/src/developer/project-file.md` (`key` in
every section, the item's plan and brief, the folder's layout, the comparison, the routes, the
revisions), `docs/MCP.md` and `docs/book/src/developer/mcp.md` (`plan_folder`), `CHANGELOG.md`, `docs/book/src/roadmap.md` (Phase 76 delivered), `CLAUDE.md` ("What's next" and
"Releases"), ADR 0025's status. **Phase 76 ships in `v0.2.0-beta.1`:** its lines go in the
`[0.2.0-beta.1]` section, together with what `[Unreleased]` holds (that release is not tagged, so
all of it ships there), `[Unreleased]` is left empty, and the section's opening, the roadmap's
"The next release" line and `CLAUDE.md` say it carries Phase 76 too. The User and Administrator guides name no source path, test or phase.

**Do:** merge the branches in round order with one gate run per round. After round 5, copy the
maintainer's database (`~/.local/share/tack/tack.db`) to the scratchpad, start `tack serve` on the
copy with `TACK_DATABASE_URL` (`crates/tack-api/src/config.rs` line 317) on a free port, export My SaaS's folder, rename a role, drop a sprint,
edit an item's brief and drop an item with a run, plan, read the costs and the kept rows, apply,
read the history and revert. Write what was seen in "How it went". Never against the real database
or the running service.

**Also for the roadmap, from the docket session (2026-11-27), not Phase 76's to build:** D1's ask of
docket is done on docket's develop (the subject comes from `task.objective`, `pod-dispatch`
13.1.0), so D1 keeps only the runner's own commit for local agents; D3 is a declared limit in
docket (under Landlock a bind to 127.0.0.1 is refused, a Unix socket works, the Docker backend has
its loopback) with no `features` entry, so the dialog's sentence stays (the frontend session's).
The frontend session's third finding becomes a card asked of docket: a decision's metadata lists
what the approval changes, so the approval can show it.

**Done when:** the gate is green on `develop`, `mdbook build docs/book` is clean, and the roadmap's
Phase 76 section says what shipped.

## When the screens need the API

The frontend session writes, in its section below, the facts its screens need that no route
serves, one line each ("the List needs, per item, whether it waits for a person"), and sends this
session one message naming them. This session decides each one: a field on an existing answer
joins the task that owns that answer, a new route becomes a task here, and a fact the board already
serves is answered with its route. No screen computes a fact the API owns, and no API task is
added for a screen without that line.

**Decided 2026-11-26 on the five asks of "The screens" §2,** in the wire names the frontend session
built against:

| Ask | Answer |
|---|---|
| A summary of each item's brief on the item list | Task I: `brief_summary { done_when, checks, risk, not_ready } \| null` on `GET /projects/{id}/items`. The rule lives in `tack_core::brief::readiness`: `not_ready` says why when there is neither a Done when (the item's or the project's) nor a check a machine runs; the screens show it and never compute it. Not ready never blocks a run |
| Every wait for a person in one project | Task N: `GET /notifications?project=<uuid>` (open by default), because the shared list keeps only the newest 50. Task T: a question in a thread arrives as `decision` with `thread_id` set; and a succeeded run becomes a `review` wait until it is accepted, turned down, merged or run again. A question, a failure and a change set already are waits; a thread turn that waits arrives as a question |
| A limit on what waits | Task N: `waiting_max` in the project's budgets, beside `queue_items_max`, null is no limit; at the limit auto-start starts nothing and each waiting task gives the reason `waiting_max` |
| A sprint's decision load | Task Q: `human_minutes_by_sprint` on the factory figures |
| The ladder's error rate per domain and level | Task Q: `work_quality.cuts` gains `domain` and `level` |

## The screens (the frontend session)

Owned by the frontend session; this session does not edit it. Two parts:

1. **The Project folder card** (ADR 0025 decisions 8 and 15): Settings → Data → **Project folder**,
   with the status line, **Export**, **Import…** (a folder picker, then the plan dialog), and
   **History** with **Revert to this…**. The plan dialog: sections in the plan's order; each row a
   chip (new, changed, removed, kept with its reason as a second line) and its fields as "path: from
   → to"; **Conflicts** with the three values and "When both changed: ask · prefer the folder ·
   prefer the board", which plans again; **Removed** rows with their cost and one box "I have read
   what these removals delete" that gates **Apply**; "Choose per conflict" shown disabled with its
   reason; **Apply** sends the plan's hash and turns a 409 into **Preview again**. The keys in grey
   under names in Roles, Fields, the profile list, and under the title on the item page. The routes
   and types are in "Decided"; until task S lands, the screens are built against them with the
   API mocked. **Built** (`f9fbf5f1`): `frontend/src/features/settings/panels/projectFolder/`, the
   types by hand in `frontend/src/shared/api/folder.ts` until S regenerates the schema. Export
   writes a folder only where the browser can (the File System Access API); elsewhere it is shown
   disabled with "run `tack folder export` in the repository".
2. **The views, redefined.** **Proposed 2026-11-26, waiting for the maintainer** as ADR 0033's
   amendment of that date (decisions 6 to 12), which holds the decisions and what a person sees.
   In short: 11 project destinations become 9 and no ability goes. **Needs you** is first and
   takes in Change sets. Calendar becomes a layout of the Timeline, and Factory metrics a part
   of the Overview, beside the plan's progress and the ladder by domain. Board, List,
   Orchestration, Sprint and Threads stay, each answering a question no other answers. A task
   without what makes it done reads **Not ready** wherever it is shown, and its risk shows when it
   is not low. "Work in progress" counts what waits for a person, against an optional limit. A
   sprint shows its decision load beside its estimate. The item page says which of its fields are
   the project folder's and which are the board's. Nothing is rebuilt before the maintainer
   answers.

   **What these screens ask of the API** (one line each, sent to this session the same day):

   - The Board, the List and the item page need, per item in the project's item list, a summary
     of its brief: whether it has a Done when, how many checks a machine runs, and its risk. Today
     `Item` carries none of these, and reading every brief for one board is a call per item.
   - **Needs you** needs, for one project, every wait for a person in one list: a question, a
     run ready for review, a failure not yet looked at, a change set to apply, a thread turn that
     waits, each with its item, its kind and when it started. If the inbox (`/notifications`) can
     be filtered to one project and carries these, that route is the answer.
   - The limit on what waits for a person needs a project setting (`waiting_max`, `null` is no
     limit) on the answer that holds the project's budgets, and a decision on whether auto-start
     stops starting runs while the limit is reached (it is the board's version of a WIP limit).
   - A sprint lane needs, per sprint, how many of its items wait for a person and the median
     human minutes per decision of its items: a cut `sprint` on the factory metrics' figures.
   - The Overview's ladder needs `work_quality.cuts` cut by `domain` and by `level` (the level the
     change merged under), so the rate a level got wrong in a domain can be read. `merged` and
     `undone` per domain are already served by `/autonomy`.
   - Served already, no ask: which item fields differ from the last revision (`/folder/status`
     changes by item key), a team's steps and methods (`/api/teams`), the run's state per item.

## How it went

- **M** (`0d9cf86c`): `diff(from, to)` is `merge(Three { base: from, board: from, file: to })`, the
  mapping that gives the semantics this plan states. `BundleAutomation` has no `on_finish_status`;
  the scenario uses `default_branch`. The integrator changed one rule: a conflict `prefer` settles
  stays listed (see "Decided").
- **A** (`4caa50ee`): besides `level_for`, `.tack` joined the checked files, because with a
  verifier's pack the hold reads the pack's flags, so the level is the guarantee and the checked
  files are what a person sees. ADR 0025 decision 14's "What a person sees" says so.
- **B** (`50889f25`): `write_project_file` takes the `ProjectWrite` by value; `Written` carries
  the project id; `put_project_budgets_in` joined the twins so the limits land in the same
  transaction; `upsert_brief_in` keeps its name. The autonomy overlay is read before the
  transaction opens.
- **F** (`e1d43b58`): `OutlineItem.key` landed with F (task I uses it). An item's `key:` used to be
  read as its `ref`; it is now the item's key, so a hand-written outline that used `key:` as a
  local name must say `ref:`. **Z writes this in the changelog and in both guides' `ref` rows**
  (`import-export.md`, `developer/project-file.md`). The integrator added two refusals: a folder
  without `project.yaml` (it would read as a project with nothing in it, and the plan would
  propose removing everything), and a `key` or `description` line in an item's YAML.
- **I** (`0249d20e`): `resolve_among` beside `resolve` (changing `resolve` would have touched
  files outside the task), `PlannedUpdate`, `ItemWrite`; `brief_summary` is never null (an item
  with no brief and no project Done when reads `not_ready`). The integrator documented the answer
  as `ItemWithBrief` (the card plus the summary, one type the handler returns and the schema
  shows) and classified the folder routes for agents, because C's `plan_folder` had left the
  MCP route test red on develop. Open for S: an import never removed an updated item's dropped
  roles, values or blockers; the bundle export stops at 100 items; updated items sent no event.
- **C** (`f8bcd56a`): `--prefer folder|board` on the CLI (the screen's words), sent as
  `file|board`; `revert <project> <n>`, `status` and `history` take no folder. `plan_folder` is a
  read tool. Its MCP guide row and the tool count were added by the integrator.
- **K** (`32b8a687`): `NewAgentProfile` carries the key (clippy refused another argument). A saved
  template now carries keys, so applying it matches this board's rows by key, which is what a
  template saved from this board means. Built-in profiles were seeded after the fill and kept an
  empty key until the next start; the integrator made the seed end with the fill (`446b1f6d`).
  Merging develop into K met only the migration list (115 then 116); 1541 tests passed on the
  combined tree.
- **T** (`02a566fb`): `thread_id` is a scalar subquery in one constant shared by the insert's
  `RETURNING` and the list queries (SQLite's `RETURNING` cannot join).
- **N** (`a8a1c441`): `RunStarted` under a new request closes the old `review` through the
  reconcile, within about 30 seconds. The integrator removed the agent's seven-day window on
  waiting reviews: a run waits however old it is. `waiting_max: 0` pauses auto-start for good. The
  thread finding became task T.
- **Q** (`1dc6efb0`): an attempt merged by hand has no merge-ledger row, so the quality cuts by
  domain and level put it under "Not merged"; `human_minutes_by_sprint` is ordered by sprint id.

(The rest is written by task Z.)
