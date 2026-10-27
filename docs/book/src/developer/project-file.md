# The Project File Format

This page is for contributors who change the project file and for integrators who write one from a script or an assistant. It is the reference: every key of every section, the order in which the file is checked and written, what is refused and what only warns, and the routes that carry it. After reading it you can produce a file the board accepts and add a new key without missing a place.

People use the file through [Import, Export & Project Files](../user-guide/import-export.md). The decision behind it is ADR 0022, which extends the outline of ADR 0017; see [Architecture Decisions](decisions.md).

## What the file is

The project file is the type `tack_core::outline::Outline`, read from JSON or YAML. It is the outline of items plus six optional sections.

| Section | Holds | Defined in |
|---|---|---|
| `project` | Name, type, description, vocabulary, workflow. Read only when the file creates a project | `OutlineProject`, `BundleWorkflow`, `BundleStatus` |
| `fields` | Custom fields of the project | `BundleField` |
| `profiles` | Agent profiles, shared by every project of the board | `BundleProfile` |
| `roles` | Project roles and the profile each one uses | `BundleRole` |
| `sprints` | Sprints | `BundleSprint` |
| `automation` | Where the code is, the default agent, autonomy and auto-start | `BundleAutomation`, `BundleAutonomy`, `BundleDomain`, `BundleBudgets`, `BundleAutoStart` |
| `items` | The items | `OutlineItem` |

All types derive `deny_unknown_fields`, so a misspelt key is an error and never data that vanishes. Every reference between sections is a name, never an id. The file is pure data: `tack_core::outline::bundle::check` reads it with a description of the board and returns a plan or every problem at once, and `to_bundle` writes a project back as the same file. Neither does any I/O.

## Keys

Optional keys are omitted from an export when empty, so an export has no `null`.

### `project`

| Key | Type | Notes |
|---|---|---|
| `name` | text | Required to create a project, unless the request gives `name` |
| `type` | text | `software`, `web`, `mobile`, `construction`, `personal`, `homework`, `maintenance`, `legal`, `research`, `event` or `custom` (default). `project_type` is also read |
| `description` | text | |
| `vocabulary` | map | Any of the 16 vocabulary keys; the rest keep the type's words. Exported sorted, and only the words that differ from the type's |
| `workflow` | object | A preset or statuses, never both |
| `workflow.preset` | text | `scrum`, `kanban`, `simple`, `construction`, `legal`, `research` or `event` |
| `workflow.statuses` | list | In order. Each has `name`, `category` (`todo`, `in_progress` or `done`) and optional `wip_limit` |
| `workflow.transitions` | list | Optional `{from, to}` pairs. Absent means any status can move to any other. Beside a `preset`, it replaces the preset's |

On an existing project the whole `project` block is ignored with a warning: the name, workflow and vocabulary are kept.

### `fields`

| Key | Type | Notes |
|---|---|---|
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
| `name` | text | Matched across the whole board |
| `kind` | text | `implementer`, `reviewer`, `researcher`, `planner` or `custom` (default) |
| `instructions` | text | What the agent is told before the task |
| `tool_policy` | object | `agent` (one of `claude-code`, `codex`, `opencode`, `docket`), `network` (`true` or `false`; absent is `false`: the role's runs start with web tools on), `tools`, and `docket.team`, `docket.denied_tools`, `docket.pipeline_by_type` |
| `limits` | object | Budgets and the time the agent may take |
| `fleet` | text | A fleet's name. Absent means any computer |
| `summary` | text | One line beside the profile's name |
| `delivers` | text | What the profile delivers, in one line of at most 160 characters. Absent keeps the board's line, or none for a new profile |

### `roles`

| Key | Type | Notes |
|---|---|---|
| `name` | text | Unique in the project |
| `color` | text | A CSS color such as `#3b82f6` |
| `icon` | text | |
| `profile` | text | A profile named in the file first, then one the board has, including the built-ins |

### `sprints`

| Key | Type | Notes |
|---|---|---|
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
| `on_finish_status` | text | A status of the workflow, any case |
| `definition_of_done` | text | |
| `autonomy.level` | text | `L3`, `L4` or `L5`. Lowered to the project's own level |
| `autonomy.domains` | list | `{name, paths, item_types, level}`. A domain's level is lowered the same way |
| `autonomy.verify_require` | list | `criteria`, `suite`, `anti_tautology`, `static_analysis` |
| `autonomy.verify_loop_max_attempts` | integer | |
| `autonomy.budgets` | object | `tokens_per_day`, `auto_merges_per_day`, `queue_items_max`. An older file's `usd_per_day` is read and ignored, with a warning in the preview |
| `autonomy.merge_on_done` | bool | |
| `auto_start.enabled` | bool, optional | Never switched on by a file. Absent leaves the project's switch as it is |
| `auto_start.status` | text | The trigger status; a status of the workflow |
| `auto_start.max_open` | integer | At least 1 |

The attestation public key is never in a file.

### `items`

| Key | Type | Notes |
|---|---|---|
| `ref` | text or number | A name other items use in `parent` and `depends_on`. `id` and `key` are also read |
| `title` | text | Required |
| `description` | text | |
| `type` | text | `epic`, `feature`, `task` (default), `subtask`, `bug`, `requirement`, or any other word as a custom type. `item_type` is also read |
| `status` | text | A status of the workflow, any case. Unknown or absent: the first status |
| `priority` | text | `critical` (or `urgent`), `high`, `medium` (default, or `normal`), `low` or `none` |
| `assignee` | text | |
| `estimate` | number | |
| `estimate_unit` | text | `story_points` (default), `hours`, `days`, or another word as a custom unit |
| `due_date` | text | `YYYY-MM-DD` or an RFC 3339 timestamp. `due` is also read |
| `tags` | list of text | |
| `recurrence` | text | `weekly`, `monthly` or `quarterly` |
| `parent` | text | Another item's `ref` or exact title. When adding to an existing project, also the id of an item already in it |
| `depends_on` | list or text | Items that must finish first, by `ref` or exact title. `blocked_by` is also read |
| `roles` | list of text | Names of the project's roles |
| `sprint` | text | A sprint's name |
| `fields` | map | Field name to value |

A `ref` wins over a title when both match. A file holds at most `MAX_OUTLINE_ITEMS` (1000) items.

## Order of application

`write` in `crates/tack-api/src/handlers/outline.rs` applies the checked plan in this order:

1. `project`, for a new project only: its vocabulary (the type's words with the file's on top) and its workflow.
2. `fields`, then `profiles`, then `roles`, then `sprints`, each in the file's order.
3. `automation`.
4. `items`, in one transaction (`Repository::create_outline_items`), with their dependencies; then each item's roles and field values.

## What `check` does

`tack_core::outline::bundle::check(outline, board)` collects problems from every section before it returns, so a writer fixes the whole file in one pass. It runs in this order:

1. `project.vocabulary` keys against the 16 known keys, and `project.workflow` (a preset or statuses, never both; a known preset; a category per status; then `WorkflowConfig::validate`).
2. `fields`: a known type, options for `select` and `multi_select`, no repeated or empty name.
3. `profiles`: a profile named like a built-in is dropped with a warning and never changed. Otherwise a known `kind`, a known `tool_policy.agent`, a boolean `tool_policy.network` when present, a `docket.team` matching `^[a-z0-9][a-z0-9-]{0,63}$`, and no repeated name. An unknown `fleet` is a warning and the profile runs on any computer.
4. `roles`: the `profile` must be in the file or on the board. No repeated name.
5. `sprints`: status `planning` or `closed`, dates written `YYYY-MM-DD`, no repeated name.
6. `automation`: the values of the table above; `check_automation` lowers levels and auto-start.
7. `items`: `tack_core::outline::resolve` (references, loops, dates, `CreateItem` validation) and then `check_item` (roles, sprints and field values).

`Board` describes the project: its workflow, fields, profiles, roles, sprints, the fleets, the known agent kinds (`HARNESS_KINDS` in `handlers/runner_admin.rs`), its autonomy level (L3 for a new project) and whether auto-start is on.

### Matching by name

For fields, profiles, roles and sprints, `match_names` pairs each entry with the board's row of the same trimmed name. A match is an `Upsert::Update` carrying the row's id, no match is an `Upsert::Create`. The board's names the file does not carry are counted as `not_mentioned` and left alone. Nothing is ever deleted by a file. Built-in profiles are not counted.

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
| A `status`, `on_finish_status` or `auto_start.status` outside the workflow, in `automation` | `auto_start.enabled: true` when the project has it off (applied as `false`) |
| `max_open` below 1; more than 1000 items | A `project` block in a file added to an existing project (ignored) |

The levels are lowered to the project's own level, which is L3 for a new project, so a file can never raise autonomy. The warning reads "autonomy.level asked: L4; applied: L3. Raise it from Settings → Automation". The auto-start warning reads "auto_start asked: true; applied: false. Turn it on from Settings → Automation". An `auto_start` block that omits `enabled` leaves the switch as it is, so a file can set the status and the cap without touching it.

### The plan and its summaries

`check` returns a `Plan`:

| Field | Meaning |
|---|---|
| `vocabulary` | The words to apply, or none |
| `fields`, `profiles`, `roles`, `sprints` | `Vec<Upsert<…>>` in the file's order |
| `automation` | The section with levels and auto-start already lowered |
| `items` | A `ResolvedOutline`: items in written order (parents first), `blocks` as index pairs, and warnings |
| `sections` | One `SectionSummary` per section the file carries |
| `warnings` | Every warning, in order |

`SectionSummary` is `{section, created, updated, not_mentioned, warnings}`. A section appears only when the file carries it. `automation` always reports `updated: 1`; `items` reports `created` and `updated: 0`, since a file never updates an item.

## How the routes write

`crates/tack-api/src/handlers/outline.rs` serves the routes. The request is read as JSON, or as YAML when `Content-Type` mentions it, and checked whole before anything is written. A refusal is a `400` whose message is the problem, or `N problems in the outline: …` joined with `;`.

- **New project** (`create_new`): the board is empty (`Board` with no fields, roles or sprints, level L3, auto-start off). On `dry_run` the plan is returned and nothing is created. Otherwise the project row is created first, then `write` runs. Any failure deletes the project again (`delete_project`), so nothing remains.
- **Existing project** (`into_existing_project`): the `project` block is removed first, with the warning. The board is the project's fields, roles, sprints, the board's profiles and fleets, and the project's own level and auto-start flag. After a successful write the new items are announced on the board's WebSocket.
- **Failure of a section.** Each configuration section is a handful of rows written one by one. A database error in one becomes `ApiError::SectionFailed { section, kept, source }`, a `500` whose message names the section: "The roles section could not be written. What the file wrote before it stays, so importing it again finishes the job." On a new project `kept` is `false` and the message ends "so nothing was created". The cause is logged on the server and never sent to the client. Every section matches by name, so running the file again finishes the job.
- **The result** is an `OutlineResult`: `dry_run`, `project`, `created`, `skipped`, `dependencies`, `items` (an `OutlineResultItem` per item: `ref`, `id`, `title`, `status`, `parent_id`), `sections` and `warnings`.

### Routes

| Method and path | Purpose |
|---|---|
| `POST /api/projects/outline` | A new project from a file. Query: `dry_run`, `name`, `project_type`, `description` |
| `POST /api/projects/{id}/outline` | A file into an existing project. Query: `dry_run` |
| `POST /api/projects/import-csv`, `POST /api/projects/{id}/import-csv` | The items of a CSV, through the same checks; `sections` is empty |
| `GET /api/projects/{id}/export?scope=bundle&format=yaml` | The project as a file. `format` is `yaml` or `json` |
| `POST /api/projects/from-template/{id}` | A new project from a template |
| `POST /api/projects/{project_id}/save-as-template` | A template from a project |
| `GET`, `POST /api/templates`; `GET`, `DELETE /api/templates/{id}` | Template CRUD; built-ins cannot be deleted |

`scope` is `snapshot` (the database dump that `POST /api/projects/import` reads) or `bundle`. A bundle with `format=csv` is refused. The schemas in `docs/openapi.json` are `Outline`, `OutlineProject`, `OutlineItem`, `Bundle*`, `OutlineResult`, `OutlineResultItem`, `SectionSummary` and `ProjectTemplate`. The CLI (`tack import`, `tack export --bundle`, `tack init --template`, `tack template save`), the MCP tool `create_outline` (which forwards every section of the file) and Settings → Data → Import all end in these routes.

## Export

`handlers/export.rs::bundle_response` gathers the rows and `tack_core::outline::bundle::to_bundle(&Parts)` builds the file. It is deterministic: exporting, importing into a new project and exporting again gives the same text. The rules:

- The workflow is always written as statuses in order, never as a preset. The vocabulary carries only the words that differ from the type's.
- Fields sorted by name. Sprints sorted by start date, then name.
- Only the profiles that the project's roles and its default point at, and not the built-ins. A profile's fleet is written by name.
- Items in dependency order (parents first) with `ref` set to `1` to `n`. Only `blocks` dependencies become `depends_on`.
- A medium priority and the story-points unit are omitted. A sprint's `status` is written only when it is `closed`. A due date at midnight is written as a date.
- No ids, no timestamps, no briefs and no keys. The attestation public key is never written.

## Templates

A template is a project file without items. Migration `091_template_bundle` adds `roles`, `profiles` and `automation` (all text, JSON) to `project_templates`, beside the existing workflow, vocabulary, custom fields and default boards.

- **Create a project from a template** (`build_project_from_template` in `handlers/templates.rs`): the template is turned back into an `Outline` and `create_from_bundle` runs the same code as an import. Afterwards the template's own workflow is applied again, because statuses in a file can only say `custom`.
- **Save a project as a template** (`save_project_as_template`): `to_bundle` supplies the roles, profiles and automation that are stored.
- **Built-ins** (`seed_builtin_templates` in `tack-db/src/repo/templates.rs`): "Web product with content" (6 roles, 6 profiles), "Library or CLI" (4, 4) and "Static content site" (4, 4). They set the project at L3 with `merge_on_done` off and carry no repository. Legal, Security and Ops are read-only reviewer profiles. The counts are asserted in `tack-db/src/repo/templates/tests.rs`.
- `tack template create-from` keeps its meaning (a project from a template); `tack template save` stores one.

## The worked example

This is the whole Meridian file, the fixture the tests read: `crates/tack-api/tests/fixtures/bundles/meridian.yaml`. Its comments say what each rule does. Imported into a new project, it is applied with autonomy L3 and auto-start off, with a warning for each.

```yaml
{{#include ../../../../crates/tack-api/tests/fixtures/bundles/meridian.yaml}}
```

## How to add a key

Take a new optional key on `automation`. Six places change, and `deny_unknown_fields` means a key missing from the first one is refused everywhere.

1. **The type.** Add the field to the struct in `crates/tack-core/src/outline.rs` (`BundleAutomation`), with `#[serde(default, skip_serializing_if = "Option::is_none")]` so an export stays free of `null`.
2. **The check.** Validate it in `crates/tack-core/src/outline/bundle.rs` (`check_automation` for this section, `check` or `check_item` for the others). Push a problem to refuse, a warning to allow. If it can raise autonomy or switch something on, lower it as `lowered_level` does.
3. **The write.** Apply it in `crates/tack-api/src/handlers/outline.rs` (`write_automation` here), and name the section in `at("…")` so a failure says where.
4. **The export.** Read it back in `to_bundle`, and gather any new row in `Parts` and `bundle_response` in `handlers/export.rs`. The export must round-trip to the same text.
5. **The stored forms, when they apply.** A key that a template should carry needs a column in `project_templates` (a migration), `build_project_from_template` and `save_project_as_template`.
6. **The contract and the tests.** Run `./scripts/regen-generated.sh` (`docs/openapi.json`, `schema.gen.ts`); add the key to the Meridian fixture with a comment; add a row to the checking tests in `crates/tack-core/src/outline/bundle/tests.rs` and a handler test in `crates/tack-api/tests/handlers/bundle.rs`; and add the key to this page's tables.

A recipe with the commands is in [Adding Features](adding-features.md#add-a-key-to-the-project-file).
