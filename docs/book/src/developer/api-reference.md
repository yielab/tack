# API Reference

Generated from [`docs/openapi.json`](../../../openapi.json) (78 paths, 109 operations) by `scripts/gen-api-reference.py` — do not hand-edit. Regenerate with `./scripts/regen-generated.sh` after the spec changes.

This page lists every path, method, parameter and request/response schema name. It does not inline schema bodies — load [`docs/openapi.json`](../../../openapi.json) into an OpenAPI viewer (Redocly, Scalar, Swagger Editor) for the full definitions, or read them directly in the spec file.

Two authentication surfaces, the WebSocket endpoint (not in this spec), and worked examples are in [`docs/API-REFERENCE.md`](../../../API-REFERENCE.md).

---

## System

Health and debug probes.

#### `GET /api/debug/db-stats`

Database statistics

| Status | Meaning | Schema |
|---|---|---|
| 200 | Per-table row counts | — |

#### `GET /api/debug/info`

System info (only in debug builds)

| Status | Meaning | Schema |
|---|---|---|
| 200 | Build, version, database size, and non-sensitive config | — |

#### `GET /api/health`

Liveness + readiness check

| Status | Meaning | Schema |
|---|---|---|
| 200 | Service is live; reports version and applied migration count | — |

---

## Projects

Projects: the top-level container for work.

#### `GET /api/projects`

| Status | Meaning | Schema |
|---|---|---|
| 200 | All projects in the workspace | `Project`[] |

#### `POST /api/projects`

**Request body:** `CreateProject`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Project created | `Project` |
| 400 | Validation error | `ErrorEnvelope` |

#### `DELETE /api/projects/{id}`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `id` | path | `string` | yes | Project ID |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Deleted | — |
| 404 | Project not found | `ErrorEnvelope` |

#### `GET /api/projects/{id}`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `id` | path | `string` | yes | Project ID |

| Status | Meaning | Schema |
|---|---|---|
| 200 | The project | `Project` |
| 404 | Project not found | `ErrorEnvelope` |

#### `PATCH /api/projects/{id}`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `id` | path | `string` | yes | Project ID |

**Request body:** `UpdateProject`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Updated project | `Project` |
| 400 | Validation error | `ErrorEnvelope` |
| 404 | Project not found | `ErrorEnvelope` |

---

## Items

Items: the universal work unit (epics, tasks, bugs, …).

#### `DELETE /api/items/{id}`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `id` | path | `string` | yes | Item ID |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Deleted | — |
| 404 | Item not found | `ErrorEnvelope` |

#### `GET /api/items/{id}`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `id` | path | `string` | yes | Item ID |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Item with roles and dependencies; carries an ETag header for a later conditional PATCH | `ItemDetail` |
| 404 | Item not found | `ErrorEnvelope` |

#### `PATCH /api/items/{id}`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `id` | path | `string` | yes | Item ID |
| `If-Match` | header | `['string', 'null']` | no | Optional ETag from GET /api/items/{id}; a stale or malformed value returns 412 and writes nothing |

**Request body:** `UpdateItem`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Updated item; carries the ETag for the exact returned snapshot | `Item` |
| 400 | Invalid transition / validation error | `ErrorEnvelope` |
| 404 | Item not found | `ErrorEnvelope` |
| 412 | If-Match did not match the current item version — nothing was written | `ErrorEnvelope` |

#### `GET /api/projects/{project_id}/items`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `project_id` | path | `string` | yes | Project ID |
| `status` | path | `['string', 'null']` | yes |  |
| `item_type` | path | — | yes |  |
| `priority` | path | — | yes |  |
| `sprint_id` | path | `['string', 'null']` | yes |  |
| `parent_id` | path | `['string', 'null']` | yes |  |
| `assignee` | path | `['string', 'null']` | yes |  |
| `tag` | path | `['string', 'null']` | yes |  |
| `search` | path | `['string', 'null']` | yes |  |
| `page` | path | `['integer', 'null']` | yes |  |
| `per_page` | path | `['integer', 'null']` | yes |  |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Paginated items | `PaginatedItems` |

#### `POST /api/projects/{project_id}/items`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `project_id` | path | `string` | yes | Project ID |

**Request body:** `CreateItem`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Item created | `Item` |
| 400 | Validation error | `ErrorEnvelope` |
| 404 | Project not found | `ErrorEnvelope` |

#### `GET /api/projects/{project_id}/items/tree`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `project_id` | path | `string` | yes | Project ID |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Item hierarchy (parents with nested children) | `Item`[] |

---

## Sprints

Sprints / iterations within a project.

#### `GET /api/projects/{project_id}/sprints`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `project_id` | path | `string` | yes | Project ID |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Sprints for the project | `Sprint`[] |

#### `POST /api/projects/{project_id}/sprints`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `project_id` | path | `string` | yes | Project ID |

**Request body:** `CreateSprint`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Sprint created | `Sprint` |
| 400 | Validation error | `ErrorEnvelope` |

#### `GET /api/sprints/{id}`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `id` | path | `string` | yes | Sprint ID |

| Status | Meaning | Schema |
|---|---|---|
| 200 | The sprint | `Sprint` |
| 404 | Sprint not found | `ErrorEnvelope` |

#### `PATCH /api/sprints/{id}`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `id` | path | `string` | yes | Sprint ID |

**Request body:** `UpdateSprint`

| Status | Meaning | Schema |
|---|---|---|
| 200 | The updated sprint | `Sprint` |
| 400 | Validation error | `ErrorEnvelope` |
| 404 | Sprint not found | `ErrorEnvelope` |

#### `PATCH /api/sprints/{id}/status`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `id` | path | `string` | yes | Sprint ID |

**Request body:** `UpdateSprintStatus`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Status updated | — |
| 400 | Validation error | `ErrorEnvelope` |
| 404 | Sprint not found | `ErrorEnvelope` |

---

## Roles

Roles / specialties and their assignment to items.

#### `DELETE /api/items/{item_id}/roles/{role_id}`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `item_id` | path | `string` | yes | Item ID |
| `role_id` | path | `string` | yes | Role ID |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Role removed from item | — |

#### `PUT /api/items/{item_id}/roles/{role_id}`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `item_id` | path | `string` | yes | Item ID |
| `role_id` | path | `string` | yes | Role ID |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Role assigned to item | — |

#### `GET /api/projects/{project_id}/roles`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `project_id` | path | `string` | yes | Project ID |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Roles for the project | `Role`[] |

#### `POST /api/projects/{project_id}/roles`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `project_id` | path | `string` | yes | Project ID |

**Request body:** `CreateRole`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Role created | `Role` |
| 400 | Validation error | `ErrorEnvelope` |

#### `DELETE /api/roles/{id}`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `id` | path | `string` | yes | Role ID |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Deleted | — |
| 404 | Role not found | `ErrorEnvelope` |

---

## Comments

Comments on items.

#### `GET /api/items/{item_id}/comments`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `item_id` | path | `string` | yes | Item ID |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Comments on the item | `Comment`[] |

#### `POST /api/items/{item_id}/comments`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `item_id` | path | `string` | yes | Item ID |

**Request body:** `CreateComment`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Comment created | `Comment` |
| 400 | Validation error | `ErrorEnvelope` |

---

## Dependencies

Directed dependency edges between items.

#### `GET /api/items/{item_id}/dependencies`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `item_id` | path | `string` | yes | Item ID |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Dependency edges for the item | `Dependency`[] |

#### `POST /api/items/{item_id}/dependencies`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `item_id` | path | `string` | yes | Source item ID |

**Request body:** `CreateDependency`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Dependency created | `Dependency` |
| 400 | Cycle detected or duplicate | `ErrorEnvelope` |

#### `DELETE /api/items/{item_id}/dependencies/{dep_id}`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `item_id` | path | `string` | yes | Item ID |
| `dep_id` | path | `string` | yes | Dependency ID |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Deleted | — |
| 404 | Dependency not found | `ErrorEnvelope` |

---

## Attachments

File attachments on items.

#### `DELETE /api/attachments/{id}`

DELETE /api/attachments/:id

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `id` | path | `string` | yes | Attachment ID |

| Status | Meaning | Schema |
|---|---|---|
| 204 | Attachment deleted | — |
| 404 | Attachment not found | `ErrorEnvelope` |

#### `GET /api/attachments/{id}`

GET /api/attachments/:id

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `id` | path | `string` | yes | Attachment ID |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Attachment file bytes | — |
| 404 | Attachment not found | `ErrorEnvelope` |

#### `GET /api/items/{item_id}/attachments`

GET /api/items/:id/attachments

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `item_id` | path | `string` | yes | Item ID |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Attachments on the item | array |
| 404 | Item not found | `ErrorEnvelope` |

#### `POST /api/items/{item_id}/attachments`

POST /api/items/:id/attachments

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `item_id` | path | `string` | yes | Item ID |

**Request body:** `string`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Attachment metadata | — |
| 400 | Missing/oversized file | `ErrorEnvelope` |
| 404 | Item not found | `ErrorEnvelope` |

---

## Boards

Saved board views and their grouped item layout.

#### `DELETE /api/boards/{id}`

Delete a board

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `id` | path | `string` | yes | Board ID |

| Status | Meaning | Schema |
|---|---|---|
| 204 | Board deleted | — |

#### `GET /api/boards/{id}`

Get a specific board

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `id` | path | `string` | yes | Board ID |

| Status | Meaning | Schema |
|---|---|---|
| 200 | The board | `Board` |
| 404 | Board not found | `ErrorEnvelope` |

#### `PATCH /api/boards/{id}`

Update a board

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `id` | path | `string` | yes | Board ID |

**Request body:** `UpdateBoard`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Updated board | `Board` |
| 422 | Validation error | `ErrorEnvelope` |

#### `GET /api/boards/{id}/view`

Get board state with items grouped and filtered

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `id` | path | `string` | yes | Board ID |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Board with items grouped into columns | `BoardViewResponse` |
| 404 | Board not found | `ErrorEnvelope` |

#### `GET /api/projects/{project_id}/boards`

List all boards for a project

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `project_id` | path | `string` | yes | Project ID |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Boards for the project | `Board`[] |

#### `POST /api/projects/{project_id}/boards`

Create a new board for a project

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `project_id` | path | `string` | yes | Project ID |

**Request body:** `CreateBoard`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Board created | `Board` |
| 404 | Project not found | `ErrorEnvelope` |
| 422 | Validation error | `ErrorEnvelope` |

---

## Custom Fields

Per-project custom field definitions and values.

#### `DELETE /api/custom-fields/{id}`

Delete a custom field

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `id` | path | `string` | yes | Custom field ID |

| Status | Meaning | Schema |
|---|---|---|
| 204 | Field deleted | — |

#### `GET /api/custom-fields/{id}`

Get a specific custom field

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `id` | path | `string` | yes | Custom field ID |

| Status | Meaning | Schema |
|---|---|---|
| 200 | The field definition | `CustomFieldDefinition` |
| 404 | Field not found | `ErrorEnvelope` |

#### `PATCH /api/custom-fields/{id}`

Update a custom field

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `id` | path | `string` | yes | Custom field ID |

**Request body:** `UpdateCustomField`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Updated field | `CustomFieldDefinition` |
| 500 | Update failed | `ErrorEnvelope` |

#### `GET /api/items/{item_id}/custom-fields`

Get all custom field values for an item

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `item_id` | path | `string` | yes | Item ID |

| Status | Meaning | Schema |
|---|---|---|
| 200 | All custom field values for the item | `CustomFieldValue`[] |

#### `DELETE /api/items/{item_id}/custom-fields/{field_id}`

Delete a custom field value

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `item_id` | path | `string` | yes | Item ID |
| `field_id` | path | `string` | yes | Custom field ID |

| Status | Meaning | Schema |
|---|---|---|
| 204 | Value deleted | — |

#### `GET /api/items/{item_id}/custom-fields/{field_id}`

Get a specific custom field value

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `item_id` | path | `string` | yes | Item ID |
| `field_id` | path | `string` | yes | Custom field ID |

| Status | Meaning | Schema |
|---|---|---|
| 200 | The field value | `CustomFieldValue` |
| 404 | Value not found | `ErrorEnvelope` |

#### `PUT /api/items/{item_id}/custom-fields/{field_id}`

Set a custom field value for an item

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `item_id` | path | `string` | yes | Item ID |
| `field_id` | path | `string` | yes | Custom field ID |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Value set | `CustomFieldValue` |
| 404 | Item or field not found | `ErrorEnvelope` |
| 422 | Value failed field validation | `ErrorEnvelope` |

#### `GET /api/projects/{project_id}/custom-fields`

List all custom fields for a project

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `project_id` | path | `string` | yes | Project ID |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Custom field definitions | `CustomFieldDefinition`[] |

#### `POST /api/projects/{project_id}/custom-fields`

Create a custom field for a project

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `project_id` | path | `string` | yes | Project ID |

**Request body:** `CreateCustomField`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Field created | `CustomFieldDefinition` |
| 404 | Project not found | `ErrorEnvelope` |

---

## Templates

Reusable project templates.

#### `POST /api/projects/from-template/{id}`

Create a project from a template

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `id` | path | `string` | yes | Template ID |

**Request body:** `CreateProjectFromTemplate`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Project created from template | `Project` |
| 404 | Template not found | `ErrorEnvelope` |
| 422 | Validation error | `ErrorEnvelope` |

#### `POST /api/projects/{project_id}/save-as-template`

Snapshot a project's configuration as a reusable template

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `project_id` | path | `string` | yes | Project ID |

**Request body:** `SaveAsTemplateRequest`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Template snapshot created | `ProjectTemplate` |
| 404 | Project not found | `ErrorEnvelope` |

#### `GET /api/templates`

List all project templates

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `project_type` | query | `ProjectType` | no |  |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Templates (optionally filtered by project type) | `ProjectTemplate`[] |

#### `POST /api/templates`

Create a new project template

**Request body:** `CreateProjectTemplate`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Template created | `ProjectTemplate` |
| 422 | Validation error (workflow shape, custom field options) | `ErrorEnvelope` |

#### `DELETE /api/templates/{id}`

Delete a template (user-created only)

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `id` | path | `string` | yes | Template ID |

| Status | Meaning | Schema |
|---|---|---|
| 204 | Template deleted | — |

#### `GET /api/templates/{id}`

Get a specific template

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `id` | path | `string` | yes | Template ID |

| Status | Meaning | Schema |
|---|---|---|
| 200 | The template | `ProjectTemplate` |
| 404 | Template not found | `ErrorEnvelope` |

---

## Import

Import from JSON/YAML/CSV, GitHub Issues, and Linear.

#### `POST /api/projects/import`

POST /api/projects/import

| Status | Meaning | Schema |
|---|---|---|
| 200 | Import result with the new project and stats | — |
| 400 | Invalid import payload | `ErrorEnvelope` |

#### `POST /api/projects/{id}/import-csv`

csv

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `id` | path | `string` | yes | Project ID |

**Request body:** `string`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Counts of created and skipped rows | — |
| 400 | Malformed CSV | `ErrorEnvelope` |
| 404 | Project not found | `ErrorEnvelope` |

#### `POST /api/projects/{id}/import-github`

github

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `id` | path | `string` | yes | Project ID |

**Request body:** `GitHubImportRequest`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Counts of created/skipped issues and rate-limit remaining | — |
| 400 | Bad repo, token, or rate limit | `ErrorEnvelope` |
| 404 | Project or repo not found | `ErrorEnvelope` |

#### `POST /api/projects/{id}/import-linear`

linear

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `id` | path | `string` | yes | Project ID |

**Request body:** `LinearImportRequest`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Counts of created and skipped issues | — |
| 400 | Bad API key, filter, or rate limit | `ErrorEnvelope` |
| 404 | Project not found | `ErrorEnvelope` |

---

## Export

Project export to JSON / YAML / CSV.

#### `GET /api/projects/{id}/export`

GET /api/projects/:id/export

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `id` | path | `string` | yes | Project ID |
| `format` | query | `string` | no |  |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Export file (JSON, YAML, or CSV per the `format` query) | — |
| 400 | Unsupported format | `ErrorEnvelope` |
| 404 | Project not found | `ErrorEnvelope` |

---

## Search

Full-text search within a project or globally.

#### `GET /api/projects/{project_id}/search`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `project_id` | path | `string` | yes | Project ID |
| `q` | query | `string` | yes |  |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Matching items | `Item`[] |

#### `GET /api/search`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `q` | query | `string` | yes |  |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Matching items across all projects | `Item`[] |

---

## Backup

Local and S3-compatible cloud backup / restore.

#### `GET /api/backup`

VACUUM INTO snapshot streamed as application/octet-stream.

| Status | Meaning | Schema |
|---|---|---|
| 200 | SQLite snapshot (secrets scrubbed) | — |
| 400 | Not a file-based database | `ErrorEnvelope` |

#### `GET /api/backup/remote`

list remote backups newest-first.

| Status | Meaning | Schema |
|---|---|---|
| 200 | Remote backup manifests, newest first | — |
| 409 | Remote backup not configured | `ErrorEnvelope` |

#### `POST /api/backup/remote`

create a bundle and upload it to the configured S3

| Status | Meaning | Schema |
|---|---|---|
| 200 | Backup manifest | — |
| 409 | Not configured, or another device has newer work | `ErrorEnvelope` |

#### `POST /api/backup/remote/restore`

download a bundle and stage it for next restart.

**Request body:** `RestoreRemoteRequest`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Restore staged for next restart | — |
| 404 | No remote backups found | `ErrorEnvelope` |
| 409 | Not configured, or restore would lose newer work | `ErrorEnvelope` |

#### `POST /api/backup/remote/verify`

download a bundle and validate it (sha256 +

**Request body:** `RestoreRemoteRequest`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Verification verdict plus the manifest | — |
| 404 | No remote backups found | `ErrorEnvelope` |
| 409 | Remote backup not configured | `ErrorEnvelope` |

#### `POST /api/restore`

Validate a SQLite backup and stage it for the next restart.

| Status | Meaning | Schema |
|---|---|---|
| 200 | Restore staged for next restart | — |
| 400 | Not a valid SQLite file | `ErrorEnvelope` |
| 409 | Uploaded schema is newer than this binary | `ErrorEnvelope` |

---

## Settings

Runtime-editable server settings (cloud backup).

#### `GET /api/settings/backup`

current cloud-backup configuration (secret masked).

| Status | Meaning | Schema |
|---|---|---|
| 200 | Cloud-backup config (secret masked as `secret_key_set`) | — |

#### `PUT /api/settings/backup`

save cloud-backup configuration.

**Request body:** `UpdateBackupSettings`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Updated config (secret masked) | — |
| 422 | Validation error | `ErrorEnvelope` |

---

## Execution Operator

Harness-agnostic runner fleet (Part III): PM-side execution-request/fleet/runner-enrollment/agent-profile management. Authenticated the same way as the rest of this API (operator session or API token); scopes idempotency and audit actor to the server-derived `x-tack-principal`, which a client cannot set (see `crate::middleware::inject_operator_principal`).

#### `GET /api/agent-profiles`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Every agent profile, by name | `AgentProfileListResponse` |

#### `POST /api/agent-profiles`

**Request body:** `CreateProfile`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Agent profile created | `CreateProfileResponse` |
| 409 | conflict (name already exists) | `RunnerV1ErrorEnvelope` |

#### `POST /api/attempts/{attempt_id}/decisions/{decision_id}/resolve`

Resolve a pending decision with an operator-supplied answer

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `attempt_id` | path | `string` | yes | Attempt ID the decision belongs to (opaque) |
| `decision_id` | path | `string` | yes | Decision ID, scoped to `attempt_id`: a decision_id that exists but belongs to a different attempt resolves as 404 not_found, indistinguishable from one that never existed at all — an attacker guessing another attempt's decision_id learns nothing. |
| `x-tack-decision-token` | header | `string` | yes | TACK_EXECUTION_DECISION_TOKEN — a second, independent operator credential *on top of* the ordinary operator auth every other `/api` route uses (never a substitute for it). Fail-closed: every call is rejected with 403 whenever the server has not configured TACK_EXECUTION_DECISION_TOKEN at all — there is no "no secret configured, allow everything" fallback the way the plain Bearer gate has for an unset TACK_API_TOKEN. |

**Request body:** `ResolveDecisionRequest`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Decision resolved — either a fresh write or a byte-identical idempotent replay of one (`replayed` distinguishes the two). | `ResolveDecisionResponseSchema` |
| 400 | invalid_request (missing/malformed answer, or answer.option_id is not one of this decision's own recorded options) | `RunnerV1ErrorEnvelope` |
| 401 | unauthorized — no x-tack-principal; a runner bearer credential never satisfies this, by construction | `RunnerV1ErrorEnvelope` |
| 403 | forbidden — x-tack-decision-token missing, unconfigured server-side, or mismatched (details.required_scope = "operator:decisions") | `RunnerV1ErrorEnvelope` |
| 404 | not_found — no decision exists for this exact (attempt_id, decision_id) pair | `RunnerV1ErrorEnvelope` |
| 409 | decision_expired / idempotency_conflict | `RunnerV1ErrorEnvelope` |
| 413 | payload_too_large (answer exceeds decision_answer_bytes_max, 32768 bytes) | `RunnerV1ErrorEnvelope` |

#### `GET /api/executions`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `item_id` | query | `string` | no |  |
| `item_ids` | query | `string` | no | Comma-separated item ids — returns exactly one row per id, its own |
| `limit` | query | `integer` | no |  |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Execution requests, newest first. `item_ids` present: exactly one row per id that has at least one execution, the batch's own most recent one. Else scoped to one item when `item_id` is given, otherwise every request the install has recorded up to `limit` | `ExecutionListResponse` |
| 400 | invalid_request (a malformed `item_ids` entry, or more ids than the route's cap) | `RunnerV1ErrorEnvelope` |

#### `POST /api/executions`

**Request body:** `CreateExecution`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Execution request created or idempotently replayed | `CreateExecutionResponse` |
| 400 | invalid_request | `RunnerV1ErrorEnvelope` |
| 404 | not_found (item does not exist) | `RunnerV1ErrorEnvelope` |
| 409 | conflict / idempotency_conflict / runner_revoked | `RunnerV1ErrorEnvelope` |

#### `GET /api/executions/{request_id}`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `request_id` | path | `string` | yes | Execution request ID (opaque) |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Execution request detail | `ExecutionDetailResponse` |
| 404 | not_found | `RunnerV1ErrorEnvelope` |

#### `GET /api/executions/{request_id}/attempts`

the operator read path

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `request_id` | path | `string` | yes | Execution request ID (opaque) |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Every attempt made against this request, oldest first (may be empty) | `AttemptListResponse` |
| 404 | not_found | `RunnerV1ErrorEnvelope` |

#### `GET /api/executions/{request_id}/attempts/{attempt_number}/artifacts`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `request_id` | path | `string` | yes | Execution request ID (opaque) |
| `attempt_number` | path | `integer` | yes | 1-based attempt number |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Every artifact manifested for this attempt, oldest first (may be empty) | `ArtifactListResponse` |
| 404 | not_found (execution_request or execution_attempt) | `RunnerV1ErrorEnvelope` |

#### `GET /api/executions/{request_id}/attempts/{attempt_number}/artifacts/{artifact_id}/content`

Download a verified artifact's raw content

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `request_id` | path | `string` | yes | Execution request ID (opaque) |
| `attempt_number` | path | `integer` | yes | 1-based attempt number within the execution request |
| `artifact_id` | path | `string` | yes | Artifact ID, scoped to the attempt that reported it (opaque) |

| Status | Meaning | Schema |
|---|---|---|
| 200 | The artifact's raw bytes. `Content-Type` is the artifact's declared `media_type`, or `application/octet-stream` when none was declared. | `string` |
| 401 | unauthorized — no authenticated operator principal | `RunnerV1ErrorEnvelope` |
| 404 | not_found (details.artifact_id) — no artifact manifest matches this (request_id, attempt_number, artifact_id) triple | `RunnerV1ErrorEnvelope` |
| 409 | conflict (details.artifact_id) — the artifact manifest exists but its content has not been verified yet; distinct from not_found, never silently treated as "gone" or zero bytes | `RunnerV1ErrorEnvelope` |

#### `GET /api/executions/{request_id}/attempts/{attempt_number}/decisions`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `request_id` | path | `string` | yes | Execution request ID (opaque) |
| `attempt_number` | path | `integer` | yes | 1-based attempt number |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Every decision raised against this attempt, oldest first (may be empty) | `DecisionListResponse` |
| 404 | not_found (execution_request or execution_attempt) | `RunnerV1ErrorEnvelope` |

#### `GET /api/executions/{request_id}/attempts/{attempt_number}/events`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `request_id` | path | `string` | yes | Execution request ID (opaque) |
| `attempt_number` | path | `integer` | yes | 1-based attempt number |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Every event this attempt has reported, oldest first (may be empty) | `EventListResponse` |
| 404 | not_found (execution_request or execution_attempt) | `RunnerV1ErrorEnvelope` |

#### `POST /api/executions/{request_id}/cancel`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `request_id` | path | `string` | yes | Execution request ID (opaque) |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Cancellation requested — not yet terminal | `CancellationRequestedResponse` |
| 404 | not_found | `RunnerV1ErrorEnvelope` |
| 409 | conflict (already terminal) | `RunnerV1ErrorEnvelope` |

#### `POST /api/executions/{request_id}/requeue`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `request_id` | path | `string` | yes | Execution request ID (opaque) |

**Request body:** `RecoveryConfirmation`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Requeued (or replayed) after an audited recovery decision | `RequeueResponse` |
| 409 | conflict / idempotency_conflict / invalid_transition | `RunnerV1ErrorEnvelope` |

#### `GET /api/runner-fleets`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Every runner fleet, by name | `FleetListResponse` |

#### `POST /api/runner-fleets`

**Request body:** `CreateFleet`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Fleet created | `CreateFleetResponse` |
| 409 | conflict (name already exists) | `RunnerV1ErrorEnvelope` |

#### `POST /api/runner-fleets/{fleet_id}/members`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `fleet_id` | path | `string` | yes | Fleet ID (opaque) |

**Request body:** `AddFleetMember`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Runner is now (or already was) a member of the fleet | `FleetMemberResponse` |
| 404 | not_found (fleet or runner does not exist) | `RunnerV1ErrorEnvelope` |

#### `DELETE /api/runner-fleets/{fleet_id}/members/{runner_id}`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `fleet_id` | path | `string` | yes | Fleet ID (opaque) |
| `runner_id` | path | `string` | yes | Runner ID (opaque) |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Runner removed from the fleet | `FleetMemberResponse` |
| 404 | not_found (runner was not a member of this fleet) | `RunnerV1ErrorEnvelope` |

#### `GET /api/runners`

the read path for `agent_runners`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `fleet_id` | path | `['string', 'null']` | yes | Optional roster filter — a runner is included only if it is a |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Every enrolled runner (optionally filtered to one fleet's roster) | `RunnerListResponse` |

#### `POST /api/runners/enrollment`

Creates a pending runner and stores only a SHA-256 enrollment-token hash.

**Request body:** `CreatePendingRunner`

| Status | Meaning | Schema |
|---|---|---|
| 200 | Pending runner created; the raw enrollment token is returned exactly once | `CreatePendingRunnerResponse` |
| 400 | invalid_request | `RunnerV1ErrorEnvelope` |
| 409 | conflict (name already exists) | `RunnerV1ErrorEnvelope` |

#### `POST /api/runners/{runner_id}/enrollment-tokens/{token_id}/revoke`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `runner_id` | path | `string` | yes | Runner ID (opaque) |
| `token_id` | path | `string` | yes | Enrollment token ID (opaque) |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Token revoked | `RevokeEnrollmentTokenResponse` |
| 404 | not_found | `RunnerV1ErrorEnvelope` |
| 409 | conflict (already consumed) | `RunnerV1ErrorEnvelope` |

#### `POST /api/runners/{runner_id}/revoke`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `runner_id` | path | `string` | yes | Runner ID (opaque) |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Runner revoked | `RevokeRunnerResponse` |
| 404 | not_found | `RunnerV1ErrorEnvelope` |

---

## Runner Protocol V1

Harness-agnostic runner fleet (Part III): the pull protocol a `tack-runner` process speaks at `/api/runner/v1` (enroll, claim, heartbeat, report). Authenticated by a distinct, per-runner hashed bearer credential — never the operator token, and never substitutable for it (docs/contracts/runner-v1/protocol.json: `credentials_are_not_substitutable`). Every wire shape is frozen by docs/contracts/runner-v1/, not independently re-specified here.

#### `POST /api/runner/v1/attempts/{attempt_id}/accept`

Report the attempt entering `preparing`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `attempt_id` | path | `string` | yes | Attempt ID, issued at claim time (opaque) |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Transition accepted or replayed | — |
| 400 | invalid_request | `RunnerV1ErrorEnvelope` |
| 401 | unauthorized | `RunnerV1ErrorEnvelope` |
| 403 | forbidden / runner_revoked | `RunnerV1ErrorEnvelope` |
| 404 | not_found | `RunnerV1ErrorEnvelope` |
| 409 | conflict / idempotency_conflict / invalid_transition / stale_lease | `RunnerV1ErrorEnvelope` |

#### `POST /api/runner/v1/attempts/{attempt_id}/artifacts`

Submit an artifact manifest (content upload is the separate PUT .../artifacts/{artifact_id}/content operation below; content download is a distinct, operator-facing route — see `execution-operator`'s "Download a verified artifact's raw content")

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `attempt_id` | path | `string` | yes | Attempt ID, issued at claim time (opaque) |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Manifest accepted; per-artifact upload URLs issued | — |
| 400 | invalid_request | `RunnerV1ErrorEnvelope` |
| 401 | unauthorized | `RunnerV1ErrorEnvelope` |
| 403 | forbidden / runner_revoked | `RunnerV1ErrorEnvelope` |
| 404 | not_found | `RunnerV1ErrorEnvelope` |
| 409 | conflict / idempotency_conflict / invalid_transition / stale_lease | `RunnerV1ErrorEnvelope` |

#### `PUT /api/runner/v1/attempts/{attempt_id}/artifacts/{artifact_id}/content`

Upload one manifested artifact's verified raw content

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `attempt_id` | path | `string` | yes | Attempt ID, issued at claim time (opaque) |
| `artifact_id` | path | `string` | yes | Artifact ID from this attempt's prior manifest submission (`POST .../artifacts`, opaque) |
| `x-tack-fencing-token` | header | `string` | yes | The attempt's current fencing token. The request body is raw bytes, so — unlike every other runner-protocol write — the fencing token cannot travel inside a JSON body, so it travels as a header instead. docs/contracts/runner-v1/ fixes the manifest exchange's payload shape, not this upload URL (see this fragment's own doc comment). |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Content verified and committed: {protocol_version, attempt_id, artifact_id, state: "content_verified", size_bytes, sha256} | — |
| 400 | invalid_request (Content-Type mismatch, or the upload stream ended early) | `RunnerV1ErrorEnvelope` |
| 401 | unauthorized | `RunnerV1ErrorEnvelope` |
| 403 | forbidden / runner_revoked | `RunnerV1ErrorEnvelope` |
| 409 | conflict (content already recorded and is immutable; or the attempt is not currently running/waiting_decision) / artifact_checksum_mismatch / stale_lease | `RunnerV1ErrorEnvelope` |
| 413 | payload_too_large (artifact_content_bytes_max) | `RunnerV1ErrorEnvelope` |

#### `POST /api/runner/v1/attempts/{attempt_id}/cancellation-observation`

Report the observed effect of a requested cancellation

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `attempt_id` | path | `string` | yes | Attempt ID, issued at claim time (opaque) |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Cancellation observation committed or replayed | — |
| 400 | invalid_request | `RunnerV1ErrorEnvelope` |
| 401 | unauthorized | `RunnerV1ErrorEnvelope` |
| 403 | forbidden / runner_revoked | `RunnerV1ErrorEnvelope` |
| 404 | not_found | `RunnerV1ErrorEnvelope` |
| 409 | conflict / idempotency_conflict / invalid_transition / stale_lease | `RunnerV1ErrorEnvelope` |

#### `POST /api/runner/v1/attempts/{attempt_id}/completion`

Report the attempt's terminal outcome

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `attempt_id` | path | `string` | yes | Attempt ID, issued at claim time (opaque) |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Completion committed or replayed | — |
| 400 | invalid_request | `RunnerV1ErrorEnvelope` |
| 401 | unauthorized | `RunnerV1ErrorEnvelope` |
| 403 | forbidden / runner_revoked | `RunnerV1ErrorEnvelope` |
| 404 | not_found | `RunnerV1ErrorEnvelope` |
| 409 | conflict / idempotency_conflict / invalid_transition / stale_lease | `RunnerV1ErrorEnvelope` |

#### `POST /api/runner/v1/attempts/{attempt_id}/decisions`

Create a decision for later out-of-band operator resolution

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `attempt_id` | path | `string` | yes | Attempt ID, issued at claim time (opaque) |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Decision recorded | — |
| 400 | invalid_request | `RunnerV1ErrorEnvelope` |
| 401 | unauthorized | `RunnerV1ErrorEnvelope` |
| 403 | forbidden / runner_revoked | `RunnerV1ErrorEnvelope` |
| 404 | not_found | `RunnerV1ErrorEnvelope` |
| 409 | conflict / idempotency_conflict / invalid_transition / stale_lease | `RunnerV1ErrorEnvelope` |

#### `POST /api/runner/v1/attempts/{attempt_id}/decisions/poll`

Poll for decision resolutions since a given timestamp

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `attempt_id` | path | `string` | yes | Attempt ID, issued at claim time (opaque) |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Resolved decisions since `after`, plus the new `next_after` cursor | — |
| 400 | invalid_request | `RunnerV1ErrorEnvelope` |
| 401 | unauthorized | `RunnerV1ErrorEnvelope` |
| 403 | forbidden / runner_revoked | `RunnerV1ErrorEnvelope` |
| 404 | not_found | `RunnerV1ErrorEnvelope` |
| 409 | conflict / idempotency_conflict / invalid_transition / stale_lease | `RunnerV1ErrorEnvelope` |

#### `POST /api/runner/v1/attempts/{attempt_id}/events`

Append a fenced, checkpointed batch of execution events

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `attempt_id` | path | `string` | yes | Attempt ID, issued at claim time (opaque) |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Batch committed (accepted/duplicate event ids, committed checkpoint) | — |
| 400 | invalid_request | `RunnerV1ErrorEnvelope` |
| 401 | unauthorized | `RunnerV1ErrorEnvelope` |
| 403 | forbidden / runner_revoked | `RunnerV1ErrorEnvelope` |
| 404 | not_found | `RunnerV1ErrorEnvelope` |
| 409 | conflict / idempotency_conflict / invalid_transition / stale_lease | `RunnerV1ErrorEnvelope` |

#### `POST /api/runner/v1/attempts/{attempt_id}/recovery-observation`

Report a post-restart recovery observation for an attempt (additive v1 operation; exact path fixed by protocol.json)

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `attempt_id` | path | `string` | yes | Attempt ID, issued at claim time (opaque) |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Recovery observation committed or replayed; server-authoritative disposition returned | — |
| 400 | invalid_request | `RunnerV1ErrorEnvelope` |
| 401 | unauthorized | `RunnerV1ErrorEnvelope` |
| 403 | forbidden / runner_revoked | `RunnerV1ErrorEnvelope` |
| 404 | not_found | `RunnerV1ErrorEnvelope` |
| 409 | conflict / idempotency_conflict / invalid_transition / stale_lease | `RunnerV1ErrorEnvelope` |

#### `POST /api/runner/v1/attempts/{attempt_id}/start`

Report the attempt entering `running`

| Param | In | Type | Required | Description |
|---|---|---|---|---|
| `attempt_id` | path | `string` | yes | Attempt ID, issued at claim time (opaque) |

| Status | Meaning | Schema |
|---|---|---|
| 200 | Transition accepted or replayed | — |
| 400 | invalid_request | `RunnerV1ErrorEnvelope` |
| 401 | unauthorized | `RunnerV1ErrorEnvelope` |
| 403 | forbidden / runner_revoked | `RunnerV1ErrorEnvelope` |
| 404 | not_found | `RunnerV1ErrorEnvelope` |
| 409 | conflict / idempotency_conflict / invalid_transition / stale_lease | `RunnerV1ErrorEnvelope` |

#### `POST /api/runner/v1/claim`

Claim the next eligible execution request for this runner or its fleet

| Status | Meaning | Schema |
|---|---|---|
| 200 | A fenced lease and the immutable request snapshot, or no_eligible_work | — |
| 400 | invalid_request | `RunnerV1ErrorEnvelope` |
| 401 | unauthorized | `RunnerV1ErrorEnvelope` |
| 403 | forbidden / runner_revoked | `RunnerV1ErrorEnvelope` |
| 404 | not_found | `RunnerV1ErrorEnvelope` |
| 409 | conflict / idempotency_conflict / invalid_transition / stale_lease | `RunnerV1ErrorEnvelope` |

#### `POST /api/runner/v1/enroll`

Exchange a single-use enrollment token for a runner identity and bearer credential

| Status | Meaning | Schema |
|---|---|---|
| 200 | Runner enrolled; the raw bearer credential is returned exactly once | — |
| 400 | invalid_request | `RunnerV1ErrorEnvelope` |
| 401 | unauthorized | `RunnerV1ErrorEnvelope` |
| 403 | forbidden / runner_revoked | `RunnerV1ErrorEnvelope` |
| 404 | not_found | `RunnerV1ErrorEnvelope` |
| 409 | conflict / idempotency_conflict / invalid_transition / stale_lease | `RunnerV1ErrorEnvelope` |

#### `POST /api/runner/v1/heartbeat`

Report liveness, capacity, and active-attempt state in one fenced batch

| Status | Meaning | Schema |
|---|---|---|
| 200 | Renewed lease facts per reported attempt | — |
| 400 | invalid_request | `RunnerV1ErrorEnvelope` |
| 401 | unauthorized | `RunnerV1ErrorEnvelope` |
| 403 | forbidden / runner_revoked | `RunnerV1ErrorEnvelope` |
| 404 | not_found | `RunnerV1ErrorEnvelope` |
| 409 | conflict / idempotency_conflict / invalid_transition / stale_lease | `RunnerV1ErrorEnvelope` |

#### `POST /api/runner/v1/refresh`

Refresh reported capabilities and optionally rotate the runner's bearer credential

| Status | Meaning | Schema |
|---|---|---|
| 200 | Capabilities accepted; a rotated credential, if requested, is returned exactly once | — |
| 400 | invalid_request | `RunnerV1ErrorEnvelope` |
| 401 | unauthorized | `RunnerV1ErrorEnvelope` |
| 403 | forbidden / runner_revoked | `RunnerV1ErrorEnvelope` |
| 404 | not_found | `RunnerV1ErrorEnvelope` |
| 409 | conflict / idempotency_conflict / invalid_transition / stale_lease | `RunnerV1ErrorEnvelope` |

---

## Local Runner

#### `GET /api/local-runner`

the persisted preference, the live runtime

| Status | Meaning | Schema |
|---|---|---|
| 200 | Embedded-runner preference, runtime state, and provider catalog | — |

#### `PUT /api/local-runner`

save the preference and start/stop the embedded

**Request body:** `UpdateLocalRunner`

| Status | Meaning | Schema |
|---|---|---|
| 204 | Preference saved and the runtime reconciled to match | — |

#### `GET /api/local-runner/secrets`

names and set-at timestamps only.

| Status | Meaning | Schema |
|---|---|---|
| 200 | Stored secret names and set-at timestamps, never values | — |

#### `DELETE /api/local-runner/secrets/{name}`

not an error if already absent.

| Status | Meaning | Schema |
|---|---|---|
| 204 | Removed (or already absent) | — |

#### `PUT /api/local-runner/secrets/{name}`

store a value. Never echoes it

**Request body:** `SetLocalRunnerSecret`

| Status | Meaning | Schema |
|---|---|---|
| 204 | Stored; the value is never echoed back | — |

---
