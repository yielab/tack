# Import, Export & Project Files

This page is for anyone who wants to move work into or out of Tack. After reading it you will know which kind of file to use, how to bring a whole project in from one file, and how to take one out again.

The examples use `http://127.0.0.1:3210` and a server started with `tack serve`. The `tack` command finds a local server and its token on its own. Every `curl` request to a server that requires a token needs `-H "Authorization: Bearer <token>"`; the [Administrator Guide](../admin-guide/administration.md) explains tokens.

---

## Which one should I use?

| I want to… | Use | Where |
|---|---|---|
| Move a whole project's setup: workflow, vocabulary, fields, agent profiles, roles, sprints, automation and items | The **project file** | [The project file](#the-project-file) |
| Start new projects that all share the same roles, agents and automation | A **template** | [Templates](#templates) |
| Create a project from a plan: a spreadsheet, a script or an AI assistant's list of tasks | An **outline** or a **CSV** | [Items only: outline and CSV](#items-only-outline-and-csv) |
| Seed a project from work already tracked elsewhere | **GitHub** or **Linear** import | [Importing GitHub and Linear issues](#importing-github-and-linear-issues) |
| Keep a copy of one project's items, sprints, dependencies and briefs, or move it to another Tack | A **JSON or YAML snapshot** | [Snapshots and CSV export](#snapshots-and-csv-export) |
| Edit the item list in a spreadsheet and bring it back | A **CSV** export, then CSV import | [Snapshots and CSV export](#snapshots-and-csv-export) |

A snapshot keeps items; a project file keeps the setup around them. For a copy of the whole board, database included, see [Backup & Restore](../admin-guide/backup-restore.md).

---

## The project file

A **project file** is one YAML or JSON document that holds a project: its workflow and vocabulary, custom fields, agent profiles, roles, sprints, automation settings and items. You can write it by hand, generate it, ask an AI assistant for it, or export it from a project you already have and edit that.

### What it holds

| Section | What it describes |
|---|---|
| `project` | Name, type, description, the project's own words (vocabulary) and its workflow, either a preset or the statuses in order. |
| `fields` | Custom fields: name, type, options, whether a value is required. |
| `profiles` | Agent profiles: name, kind, instructions, which agent runs it, its team, its fleet and its limits. |
| `roles` | Project roles: name, color and the profile each role uses. |
| `sprints` | Name, goal and dates. A sprint is created planned or closed, never active. |
| `automation` | The project's folder or repository, branch, how the agent works, default profile and model, the status a finished run moves to, the definition of done, autonomy and auto-start. |
| `items` | The items, with their hierarchy and dependencies, plus the role, sprint, field values and repeat of each. |

Every section is optional. A file with only `items` is an outline. The full list of keys, with their types and defaults, is in [The Project File Format](../developer/project-file.md).

### A small example

This is a shortened version of the Fabulada example in the Developer Guide. It sets a workflow, one field, one agent profile for a docket team, one role, one sprint, some automation and three items.

```yaml
project:
  name: Fabulada
  type: web
  description: Dialect-native bedtime stories, read aloud by families.
  vocabulary:
    task: Pieza
    sprint: Semana
  workflow:
    statuses:
      - { name: Ideas, category: todo }
      - { name: Por hacer, category: todo }
      - { name: En curso, category: in_progress, wip_limit: 5 }
      - { name: En revisión, category: in_progress }
      - { name: Hecho, category: done }

fields:
  - { name: Dialecto, type: select, required: true, options: [rioplatense, mexicano, colombiano, chileno, peninsular] }

profiles:
  - name: Fabulada content writer
    kind: custom
    instructions: >
      Write and edit stories, scripts and newsletter copy in the dialect the task names.
      Keep every sentence readable aloud by a parent at bedtime.
    tool_policy:
      agent: docket
      tools: { docket: ["*"] }
      docket:
        team: fab-content
    limits: { budgets: { tokens: 300000 }, timeout_seconds: 1800 }

roles:
  - { name: Contenido, color: "#f59e0b", profile: Fabulada content writer }

sprints:
  - { name: Semana 42, goal: Lanzar la colección rioplatense, start: 2026-10-12, end: 2026-10-18 }

automation:
  default_profile: Fabulada content writer
  on_finish_status: En revisión
  definition_of_done: >
    Content pieces carry their dialect and read aloud without a stumble.
  auto_start:
    enabled: true                 # accepted, applied as off, with a warning
    status: Por hacer
    max_open: 3

items:
  - { ref: C1, title: Colección rioplatense, type: epic, priority: high, roles: [Contenido], fields: { Dialecto: rioplatense } }
  - ref: S1
    title: El zorro que no podía dormir
    type: feature
    parent: C1
    status: Por hacer
    priority: high
    roles: [Contenido]
    sprint: Semana 42
    fields: { Dialecto: rioplatense }
    estimate: 3
    estimate_unit: hours
    description: Un cuento de 600 palabras, voseo, para leer en seis minutos.
  - { ref: S2, title: La luna de Rosario, type: feature, parent: C1, status: Por hacer, roles: [Contenido], sprint: Semana 42, fields: { Dialecto: rioplatense } }
```

Items refer to each other by `ref`, to roles, sprints and fields by name, and a role refers to its profile by name. A profile's `tool_policy.docket.team` names the docket team that runs it; see [Running Agents on Tasks](running-agents.md).

### How to import it

**In the web UI.** Open **Project settings → Data → Import**.

- **New project from a file…** creates a project from the file.
- **Add items from a file…** reads the file into the project you are in. Its roles, fields, profiles, sprints and automation are applied to that project too.

Both show a preview first. Nothing is written until you confirm with **Create project** or **Add items**.

**From the command line.**

```sh
tack import project.yaml --dry-run      # check the file and show the preview, write nothing
tack import project.yaml                # create the project the file names
tack import project.yaml -p <project>   # read the file into an existing project
```

The format comes from the extension: `.csv`, `.yaml` or `.yml`, otherwise JSON. `--name` and `--type` override the file's `project` block for a new project.

**From an AI assistant (MCP).** With `tack mcp` set up (see [the MCP guide](https://github.com/yielab/tack/blob/develop/docs/MCP.md)), the assistant calls `create_outline` with `project` to create a project or `project_id` to add to one, and with `items` plus any other section of the file. Ask it to pass `dry_run: true` first.

**From the API.** `POST /api/projects/outline` creates a project and `POST /api/projects/{id}/outline` reads a file into one. Both take JSON, or YAML when `Content-Type` mentions `yaml`, and both take `dry_run=true`.

```sh
curl -X POST "http://127.0.0.1:3210/api/projects/outline?dry_run=true" \
  -H "Content-Type: application/x-yaml" --data-binary @project.yaml
```

### The preview

The preview lists each section the file carries, then the items:

- **Section lines** read like "Roles: 2 new, 5 updated, 3 not mentioned". "New" were created, "updated" matched a name Tack already had, and "not mentioned" are names the project has that the file does not carry. Those are left alone.
- **Warnings** appear under the section they belong to. The web preview repeats them under "Read differently than written".
- **Items** are listed with the status each will start in, and the number of dependencies.

For the example above, `tack import --dry-run` prints one line per section, in the order fields, profiles, roles, sprints, automation, items, and a warning that auto-start was asked on but applied as off.

If the file cannot be imported, the preview shows **This file cannot be imported** and lists every problem at once, so you can fix the whole file in one pass.

### The rules that matter

- **Names are the references.** An item names its roles, sprint and field values. A role's `profile` names a profile of the same file first, then one Tack already has. An unknown key anywhere is refused, so a misspelt key is reported instead of lost.
- **The whole file is checked before anything is written.** Either all of it can be written or none of it is.
- **Nothing is deleted.** Importing into an existing project adds new names and updates the names the file mentions, using only the keys it gives. Roles, fields, sprints and profiles the file does not mention stay as they are. Items in a file are always added.
- **The `project` block is read only for a new project.** For an existing one the name, workflow and vocabulary are kept, with a warning that says where to change them: Settings.
- **Built-in profiles are never changed.** A role may use Implementer, Reviewer, Researcher or Planner. A file that redefines one gets a warning and the built-in stays.
- **A file cannot raise autonomy or switch auto-start on.** A level above the project's current one is applied at the project's current level, and `auto_start: enabled: true` is applied as off unless the project already has it on. Each time the preview says what was asked and what was applied. Raise it yourself from **Settings → Automation**; see [Automating a Project](automation.md).
- **A new project starts at L3.** So a file that asks for L4 on a new project gets L3 and a warning.

### What is refused, and what is only a warning

The file is **refused** for:

- an unknown key, or a value of the wrong kind;
- a section entry with no name, or with a name used twice;
- a vocabulary word that is not one of the 16 keys;
- a workflow that gives both a preset and statuses, neither, an unknown preset, a status category that is not `todo`, `in_progress` or `done`, a duplicate status name, or no status in one of those three categories;
- a field of an unknown type, a select field without options, or an item value of the wrong type or outside the field's options;
- a profile with an unknown kind, an unknown agent, or a team name that is not lowercase letters, digits and hyphens;
- a role that names a profile found neither in the file nor on the board;
- a sprint with a date that is not `YYYY-MM-DD` or a status other than `planning` or `closed`;
- an automation setting with an unknown code origin, workspace mode or default agent, a folder that is not an absolute path, a URL that is not a URL, an unknown default profile, a finish or auto-start status the workflow does not have, an auto-start cap below 1, or a level other than L3, L4 or L5;
- an item that names a role, sprint or field found neither in the file nor in the project;
- the item rules of the outline: no title, a `ref` used twice, a `parent` or `depends_on` that names no item or an ambiguous title, a parent or dependency loop, a due date that is not a date, a title over 500 characters, a negative estimate, more than 20 tags, or more than 1,000 items.

The file is **accepted with a warning** for:

- a built-in profile it tries to redefine;
- a fleet name the board does not have: the profile then runs on any computer;
- an autonomy level or auto-start the import lowered;
- an item whose status the workflow does not have: it starts in the first status;
- an unknown priority, which becomes medium;
- an item with no value for a required field;
- a CSV column Tack does not read.

A spreadsheet written for another tool should not have to be rewritten to import it, so the preview shows each warning before anything is written.

### If a section fails to write

The checks catch problems in the file. A failure while writing, such as a full disk, is different. The answer then names the section: "The roles section could not be written."

- **A new project:** nothing was created, because the half-written project is removed.
- **An existing project:** what the file wrote before that section stays, and the answer says so. Items are written last, in one step, so a failed section never leaves half a set of items. Import the same file again to finish: names already written are matched and updated.

### Exporting a project file

- In the web UI: **Project settings → Data → Export → Export project file (YAML)**.
- From the command line: `tack export <project> --bundle -o project.yaml`. Add `--format json` for JSON.
- Over the API: `GET /api/projects/{id}/export?scope=bundle&format=yaml`.

The export writes the same file back: items in written order, only the profiles the project's roles and default point at, and no ids. It never writes the signing key that merge verification uses; paste that on the Merge rung of Settings → Automation. Exporting, importing into a new project and exporting again gives the same text.

### Templates

A **template** is a project file without items. It gives every new project the same workflow, vocabulary, roles, profiles and automation.

- **Use one:** on the **Templates** page choose **Use Template**, or run `tack init "My site" --template web-product-with-content`. The name is matched without regard to case and `-` reads as a space, or give the template's id.
- **Save one:** `tack template save <project> --name "My template"` stores a project's roles, profiles, automation, workflow and vocabulary.
- **Built-in templates:** **Web product with content**, **Library or CLI** and **Static content site** each bring their roles and profiles, with merging at L3.
- **What the Templates page shows:** each card carries chips for what the template holds, such as "5 statuses", "3 roles", "3 profiles" and "Merges at L3". Hover a card to see its workflow columns and each role with its profile.

---

## Items only: outline and CSV

When you only have a list of tasks, write an **outline** (JSON or YAML with `items`, and optionally `project`) or a **CSV** with a header row, one item per row. Both are read by the same rules as the project file.

```yaml
project:
  name: Website relaunch
  type: web                  # optional; default custom
  description: Q4 relaunch   # optional
items:
  - ref: design
    title: Design
    type: epic
  - title: Wireframes
    parent: design
    depends_on: [research]
    due_date: 2026-11-01
    priority: high
  - ref: research
    title: User research
    status: In Progress
    tags: [ux]
    description: |
      Interview five users.
      Summarise the findings.
```

Items can come in any order; a parent is always written before its children.

| Field | Description |
|-------|-------------|
| `ref` | A name other items use in `parent` and `depends_on`. Any text or number, unique in the file. Optional. |
| `title` | Required. |
| `description` | Optional. |
| `type` | `epic`, `feature`, `task` (the default), `subtask`, `bug`, `requirement`, or any other word as a custom type. |
| `status` | A status of the project's workflow, in any case. Absent: the workflow's first status. |
| `priority` | `critical`, `high`, `medium` (the default), `low` or `none`. |
| `assignee` | Optional. |
| `estimate` | A number. |
| `due_date` | `YYYY-MM-DD`, or an RFC 3339 timestamp. |
| `tags` | A list of words. |
| `recurrence` | `weekly`, `monthly` or `quarterly`. Closing the task creates the next one. |
| `parent` | The parent's `ref`, or its exact title. When adding to an existing project, also the id of an item already in it. |
| `depends_on` | The `ref`s or titles of items that must finish first. Each becomes a *blocks* dependency. |

The CSV uses the same names as column headers, matched in any case. Only `title` is required. These spellings are also read:

| Column | Also read as |
|--------|--------------|
| `ref` | `id`, `key` |
| `type` | `item_type` |
| `due_date` | `due` |
| `tags` | `labels` |
| `recurrence` | `repeats` |
| `parent` | `parent_id`, `parent_ref` |
| `depends_on` | `blocked_by` |

`tags` and `depends_on` hold several values separated by `;` or `|`, never `,`, which a title may contain. Quoted cells may contain commas, quotes (written `""`) and line breaks. Rows with no title are skipped and counted. A CSV exported from Tack imports again with its hierarchy, because its `id` and `parent_id` columns are read as `ref` and `parent`.

**Import it** with the same steps as the project file: **Project settings → Data → Import** in the web UI, `tack import plan.yaml` or `tack import tasks.csv --name "Relaunch" -t web` on the command line, or `create_outline` from an assistant. For the API, `POST /api/projects/import-csv?name=…` creates a project from a CSV (`name` is required) and `POST /api/projects/{id}/import-csv` adds a CSV's rows to a project. A new project from a CSV is named after the file unless you pass `--name`.

A successful import answers:

```json
{
  "dry_run": false,
  "project": { "id": "…", "name": "Website relaunch", "…": "…" },
  "created": 3,
  "skipped": 0,
  "dependencies": 1,
  "items": [
    { "ref": "design", "id": "…", "title": "Design", "status": "Backlog", "parent_id": null }
  ],
  "warnings": []
}
```

`items` is in written order, parents first. On a dry run `id` is null and `project` is absent. A refused file answers `400` with every problem in the message.

Items from an outline are recorded with `source: outline` and items from a CSV with `source: csv_import`. Both are untrusted: when such an item is handed to an agent, its text is labelled as data to work from rather than instructions, because Tack cannot tell who wrote it.

---

## Importing GitHub and Linear issues

### GitHub

`POST /api/projects/{id}/import-github` fetches the issues of a repository and creates one Tack item per issue in the project. Pull requests are skipped. Each created item stays linked to its issue, so [its status and comments can sync](#keeping-github-issues-in-sync).

```sh
curl -X POST http://127.0.0.1:3210/api/projects/3f1c2b9a-8d4e-4a77-9b21-0c5e6f7a8b90/import-github \
  -H "Content-Type: application/json" \
  -d '{
    "repo": "rust-lang/rust",
    "token": "ghp_yourPersonalAccessToken",
    "import_closed": false,
    "label_filter": ["bug", "good first issue"]
  }'
```

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `repo` | yes | — | Repository as `owner/repo` or a full URL (`https://github.com/owner/repo`, with or without a `.git` suffix or trailing slash). |
| `token` | no | none | A GitHub personal access token. Without one, GitHub allows 60 requests an hour; with one, 5,000. Reading a private repository needs a token with `repo` scope. |
| `import_closed` | no | `false` | When `false`, only open issues are imported. |
| `label_filter` | no | `[]` | When non-empty, only issues carrying at least one of these labels are imported (case-insensitive). |

Tack pages through the repository 100 issues at a time, so one call imports every matching issue.

| GitHub | Tack item |
|--------|-----------|
| `number` + `title` | Title, written `[#123] Issue title` |
| `body` | Description, starting with a `GitHub Issue: <url>` line |
| `labels` | Tags, one per label |
| `state` (`open` / `closed`) | The first workflow status for an open issue, the first Done status for a closed one |
| `assignee.login` | Assignee |

Every imported item is a task. The answer reports counts, and `skipped` covers pull requests, issues the label filter removed, and rows that failed to create:

```json
{ "created": 42, "skipped": 3, "rate_limit_remaining": 4958 }
```

### Linear

`POST /api/projects/{id}/import-linear` fetches issues from Linear and creates items, 50 issues a page until all matching issues are in.

```sh
curl -X POST http://127.0.0.1:3210/api/projects/3f1c2b9a-8d4e-4a77-9b21-0c5e6f7a8b90/import-linear \
  -H "Content-Type: application/json" \
  -d '{
    "api_key": "lin_api_yourKeyHere",
    "team_id": "ENG",
    "import_completed": false,
    "label_filter": ["frontend"]
  }'
```

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `api_key` | yes | — | A Linear personal API key, created at `https://linear.app/settings/api`. |
| `team_id` | no | none | Import only this team's issues, by key such as `ENG`. |
| `project_id` | no | none | Import only this Linear project's issues. Wins over `team_id` when both are set. |
| `import_completed` | no | `false` | When `false`, completed and cancelled issues are skipped. |
| `label_filter` | no | `[]` | When non-empty, only issues carrying at least one matching label are imported (case-insensitive). |

With neither `team_id` nor `project_id`, every issue the key can read is fetched.

| Linear | Tack item |
|--------|-----------|
| `identifier` + `title` | Title, written `[ENG-123] Issue title` |
| `description` | Description, starting with a `Linear Issue: <url>` line |
| `labels` | Tags, one per label |
| `state.type` (`completed` / `cancelled`) | The first Done status; every other state maps to the first workflow status |
| `assignee.name` | Assignee |
| `priority` | Priority: 1 Urgent is Critical, 2 High, 3 Medium, 4 Low, 0 No priority is unset |

The answer is `{ "created": N, "skipped": N }`.

### Keeping GitHub issues in sync

Items imported from GitHub stay linked to their issue, and you can link any other item by hand with the **Link GitHub issue** row in the item's side panel. When the board has a GitHub token, the link carries state and comments in both directions:

- **Out, on every change.** Moving a linked item into a Done status closes its issue. Moving it back out reopens it. A comment on the item is posted on the issue.
- **In, on a poll.** A closed issue moves its item to the first Done status. A reopened one moves it to the first to-do status. A new comment on the issue appears on the item, attributed to its GitHub login.

Both directions are best-effort: a failure is logged and never blocks the item update. A change that came in never triggers a push back out. Title edits, labels and assignees are not mirrored. Tack listens on your computer only by default, so GitHub cannot call it; the inbound side polls instead.

Sync is off until an administrator sets a GitHub token and a poll interval; the settings are in the [Configuration Reference](../admin-guide/configuration-reference.md). A project can also carry its own token as a secret reference. For the full behaviour, see [GitHub Sync](https://github.com/yielab/tack/blob/develop/docs/GITHUB-SYNC.md).

---

## Snapshots and CSV export

### A JSON or YAML snapshot

`GET /api/projects/{id}/export?format=json` returns a complete snapshot of one project as a download named `<project-name>-export.json`. `format=yaml` gives the same structure as YAML. In the web UI use **Project settings → Data → Export → Export JSON** or **Export YAML**; from the command line, `tack export <project> -o snapshot.json`.

```sh
curl -OJ "http://127.0.0.1:3210/api/projects/3f1c2b9a-8d4e-4a77-9b21-0c5e6f7a8b90/export?format=json"
```

The snapshot holds:

- `project`: the full project record, including workflow and vocabulary;
- `items`: every item;
- `sprints`: every sprint;
- `dependencies`: every dependency;
- `briefs`: every item's [brief](items.md#acceptance-criteria-and-for-the-agent), meaning its acceptance criteria, constraints, definition of done and risk;
- `metadata`: when it was exported, the Tack version and the totals.

It does not hold roles, agent profiles or automation settings; use the project file for those.

To restore a snapshot, choose **Project settings → Data → Import → Import from JSON / YAML…**, or `POST /api/projects/import`. It creates a new project with the items, sprints, parent links, dependencies and briefs. A brief in the file is checked by the same rules as a save from the Brief tab, and the import is refused if one breaks them, since a snapshot can be edited by hand. The answer counts `briefs_imported` beside the other totals. The `tack import` command reads outlines and CSV files, not snapshots.

### A CSV export

`GET /api/projects/{id}/export?format=csv` returns a flat item list as `<project-name>-export.csv`, or use **Export CSV** in the web UI, or `tack export <project> --format csv -o items.csv`.

```sh
curl -OJ "http://127.0.0.1:3210/api/projects/3f1c2b9a-8d4e-4a77-9b21-0c5e6f7a8b90/export?format=csv"
```

One row per item, with these columns:

| Column | Description |
|--------|-------------|
| `id` | Item id |
| `title` | Item title |
| `description` | Description, or empty |
| `type` | Task, bug, epic and so on |
| `status` | Current workflow status |
| `priority` | Item priority |
| `assignee` | Assignee, or empty |
| `estimate` | Estimate, or empty |
| `due_date` | `YYYY-MM-DD`, or a full timestamp when the due date has a time of day |
| `tags` | Tags separated by `;` |
| `recurrence` | `weekly`, `monthly` or `quarterly`, or empty |
| `parent_id` | Parent item id, or empty for a top-level item |
| `created_at` | Creation time (RFC 3339); not read on import |

Cells are quoted where needed, so commas and line breaks survive. The file imports again through [an import from a file](#items-only-outline-and-csv), hierarchy included. The CSV covers items only: it has no sprints, dependencies, briefs or workflow. Use a snapshot or the project file when you need those.
