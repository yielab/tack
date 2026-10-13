# ADR 0076: a project and its items from one outline or CSV

**Status: ACCEPTED 2026-10-12 (proposed 2026-10-11); executed in `develop` on 2026-10-11.**

**Decide:** that Tack creates a project and many items — or adds many items to a project — from
one file in one request: an **outline** (JSON or YAML) or a **CSV**, through the same rules. An
item names its parent and the items it waits on by a `ref` the writer chooses, or by an exact
title, so the file needs no database ids. The whole file is checked before anything is written,
every problem is reported at once, and the write is one transaction. The same path is reached
from the web UI, `tack import`, the MCP tool `create_outline`, and the HTTP API.

**Why now:** the way to fill a project from an AI assistant, an agent harness or a spreadsheet
was one `create_item` call per item (two with a description), with no project creation over
MCP, and a CSV import that split rows on line breaks, ignored parents and due dates, and could
not create the project it imported into. A forty-item plan from a model cost eighty tool calls
and left a half-written project when one of them failed.

**If you do nothing:** a model's plan reaches Tack one item at a time, a refused item halfway
leaves the rest written, and a CSV with a multi-line description or a parent column imports
wrong without saying so.

## Decisions

| # | Decision |
|---|---|
| 1 | One format, `tack_core::outline`: `project {name, project_type, description}` and `items[]` with `ref`, `title`, `description`, `type`, `status`, `priority`, `assignee`, `estimate`, `due_date`, `tags`, `parent`, `depends_on`. A CSV row is an outline item; its columns are the same names. |
| 2 | **Refused, all at once:** a missing title, a reused `ref`, a `parent` or `depends_on` naming nothing (or a title shared by several items), a parent or dependency loop, a due date that is not a date, a field the item routes would reject, a parent id outside the project, more than 1,000 items, an unknown key in an outline. **Accepted with a warning:** a status the workflow lacks (the item starts in the first status), a priority Tack does not know (medium), a CSV column with no field. A spreadsheet's vocabulary is not a reason to refuse the file; the dry run shows each warning before anything is written. |
| 3 | `?dry_run=true` on every route runs every check and returns what would be created. The UI always previews before it writes. |
| 4 | The items, their parent links and their `blocks` dependencies are written in one SQLite transaction. A new project is created first and deleted if that transaction fails. |
| 5 | Items from an outline carry a new provenance, `source: outline`, untrusted like every other import: Tack cannot tell whether a person, a script, or a model reading someone else's text wrote it. A CSV keeps `csv_import`. |
| 6 | Routes: `POST /api/projects/outline`, `POST /api/projects/{id}/outline`, `POST /api/projects/import-csv?name=`, and the existing `POST /api/projects/{id}/import-csv`, whose response gains the outline result's fields. The CSV export writes the columns the import reads (`id` as ref, `parent_id` as parent), so an export re-imports with its hierarchy. |
| 7 | MCP gains `create_project` and `create_outline`; `create_item` gains `description`, `due_date` and `tags`. |

## Alternatives not taken

- **Nested `children` in the outline.** Natural for a model, but a CSV cannot express it, and
  two shapes would mean two sets of rules. Flat items with `parent` serve both.
- **Reusing the export snapshot (`POST /api/projects/import`).** It expects whole `Project` and
  `Item` records — ids, timestamps, a workflow — which no person or model should have to write,
  and it restores a project rather than checking a plan.
- **Refusing unknown statuses.** A CSV written for another tool says "Hecho" or "Closed"; refusing
  the whole file for that would make the import useless for exactly the files it exists for.
- **A remote MCP transport for web chat assistants.** Out of scope: it changes who can reach the
  API (ADR 0059). A web assistant can write an outline to paste or upload today.
