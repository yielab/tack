# The Project File Format

This page is for contributors who change the project file and for integrators who write one from a script or an assistant. It is the reference: every key of every section, the order in which the file is checked and written, what is refused and what only warns, and the routes that carry it. After reading it you can produce a file the board accepts and add a new key without missing a place.

People use the file through [Import, Export & Project Files](../user-guide/import-export.md). The decision behind it is ADR 0022, which extends the outline of ADR 0017; ADR 0025 and its amendment keep it in step with a repository, as [the project folder](#the-project-folder) below describes. See [Architecture Decisions](decisions.md).

## What the file is

The project file is the type `tack_core::outline::Outline`, read from JSON or YAML. It is the outline of items plus eight optional sections.

| Section | Holds | Defined in |
|---|---|---|
| `project` | Name, type, description, vocabulary, workflow. Read only when the file creates a project | `OutlineProject`, `BundleWorkflow`, `BundleStatus` |
| `types` | The project's item types: a catalog with a level each | `BundleType` |
| `fields` | Custom fields of the project | `BundleField` |
| `profiles` | Agent profiles, shared by every project of the board | `BundleProfile` |
| `roles` | Project roles and the profile each one uses | `BundleRole` |
| `sprints` | Sprints | `BundleSprint` |
| `automation` | Where the code is, the default agent, autonomy and auto-start | `BundleAutomation`, `BundleAutonomy`, `BundleDomain`, `BundleBudgets`, `BundleAutoStart` |
| `teams` | The teams the file expects its profiles to name; described, never created | `BundleTeam` |
| `items` | The items | `OutlineItem` |

All types derive `deny_unknown_fields`, so a misspelt key is an error and never data that vanishes. Every reference between sections is a name or a `key`, never an id. A `key` is the name that survives a rename (`tack_core::outline::key`): `key_of(text)` makes one (trim, lowercase, every run of characters that are not alphanumeric becomes one `-`, no `-` at either end, at most 64 characters, `unnamed` when nothing is left) and `is_key(text)` says whether a text is one. The file is pure data: `tack_core::outline::bundle::check` reads it with a description of the board and returns a plan or every problem at once, and `to_bundle` writes a project back as the same file. Neither does any I/O.

## Keys

Optional keys are omitted from an export when empty, so an export has no `null`.

### `project`

| Key | Type | Notes |
|---|---|---|
| `name` | text | Required to create a project, unless the request gives `name` |
| `type` | text | `software`, `web`, `mobile`, `construction`, `personal`, `homework`, `maintenance`, `legal`, `research`, `event` or `custom` (default). `project_type` is also read |
| `description` | text | |
| `vocabulary` | map | Any of the ten vocabulary keys (the words that are not item types); the rest keep the type's words. Exported sorted, and only the words that differ from the type's |
| `workflow` | object | A preset or statuses, never both |
| `workflow.preset` | text | `scrum`, `kanban`, `simple`, `construction`, `legal`, `research` or `event` |
| `workflow.statuses` | list | In order. Each has `name`, `category` (`todo`, `in_progress` or `done`) and optional `wip_limit` |
| `workflow.transitions` | list | Optional `{from, to}` pairs. Absent means any status can move to any other. Beside a `preset`, it replaces the preset's |

On an existing project the whole `project` block is ignored with a warning: the name, workflow and vocabulary are kept.

### `types`

| Key | Type | Notes |
|---|---|---|
| `key` | text | 1 to 40 lowercase letters or digits of any script, `_` or `-`; unique in the section. Items name their type by it |
| `name` | text | What the project calls the type; 1 to 60 characters |
| `level` | text | `group` (holds work, like an epic) or `work` |
| `color` | text | `accent`, `accent2`, `danger`, `warning`, `success` or `neutral`: a token name, never a hex value |
| `default` | bool | The default work type. A file that creates a project needs exactly one, and it is `work`. Omitted from an export unless true |

A file that creates a project takes its `types` whole; a file that adds to a project adds the keys it lacks and takes the `name` and `color` of the ones it has (their `level` and the default stay, with a warning when the level differs). A file that creates a project with no `types` starts with the preset of its type. An item's `type` is read as a key or a name in any case; a word the catalog lacks is added as a work type and listed in the preview under `new_types`.

### `fields`

| Key | Type | Notes |
|---|---|---|
| `key` | text | Optional, first in the entry. Passes `is_key`; see [Matching](#matching-by-key-then-by-name) |
| `name` | text | The reference items use. Unique within the section |
| `type` | text | `text`, `number`, `date`, `boolean`, `select`, `multi_select`, `url`, `email` or `long_text` |
| `description` | text | |
| `required` | bool | An item without a value gets a warning |
| `default` | any | The value a new item starts with |
| `options` | list of text | Required for `select` and `multi_select` |
| `validation` | object | `pattern`, `min_length`, `max_length`, `min`, `max`, `max_items` |

### `profiles`

| Key | Type | Notes |
|---|---|---|
| `key` | text | Optional. A profile is the board's, so its key is unique on the board |
| `name` | text | Matched across the whole board |
| `kind` | text | `implementer`, `reviewer`, `researcher`, `planner` or `custom` (default) |
| `instructions` | text | What the agent is told before the task |
| `tool_policy` | object | `agent` (one of `claude-code`, `codex`, `opencode`, `docket`), `network` (`true` or `false`; absent is `false`: the role's runs start with web tools on; a profile whose `tools` for the agent include `*`, WebFetch or WebSearch starts them on too, except for docket), `tools`, and `docket.team`, `docket.denied_tools`, `docket.pipeline_by_type` |
| `limits` | object | Budgets and the time the agent may take |
| `fleet` | text | A fleet's name. Absent means any computer |
| `summary` | text | One line beside the profile's name |
| `delivers` | text | What the profile delivers, in one line of at most 160 characters. Absent keeps the board's line, or none for a new profile |

### `teams`

Descriptive: an import creates nothing from it, and a section line is not written for it. The preview compares it with what the runners report (see `missing`, below).

| Key | Type | Notes |
|---|---|---|
| `name` | text | docket's project id, `^[a-z0-9][a-z0-9-]{0,63}$`; unique in the section |
| `shape` | text | `build`, `respond`, `operate` or `routine` |
| `delivers` | text | `branch`, `report` or `records` |
| `methods` | list of text | The methods a task may name on the team, named like the team |

An export writes the teams the exported profiles name through `docket.team`, with the shape, delivery and methods the runners report of each; a team no runner reports is written by name alone.

### `roles`

| Key | Type | Notes |
|---|---|---|
| `key` | text | Optional. Unique in the project |
| `name` | text | Unique in the project |
| `color` | text | A CSS color such as `#3b82f6` |
| `icon` | text | |
| `profile` | text | A profile named in the file first, then one the board has, including the built-ins |

### `sprints`

| Key | Type | Notes |
|---|---|---|
| `key` | text | Optional. Unique in the project |
| `name` | text | |
| `goal` | text | |
| `start`, `end` | date | `YYYY-MM-DD` |
| `status` | text | `planning` (default) or `closed`. An import never activates a sprint |

### `automation`

Only the keys the file names change. On an export every key is written.

| Key | Type | Notes |
|---|---|---|
| `code_origin` | text | `folder`, `new_folder`, `url` or `none` |
| `repository` | text | An absolute path for a folder origin, a URL for a `url` origin |
| `default_branch` | text | |
| `workspace_mode` | text | `local_branch`, `in_place` or `clone` |
| `prepare` | text, optional | The install command that replaces the one the lockfile implies; empty turns the install off. Absent leaves the project's setting as it is; one line only |
| `push_after_run` | bool | |
| `default_harness` | text | The agent a run uses when none is named; one of the four kinds above |
| `default_profile` | text | A profile name |
| `default_model` | text | |
| `definition_of_done` | text | |
| `autonomy.level` | text | `L3`, `L4` or `L5`. Lowered to the project's own level |
| `autonomy.domains` | list | `{name, paths, item_types, level}`. A domain's level is lowered the same way |
| `autonomy.verify_require` | list | `criteria`, `suite`, `anti_tautology`, `static_analysis` |
| `autonomy.verify_loop_max_attempts` | integer | |
| `autonomy.budgets` | object | `tokens_per_day`, `auto_merges_per_day`, `queue_items_max`, `waiting_max`; any other key is refused. `waiting_max` is how many open waits for a person (the project's open `needs_you` notifications) the project may have; absent is no limit, and at the limit auto-start starts nothing and every task it lists as waiting says why. A file may set it, because it only holds work back |
| `autonomy.merge_on_done` | bool | |
| `auto_start.enabled` | bool, optional | Never switched on by a file. Absent leaves the project's switch as it is |
| `auto_start.status` | text | The trigger status; a status of the workflow |
| `auto_start.max_open` | integer | At least 1 |

The attestation public key is never in a file.

### `items`

| Key | Type | Notes |
|---|---|---|
| `key` | text | The item's key, its file name in a project folder. An item whose key the project has is updated in its plan fields; absent, the item is added. A new item's key passes `is_key` |
| `ref` | text or number | A name other items use in `parent` and `depends_on`, in this file only; never written by an export. `id` is also read. `key` is not: it is the item's key |
| `title` | text | Required |
| `description` | text | |
| `type` | text | A key or a name of the project's `types` in any case; absent, the default work type. A word the catalog lacks is added as a work type. `item_type` is also read |
| `status` | text | A status of the workflow, any case. Unknown or absent: the first status. Read for a new item, kept for one that exists |
| `priority` | text | `critical` (or `urgent`), `high`, `medium` (default, or `normal`), `low` or `none`. Read for a new item, kept for one that exists |
| `assignee` | text | Read for a new item, kept for one that exists |
| `estimate` | number | |
| `estimate_unit` | text | `story_points` (default), `hours`, `days`, or another word as a custom unit |
| `due_date` | text | `YYYY-MM-DD` or an RFC 3339 timestamp. `due` is also read. Read for a new item, kept for one that exists |
| `tags` | list of text | |
| `recurrence` | text | `weekly`, `monthly` or `quarterly` |
| `parent` | text | Another item's `key`, `ref` or exact title. When adding to an existing project, also the key or the id of an item already in it |
| `depends_on` | list or text | Items that must finish first, by `key`, `ref` or exact title. `blocked_by` is also read. Compared as a sorted set |
| `roles` | list of text | Names of the project's roles. Compared sorted |
| `sprint` | text | A sprint's name. Read for a new item, kept for one that exists |
| `fields` | map | Field name to value |
| `brief` | object | What makes the item done, in brief-v1's shape (`docs/contracts/brief-v1/`) without `item_id`: `definition_of_done` (text), `risk` (`low`, `medium` or `high`), `acceptance` (criteria) and `constraints`. Checked with `tack_core::brief::validate`, as the Brief tab is; a brief with none of its four parts is no brief |

A `ref` wins over a title when both match, and a `key` wins over both. A file holds at most `MAX_OUTLINE_ITEMS` (1000) items.

An item's **plan** is `title`, `description`, `type`, `parent`, `depends_on`, `roles`, `tags`, `estimate`, `estimate_unit`, `recurrence`, `fields` and `brief`. Its **board's keys** are `status`, `priority`, `assignee`, `sprint` and `due_date`: read for a new item, ignored for one that exists (the check warns "status is the board's: kept"), and never written to a folder. An item's run settings never enter a file.

## Order of application

`to_write` in `crates/tack-api/src/handlers/outline.rs` turns the checked plan into one `ProjectWrite`, and `Repository::write_project_file` (`crates/tack-db/src/repo/project_file.rs`) writes it on one `BEGIN IMMEDIATE`, through the repositories' `_in` functions that take the caller's connection. In this order:

1. `project`, for a new project only: the row, its vocabulary (the type's words with the file's on top) and its workflow. For an existing project, the project's own name, description and vocabulary.
2. `types`, when the plan changes the catalog (`set_item_types_in`), so the items that follow find their types.
3. `fields`, then `profiles`, then `roles`, then `sprints`, each in the file's order, each created with its key.
4. `automation`: the project's own keys, and the autonomy settings with the budgets.
5. `items` the file adds, with their dependencies, roles, field values and briefs; then the items it updates in their plan fields, and their briefs (`upsert_brief_in`, so each change is a version with its hash).

A failure anywhere rolls the transaction back and names the section.

## What `check` does

`tack_core::outline::bundle::check(outline, board)` collects problems from every section before it returns, so a writer fixes the whole file in one pass. It runs in this order:

1. `project.vocabulary` keys against the ten known keys, and `project.workflow` (a preset or statuses, never both; a known preset; a category per status; then `WorkflowConfig::validate`).
2. `types` (before the fields): valid and unique keys, a name each, and for a new project exactly one default work type (`check_catalog`).
3. `fields`: a known type, options for `select` and `multi_select`, no repeated or empty name.
4. `profiles`: a profile named like a built-in is dropped with a warning and never changed. Otherwise a known `kind`, a known `tool_policy.agent`, a boolean `tool_policy.network` when present, a `docket.team` matching `^[a-z0-9][a-z0-9-]{0,63}$`, and no repeated name. An unknown `fleet` is a warning and the profile runs on any computer.
5. `roles`: the `profile` must be in the file or on the board. No repeated name.
6. `sprints`: status `planning` or `closed`, dates written `YYYY-MM-DD`, no repeated name.
7. `automation`: the values of the table above; `check_automation` lowers levels and auto-start, and refuses a domain that names a type key the catalog lacks.
8. `items`: `tack_core::outline::resolve` (references, loops, dates, `CreateItem` validation) and then `check_item` (roles, sprints and field values); each item's word is then given the key of its type, adding a work type for a word the catalog lacks.

`Board` describes the project: its workflow, its type catalog (`item_types`, and `new_project`, which says whether the file's `types` replace it), fields, profiles, roles, sprints, the fleets, the known agent kinds (`HARNESS_KINDS` in `handlers/runner_admin.rs`), its autonomy level (L3 for a new project) and whether auto-start is on.

### Matching by key, then by name

For fields, profiles, roles and sprints, `match_rows` pairs each entry with a row of the board by one rule. An entry with a `key` is the board's row with that key, whatever its name, so a new name under the same key is a rename. A key no row has creates a row with that key, unless a row of another name holds it (refused: `roles: key "dev" is already the key of "Dev Team"`), and a row with the same name under another key is refused too (`roles: "Dev" is "dev-team" on the board; write that key or drop the name`). A key that creates a row must pass `is_key`. An entry without a key is the row of the same trimmed name, or creates one with `key_of(name)`, refused if that key is taken. A match is an `Upsert::Update` carrying the row's id; no match is an `Upsert::Create` carrying the key it will be created with. The board's rows the file does not carry are counted as `not_mentioned` and left alone. Built-in profiles are not counted.

**Items** are matched by `key` only. An item without a key is always created (ADR 0017). An item with a key the board has (`Board.items`) is a `PlannedUpdate` of its plan fields, checked as a new item's would be; it is never moved or removed by `tack import`. `parent` and `depends_on` may name a key (of the file or of the board) or a `ref` or an exact title. A file exported and imported twice therefore adds no item. `tack import` and `POST …/outline` never remove anything; removing is the folder's apply, below.

### Errors and warnings

A problem refuses the whole file. A warning lets it through and is reported.

| Refused (problem) | Only a warning |
|---|---|
| An unknown key, a bad type, kind, category, level or date | A status or priority the workflow does not know (the item gets the default) |
| A repeated or empty name in a section | An unknown recurrence word (the task does not repeat) |
| A reference to a role, sprint, field, profile, parent or dependency that is nowhere | A profile named like a built-in (dropped) |
| A parent or `depends_on` loop; an item that is its own parent or dependency | A `fleet` the board lacks |
| A value that fails its field's type or options | An item with no value for a required field |
| `repository` that is not a path or URL for its origin | `autonomy.level` or a domain level above the project's own (lowered) |
| A `status` or `auto_start.status` outside the workflow, in `automation` | `auto_start.enabled: true` when the project has it off (applied as `false`) |
| `max_open` below 1; more than 1000 items | A `project` block in a file added to an existing project (ignored) |
| A new `key` that fails `is_key`, is held by a row of another name, or is used twice; a name that is on the board under another key | A board's key (`status`, `priority`, `assignee`, `sprint`, `due_date`) on an item the board has: kept |
| A brief `brief::validate` refuses (two criteria with the same id, a wrong risk); the problem names the item | |

The levels are lowered to the project's own level, which is L3 for a new project, so a file can never raise autonomy. The warning reads "autonomy.level asked: L4; applied: L3. Raise it from Settings → Automation". The auto-start warning reads "auto_start asked: true; applied: false. Turn it on from Settings → Automation". An `auto_start` block that omits `enabled` leaves the switch as it is, so a file can set the status and the cap without touching it.

### The plan and its summaries

`check` returns a `Plan`:

| Field | Meaning |
|---|---|
| `vocabulary` | The words to apply, or none |
| `types` | The whole catalog the project ends with, when the file changes it; `new_types` names the work types the items' words added |
| `fields`, `profiles`, `roles`, `sprints` | `Vec<Upsert<…>>` in the file's order |
| `automation` | The section with levels and auto-start already lowered |
| `items` | A `ResolvedOutline`: items in written order (parents first), `blocks` as index pairs, and warnings |
| `sections` | One `SectionSummary` per section the file carries |
| `warnings` | Every warning, in order |

`SectionSummary` is `{section, created, updated, not_mentioned, warnings}`. A section appears only when the file carries it. `automation` always reports `updated: 1`; `items` reports `created` (items without a key, or with a key the board lacks) and `updated` (items whose key the board has), and `not_mentioned: 0`.

## How the routes write

`crates/tack-api/src/handlers/outline.rs` serves the routes. The request is read as JSON, or as YAML when `Content-Type` mentions it, and checked whole before anything is written. A refusal is a `400` whose message is the problem, or `N problems in the outline: …` joined with `;`.

- **New project** (`create_new`): the board is empty (`Board` with no fields, roles or sprints, level L3, auto-start off). On `dry_run` the plan is returned and nothing is created. Otherwise one `write_project_file` call creates the project row and everything else in the same transaction, so a failure leaves no project behind.
- **Existing project** (`into_existing_project`): the `project` block is removed first, with the warning. The board is built by `board_of(state, project)` (`OwnedBoard`, which owns what `Board` borrows): the project's fields, roles, sprints and items with their keys, the board's profiles and fleets, and the project's own level and auto-start flag. After a successful write the new items are announced on the board's WebSocket.
- **Failure of a section.** One transaction writes every section. A database error in one becomes `ApiError::SectionFailed { section, source }`, a `500` whose message names the section: "The roles section could not be written; nothing was changed." The cause is logged on the server and never sent to the client. Nothing was written, so running the file again after fixing the cause is the whole recovery.
- **The result** is an `OutlineResult`: `dry_run`, `project`, `created`, `skipped`, `dependencies`, `items` (an `OutlineResultItem` per item: `ref`, `id`, `title`, `status`, `parent_id`), `sections`, `missing` and `warnings`. `missing` (`tack_core::outline::missing`) lists, per entry, `kind` (`team` or `harness`), `name`, `shape` for a team and `fix`: a team the file's `teams` or a profile's `docket.team` names that no active runner reports, with `docket init <name> --blueprint <shape>` and `--recipe <method>` for the first method that is a shipped docket recipe (`docket init` applies one recipe; any shipped recipe can be named per task once the team has the roles it needs); and an agent a profile or `automation.default_harness` runs that no runner reports, with how to install it. It is empty for a CSV and for a file that names none.

### Routes

| Method and path | Purpose |
|---|---|
| `POST /api/projects/outline` | A new project from a file. Query: `dry_run`, `name`, `project_type`, `description` |
| `POST /api/projects/{id}/outline` | A file into an existing project. Query: `dry_run` |
| `POST /api/projects/import-csv`, `POST /api/projects/{id}/import-csv` | The items of a CSV, through the same checks; `sections` is empty |
| `GET /api/projects/{id}/export?scope=bundle&format=yaml` | The project as a file. `format` is `yaml` or `json`. Records a `board` revision when the plan differs from the last |
| `GET /api/projects/{id}/folder` and five more | The project folder; see [The six routes](#the-six-routes) |
| `POST /api/projects/from-template/{id}` | A new project from a template |
| `POST /api/projects/{project_id}/save-as-template` | A template from a project |
| `GET`, `POST /api/templates`; `GET`, `DELETE /api/templates/{id}` | Template CRUD; built-ins cannot be deleted |

`scope` is `snapshot` (the database dump that `POST /api/projects/import` reads) or `bundle`. A bundle with `format=csv` is refused. The schemas in `docs/openapi.json` are `Outline`, `OutlineProject`, `OutlineItem`, `Bundle*`, `OutlineResult`, `OutlineResultItem`, `SectionSummary` and `ProjectTemplate`. The CLI (`tack import`, `tack export --bundle`, `tack init --template`, `tack template save`), the MCP tool `create_outline` (which forwards every section of the file) and Settings → Data → Import all end in these routes. `tack folder` and Settings → Data → Project folder end in the folder routes, and the MCP tool `plan_folder` in `POST …/folder/plan`.

## Export

`handlers/export.rs::bundle_of` gathers the rows (every visible item, paged through, and their briefs read once with `list_item_briefs_for_project`) and `tack_core::outline::bundle::to_bundle(&Parts)` builds the file. `bundle_response`, the folder routes and `save_project_as_template` all read `bundle_of`, so the file is built in one place. It is deterministic: exporting, importing into a new project and exporting again gives the same text. The rules:

- The workflow is always written as statuses in order, never as a preset. The vocabulary carries only the words that differ from the type's, and `types` carries the whole catalog in order.
- Fields sorted by name. Sprints sorted by start date, then name.
- Only the profiles that the project's roles and its default point at, and not the built-ins. A profile's fleet is written by name.
- Items in dependency order (parents first), each with its `key`, naming its parent and blockers by key; `ref` is never written. Only `blocks` dependencies become `depends_on`.
- A medium priority and the story-points unit are omitted. A sprint's `status` is written only when it is `closed`. A due date at midnight is written as a date.
- Every role, field, sprint, profile and item carries its `key`, and an item carries its `brief`. No ids, no timestamps. The attestation public key is never written.

## The project folder

ADR 0025 and its amendment keep a project's plan in a `.tack/` folder beside the code. The folder is the project file split into files; reading it gives the same `Outline` the single file gives, and the same `check` runs on it. The user's view is [Keeping the plan in the repository](../user-guide/import-export.md#keeping-the-plan-in-the-repository).

### The layout

`tack_core::outline::folder` reads and writes it. Nothing there touches a disk: paths are relative to `.tack/`.

| Path | Holds |
|---|---|
| `project.yaml` | The `Outline` with `items` empty, as the single file writes it |
| `items/<key>.md` | `---`, the item's plan fields as YAML, `---`, then the description as Markdown (nothing when it has none) |

`write(&Outline) -> BTreeMap<String, String>` writes every visible item in key order, without `key`, `ref`, `description` (the body) and the board's keys. The key is the file name without `.md`. `read(&BTreeMap<String, String>) -> Result<Outline, Vec<String>>` collects every problem and returns them together, each naming its file: an unknown path, a file without its two `---` lines, a name that fails `is_key`, YAML that does not parse, a `key` or `description` line in an item's YAML, and a folder without `project.yaml` (it would read as a project with nothing in it, and the plan would propose removing everything). Items come back in key order.

### Keys in the store

Migration `115_keys` adds `key TEXT NOT NULL DEFAULT ''` to `roles`, `custom_field_definitions`, `sprints`, `agent_profiles` and `items`, with a partial unique index `(project_id, key) WHERE key <> ''` (`agent_profiles(key)`, a profile being the board's). `migrations::run_all` then calls `fill_missing_keys`, which gives every row with an empty key `key_of(name or title)`, made unique within its project with `-2`, `-3`, … in creation order, in one transaction; it is idempotent. The hidden items of threads and triggers keep `''` and are never in a folder. Every create path writes a key: the four `Create…` bodies take an optional `key` (400 when it fails `is_key`, 409 when taken), and no update route changes one.

### The comparison

`tack_core::outline::bundle::sync` is pure. `merge(&Three { base, board, file }, prefer)` returns the merged outline, the `Change`s and the `Conflict`s; `diff(from, to)` is the two-way form the status route reads. `prefer` is `Prefer::File` or `Prefer::Board`. It compares `project` (`name`, `description`, `vocabulary`; `type` and `workflow` are reported kept), `types`, `fields`, `profiles`, `roles`, `sprints`, `automation` (one entry) and `items` (their plan fields). `teams` is never compared. An entry's identity is its `key`, else `key_of(name)` (`key_of(title)` for an item). Objects are compared by dotted path (`autonomy.budgets.tokens_per_day`, `brief.risk`); arrays and scalars are leaves, except `depends_on` and `roles`, compared sorted.

For a field at path `p` of an entry on both sides:

| Case | Result |
|---|---|
| `board == file` | That value |
| `base == board` | The file's value: a change |
| `base == file` | The board's value: kept |
| All three differ | A `Conflict`, listed whether or not `prefer` settles it; the merged outline takes `prefer`'s side, or the board's when there is none |
| No base entry | `board == file`: that value; otherwise a conflict |

For an entry on one side only:

| Where | Case | Result |
|---|---|---|
| In the file, not on the board | In base and `file == base` | Kept absent: "removed on the board since the last revision" |
| | In base and `file != base` | A conflict (`prefer=file` creates, `prefer=board` keeps absent) |
| | Not in base | Create |
| On the board, not in the file | In base and `board == base` | A removal candidate |
| | In base and `board != base` | A conflict (`prefer=file` removes, `prefer=board` keeps) |
| | Not in base | Kept: "created on the board since the last revision", or "no revision yet: export once to make the folder authoritative" |

`applies` is `conflicts.is_empty() || prefer.is_some()`. `merge` only proposes removals; the route turns each into a removal or a kept row with its reason. An item's board keys in the file are kept with the reason "the board's".

### What a removal costs

`tack_db::repo::project_file::usage_of` is the one owner of what a removal costs and whether it is refused.

| Kind | Cost or refusal |
|---|---|
| Role | The cards that lose it (`item_roles`) |
| Field | The values deleted with it (`custom_field_values`) |
| Sprint | The cards that leave it (`items.sprint_id`) |
| Type | Refused when in use, by the catalog (`set_item_types`) |
| Profile | Refused when a role of another project or another project's default names it, or a run of it is not terminal. A built-in profile is never a candidate |
| Item | Kept, with the reason, when it has a row in `execution_requests`, `comments`, `attachments`, `change_sets` (`anchor_item_id`), `github_links`, or visible children (`items.parent_id`). Otherwise removed, and its cost is its blockers (`dependencies`) |

### The revisions

Migration `117_plan_revisions` adds `project_plan_revisions (project_id, number, origin, hash, body, created_at)`, with `origin` one of `board`, `file` or `revert`. `body` is the outline the folder carries (the configuration plus each item's plan, no board keys) as canonical JSON, and `hash` is its SHA-256 in hex. `record_revision_in` writes one with the next number, and none when the last revision has the same hash. A revision is recorded by `GET …/folder` and `GET …/export?scope=bundle` when the live plan differs from the last (`scope=snapshot` and `format=csv` record none), and by an apply: a `board` revision first when the live plan differs from the last, then a `file` (or `revert`) revision whose body is the merged plan. Revisions are kept for ever.

### The six routes

| Route | Answers |
|---|---|
| `GET /api/projects/{id}/folder` | `{ files: {path: text} }`; records a `board` revision |
| `GET …/folder/status` | `{ last_revision, board_hash, changes }`, `changes` being `diff(last.body, board)` |
| `POST …/folder/plan` | The `FolderPlan`; writes nothing |
| `POST …/folder/apply` | `FolderApplied { revision, changes, removals }` |
| `GET …/folder/revisions` | `[RevisionSummary]`, newest first, without bodies |
| `GET …/folder/revisions/{n}` | `{ files }` of that revision |

`plan` and `apply` take `{ files: {path: text} }` in the body, or `?revision=<n>` with no body to go back to a revision, and `prefer=file|board`. `apply` also needs `plan_hash=<hex>`. They share one function: read the files with `folder::read` (or a revision's body), `bundle_of` as the board, the last revision as the base, `merge`, `check` on the merged outline (a `400` with every problem, as for a file), the removals through `usage_of`, then the hash.

`FolderPlan` is `{ plan_hash, base, changes, conflicts, removals: [{ section, key, name, cost }], warnings, applies }`. `plan_hash` is `sha256(merged body ‖ "\n" ‖ the board's body hash ‖ "\n" ‖ the removals as "section/key" lines ‖ "\n" ‖ prefer)` in hex; `apply` recomputes it from the request and the live board, and a mismatch is a `409` with "The project changed since the preview; preview again".

### Who may call them

Reading routes and `plan` are open to any principal that may read the project. `apply` is for `PrincipalKind::Operator` only: an agent's credential is answered `403` by the agent-credential middleware, before the handler, with "This route is not available to an agent credential; it needs the operator's token." (`agent_access.rs` classifies the plan as a read and leaves the apply to its default, a person's). The handler checks the principal again and answers any other principal that is not the operator with "An agent may preview the project folder; a person applies it".

### The apply

One `write_project_file` transaction, in this order: a `board` revision when the live plan's hash differs from the last revision's; the removals (profiles, then items, roles, fields, sprints); the creates and updates of the merged plan, sections in the file's order, items parents first; the briefs; the project's name, description and vocabulary; a `file` or `revert` revision; commit. Then `BoardConfigUpdated { project_id }` goes out on the board socket, and one `ItemCreated`, `ItemUpdated` or `ItemDeleted` per item touched. An updated item takes the file's roles, field values and blockers exactly: the ones the file drops are removed.

### A change under `.tack/`

`tack_core::autonomy::level_for` answers `(L3, Some("project folder"))` whenever any path of a change starts with `.tack/`, before the domains are read (`PROJECT_FOLDER_DOMAIN`), because a run must not change its own contract. `tack_protocol::checked_files` lists `.tack` among the directories whose content decides how the work is checked, so its files appear under "The agent changed how it is checked" and **Merge** asks for a reason. The board reads the folder only from what a person applies, never from a run's work tree.

## Templates

A template is a project file without items. Migration `091_template_bundle` adds `roles`, `profiles` and `automation` (all text, JSON) to `project_templates`, beside the existing workflow, vocabulary, custom fields and default boards; migration `106_template_teams` adds `teams` (a JSON list of the file's `teams` entries) and migration `107_item_types` adds `item_types` (a JSON list of the file's `types` entries).

- **Create a project from a template** (`build_project_from_template` in `handlers/templates.rs`): the template is turned back into an `Outline` and `create_from_bundle` runs the same code as an import. Afterwards the template's own workflow is applied again, because statuses in a file can only say `custom`.
- **Save a project as a template** (`save_project_as_template`): `bundle_of(…, &[])` supplies the roles, profiles and automation that are stored. A saved template carries keys, so applying it matches the board's rows by key.
- **Built-ins** (`seed_builtin_templates` in `crates/tack-db/src/repo/templates.rs`): "Web product with content", "Library or CLI", "Static content site" and "Digital business". Each is a set of roles with a profile apiece, and all of them set the project at L3 with `merge_on_done` off and carry no repository. Legal, Security and Ops are read-only reviewer profiles. "Digital business" has six departments (Software, Operations, Support, Growth, Finance, Insight); no profile in it names an agent or a team, and its `teams` section lists the four folder teams docket would run them with. The tests in `crates/tack-db/src/repo/templates/tests.rs` assert each template's roles and profiles.
- `tack template create-from` keeps its meaning (a project from a template); `tack template save` stores one.

## The worked example

This is the whole My SaaS file, the fixture the tests read: `crates/tack-api/tests/fixtures/bundles/my-saas.yaml`. Its comments say what each rule does. Imported into a new project, it is applied with autonomy L3 and auto-start off, with a warning for each.

```yaml
{{#include ../../../../crates/tack-api/tests/fixtures/bundles/my-saas.yaml}}
```

## How to add a key

Take a new optional key on `automation`. Six places change, and `deny_unknown_fields` means a key missing from the first one is refused everywhere.

1. **The type.** Add the field to the struct in `crates/tack-core/src/outline.rs` (`BundleAutomation`), with `#[serde(default, skip_serializing_if = "Option::is_none")]` so an export stays free of `null`.
2. **The check.** Validate it in `crates/tack-core/src/outline/bundle.rs` (`check_automation` for this section, `check` or `check_item` for the others). Push a problem to refuse, a warning to allow. If it can raise autonomy or switch something on, lower it as `lowered_level` does.
3. **The write.** Apply it in `crates/tack-api/src/handlers/outline.rs` (`write_automation` here), and name the section in `at("…")` so a failure says where.
4. **The export.** Read it back in `to_bundle`, and gather any new row in `Parts` and `bundle_of` in `handlers/export.rs`. The export must round-trip to the same text. The comparison of [the project folder](#the-project-folder) reads the struct's fields by their dotted path, so a new plan field is compared with no change; a field only the board writes goes in `ITEM_BOARD_KEYS` or `PROJECT_BOARD_KEYS` in `sync.rs`.
5. **The stored forms, when they apply.** A key that a template should carry needs a column in `project_templates` (a migration), `build_project_from_template` and `save_project_as_template`.
6. **The contract and the tests.** Run `./scripts/regen-generated.sh` (`docs/openapi.json`, `schema.gen.ts`); add the key to the My SaaS fixture with a comment; add a row to the checking tests in `crates/tack-core/src/outline/bundle/tests.rs` and a handler test in `crates/tack-api/tests/handlers/bundle.rs`; and add the key to this page's tables.

A recipe with the commands is in [Adding Features](adding-features.md#add-a-key-to-the-project-file).
