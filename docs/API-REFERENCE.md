# Tack API Reference

**Tack version:** 0.1.0-beta.6
**Base URL:** `http://localhost:3210/api`
**WebSocket URL:** `ws://localhost:3210/api/projects/{id}/boards/live`

**Total endpoints:** 68 REST + 1 WebSocket

> This is the canonical, request/response-level API reference. The mdBook page
> [Developer → API Reference](book/src/developer/api-reference.md) is a shorter
> endpoint summary that links here for full schemas.

---

## Table of Contents

1. [Authentication](#authentication)
2. [Projects](#projects)
3. [Board](#board)
4. [Items](#items)
5. [Dependencies](#dependencies)
6. [Attachments](#attachments)
7. [Sprints](#sprints)
8. [Roles](#roles)
9. [Comments](#comments)
10. [Search](#search)
11. [Export/Import](#exportimport)
12. **[NEW v1.2]** [Templates](#templates)
13. **[NEW v1.2]** [Custom Fields](#custom-fields)
14. **[NEW v1.2]** [Multiple Boards](#multiple-boards)
15. [WebSocket Events](#websocket-events)
16. [Alexa Voice Integration](#alexa-voice-integration)
17. [Remote Cloud Backup](#remote-cloud-backup)
18. [Error Responses](#error-responses)

---

## Authentication

Authentication is **optional and off by default** (solo/small-team focus). When the
server is started with `TACK_API_TOKEN` set, every `/api/*` route except
`GET /api/health` and `POST /api/alexa` requires a matching bearer token:

```http
Authorization: Bearer <token>
```

Requests with a missing or wrong token receive `401 Unauthorized`. There are no
per-user accounts — all clients share the single token. See
[Administration & Security](book/src/user-guide/administration.md) for setup.

---

## Projects

### List All Projects

```http
GET /api/projects
```

**Response:**
```json
[
  {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "workspace_id": "default",
    "name": "My Software Project",
    "description": "A web application for task management",
    "project_type": "software",
    "vocabulary": {
      "task": "User Story",
      "sprint": "Iteration"
    },
    "workflow": {
      "statuses": [
        {
          "name": "Todo",
          "category": "backlog",
          "wip_limit": null
        },
        {
          "name": "In Progress",
          "category": "in_progress",
          "wip_limit": 3
        },
        {
          "name": "Done",
          "category": "done",
          "wip_limit": null
        }
      ],
      "transitions": null
    },
    "created_at": "2026-03-16T10:00:00Z",
    "updated_at": "2026-03-16T10:00:00Z",
    "archived": false
  }
]
```

### Create Project

```http
POST /api/projects
Content-Type: application/json

{
  "name": "My Project",
  "description": "Optional description",
  "template": "software"
}
```

**Templates:** `software`, `web`, `mobile`, `construction`, `personal`, `homework`, `maintenance`, `custom`

**Response:** `201 Created` + Project object

### Get Project

```http
GET /api/projects/{id}
```

**Response:** Project object

### Update Project

```http
PUT /api/projects/{id}
Content-Type: application/json

{
  "name": "Updated Name",
  "description": "Updated description"
}
```

**Response:** Updated Project object

### Delete Project

```http
DELETE /api/projects/{id}
```

**Response:** `204 No Content`

---

## Board

### Get Board State

```http
GET /api/projects/{id}/board
```

**Response:**
```json
{
  "columns": [
    {
      "status": "Todo",
      "wip_limit": null,
      "wip_exceeded": false,
      "items": [
        {
          "id": "660e8400-e29b-41d4-a716-446655440001",
          "project_id": "550e8400-e29b-41d4-a716-446655440000",
          "parent_id": null,
          "title": "Implement user authentication",
          "description": "Add JWT-based authentication",
          "item_type": "task",
          "status": "Todo",
          "priority": "high",
          "estimate": 5,
          "estimate_unit": "story_points",
          "tags": ["backend", "security"],
          "sort_order": 0,
          "sprint_id": null,
          "due_date": null,
          "started_at": null,
          "completed_at": null,
          "created_at": "2026-03-16T10:30:00Z",
          "updated_at": "2026-03-16T10:30:00Z"
        }
      ]
    }
  ]
}
```

### Update Board Config

```http
PATCH /api/projects/{id}/board
Content-Type: application/json

{
  "wip_limits": {
    "In Progress": 3,
    "Code Review": 2
  }
}
```

**Response:** Updated board state + broadcasts `BoardConfigUpdated` event

### WebSocket Connection (Real-time Updates)

```javascript
const ws = new WebSocket('ws://localhost:3210/api/projects/{id}/boards/live');

ws.onmessage = (event) => {
  const boardEvent = JSON.parse(event.data);
  console.log('Event:', boardEvent.event_type);
  // Handle: ItemCreated, ItemUpdated, ItemDeleted, BoardConfigUpdated, Ping
};
```

---

## Items

### List Items (Scoped to Project)

```http
GET /api/projects/{id}/items?status=Todo&priority=high&limit=50&offset=0
```

**Query Parameters:**
- `status` - Filter by status
- `priority` - Filter by priority (critical, high, medium, low, none)
- `item_type` - Filter by type (epic, feature, task, subtask, bug, requirement)
- `sprint_id` - Filter by sprint
- `parent_id` - Filter by parent (use "null" for top-level items)
- `limit` - Page size (default: 50, max: 100)
- `offset` - Pagination offset

**Response:** Array of Item objects

### Create Item

```http
POST /api/projects/{id}/items
Content-Type: application/json

{
  "title": "Implement user authentication",
  "description": "Add JWT-based authentication with refresh tokens",
  "item_type": "task",
  "status": "Todo",
  "priority": "high",
  "estimate": 5,
  "estimate_unit": "story_points",
  "tags": ["backend", "security"],
  "parent_id": null,
  "sprint_id": null,
  "due_date": "2026-03-20T00:00:00Z"
}
```

**Response:** `201 Created` + Item object + broadcasts `ItemCreated` event

### Get Item

```http
GET /api/items/{id}
```

**Response:** Item object

### Update Item

```http
PATCH /api/items/{id}
Content-Type: application/json

{
  "status": "In Progress",
  "priority": "critical",
  "estimate": 8
}
```

**Response:** Updated Item object + broadcasts `ItemUpdated` event

**Note:** Status changes trigger workflow validation. Auto-updates `started_at` when moving to in-progress, `completed_at` when moving to done.

### Delete Item

```http
DELETE /api/items/{id}
```

**Response:** `204 No Content` + broadcasts `ItemDeleted` event

---

## Dependencies

### List Dependencies

```http
GET /api/items/{id}/dependencies
```

**Response:**
```json
{
  "blocks": [
    {
      "id": "770e8400-e29b-41d4-a716-446655440002",
      "title": "Backend API implementation",
      "status": "In Progress"
    }
  ],
  "blocked_by": [
    {
      "id": "880e8400-e29b-41d4-a716-446655440003",
      "title": "Database schema design",
      "status": "Done"
    }
  ]
}
```

### Add Dependency

```http
POST /api/items/{id}/dependencies
Content-Type: application/json

{
  "depends_on_id": "880e8400-e29b-41d4-a716-446655440003"
}
```

**Response:** `201 Created`

**Validation:** Prevents cycles (A depends on B, B depends on A)

### Remove Dependency

```http
DELETE /api/items/{source_id}/dependencies/{target_id}
```

**Response:** `204 No Content`

---

## Attachments

### Upload Attachment

```http
POST /api/items/{id}/attachments
Content-Type: multipart/form-data

file: <binary data>
```

**Limits:** Max 50MB per file

**Response:**
```json
{
  "id": "990e8400-e29b-41d4-a716-446655440004",
  "item_id": "660e8400-e29b-41d4-a716-446655440001",
  "filename": "screenshot.png",
  "mime_type": "image/png",
  "size_bytes": 1024000,
  "storage_path": "storage/660e8400-e29b-41d4-a716-446655440001/screenshot-uuid.png",
  "uploaded_at": "2026-03-16T11:00:00Z"
}
```

### Download Attachment

```http
GET /api/attachments/{id}
```

**Response:** Binary file with proper Content-Type and Content-Disposition headers

### Delete Attachment

```http
DELETE /api/attachments/{id}
```

**Response:** `204 No Content`

---

## Sprints

### List Sprints

```http
GET /api/projects/{id}/sprints?status=active
```

**Query Parameters:**
- `status` - Filter by status (planning, active, review, closed)

**Response:** Array of Sprint objects

### Create Sprint

```http
POST /api/projects/{id}/sprints
Content-Type: application/json

{
  "name": "Sprint 1",
  "goal": "Implement authentication and user management",
  "start_date": "2026-03-16T00:00:00Z",
  "end_date": "2026-03-30T00:00:00Z",
  "status": "planning"
}
```

**Response:** `201 Created` + Sprint object

### Update Sprint

```http
PATCH /api/sprints/{id}
Content-Type: application/json

{
  "status": "active",
  "goal": "Updated goal"
}
```

**Response:** Updated Sprint object

### Delete Sprint

```http
DELETE /api/sprints/{id}
```

**Response:** `204 No Content`

---

## Roles

### List Roles

```http
GET /api/projects/{id}/roles
```

**Response:** Array of Role objects

### Create Role

```http
POST /api/projects/{id}/roles
Content-Type: application/json

{
  "name": "Backend Developer",
  "description": "Handles API and database development"
}
```

**Response:** `201 Created` + Role object

### Assign Role to Item

```http
POST /api/roles/{role_id}/items/{item_id}
```

**Response:** `204 No Content`

### Unassign Role

```http
DELETE /api/roles/{role_id}/items/{item_id}
```

**Response:** `204 No Content`

---

## Comments

### List Comments

```http
GET /api/items/{id}/comments
```

**Response:** Array of Comment objects

### Create Comment

```http
POST /api/items/{id}/comments
Content-Type: application/json

{
  "content": "This needs to be prioritized for the next sprint",
  "author": "John Doe"
}
```

**Response:** `201 Created` + Comment object

---

## Search

### Global Search (All Projects)

```http
GET /api/search?q=authentication&workspace_id=default
```

**Query Parameters:**
- `q` - Search query (min 2 characters)
- `workspace_id` - Optional workspace filter

**Response:** Array of matching Item objects

**Note:** Uses SQLite FTS5 full-text search on titles, descriptions, and tags

### Project-Scoped Search

```http
GET /api/projects/{id}/search?q=authentication
```

**Response:** Array of matching Item objects within the project

---

## Export/Import

### Export Project (JSON)

```http
GET /api/projects/{id}/export?format=json
```

**Response:** Complete project snapshot including items, sprints, workflow config

### Export Project (CSV)

```http
GET /api/projects/{id}/export?format=csv
```

**Response:** CSV file with item list (id, title, type, status, priority, parent_id, created_at)

### Import Project (Placeholder)

```http
POST /api/projects/import
Content-Type: application/json

{
  "project": { /* Project data */ },
  "items": [ /* Items array */ ],
  "sprints": [ /* Sprints array */ ]
}
```

**Status:** Basic validation implemented, full import pending

---

## Templates

**NEW in v1.2** - Reusable project blueprints with workflow, vocabulary, custom fields, and boards

### Create Template

```http
POST /api/templates
Content-Type: application/json

{
  "name": "Software Development (Scrum)",
  "description": "Full-featured scrum template with sprints and roles",
  "project_type": "software",
  "vocabulary": {
    "task": "User Story",
    "sprint": "Sprint",
    "epic": "Epic"
  },
  "workflow": {
    "workflow_type": "scrum",
    "statuses": [
      {"name": "Backlog", "category": "todo", "wip_limit": null, "order": 0},
      {"name": "In Progress", "category": "in_progress", "wip_limit": 5, "order": 1},
      {"name": "Done", "category": "done", "wip_limit": null, "order": 2}
    ]
  },
  "custom_fields": [
    {"name": "Customer", "field_type": "text", "required": false}
  ],
  "default_boards": [
    {"name": "Main Board", "grouping": "status", "is_default": true}
  ],
  "is_builtin": false
}
```

**Response:**
```json
{
  "id": "template-uuid",
  "name": "Software Development (Scrum)",
  "description": "Full-featured scrum template",
  "project_type": "software",
  "is_builtin": false,
  "created_at": "2026-03-17T10:00:00Z",
  "updated_at": "2026-03-17T10:00:00Z"
}
```

### List Templates

```http
GET /api/templates
GET /api/templates?project_type=software
```

**Response:**
```json
[
  {
    "id": "template-uuid",
    "name": "Software Development (Scrum)",
    "project_type": "software",
    "is_builtin": false,
    "created_at": "2026-03-17T10:00:00Z"
  }
]
```

### Get Template

```http
GET /api/templates/{id}
```

**Response:** Full template object with all configuration

### Delete Template

```http
DELETE /api/templates/{id}
```

**Note:** Built-in templates (is_builtin=true) cannot be deleted

**Response:** 204 No Content

### Create Project from Template

```http
POST /api/projects/from-template/{template_id}
Content-Type: application/json

{
  "name": "My New Project",
  "description": "Created from template"
}
```

**Response:** Complete project object with template configuration applied

**What Gets Applied:**
- Workflow configuration (statuses, WIP limits)
- Vocabulary mappings
- Custom field definitions
- Default boards

---

## Custom Fields

**NEW in v1.2** - User-defined metadata fields with 9 types

### Field Types

- `text` - Short text input
- `long_text` - Multi-line text area
- `number` - Numeric input
- `date` - Date picker
- `boolean` - True/false checkbox
- `select` - Single choice dropdown (requires options)
- `multi_select` - Multiple choice (requires options)
- `url` - Website link with validation
- `email` - Email address with validation

### Create Custom Field

```http
POST /api/projects/{project_id}/custom-fields
Content-Type: application/json

{
  "name": "Customer",
  "field_type": "text",
  "description": "Customer name for this work item",
  "required": false,
  "default_value": null,
  "options": null,
  "validation": null
}
```

**For select/multi_select fields:**
```json
{
  "name": "Priority Level",
  "field_type": "select",
  "options": ["Low", "Medium", "High", "Critical"],
  "required": true
}
```

**Response:**
```json
{
  "id": "field-uuid",
  "project_id": "project-uuid",
  "name": "Customer",
  "field_type": "text",
  "description": "Customer name for this work item",
  "required": false,
  "default_value": null,
  "options": null,
  "validation": null,
  "created_at": "2026-03-17T10:00:00Z",
  "updated_at": "2026-03-17T10:00:00Z"
}
```

### List Custom Fields

```http
GET /api/projects/{project_id}/custom-fields
```

**Response:**
```json
[
  {
    "id": "field-uuid",
    "project_id": "project-uuid",
    "name": "Customer",
    "field_type": "text",
    "required": false
  }
]
```

### Get Custom Field

```http
GET /api/custom-fields/{id}
```

**Response:** Full field definition object

### Update Custom Field

```http
PATCH /api/custom-fields/{id}
Content-Type: application/json

{
  "name": "Client Name",
  "description": "Updated description",
  "required": true
}
```

**Response:** Updated field object

### Delete Custom Field

```http
DELETE /api/custom-fields/{id}
```

**Note:** Cascade deletes all field values for items

**Response:** 204 No Content

### Set Field Value (Upsert)

```http
PUT /api/items/{item_id}/custom-fields/{field_id}
Content-Type: application/json

{
  "value": "Acme Corp"
}
```

**For different field types:**
- `text`, `long_text`, `url`, `email`: String value
- `number`: Numeric value
- `date`: ISO 8601 string ("2026-03-17T00:00:00Z")
- `boolean`: true/false
- `select`: Single string from options
- `multi_select`: Array of strings from options

**Response:**
```json
{
  "item_id": "item-uuid",
  "field_id": "field-uuid",
  "value": "Acme Corp",
  "updated_at": "2026-03-17T10:00:00Z"
}
```

### Get Field Value

```http
GET /api/items/{item_id}/custom-fields/{field_id}
```

**Response:** Field value object or 404 if not set

### Get All Field Values for Item

```http
GET /api/items/{item_id}/custom-fields
```

**Response:**
```json
[
  {
    "field_id": "field-uuid",
    "field_name": "Customer",
    "field_type": "text",
    "value": "Acme Corp"
  }
]
```

### Delete Field Value

```http
DELETE /api/items/{item_id}/custom-fields/{field_id}
```

**Response:** 204 No Content

---

## Multiple Boards

**NEW in v1.2** - Create unlimited boards per project with different groupings

### Grouping Options

- `status` - Group by item status (default)
- `priority` - Group by priority level
- `item_type` - Group by item type (epic, task, etc.)
- `sprint` - Group by sprint
- `assignee` - Group by assigned role
- `custom_field` - Group by custom field value (use `{"custom_field": "field-uuid"}`)

### Create Board

```http
POST /api/projects/{project_id}/boards
Content-Type: application/json

{
  "name": "Priority View",
  "description": "Items grouped by priority",
  "grouping": "priority",
  "filters": null,
  "is_default": false
}
```

**For custom field grouping:**
```json
{
  "name": "Customer Board",
  "grouping": {"custom_field": "field-uuid"},
  "is_default": false
}
```

**Response:**
```json
{
  "id": "board-uuid",
  "project_id": "project-uuid",
  "name": "Priority View",
  "description": "Items grouped by priority",
  "grouping": "priority",
  "filters": null,
  "is_default": false,
  "created_at": "2026-03-17T10:00:00Z",
  "updated_at": "2026-03-17T10:00:00Z"
}
```

**Note:** When creating a board with is_default=true, other boards are automatically unmarked

### List Boards

```http
GET /api/projects/{project_id}/boards
```

**Response:**
```json
[
  {
    "id": "board-uuid",
    "name": "Main Board",
    "grouping": "status",
    "is_default": true
  },
  {
    "id": "board-uuid-2",
    "name": "Priority View",
    "grouping": "priority",
    "is_default": false
  }
]
```

### Get Board

```http
GET /api/boards/{id}
```

**Response:** Full board object

### Update Board

```http
PATCH /api/boards/{id}
Content-Type: application/json

{
  "name": "Updated Name",
  "grouping": "sprint",
  "is_default": true
}
```

**Response:** Updated board object

### Delete Board

```http
DELETE /api/boards/{id}
```

**Note:** Cannot delete the last board or a default board without setting another as default first

**Response:** 204 No Content

### Get Board View (with Grouped Items)

```http
GET /api/boards/{id}/view
```

**Response:**
```json
{
  "board": {
    "id": "board-uuid",
    "name": "Priority View",
    "grouping": "priority"
  },
  "columns": [
    {
      "name": "Critical",
      "items": [
        {
          "id": "item-uuid",
          "title": "Fix production bug",
          "priority": "critical",
          "status": "In Progress"
        }
      ]
    },
    {
      "name": "High",
      "items": [...]
    }
  ]
}
```

**Smart Grouping:** Items are automatically grouped based on the board's grouping configuration

---

## WebSocket Events

### Event Types

**ItemCreated**
```json
{
  "event_type": "ItemCreated",
  "project_id": "550e8400-e29b-41d4-a716-446655440000",
  "item_id": "660e8400-e29b-41d4-a716-446655440001",
  "timestamp": "2026-03-16T11:00:00Z"
}
```

**ItemUpdated**
```json
{
  "event_type": "ItemUpdated",
  "project_id": "550e8400-e29b-41d4-a716-446655440000",
  "item_id": "660e8400-e29b-41d4-a716-446655440001",
  "timestamp": "2026-03-16T11:00:00Z"
}
```

**ItemDeleted**
```json
{
  "event_type": "ItemDeleted",
  "project_id": "550e8400-e29b-41d4-a716-446655440000",
  "item_id": "660e8400-e29b-41d4-a716-446655440001",
  "timestamp": "2026-03-16T11:00:00Z"
}
```

**BoardConfigUpdated**
```json
{
  "event_type": "BoardConfigUpdated",
  "project_id": "550e8400-e29b-41d4-a716-446655440000",
  "timestamp": "2026-03-16T11:00:00Z"
}
```

**Ping** (Keepalive)
```json
{
  "event_type": "Ping",
  "timestamp": "2026-03-16T11:00:00Z"
}
```

### Client Implementation

```javascript
const projectId = '550e8400-e29b-41d4-a716-446655440000';
const ws = new WebSocket(`ws://localhost:3210/api/projects/${projectId}/boards/live`);

ws.onopen = () => console.log('Connected');
ws.onerror = (error) => console.error('WebSocket error:', error);
ws.onclose = () => console.log('Disconnected');

ws.onmessage = (event) => {
  const boardEvent = JSON.parse(event.data);

  switch (boardEvent.event_type) {
    case 'ItemCreated':
    case 'ItemUpdated':
    case 'ItemDeleted':
    case 'BoardConfigUpdated':
      // Refetch board data
      fetchBoard();
      break;
    case 'Ping':
      // Keepalive, ignore
      break;
  }
};
```

---

## Alexa Voice Integration

### Receive Alexa Skill Request

```http
POST /api/alexa
```

Receives requests from an Amazon Alexa custom skill and maps voice intents onto
the regular item/workflow logic. See [ALEXA.md](ALEXA.md) for skill setup and
the interaction model.

**Enabling:** disabled by default. Set `TACK_ALEXA_SKILL_ID` (or
`alexa_skill_id` in `tack.toml`) to your skill's ID (`amzn1.ask.skill.…`).

**Authentication:** this route is exempt from the Bearer-token gate (Alexa
cannot send custom headers). Instead, every request's embedded
`applicationId` is verified against the configured skill ID (constant-time
comparison), and timestamps older than ±150 seconds are rejected.

**Supported intents:**

| Intent | Slots | Action |
| ------ | ----- | ------ |
| `AddTaskIntent` | `title` (required), `project` (optional) | Creates an item at the workflow's initial status |
| `ListTasksIntent` | `project` (optional) | Speaks the count and first few open items |
| `CompleteTaskIntent` | `title` (required), `project` (optional) | Moves the matching open item to the first Done status (validates transitions and WIP limits) |

When no `project` slot is given, the most recently updated project is used and
its name is always spoken back.

Responses are localised from the request's `locale` field: `es-*` locales get
Spanish speech, everything else English.

**Responses:** user-level problems (unknown project, missing slot, invalid
transition) return `200 OK` with spoken text, as the Alexa protocol requires.
HTTP errors are reserved for verification failures:

| Status | Meaning |
| ------ | ------- |
| `404` | Integration not enabled (`alexa_skill_id` unset) |
| `403` | Application ID missing or does not match the configured skill ID |
| `400` | Request timestamp missing or outside the ±150 s tolerance |

---

## Remote Cloud Backup

Requires a cloud destination to be configured — either via the `TACK_BACKUP_*`
environment variables (see [configuration](../CLAUDE.md)) or at runtime through the
settings endpoint below (UI: **Settings → Cloud Backup**). The backup/list/restore
endpoints return `409 Conflict` when no destination is configured.

### Trigger Remote Backup

```http
POST /api/backup/remote
```

Creates a `.tar.zst` bundle of the SQLite database + attachments directory and uploads
it to the configured S3-compatible bucket. Prunes old backups to keep only
`TACK_BACKUP_RETENTION` (default 10) copies.

**Response `200`:**
```json
{
  "format_version": 1,
  "created_at": "2026-06-12T15:04:05+00:00",
  "migration_version": 18,
  "db_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
  "install_id": "a1b2c3d4-...",
  "item_count": 42,
  "object_key": "tack/tack-backup-2026-06-12T15-04-05+00-00Z.tar.zst",
  "bundle_size_bytes": 1048576
}
```

### List Remote Backups

```http
GET /api/backup/remote
```

Returns all backups in the bucket, newest first, by reading lightweight sidecar
manifests (no full bundle download needed).

**Response `200`:** array of manifest objects (same shape as above).

### Stage Remote Restore

```http
POST /api/backup/remote/restore
Content-Type: application/json

{ "key": "tack/tack-backup-2026-06-12T15-04-05+00-00Z.tar.zst" }
```

Downloads the bundle and stages it for the next server restart. Omit `key` (or send
an empty body) to restore the latest backup automatically.

**Rejection:** returns `409 Conflict` when the snapshot's `migration_version` is
higher than the running binary's — upgrade Tack before restoring.

**Response `200`:**
```json
{
  "staged": true,
  "restart_required": true,
  "object_key": "tack/tack-backup-2026-06-12T15-04-05+00-00Z.tar.zst",
  "message": "Restore staged. Restart the server to apply."
}
```

After the server restarts, both the database and the attachments directory are
swapped atomically. The previous DB and attachments are preserved as `<path>.bak`
for manual recovery.

### Cloud Backup Settings

```http
GET /api/settings/backup
```

Returns the effective cloud-backup configuration — env defaults with any
UI-saved overrides applied. The secret key is **never** returned; instead a
`secret_key_set` boolean indicates whether one is stored.

**Response `200`:**
```json
{
  "configured": true,
  "endpoint": "https://<account>.r2.cloudflarestorage.com",
  "bucket": "my-tack-backups",
  "region": "auto",
  "access_key": "AKIA…",
  "secret_key_set": true,
  "prefix": "tack",
  "retention": 10
}
```

```http
PUT /api/settings/backup
Content-Type: application/json

{
  "endpoint": "https://<account>.r2.cloudflarestorage.com",
  "bucket": "my-tack-backups",
  "region": "auto",
  "access_key": "AKIA…",
  "secret_key": "…",
  "prefix": "tack",
  "retention": 10
}
```

Saves the configuration to the `app_meta` table. Leave `secret_key` blank to keep
the currently stored secret (so the masked UI field can be submitted untouched).
Any blank string field clears that override and falls back to the environment
default. Returns the same masked shape as `GET`.

---

## Error Responses

### Standard Error Format

```json
{
  "error": "Item not found",
  "details": "No item exists with id: 660e8400-e29b-41d4-a716-446655440001"
}
```

### HTTP Status Codes

- `200 OK` - Successful GET/PATCH
- `201 Created` - Successful POST
- `204 No Content` - Successful DELETE
- `400 Bad Request` - Invalid input, validation error
- `404 Not Found` - Resource not found
- `409 Conflict` - Workflow validation error, WIP limit exceeded
- `500 Internal Server Error` - Server error

### Common Errors

**Workflow Validation Error (409 Conflict)**
```json
{
  "error": "Invalid status transition",
  "details": "Cannot move from 'Permit' to 'Handover' in construction workflow"
}
```

**WIP Limit Exceeded (409 Conflict)**
```json
{
  "error": "WIP limit exceeded",
  "details": "Column 'In Progress' has limit of 3, currently has 3 items"
}
```

**Dependency Cycle Detected (409 Conflict)**
```json
{
  "error": "Dependency cycle detected",
  "details": "Adding this dependency would create a cycle: A → B → C → A"
}
```

---

## Rate Limiting

**Current Status:** No rate limiting implemented

For production deployments, consider adding rate limiting at the reverse proxy level (e.g., Caddy, nginx).

---

## CORS

**Current Status:** CORS enabled for all origins (development mode)

For production, configure `TACK_CORS_ORIGIN` environment variable to restrict origins.

---

## API Versioning

**Current Status:** No versioning (v1 implicit)

Future API changes will use `/api/v2` prefix for breaking changes.

---

## Health Check

```http
GET /api/health
```

**Response:**
```json
{
  "status": "healthy",
  "version": "1.0.0",
  "uptime_seconds": 86400
}
```

---

## Debug Endpoints (Development Only)

### System Info

```http
GET /api/debug/info
```

**Response:**
```json
{
  "version": "1.0.0",
  "database": "sqlite:tack.db",
  "log_level": "debug",
  "storage_dir": "./storage"
}
```

### Database Stats

```http
GET /api/debug/db-stats
```

**Response:**
```json
{
  "projects": 5,
  "items": 127,
  "sprints": 12,
  "attachments": 34,
  "database_size_mb": 15.2
}
```

---

## Examples

See [API-EXAMPLES.md](./API-EXAMPLES.md) for complete workflow examples with curl commands.
