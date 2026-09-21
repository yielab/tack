# Import and Export

Tack can pull issues in from GitHub and Linear, keep linked GitHub issues in sync both ways, and export an entire project to JSON, YAML, or CSV. All import and export operations are exposed over the HTTP API; there is no dedicated CLI subcommand for them.

The examples below use a base URL of `http://127.0.0.1:3210` and assume a server started with `tack serve`. If you set `TACK_API_TOKEN`, add `-H "Authorization: Bearer <token>"` to every request.

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

For full details, see [GitHub Sync](../../../GITHUB-SYNC.md).

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
- `metadata` — `exported_at` timestamp, the exporting Tack `version`, and totals for items, sprints, and dependencies

`format` defaults to `json`, so omitting the query parameter produces the same result. A `format=yaml` variant is also available and produces the identical structure as YAML.

This snapshot is the same shape accepted by `POST /api/projects/import`, so an exported JSON or YAML file can be re-imported to recreate the project (items, sprints, parent links, and dependencies are all restored into a brand-new project).

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
| `title` | Item title (commas replaced with spaces) |
| `type` | Item type (task, bug, epic, etc.) |
| `status` | Current workflow status |
| `priority` | Item priority |
| `assignee` | Assignee, or empty if unassigned |
| `parent_id` | Parent item UUID, or empty if top-level |
| `created_at` | Creation timestamp (RFC 3339) |

CSV export covers items only — it does not include sprints, dependencies, or workflow configuration. Use JSON or YAML export for a full, re-importable backup.

---

## Which should I use?

Use **GitHub** or **Linear** import to seed a Tack project from work already tracked elsewhere; choose GitHub import (with `TACK_GITHUB_TOKEN` set) if you also want completed Tack items to close their upstream issues. Use **JSON** (or YAML) export for a complete, re-importable backup or to move a project between Tack instances, since it preserves workflow, sprints, dependencies, and hierarchy. Use **CSV** export when you only need a quick item list for a spreadsheet or report.
