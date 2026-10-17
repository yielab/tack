# Import and Export

Tack can create a project and its items from one outline or CSV file, pull issues in from GitHub and Linear, keep linked GitHub issues in sync both ways, and export an entire project to JSON, YAML, or CSV. Every import and export is exposed over the HTTP API; files can also be imported from **Project settings → Data**, with `tack import`, and from an AI assistant through the MCP tool `create_outline`.

The examples below use a base URL of `http://127.0.0.1:3210` and assume a server started with `tack serve`. If you set `TACK_API_TOKEN`, add `-H "Authorization: Bearer <token>"` to every request.

---

## How do I create a project and its items from a file?

Write the plan once — by hand, from a script, from a spreadsheet, or by asking an AI assistant — and Tack creates the project, every item, their hierarchy and their dependencies in one step. Two file formats are read, through the same rules:

- an **outline**, as JSON or YAML;
- a **CSV** with a header row, one item per row.

The whole file is checked before anything is written. If anything is wrong, nothing is created and the error lists every problem at once, so a person or a model can fix the file in one pass. When the file is accepted, everything is written in one transaction.

### The outline

```yaml
project:
  name: Website relaunch
  project_type: web          # optional; default custom
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

The same outline as JSON uses the same keys. Items can come in any order; a parent is always written before its children.

| Field | Description |
|-------|-------------|
| `ref` | A name for the item that other items use in `parent` and `depends_on`. Any text or number, unique in the outline. Optional. |
| `title` | Required. |
| `description` | Optional. |
| `type` | `epic`, `feature`, `task` (the default), `subtask`, `bug`, `requirement`, or any other word as a custom type. |
| `status` | A status of the project's workflow, in any case. Absent: the workflow's first status. |
| `priority` | `critical`, `high`, `medium` (the default), `low` or `none`. |
| `assignee` | Optional. |
| `estimate` | A number. |
| `due_date` | `YYYY-MM-DD`, or an RFC 3339 timestamp. |
| `tags` | A list of words. |
| `parent` | The parent's `ref`, or its exact title. When adding to an existing project, also the id of an item already in it. |
| `depends_on` | The `ref`s or titles of items that must finish first. Each becomes a *blocks* dependency. |

`project` is read only when the file creates a project. An outline with any other key is refused, so a misspelt field is reported rather than lost.

### The CSV

Only `title` is required. Headers are matched in any case and use the outline's names; these spellings are also read:

| Column | Also read as |
|--------|--------------|
| `ref` | `id`, `key` |
| `type` | `item_type` |
| `due_date` | `due` |
| `tags` | `labels` |
| `parent` | `parent_id`, `parent_ref` |
| `depends_on` | `blocked_by` |

`tags` and `depends_on` hold several values separated by `;` or `|` — never `,`, which a title may contain. Quoted cells may contain commas, quotes (written `""`) and line breaks. Rows with no title are skipped and counted. A CSV exported from Tack re-imports with its hierarchy, since its `id` and `parent_id` columns are read as `ref` and `parent`.

### The project file

An outline can carry more than items. A **project file** is an outline with six more sections, so a whole project — its fields, agents, roles, sprints and automation as well as its items — goes in and out as one file you can edit:

```yaml
project:    { name: Fabulada, type: web }
fields:     [{ name: Dialect, type: select, options: [rioplatense, mexicano] }]
profiles:   [{ name: Content writer, kind: custom, instructions: Write stories. }]
roles:      [{ name: Content, color: "#f59e0b", profile: Content writer }]
sprints:    [{ name: Week 42, goal: Launch the first collection }]
automation: { default_profile: Content writer, on_finish_status: Review }
items:      [{ ref: C1, title: First collection, type: epic, roles: [Content], sprint: Week 42 }]
```

| Section | Takes |
|---------|-------|
| `project` | Name, type, description, the project's vocabulary and its workflow (a preset or the statuses in order). Read only when the file creates a project. |
| `fields` | Custom fields: name, type, options, whether required. Items give their values under `fields`. |
| `profiles` | Agent profiles: name, kind, instructions, which agent runs it and its team, limits. |
| `roles` | Roles: name, color and the profile each one uses. Items name their roles under `roles`. |
| `sprints` | Name, goal, dates. A sprint is created planned or closed, never active. |
| `automation` | The project's folder or repository, branch, how the agent works, its default profile and model, the status a finished run moves to, the definition of done, autonomy and auto-start. |
| `items` | The items of the outline above, plus `roles`, `sprint`, `fields`, `recurrence` and `estimate_unit`. |

The rules:

- **Names are the references.** A role's `profile` names a profile of the same file first, then one Tack already has. An item names its roles, sprint and field values the same way. An unknown key anywhere is refused.
- **The whole file is checked before anything is written**, and every problem is reported at once.
- **Nothing is deleted.** Importing into an existing project adds new names and updates the names the file mentions, with the keys it gives. The project's own roles, fields, sprints and profiles that the file does not mention are counted ("3 not mentioned") and left alone. Items are always added.
- **Built-in profiles are never changed.** A role may use Implementer, Reviewer, Researcher or Planner; a file that tries to redefine one gets a warning.
- **A file cannot raise autonomy.** An autonomy level above L3 and `auto_start: enabled: true` are accepted and applied as L3 and off, each with a warning that says what was asked and what was applied. Turn them on from Settings → Automation.
- **Refused:** an unknown key, a field value of the wrong type or not among the field's options, a role that names no profile, an item that names no role, sprint or status, a status the workflow does not have in `automation`. **Only a warning:** an item missing a required field, a team or machine Tack does not know.

The preview lists each section before the items — "Roles: 2 new, 5 updated, 3 not mentioned" — with that section's warnings under it.

**Export.** `GET /api/projects/{id}/export?scope=bundle&format=yaml` (or `json`) writes the same file back: items in written order, only the profiles the project's roles and default point at, no ids or dates, and never the signing key. Exporting, importing into a new project and exporting again gives the same text. From the command line, `tack export <project> --bundle -o project.yaml`; in the web UI, **Export project file (YAML)** on **Project settings → Data**.

**Templates.** A template is a project file without items. `tack init "My site" --template web-product-with-content` creates a project from one (a name works with `-` for spaces, in any case, or use the template's id); `tack template save <project> --name "My template"` stores a project's roles, profiles, automation, workflow and vocabulary as one. Three templates come built in, each with its roles and profiles at autonomy L3: **Web product with content**, **Library or CLI** and **Static content site**.

### What is refused, and what is only a warning

The file is **refused** for: an item with no title; a `ref` used twice; a `parent` or `depends_on` that names no item, or a title shared by several items (give them refs); an item that is its own parent, or parents that loop; dependencies that loop; a due date that is not a date; a value the item routes reject (a title over 500 characters, a negative estimate, more than 20 tags); a parent id that is not an item of the project; more than 1,000 items.

The file is **accepted with a warning** for a status the workflow does not have (the item starts in the first status), a priority Tack does not know (the item is medium), and a CSV column Tack does not read. A spreadsheet written for another tool should not have to be rewritten to be imported; the preview shows each warning before anything is written.

### From the web UI

**Project settings → Data → Import**:

- **Add items from a file…** adds the items to this project.
- **New project from a file…** creates a project. A CSV's project is named after the file.

Either way, Tack first shows what would be created — the items, the number of dependencies and any warnings — or every reason the file cannot be imported. Nothing is written until you confirm.

### From the command line

```sh
tack import plan.yaml --dry-run              # check and show, write nothing
tack import plan.yaml                        # create the project the outline names
tack import tasks.csv --name "Relaunch" -t web
tack import tasks.csv -p <project-id>        # add to an existing project
```

The format is chosen by extension: `.csv`, `.yaml`/`.yml`, otherwise JSON. `--name` and `--type` override the outline's `project`.

### From an AI assistant (MCP)

With `tack mcp` configured (see [MCP](https://github.com/yielab/tack/blob/develop/docs/MCP.md)), an assistant calls `create_outline` with either `project` (to create one) or `project_id` (to add to one), and `items` in the outline format. Ask it to call with `dry_run: true` first. A web chat assistant that cannot reach Tack can write the outline as YAML for you to save and import.

### Via API

| Route | Does |
|-------|------|
| `POST /api/projects/outline` | Creates a project from an outline. |
| `POST /api/projects/{id}/outline` | Adds an outline's items to a project; its `project` block is ignored, with a warning. |
| `POST /api/projects/import-csv?name=…` | Creates a project from a CSV; `name` is required. |
| `POST /api/projects/{id}/import-csv` | Adds a CSV's rows to a project. |

The outline routes read JSON, or YAML when `Content-Type` mentions `yaml`; the CSV routes read `text/csv`. Every route takes `dry_run=true`. The routes that create a project also take `name`, `project_type` and `description`, which override the outline's own.

```sh
curl -X POST "http://127.0.0.1:3210/api/projects/outline?dry_run=true" \
  -H "Content-Type: application/x-yaml" --data-binary @plan.yaml
```

The response:

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

## How do I import GitHub issues?

`POST /api/projects/{id}/import-github` fetches the issues from a repository and creates one Tack item per issue in the target project. Pull requests are skipped automatically. Each created item is recorded in the `github_links` table so its status and comments can later sync with GitHub (see [GitHub sync](#how-do-i-keep-github-issues-in-sync-after-import)).

**Via API:**

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

Request fields:

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `repo` | yes | — | Repository as `owner/repo` or a full URL (`https://github.com/owner/repo`, with or without a `.git` suffix or trailing slash). |
| `token` | no | none | GitHub personal access token. Unauthenticated calls work but are limited to 60 requests/hour; a token raises this to 5,000/hour. A token with `repo` scope is required to read private repositories. |
| `import_closed` | no | `false` | When `false`, only open issues are imported. When `true`, both open and closed issues are imported. |
| `label_filter` | no | `[]` | When non-empty, only issues carrying at least one of these labels are imported (case-insensitive). All others are skipped. |

Tack pages through the repository 100 issues at a time until every matching issue has been processed, so a single call imports the whole repo.

Field mapping:

| GitHub | Tack item |
|--------|-----------|
| `number` + `title` | Title, formatted as `[#123] Issue title` |
| `body` | Description, prefixed with a `GitHub Issue: <url>` line |
| `labels` | Tags (one tag per label) |
| `state` (`open` / `closed`) | Status — the first workflow status by order for open issues, the first Done-category status for closed issues |
| `assignee.login` | Assignee |

Every imported item is created as a Task. The response reports counts:

```json
{ "created": 42, "skipped": 3, "rate_limit_remaining": 4958 }
```

`skipped` covers pull requests, issues filtered out by `label_filter`, and any rows that failed to create.

---

## How do I import Linear issues?

`POST /api/projects/{id}/import-linear` fetches issues from Linear's GraphQL API and creates Tack items. Pagination is cursor-based (50 issues per page) and runs until all matching issues are imported.

**Via API:**

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

Request fields:

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `api_key` | yes | — | Linear personal API key. Create one at `https://linear.app/settings/api`. |
| `team_id` | no | none | Import only issues from this team. Accepts the team key/slug (for example `ENG`). |
| `project_id` | no | none | Import only issues from this Linear project ID. Takes precedence over `team_id` when both are set. |
| `import_completed` | no | `false` | When `false`, completed and cancelled issues are skipped. When `true`, they are imported. |
| `label_filter` | no | `[]` | When non-empty, only issues carrying at least one matching label are imported (case-insensitive). |

When neither `team_id` nor `project_id` is given, every issue accessible to the API key is fetched.

Field mapping:

| Linear | Tack item |
|--------|-----------|
| `identifier` + `title` | Title, formatted as `[ENG-123] Issue title` |
| `description` | Description, prefixed with a `Linear Issue: <url>` line |
| `labels` | Tags (one tag per label) |
| `state.type` (`completed` / `cancelled`) | Status — first Done-category status; all other states map to the first workflow status by order |
| `assignee.name` | Assignee |
| `priority` | Priority (see below) |

Priority mapping:

| Linear priority | Tack priority |
|-----------------|---------------|
| 1 (Urgent) | Critical |
| 2 (High) | High |
| 3 (Medium) | Medium |
| 4 (Low) | Low |
| 0 (No priority) | unset |

The response reports `{ "created": N, "skipped": N }`.

---

## How do I keep GitHub issues in sync after import?

Items imported from GitHub stay linked to their source issue, and any other item can be linked by hand — the **Link GitHub issue** row in the item's side panel, or `PUT /api/items/{id}/github-link` with `{"repo": "owner/name", "issue_number": 42}` (`DELETE` unlinks). When `TACK_GITHUB_TOKEN` is set, the link carries state and comments in both directions:

- **Out, on every change:** moving a linked item into a Done-category status closes its GitHub issue; moving it back out of Done reopens it. A comment posted on the item is posted onto the issue.
- **In, on a poll** (`TACK_GITHUB_POLL_SECONDS`, off at `0`): a closed issue moves its item to the workflow's first Done-category status, a reopened one to the first Todo-category status, and a new comment on the issue appears on the item, attributed to its GitHub login.

Both directions are best-effort — failures are logged but never block or fail the item update or the comment — and neither echoes: a change that came in never triggers a push back out. Title edits and same-category status moves trigger no GitHub call; labels and assignees are not mirrored. There is no webhook receiver: Tack listens on loopback by default, so GitHub could not reach it.

The feature is off by default and configured through environment variables. A project can also carry its own token as a secret reference (`PATCH /api/projects/{id}` with `{"github_token_ref": "store:<name>"}`), resolved before the global one and never returned by any route:

| Variable | Default | Description |
|----------|---------|-------------|
| `TACK_GITHUB_TOKEN` | none | PAT with `repo` scope. Enables both directions; never logged. Without it, the link is inert. |
| `TACK_GITHUB_API_BASE` | `https://api.github.com` | API root override for GitHub Enterprise or testing. |
| `TACK_GITHUB_POLL_SECONDS` | `0` | Inbound poll interval in seconds; `0` is off. Needs the token too. |

For full details, see [GitHub Sync](https://github.com/yielab/tack/blob/develop/docs/GITHUB-SYNC.md).

---

## How do I export a project to JSON?

`GET /api/projects/{id}/export?format=json` returns a complete, downloadable snapshot of the project as an attachment named `<project-name>-export.json`.

**Via API:**

```sh
curl -OJ "http://127.0.0.1:3210/api/projects/3f1c2b9a-8d4e-4a77-9b21-0c5e6f7a8b90/export?format=json"
```

The snapshot contains:

- `project` — full project record including workflow and vocabulary
- `items` — every item in the project
- `sprints` — all sprints
- `dependencies` — all dependency edges
- `briefs` — every item's [brief](items.md#acceptance-criteria-and-for-the-agent) (acceptance criteria, constraints, definition of done, risk)
- `metadata` — `exported_at` timestamp, the exporting Tack `version`, and totals for items, sprints, and dependencies

`format` defaults to `json`, so omitting the query parameter produces the same result. A `format=yaml` variant is also available and produces the identical structure as YAML.

This snapshot is the same shape accepted by `POST /api/projects/import`, so an exported JSON or YAML file can be re-imported to recreate the project (items, sprints, parent links, dependencies and briefs are all restored into a brand-new project). A brief in the file is checked by the same rules as a save from the Brief tab, and the import is refused if one breaks them, since an export file can be edited by hand. The response counts `briefs_imported` beside the other totals.

---

## How do I export a project to CSV?

`GET /api/projects/{id}/export?format=csv` returns a flat, spreadsheet-friendly item list as an attachment named `<project-name>-export.csv`.

**Via API:**

```sh
curl -OJ "http://127.0.0.1:3210/api/projects/3f1c2b9a-8d4e-4a77-9b21-0c5e6f7a8b90/export?format=csv"
```

The CSV has one row per item with these columns:

| Column | Description |
|--------|-------------|
| `id` | Item UUID |
| `title` | Item title |
| `description` | Item description, or empty |
| `type` | Item type (task, bug, epic, etc.) |
| `status` | Current workflow status |
| `priority` | Item priority |
| `assignee` | Assignee, or empty if unassigned |
| `estimate` | Estimate, or empty |
| `due_date` | `YYYY-MM-DD`, or a full timestamp when the due date has a time of day |
| `tags` | Tags separated by `;` |
| `parent_id` | Parent item UUID, or empty if top-level |
| `created_at` | Creation timestamp (RFC 3339) |

Cells are quoted where needed, so commas and line breaks survive. The file re-imports into any project through [an import from a file](#how-do-i-create-a-project-and-its-items-from-a-file), hierarchy included.

CSV export covers items only — it does not include sprints, dependencies, briefs, or workflow configuration. Use JSON or YAML export for a full, re-importable backup.

---

## Which should I use?

Use an **outline** or a **CSV** to create a project from a plan — your own, a spreadsheet's, or an AI assistant's. Use **GitHub** or **Linear** import to seed a Tack project from work already tracked elsewhere; choose GitHub import (with `TACK_GITHUB_TOKEN` set) if you also want completed Tack items to close their upstream issues. Use **JSON** (or YAML) export for a complete, re-importable backup or to move a project between Tack instances, since it preserves workflow, sprints, dependencies, and hierarchy. Use **CSV** export for an item list to edit in a spreadsheet and import again.
