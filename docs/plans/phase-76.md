# Plan: Phase 76 — the project file is kept in sync

**Status: proposed 2026-10-18.** Serves ADR 0084
(`docs/adr/0084-the-project-file-is-kept-in-sync-not-only-imported.md`). Written against `develop`
at `cebaef77`. Every path and line below was read that day; re-run `rg -n` before trusting a line
number. Re-checked the same day against Phase 75's task D (`d72be6ce`): migration 092 is
`092_drop_usd_per_day`, so this phase's two are 095 and 096 (Phase 77 took 093 and 094 on 2026-10-19); D also moved lines in
`handlers/outline.rs` (`read_outline` now warns on an old file's `usd_per_day`, `write_automation`
no longer writes it), `export.rs` and `templates.rs`. Nothing else in this plan changes.

What this phase does, in one paragraph. A project's configuration (everything in the project file
but the items) can be exported, edited and imported again, and the board follows: a key on every
role, field, sprint and profile survives a rename; an import is a three-way comparison of the file,
the board and the last revision, so a thing removed from the file is deleted, a thing changed on the
board is kept, and a field both changed is a conflict; a deletion shows what it costs; every export
and import leaves a revision the board can go back to; the preview's hash travels to the apply; and
every import of a project file, old or new, is written in one transaction. Drupal's `drush cex` /
`drush cim`, with the comparison Drupal does not do.

## The rule for every task: one owner per fact

| Fact | Its one owner after this phase | Callers that stop computing it |
|---|---|---|
| A thing's key from its name | `tack_core::outline::key::key_of` | — (new) |
| The three-way comparison | `tack_core::outline::bundle::sync::merge`; two-way `diff` beside it | — (new); the status route reads `diff` |
| Which board row a file entry is | `bundle::match_rows` (today `match_names`): by key, then by name | — |
| The project's configuration as a file | `handlers::export::bundle_of(state, project, with_items)` | `bundle_response`, `templates::save_project_as_template` (its own `BundleAutonomy` mapping goes) |
| What a file is checked against | `handlers::outline::board_of(state, project)` | `into_existing_project` builds it inline today; the config routes read it |
| Writing a checked plan | `handlers::outline::write(tx, …)` on one connection | `create_new`'s compensating `delete_project`; `SectionFailed.kept` |
| A row's insert, update and delete SQL | the `*_in(conn, …)` twin in its repository module | the pool method is a one-line wrapper; `runner_admin::delete_profile`'s inline `DELETE` |
| What a deletion costs | `tack_db::repo::config::usage_of(conn, …)` | — (new); the Fields tab's "all values" sentence reads it later |
| The revisions | `tack_db::repo::config::{last_revision, record_revision, list_revisions, revision}` | — (new) |
| The plan's hash | `handlers::config::plan_hash(&Merged, board_hash)` | — (new) |

No task adds a layer, a trait, a loop, a table or a setting the task does not name. A new number
is a named constant beside the code that reads it, never a config key.

## Decided — not re-opened by any task

Every row of ADR 0084's table and its four defaults, plus:

| Question | Decision |
|---|---|
| Migrations | Two, in the style of `MIGRATION_091` (several statements under one name; `apply_migrations` runs them in one transaction, `migrations.rs` line 391 — `CONTRIBUTING.md`'s "one ALTER per name" predates that and task Z corrects it). `095_config_keys`: for each of `roles`, `custom_field_definitions`, `sprints`, `agent_profiles`: `ADD COLUMN key TEXT NOT NULL DEFAULT ''`; `UPDATE … SET key = lower(replace(trim(name), ' ', '-'))`; a second `UPDATE` suffixing `'-' \|\| substr(id, 1, 8)` on every row whose `(project_id, key)` (for profiles: `key`) is not the first of its group; `CREATE UNIQUE INDEX idx_<table>_key ON <table>(project_id, key)` (`agent_profiles(key)`). `096_config_revisions`: the table below. The SQL backfill is ASCII-lowercase; a key is opaque after creation, so it need not equal `key_of(name)`. |
| The revisions table | `project_config_revisions (project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE, number INTEGER NOT NULL, origin TEXT NOT NULL CHECK (origin IN ('board','file','revert')), hash TEXT NOT NULL, body TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT (datetime('now')), PRIMARY KEY (project_id, number))`. `body` is the configuration as canonical JSON (`serde_json::to_string` of the `Outline` with `items` empty; keys of every map sorted — `Outline` already serializes maps sorted). `hash` is the SHA-256 of `body`, hex. |
| The key | `key_of(name)`: trim; lowercase (Unicode); every run of characters that are not alphanumeric (`char::is_alphanumeric`) becomes one `-`; leading and trailing `-` removed; cut to 64 chars at a char boundary; empty → `unnamed`. A key in a file must already be of that form (`^[\p{L}\p{N}][\p{L}\p{N}-]{0,63}$`, lowercase) or the file is refused naming the entry. |
| Matching | File entry has `key` → the board row with that key; none → create with that key, unless the key is taken by a row of another name (refused: "roles: key \"dev\" is already the key of \"Dev Team\""); a row with the same *name* but another key → refused ("roles: \"Dev\" is \"dev-team\" on the board; write that key or drop the name"). File entry has no `key` → the board row with that name, as today; none → create with `key_of(name)`, refused if that key is taken. The rule lives once, in `match_rows`, and serves the outline import and the sync alike. |
| What `merge` compares | The `project` entry (fields `name`, `description`, `vocabulary`; `type` and `workflow` are left out and reported as kept with their reason), the four keyed sections, and `automation` as one entry. Objects are compared recursively by dotted path (`autonomy.budgets.tokens_per_day`, `tool_policy.docket.team`); arrays and scalars are leaves. `items` are never read. |
| The three-way rule, per field at path `p` of an entry present on both sides | `board == file` → that; `base == board` → file's (a change); `base == file` → board's (kept); else conflict → `prefer`'s side, or a `Conflict` and the field keeps the board's value in the merged outline. A base entry absent (no revision, or the entry is new on both sides) counts as `base == board` for nothing: `board == file` → that, else conflict. |
| The three-way rule, per entry | In file, not on board: in base and `file == base` → kept absent ("removed on the board since the last revision"); in base and `file != base` → conflict (`prefer=file` creates, `prefer=board` keeps absent); not in base → create. On board, not in file: in base and `board == base` → delete; in base and `board != base` → conflict (`prefer=file` deletes, `prefer=board` keeps); not in base → kept ("created on the board since the last revision", or "no revision yet: export once to make the file authoritative"). |
| What a deletion costs, and what is refused | `usage_of`: a role → `COUNT(*) FROM item_roles WHERE role_id`; a field → `COUNT(*) FROM custom_field_values WHERE field_id`; a sprint → `COUNT(*) FROM items WHERE sprint_id`; a profile → the `(project name, role name)` pairs of `roles` with that `agent_profile_id` in other projects, the names of other projects whose `default_profile_id` is it, and the count of `execution_requests` with that `agent_profile_id` whose `state` is not terminal (`execution.rs` `terminal()`, line 761). A profile with any of the three is a problem (400), not a warning. A built-in profile is never a candidate: it is not in the board's side of the comparison (`to_bundle` leaves built-ins out). |
| The project's own settings in a sync | `name`, `description` and `vocabulary` are applied through `update_project_in` (an `UpdateProject` with those three keys); the `project` block's `type` and `workflow` are never written and are reported kept. |
| The plan's hash | `plan_hash = sha256(body of the merged outline ‖ "\n" ‖ board hash ‖ "\n" ‖ the deletions as "section/key" lines ‖ "\n" ‖ prefer)`, hex. The apply recomputes it from the request body and the live board; a mismatch is 409 `{"error": "The project changed since the preview; preview again"}`. |
| The apply, in order, in one `BEGIN IMMEDIATE` transaction (`execution.rs` line 2910 is the precedent) | record a `board` revision of the live configuration when its hash differs from the last revision's; the deletions (profiles, then roles, fields, sprints — the FKs already `SET NULL` or cascade: `roles.agent_profile_id`, `items.sprint_id`, `item_roles`, `custom_field_values`); `write(tx, …)` with the merged plan; the project's three keys when changed; a `file` (or `revert`) revision whose body is the merged configuration; commit; then `BoardEvent::BoardConfigUpdated { project_id }`. |
| An export records a revision | `GET …/export?scope=bundle\|config` records a `board` revision when the hash differs from the last. `scope=snapshot` and `format=csv` do not. |
| Routes | `GET /api/projects/{id}/config/status` → `{ last_revision: {number, origin, created_at, hash} \| null, board_hash, changes: [Change] }` (two-way `diff(last.body, board)`; `changes: []` and `last_revision: null` when there is none). `POST …/config/plan` and `POST …/config/apply`: body = the file (JSON, or YAML when `Content-Type` mentions yaml, read with `outline::read_outline`, made `pub(crate)`, as the outline routes do, so its warnings — an old file's `usd_per_day` — reach the plan's `warnings`) **or** `?revision=<n>` with an empty body; query `prefer=file\|board` optional; `apply` also needs `plan_hash=<hex>`. `GET …/config/revisions` → `[{number, origin, created_at, hash}]` newest first; `GET …/config/revisions/{n}` → the body as the file (`format=json\|yaml`, default yaml). |
| The plan's answer | `ConfigPlan { plan_hash, base: Option<RevisionSummary>, changes: Vec<Change>, conflicts: Vec<Conflict>, deletions: Vec<Deletion { section, key, name, cost: Vec<String> }>, warnings: Vec<String>, applies: bool }`. `applies` is false while there is a conflict without `prefer`. Problems (anything `check` or the usage rule refuses) are a 400 with every problem, as today. The apply answers `ConfigApplied { revision: RevisionSummary, changes, deletions }`. |
| A key on the wire | `Role`, `Sprint`, `CustomFieldDefinition` and the profile response gain `key: String`; `CreateRole`, `CreateSprint`, `CreateCustomField` and the profile create body gain `key: Option<String>` (checked with the same rule; default `key_of(name)`, 409 when taken). No update route changes a key. |
| The CLI | `tack config <export \| import \| status \| history \| revert>`; `import` and `revert` print the plan and apply unless `--dry-run`; a plan with deletions is applied only with `--yes` (otherwise the plan is printed and the exit code is 1 with "add --yes to apply the N deletions"); `--prefer file\|board`; `--json` everywhere. `tack import` and `tack export --bundle` do not change. |
| The MCP tool | `sync_config { project_id*, file (object) \| revision (number), dry_run (default true in the description), prefer }` → the plan or the apply. `create_outline`'s description gains one sentence: it never deletes; `sync_config` does. |
| Words on screen | key, revision, configuration, kept, removed, conflict, "prefer the file / prefer the board". Never sync, merge, base, upsert, prune. |
| Generated files | A task that changes an API type regenerates and stages them (`./scripts/regen-generated.sh`). When two branches conflict only in them, the integrator regenerates after the merge instead of resolving the conflict by hand. |
| `CHANGELOG.md` | Written by task Z, by hand, in `[Unreleased]`. |

## Rounds

At most two agents building cargo at once (`feedback_agent_load_caps`); the frontend task runs
beside them. Worktrees under `/var/tmp/tack-worktrees/p76-<id>` cut from `develop`,
`CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/<id>`.

| Round | Tasks | Needs |
|---|---|---|
| 0 | ADR 0084, this plan, the roadmap section (integrator) | the maintainer's approval |
| 1 | B (one transaction), M (the key and the comparison, pure) | — |
| 2 | A (keys everywhere), S (the routes and the revisions) | B and M, and Phase 75's V (it edits the `create_profile` and `check` A edits); A and S touch `migrations.rs` on two separate lines each, and neither builds a `Board`: both read B's `board_of` |
| 3 | C (CLI and MCP), F (the screens) | S's routes; F also reads A's `key` on the wire |
| 4 | Z (integrator): docs, changelog, the gate, a run against a copy of the real database | everything |

Ceilings: Sonnet one layer 100 calls, two layers 150. One that reaches it hands off and is
replaced fresh with a shorter brief.

## How a task is handed to an agent

As Phase 74: one agent, one task, one branch from `develop` named `phase76/<id>`; the brief is
the task's section of this file plus these lines, and nothing else:

```
Task <id> from docs/plans/phase-76.md. Read that section, its "The rule for every task" and "Decided" tables, and the files it lists, nothing wider.
The ADR is docs/adr/0084-the-project-file-is-kept-in-sync-not-only-imported.md; its table and this plan's "Decided" table are the decision, do not re-open them.
Touch only the files the task lists. A change needed outside them is a finding: stop and report it.
Fix the fact in its owner and make the other callers read it; never add a second copy of a rule.
Add only the tests the task names, RED first. Prefer a row in an existing table test over a new function.
Do not add a type, trait, option, flag, loop or helper the task does not name.
Never call a real model endpoint; never run docket or any CLI against the operator's own HOME, ~/.docket or ~/Sites.
Tack's words on every screen: key, revision, configuration, kept, removed, conflict; never sync, merge, base, upsert, prune, pod, harness.
Build and test with CARGO_TARGET_DIR=<given>, --build-jobs 4, --test-threads 4; only the binaries you change while working.
Finish by running PATH=$HOME/.cargo/bin:$PATH .githooks/pre-push once and reporting its real output. Do not commit.
Report: what changed, per file, in one line each; what you measured; what you could not do; your tool-call count.
```

## The tasks

### B — one transaction, and the delete that was missing (Sonnet, two layers)

**Files:** `crates/tack-db/src/repo/roles.rs`, `crates/tack-db/src/repo/sprints.rs`,
`crates/tack-db/src/repo/custom_fields.rs`, `crates/tack-db/src/repo/projects.rs` (`create_project`,
`update_project`), `crates/tack-db/src/repo/autonomy.rs` (`get_settings`, `put_settings`),
`crates/tack-db/src/repo/execution.rs` (`write_agent_profile`, a new `delete_agent_profile`),
`crates/tack-db/src/repo/items.rs` (`create_outline_items`), `crates/tack-api/src/handlers/outline.rs`,
`crates/tack-api/src/handlers/runner_admin.rs` (`delete_profile` only, line 1183),
`crates/tack-api/src/error.rs` (`SectionFailed`), a new
`crates/tack-db/tests/repository/config_write.rs` registered in `crates/tack-db/tests/repository.rs`,
`crates/tack-api/tests/handlers/bundle.rs`.

**The pattern, already in the tree:** `insert_item(conn: &mut SqliteConnection, …)` in `items.rs`
line 1041 does the SQL "on whatever connection the caller holds"; `create_outline_items` opens a
transaction and passes `&mut *tx`. Every function below gets the same shape: a free function
`<name>_in(conn: &mut SqliteConnection, …)` that holds the SQL, and the existing pool method or
pool function becomes `let mut conn = pool.acquire().await?; <name>_in(&mut conn, …).await`. The
public signatures and return types do not change, so no caller outside the listed files changes.

**Do:**

- Twins for: `create_role`, `update_role`, `delete_role`, `assign_role_to_item` (`roles.rs`);
  `create_sprint`, `update_sprint`, `update_sprint_status` and a new `delete_sprint` (+ the pool
  method; `DELETE FROM sprints WHERE id = ?`, returns whether a row went) (`sprints.rs`);
  `create_field`, `get_field`, `update_field`, `delete_field`, `set_field_value`
  (`custom_fields.rs`); `create_project`, `update_project` (`projects.rs`); `get_settings`,
  `put_settings` (`autonomy.rs` — `put_settings_in` drops the `pool.begin()` it opens today and
  runs on the given connection; the pool version keeps its transaction by calling the twin inside
  it); `write_agent_profile` and a new `delete_agent_profile` + `delete_agent_profile_in`
  (`DELETE FROM agent_profiles WHERE id = ? AND builtin = 0`, returns whether a row went)
  (`execution.rs`); `create_outline_items_in(conn, …)` with `create_outline_items` keeping its
  transaction around the twin (`items.rs`).
- `runner_admin::delete_profile` calls `state.repo.delete_agent_profile(&id)` instead of its inline
  `DELETE`; the builtin check above it stays as is.
- `handlers/outline.rs`: `write` takes `conn: &mut SqliteConnection` and calls only `_in` twins;
  `write_profiles` and `write_automation` the same. `create_new` opens
  `state.repo.pool().begin_with("BEGIN IMMEDIATE")`, creates the project with `create_project_in`,
  calls `write`, commits, then reads the project back; the `delete_project` compensation and its
  comment go. `into_existing_project` opens the same transaction around `write`, commits, then
  broadcasts as today. Its inline block that builds the `Board` becomes
  `pub(crate) async fn board_of(state, project: &Project) -> ApiResult<OwnedBoard>`, where
  `OwnedBoard` owns what `Board` borrows (fields, profiles, roles, sprints, fleets, level,
  auto_start) and has `fn borrow(&self) -> Board<'_>`: B rewrites that function anyway, so A
  changes the board's rows in one place and S only calls it. `info!(… "Outline written")` stays after the commit.
- `ApiError::SectionFailed { section, source }`: the `kept` field goes; the message is
  "The {section} section could not be written; nothing was changed." The test at `error.rs` line
  175 keeps one row.

**Tests (RED first):**

- `config_write.rs`: on one `BEGIN IMMEDIATE` transaction, `create_role_in`, `create_sprint_in`,
  `create_field_in` succeed and a fourth statement fails (a role for a project id that does not
  exist); after the rollback the three tables hold nothing for the project. A second test:
  `delete_sprint` clears `items.sprint_id` of the sprint's items (FK `SET NULL`,
  `migrations.rs` line 580) and `delete_agent_profile` refuses a built-in (returns `false`).
- `bundle.rs`: `a_file_that_cannot_be_written_is_refused_whole` keeps its claim; read it first — if
  it provoked a failure in a later section to prove "kept", it now proves that nothing is kept
  (every section's count is what it was before the request) and the message names the section.

**Done when:** `cargo nextest run --workspace -E 'package(tack-db) + package(tack-api)'` and the
gate are green; `rg -n "kept" crates/tack-api/src/error.rs` finds nothing.

### M — the key, and the comparison (Sonnet, one layer, pure)

**Files:** a new `crates/tack-core/src/outline/key.rs` (+ `pub mod key;` in `outline.rs`), a new
`crates/tack-core/src/outline/bundle/sync.rs` (+ `pub mod sync;` in `bundle.rs`) with its tests in
`crates/tack-core/src/outline/bundle/sync/tests.rs` (the `mod tests;` style `bundle.rs` line 1049
uses). Nothing else; no I/O; no new dependency (`serde_json` and `sha2` are not needed here).

**Do:**

- `key.rs`: `pub fn key_of(name: &str) -> String` and `pub fn is_key(text: &str) -> bool`, exactly
  as "Decided" says. Doc comment: what the key is for (ADR 0084 §2) in two lines.
- `sync.rs`, public surface and nothing more:

  ```rust
  pub struct Three<'a> { pub base: Option<&'a Outline>, pub board: &'a Outline, pub file: &'a Outline }
  #[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)] #[serde(rename_all = "lowercase")]
  pub enum Prefer { File, Board }
  #[derive(Serialize)] #[serde(rename_all = "snake_case")]
  pub enum Verb { Create, Update, Delete, Keep }
  #[derive(Serialize)] pub struct FieldChange { pub path: String, pub from: Value, pub to: Value }
  #[derive(Serialize)] pub struct Change { pub section: &'static str, pub key: String, pub name: String, pub verb: Verb, pub fields: Vec<FieldChange>, pub reason: Option<String> }
  #[derive(Serialize)] pub struct Conflict { pub section: &'static str, pub key: String, pub name: String, pub path: String, pub base: Value, pub board: Value, pub file: Value }
  pub struct Merged { pub outline: Outline, pub changes: Vec<Change>, pub conflicts: Vec<Conflict> }
  pub fn merge(three: &Three<'_>, prefer: Option<Prefer>) -> Merged
  pub fn diff(from: &Outline, to: &Outline) -> Vec<Change>
  ```

  Sections in this order: `project` (one entry with key `project`, fields `name`, `description`,
  `vocabulary`), `fields`, `profiles`, `roles`, `sprints`, `automation` (one entry with key
  `automation`). An entry's identity is its `key`, else `key_of(name)` (the `key` field lands in
  task A; until then `key` is always absent — write `entry_key(key: Option<&str>, name: &str)` now
  and let A pass the field). Each entry is `serde_json::to_value` of its struct with `key` removed;
  `diff_values(base, board, file, path, &mut fields, &mut conflicts)` recurses into objects and
  treats arrays and scalars as leaves; `set_path(&mut Value, &str, Value)` writes a dotted path.
  The merged outline is built back with `serde_json::from_value` per entry, deletions left out,
  `items` empty, `project.type` and `project.workflow` copied from the board's outline. `Keep`
  entries carry a `reason` from "Decided". Order of `changes`: the file's order, then the board-only
  entries by name. `diff(from, to)` is `merge(&Three { base: Some(from), board: to, file: from }, None)`'s
  changes with every verb seen from `to`'s side (`Keep` becomes the verb of what the board did:
  a board-only entry is `Create`, a board-changed field `Update`, a board-removed entry `Delete`),
  and no conflicts can arise since `file == base`.

**Tests (RED first), in `sync/tests.rs`, one table test with a row per rule plus two scenarios:**

- `key_of`: `"Contenido"` → `contenido`; `"En revisión"` → `en-revisión`; `"  Dev  Team!! "` →
  `dev-team`; `"!!!"` → `unnamed`; 70 letters → 64; `is_key("Dev")` false, `is_key("dev-2")` true.
- The per-field and per-entry rules of "Decided", each a row: `(base, board, file, prefer) →
  (verb, fields, conflicts, reason)`; include `base: None` rows.
- Scenario one: Fabulada's fixture (`crates/tack-api/tests/fixtures/bundles/fabulada.yaml`, read
  with `include_str!` as `bundle/tests.rs` line 5 does) as base and board; the file renames role
  `Contenido` to `Content` under the same key, drops a sprint, changes `automation.on_finish_status`;
  the board (a second copy) changed `roles[Contenido].color` and added a role `Ops`. Expect: one
  `Update` with `fields = [name]` carrying the board's colour in the merged outline, one `Delete`,
  one `Update` on `automation` with path `on_finish_status`, one `Keep` for `Ops` with its reason,
  no conflicts; `merged.outline` serialises with the board's `type` and `workflow`.
- Scenario two: both change `automation.autonomy.budgets.tokens_per_day` to different values:
  one conflict at that path, `applies` would be false; with `Prefer::File` no conflict and the
  file's value; with `Prefer::Board` the board's.
- `diff(from, to)`: a board that added a role and changed a field's `required` yields `Create` and
  `Update` from `to`'s side and nothing else.

**Done when:** `cargo nextest run -p tack-core` and the gate are green.

### A — a key on every configured thing (Sonnet, two layers)

**Files:** `crates/tack-db/src/migrations.rs` (`MIGRATION_095` + its line in `all_migrations()`),
a new `crates/tack-db/tests/migrations/config_keys.rs` registered in
`crates/tack-db/tests/migrations.rs`, `crates/tack-core/src/models.rs` (`Role`, `Sprint`,
`CustomFieldDefinition`, `CreateRole`, `CreateSprint`, `CreateCustomField`),
`crates/tack-core/src/outline.rs` (`BundleField`, `BundleProfile`, `BundleRole`, `BundleSprint`),
`crates/tack-core/src/outline/bundle.rs` (`ProfileRow`, `Board`, `match_names` → `match_rows`,
`check`, `to_bundle`), `crates/tack-core/src/outline/bundle/tests.rs`,
`crates/tack-db/src/repo/roles.rs` (`RoleRow`, `create_role_in`, `list_roles`),
`crates/tack-db/src/repo/sprints.rs` (its row, `create_sprint_in`), `crates/tack-db/src/repo/custom_fields.rs`
(its row mapping, `create_field_in`), `crates/tack-db/src/repo/execution.rs`
(`list_agent_profile_rows`, `write_agent_profile_in`), `crates/tack-api/src/handlers/runner_admin.rs`
(`create_profile` line 893, the profile response struct `list_profiles` returns),
`crates/tack-api/src/handlers/outline.rs` (B's `board_of` and its `OwnedBoard`, `write`'s maps),
`crates/tack-api/src/handlers/templates.rs` (`build_project_from_template` passes roles and profiles
through unchanged — read only to confirm), `crates/tack-api/tests/handlers/bundle.rs`,
`crates/tack-api/tests/handlers/crud.rs` (one row), the generated files.

**Do:**

- The migration and its test as "Decided" says. The test (`config_keys.rs`, the shape of
  `template_bundle.rs`): at `092_drop_usd_per_day` insert two roles named `Dev Team` and `dev-team`
  in one project and a profile `Writer`; run all; the first role's key is `dev-team`, the second's
  `dev-team-<8 chars of its id>`, the profile's `writer`; the unique indexes exist
  (`PRAGMA index_list`).
- Models: `pub key: String` on `Role`, `Sprint`, `CustomFieldDefinition`; `pub key: Option<String>`
  (serde default) on the three `Create*` DTOs with a `#[validate(custom …)]` or a check in the
  repository twin that calls `key::is_key`; the twin writes `input.key.unwrap_or_else(|| key_of(&input.name))`
  and maps a unique violation to the error the handler already turns into 409
  (`is_unique_violation`, as `runner_protocol.rs` does). `ProfileRow` gains `pub key: String`;
  `list_agent_profile_rows` selects it; `write_agent_profile_in` writes it; `create_profile` sets
  `key_of(&input.name)` (or the body's `key`) in its `INSERT`, and the profile response carries it.
- The file: `pub key: Option<String>` as the **first** field of `BundleField`, `BundleProfile`,
  `BundleRole`, `BundleSprint` (so an export reads `- key: contenido` then `name:`), serde default,
  `skip_serializing_if = "Option::is_none"`.
- `bundle.rs`: `pub struct Named { pub id: String, pub key: String, pub name: String }`;
  `Board.roles` and `Board.sprints` become `&[Named]` (the two callers build them from
  `list_roles`/`list_sprints`); `existing_fields` and `existing_profiles` inside `check` become
  `Vec<Named>` too. `match_names` becomes `match_rows(section, rows, key: impl Fn(&T) -> Option<&str>,
  name: impl Fn(&T) -> &str, existing: &[Named], problems)` with the matching rule of "Decided";
  `Upsert::Create` carries the key it will be created with (`Upsert::Create { key: String, value: T }`)
  so `write` passes it to the `Create*` DTO. A file key that fails `is_key` is a problem naming the
  entry. `to_bundle` writes every row's key; `Parts.roles`/`sprints`/`fields` already carry models
  that now have it; profiles from `ProfileRow`.
- `write` (`outline.rs`): `role_ids`, `sprint_ids`, `field_ids`, `profile_ids` keep being keyed by
  **name** (items name their roles, sprints and fields by name — ADR 0081 §1), so nothing changes
  for items; the `Create*` DTOs get `key: Some(created_key)`.

**Tests (RED first):**

- `bundle/tests.rs`, rows in `every_rule_of_the_check_speaks_in_its_own_section`: a role with
  `key: Dev` (refused, not a key); a role whose key is already another board row's key (refused,
  names both); a role with the board row's name and another key (refused); a role with a board
  row's key and a new name (an `Update`, the rename). A new `the_export_writes_every_key` beside
  `the_export_numbers_items_parents_first_and_leaves_ids_and_dates_out`.
- `bundle.rs` (API): in `the_export_is_the_import`, the exported YAML carries `key:` under every
  role, field, profile and sprint, and importing it back into a fresh project and exporting again
  is the same text (the test's existing claim, now with keys). In
  `a_dry_run_writes_nothing_and_a_second_import_adds_and_updates`, a second import that renames
  role `Contenido` to `Content` under `key: contenido` updates the row (same id, new name) and the
  section line says "1 updated".
- `crud.rs`: `POST /api/projects/{id}/roles` with `{ "name": "Dev Team" }` answers `key: "dev-team"`;
  a second with `{ "name": "Other", "key": "dev-team" }` is 409.

**Done when:** `cargo nextest run --workspace -E 'package(tack-core) + package(tack-db) + package(tack-api)'`,
`./scripts/regen-generated.sh` leaves a clean diff after staging, and the gate are green.

### S — the routes, the revisions, the plan (Sonnet, two layers)

**Files:** a new `crates/tack-api/src/handlers/config.rs` (+ `pub mod config;` in `handlers.rs`),
`crates/tack-api/src/router.rs` (five routes beside line 260), `crates/tack-api/src/openapi.rs`
(the paths and the schemas: `ConfigStatus`, `ConfigPlan`, `ConfigApplied`, `RevisionSummary`,
`Deletion`, and `sync::{Change, Conflict, FieldChange, Verb, Prefer}` — the last five need
`#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]` added in `sync.rs`, which is the one
line this task may change there), `crates/tack-db/src/migrations.rs` (`MIGRATION_096` + its line),
a new `crates/tack-db/src/repo/config.rs` (+ `pub mod config;` in `repo.rs`),
`crates/tack-api/src/handlers/export.rs` (`bundle_of`, the revision on export),
`crates/tack-api/src/handlers/templates.rs` (`save_project_as_template` reads `bundle_of`),
`crates/tack-api/src/handlers/outline.rs` (`read_outline` made `pub(crate)`; `board_of` is B's), a new `crates/tack-api/tests/handlers/config.rs`
registered in `crates/tack-api/tests/handlers.rs`, a new `crates/tack-api/tests/fixtures/bundles/fabulada-config-v2.yaml`,
the generated files.

**Do:**

- `repo/config.rs`: `record_revision_in(conn, project_id, origin: &str, body: &str) -> Result<Option<RevisionRow>>`
  (returns `None` when the last revision has the same hash; computes the hash with `sha2`; number =
  last + 1), `last_revision(pool, project_id)`, `list_revisions(pool, project_id)` (summaries, no
  body), `revision(pool, project_id, n) -> Option<RevisionRow>`, and `usage_of(conn, …)` with one
  function per kind returning the "Decided" shapes (`role_usage`, `field_usage`, `sprint_usage`,
  `profile_usage`). `RevisionRow { number, origin, hash, body, created_at }`.
- `export.rs`: `pub(crate) async fn bundle_of(state, project: &Project, items: &[Item]) -> ApiResult<Outline>`
  holds what `bundle_response` builds today (lines 184–242, the `BundleAutonomy` mapping included);
  `bundle_response` calls it and then records a `board` revision (body = the outline with `items`
  cleared) for `scope=bundle` and the new `scope=config` (same handler, items left out of the
  text). `templates::save_project_as_template` calls `bundle_of(state, &project, &[])` and drops its
  own copy of the mapping (lines 440–486).
- `handlers/config.rs`: the five handlers of "Decided". `plan` and `apply` share one
  `async fn planned(state, project_id, body, query) -> ApiResult<(Merged, Plan, Vec<Deletion>, ConfigPlan)>`:
  read the file (body or revision), `bundle_of(project, &[])` as the board, `last_revision` as the
  base, `merge`, then `check(&merged.outline, &board.borrow())` (problems → 400), then the
  deletions' cost (a profile in use → a problem, 400), then the hash. `apply` compares the hash,
  opens `BEGIN IMMEDIATE`, and runs the "Decided" order through B's `write(tx, …)` and the `_in`
  twins; the deletions resolve their ids through `Named.key`. `status` is `diff(last.body, board)`.
  `plan_hash` is one function beside them.
- `BoardEvent::BoardConfigUpdated { project_id }` after the commit.

**Tests (RED first), `tests/handlers/config.rs`, on `bundle::app()`:**

- `an_export_records_a_revision_once`: two `scope=config` exports in a row → one revision; a role
  created through `POST …/roles` then an export → two.
- `a_plan_from_the_second_file_names_every_change_and_its_cost`: import `fabulada.yaml`, export
  (the base), then plan `fabulada-config-v2.yaml` (the fixture: role `Contenido` renamed to
  `Content` under its key, field `Dialecto` removed, sprint `Semana 42` removed,
  `automation.on_finish_status` changed, one new role) after setting a `Dialecto` value on two
  items: the answer has the rename as an `update` with `fields = [name]`, two `deletions` with
  `cost = ["2 values are deleted with it"]` and `["3 cards leave it"]`, `applies: true`, and a
  `plan_hash`; nothing is written (the role still has its old name).
- `an_apply_with_a_stale_hash_is_refused`: plan, create a role on the board, apply with the plan's
  hash → 409 and nothing changed.
- `an_apply_writes_everything_or_nothing_and_leaves_two_revisions`: apply v2 → 200; the role is
  renamed, the field and its values are gone, the sprint is gone and its items have no sprint, the
  automation key changed, the new role exists; `GET …/config/revisions` lists `board` then `file`
  on top; `GET …/config/status` answers `changes: []`. Then `POST …/config/plan?revision=<the
  board one>` shows the inverse, and applying it restores the role's name (a `revert` revision).
- `what_the_board_changed_is_kept_and_a_double_change_is_a_conflict`: after the base, `PATCH` the
  role's colour on the board and change its name in the file → `update` with one field and the
  board's colour kept; change the colour on both sides → one `conflict`, `applies: false`; with
  `?prefer=file` it applies the file's.
- `a_profile_another_project_uses_is_not_removed`: two projects whose roles use one profile; a
  file for the first that drops the profile → 400 naming the second project.
- `a_first_plan_with_no_revision_deletes_nothing`: a fresh project with a role, a file without it →
  the role is `keep` with the "no revision yet" reason.
- `the_project_block_is_applied_for_three_keys_only`: a file with a new `project.name` and a
  `project.workflow` → the name changes, the workflow does not, one warning names Settings → Workflow.

**Done when:** `cargo nextest run --workspace -E 'package(tack-api) + package(tack-db)'`,
`./scripts/regen-generated.sh` leaves a clean diff after staging, and the gate are green.

### C — `tack config`, and the MCP tool (Sonnet, one layer)

**Files:** `crates/tack-cli/src/main.rs` (`Commands::Config { action: ConfigAction }` beside
`Commands::Template`, `enum ConfigAction`, `cmd_config*` beside `cmd_import` line 1452),
`crates/tack-cli/src/client.rs` only if a text `GET` with query parameters is missing (read it
first; `post_text` exists), `crates/tack-cli/src/mcp.rs` (the `sync_config` arm beside
`create_outline` line 217 and its spec beside line 684), `crates/tack-cli/tests/cli_test.rs`
(rows beside the import tests, line 53 on, using the same wiremock pattern).

**Do:**

- `tack config export <project> [-o file] [--format yaml|json]` → `GET …/export?scope=config&format=…`,
  stdout without `-o`. `tack config status <project> [--json]` → prints "Last revision #n (from
  the board, <date>)" or "No revision yet: export once", then one line per change
  (`<verb> <section> <name>: <path> <from> → <to>`). `tack config import <project> <file>
  [--dry-run] [--prefer file|board] [--yes] [--json]` → `POST …/config/plan`, prints the plan
  (changes; conflicts with both values; deletions with their cost lines; warnings), then unless
  `--dry-run`: if conflicts and no `--prefer`, exit 1 "N conflicts: add --prefer file or --prefer
  board, or change the file"; if deletions and no `--yes`, exit 1 "add --yes to apply the N
  deletions"; else `POST …/config/apply?plan_hash=…` and print "Applied as revision #n".
  `tack config history <project> [--json]` → the list. `tack config revert <project> <n>` → the
  same as `import` with `?revision=n` and no body. One `print_plan(&Value)` for `import`, `revert`
  and `status`.
- `sync_config` in `mcp.rs`: `project_id` required, `file` (object) or `revision` (integer), one
  of them; `dry_run` (default true — say so in the description: "pass dry_run: false to apply";
  the tool sends the plan's hash itself on apply by planning first in the same call), `prefer`.
  `create_outline`'s description gains "It never removes anything; to make the file authoritative
  for the configuration use sync_config."
- `tool_specs()` gains the entry; `docs/MCP.md`'s table row is task Z's.

**Tests (RED first), `cli_test.rs`:** `config import --dry-run` posts to `/config/plan` with the
YAML content type and prints a deletion's cost line; `config import` with a plan that has a
deletion and no `--yes` exits 1 and never calls `/config/apply`; with `--yes` it calls apply with
the hash the plan returned; `config revert 3` posts to `/config/plan?revision=3` with an empty
body. MCP: `sync_config` with both `file` and `revision` is an error; with `dry_run` absent it only
plans.

**Done when:** `cargo nextest run -p tack-cli` and the gate are green.

### F — the Configuration card, the plan dialog, the history, the key (Sonnet, frontend)

**Files:** a new `frontend/src/shared/api/config.ts` (+ `config` in `frontend/src/shared/api/index.ts`,
rows in `frontend/src/shared/api/resources.test.ts`), `frontend/src/features/settings/panels/DataPanel.tsx`,
new `frontend/src/features/settings/panels/config/ConfigCard.tsx`, `ConfigPlanDialog.tsx`,
`ConfigHistory.tsx`, `frontend/src/features/settings/panels/panels.test.tsx` (rows beside
`data_panel_file_import_previews_then_writes_on_confirm`, line 211),
`frontend/src/features/settings/panels/RolesPanel.tsx`, `FieldsPanel.tsx`,
`frontend/src/features/agents/runnerFleet/RunnerFleetSection.tsx` (the profile list: the key under
the name, nothing else), `frontend/src/shared/help/routes.ts` (one key, `configFile`, pointing at
the section task Z adds).

**Do:**

- `config.ts`: `status(projectId)`, `plan(projectId, text | { revision }, { prefer? })`,
  `apply(projectId, text | { revision }, { planHash, prefer? })`, `revisions(projectId)`,
  `revisionFile(projectId, n)`; types from `schema.gen.ts` (`ConfigStatus`, `ConfigPlan`,
  `ConfigApplied`, `RevisionSummary`). `data.exportProject` gains `scope: 'config'`.
- `ConfigCard` in `DataPanel`, a third card titled **Configuration**: one status line ("Last
  revision #7 · from a file · 2026-10-18 14:03 · 3 changes on the board since" or "No revision yet
  — export once to make the file authoritative"), with **Show** opening the dialog in read-only mode
  over `status.changes`; buttons **Export configuration (YAML)**, **Import configuration…** (a file
  input, then the dialog over the plan), **History** (the `ConfigHistory` list, each row with
  **Revert to this…** opening the dialog over `plan({ revision })`).
- `ConfigPlanDialog` (a `Modal`, as `ImportFromFile` uses): sections in the plan's order; each
  change one row with a chip — new (green), changed (amber), removed (red), kept (grey, with its
  reason as the row's second line) — and its fields as "path: from → to" lines; **Conflicts** with
  both values and a select "When both changed: ask · prefer the file · prefer the board" that
  re-plans; **Removed** rows carry their cost lines and one checkbox "I have read what these
  removals delete" that gates **Apply**; a control "Choose per conflict" rendered disabled with the
  title "Comes in a later phase; choose a side for all of them or change the file"
  (`feedback_ui_deferred_shown_disabled`); **Apply** disabled while `applies` is false or the box is
  unticked, calls `apply` with the plan's hash, shows the 409 text as the dialog's error and offers
  **Preview again**; on success toasts "Applied as revision #n" and calls `refetch()` from the
  project context.
- `RolesPanel`, `FieldsPanel`, the profile list: the key in the tertiary colour under the name;
  the create forms do not ask for one (the server derives it).

**Tests (RED first):** `resources.test.ts`: the five calls hit their routes with the right method
and content type. `panels.test.tsx`: the card shows the "no revision yet" sentence from a
`status` with `last_revision: null`; a plan with one deletion keeps **Apply** disabled until the
box is ticked, then posts to `/config/apply?plan_hash=…`; a plan with a conflict and no preference
shows the conflict and keeps **Apply** disabled; the role list prints the key under the name.

**Done when:** `cd frontend && npm test -- resources panels` and `npm run build` are green and the
gate passes.

### Z — the integrator: docs, changelog, the gate, a run on a copy of the real database

**Files:** `docs/book/src/user-guide/import-export.md` (a new section "Keeping the configuration
in a file" after "Exporting a project file": export, status, import, what is removed and what it
costs, kept and conflicts, history and revert, the key, the `.tack/project.yaml` convention; "If a
section fails to write" rewritten to "nothing is written"; "The rules that matter" row "Nothing is
deleted" rewritten to "`tack import` deletes nothing; a configuration import removes what the file
removed and shows the cost first"), `docs/book/src/user-guide/cli.md` (`tack config` under "Import
and export"), `docs/book/src/developer/project-file.md` (`key` in every section's table; the merge
rule; the routes; the revisions table), `docs/MCP.md` and `docs/book/src/developer/mcp.md`
(`sync_config`), `CONTRIBUTING.md` ("Writing a Migration": the statements of one name run in one
transaction; cite `apply_migrations`), `CHANGELOG.md` (`[Unreleased]`: Added — the configuration
file, keys, revisions, `tack config`, `sync_config`; Changed — an import writes in one transaction,
an export of a project file records a revision; Removed — the "import again to finish" answer),
`docs/book/src/roadmap.md` (Phase 76 delivered; the gaps: per-field conflicts, items by key, the
workflow in the file), `CLAUDE.md` (the "What's next" line), ADR 0084's status. If Phase 75's Z
is open at the same time, one pass writes both phases' changelog, roadmap and `CLAUDE.md` lines.

**Do:** merge the branches in round order with one gate run per round; after round 3 copy the
maintainer's database (`~/.local/share/tack/tack.db` or the path `tack serve` logs) to the
scratchpad, start `tack serve` on the copy with `TACK_DATABASE_URL`, export Fabulada's
configuration, change a role's name and drop a sprint in the file, plan, read the costs, apply,
check the history and revert; write what was seen in this plan's "How it went" section. Never
against the real database.

**Done when:** the gate is green on `develop`, `mdbook build docs/book` is clean, and the roadmap's
Phase 76 section says what shipped.

## How it went

(Written by task Z.)
