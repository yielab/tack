# FlexPM API Reference

**Base URL:** `http://localhost:3210/api`

All request/response bodies use JSON. Set `Content-Type: application/json` for POST/PATCH/PUT requests.

---

## Health & Debug

### `GET /health`

Liveness check. Always returns 200.

**Response:**
```json
{"status": "ok", "service": "flexpm", "version": "0.1.0"}
```

### `GET /debug/info`

Server and database information.

**Response:**
```json
{
  "version": "0.1.0",
  "build": "debug",
  "database": {"size_bytes": 45056, "url": "sqlite:flexpm.db?mode=rwc"},
  "config": {"host": "127.0.0.1", "port": 3210, "log_level": "info"}
}
```

### `GET /debug/db-stats`

Row counts for all tables.

**Response:**
```json
{
  "tables": {
    "projects": 3,
    "items": 42,
    "sprints": 2,
    "roles": 5,
    "comments": 12,
    "dependencies": 4,
    "attachments": 0
  }
}
```

---

## Projects

### `POST /projects`

Create a new project.

**Body:**
| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | yes | Project name |
| `description` | string | no | Project description |
| `project_type` | string | yes | One of: `software`, `web`, `mobile`, `construction`, `personal`, `homework`, `maintenance`, `custom` |
| `template` | string | no | Template name (reserved for future use) |

**Example:**
```json
{
  "name": "Kitchen Reno",
  "description": "Full kitchen renovation project",
  "project_type": "construction"
}
```

**Response:** `200` — Full project object with auto-generated vocabulary and workflow.

### `GET /projects`

List all active (non-archived) projects.

**Response:** `200` — Array of project objects.

### `GET /projects/{id}`

Get a single project by UUID.

**Response:** `200` or `404`

### `PATCH /projects/{id}`

Update project settings.

**Body (all fields optional):**
| Field | Type | Description |
|-------|------|-------------|
| `name` | string | New project name |
| `description` | string | New description |
| `vocabulary` | object | Vocabulary override map (see below) |
| `workflow` | object | Workflow config override (see below) |
| `archived` | boolean | Archive/unarchive the project |

**Vocabulary object:** Key-value map where keys are vocabulary keys and values are display labels.
```json
{
  "vocabulary": {
    "task": "Work Item",
    "sprint": "Iteration",
    "epic": "Theme"
  }
}
```

Valid vocabulary keys: `epic`, `feature`, `task`, `subtask`, `bug`, `requirement`, `sprint`, `backlog`, `board`, `blocker`, `story_points`, `assignee`, `deliverable`, `phase`, `milestone`, `release`.

**Workflow object:**
```json
{
  "workflow": {
    "workflow_type": "custom",
    "statuses": [
      {"name": "Ideas", "category": "todo", "wip_limit": null, "order": 0},
      {"name": "Working", "category": "in_progress", "wip_limit": 3, "order": 1},
      {"name": "Shipped", "category": "done", "wip_limit": null, "order": 2}
    ],
    "transitions": null
  }
}
```

Status categories: `todo`, `in_progress`, `done`.
Set `transitions` to `null` for unrestricted movement, or provide an array:
```json
"transitions": [
  {"from": "Ideas", "to": "Working"},
  {"from": "Working", "to": "Shipped"},
  {"from": "Working", "to": "Ideas"}
]
```

### `DELETE /projects/{id}`

Delete a project and all its items, sprints, roles, etc. (cascading delete).

**Response:** `200` `{"deleted": true}` or `404`

---

## Items

Items are the universal work unit. They can be epics, features, tasks, subtasks, bugs, requirements, or custom types. Items support unlimited nesting via `parent_id`.

### `POST /projects/{project_id}/items`

Create a new item. It automatically receives the first status in the project's workflow.

**Body:**
| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `title` | string | yes | | Item title |
| `description` | string | no | `null` | Markdown description |
| `item_type` | string | no | `"task"` | `epic`, `feature`, `task`, `subtask`, `bug`, `requirement`, or custom |
| `parent_id` | uuid | no | `null` | Parent item (for nesting) |
| `priority` | string | no | `"medium"` | `critical`, `high`, `medium`, `low`, `none` |
| `estimate` | number | no | `null` | Numeric estimate |
| `estimate_unit` | string | no | `"story_points"` | `story_points`, `hours`, `days` |
| `tags` | string[] | no | `[]` | Free-form tags |
| `due_date` | string | no | `null` | ISO 8601 datetime |
| `sprint_id` | uuid | no | `null` | Assign to a sprint |

**Example:**
```json
{
  "title": "Implement login page",
  "description": "Build the login form with email/password authentication",
  "item_type": "task",
  "parent_id": "550e8400-e29b-41d4-a716-446655440000",
  "priority": "high",
  "estimate": 8,
  "estimate_unit": "story_points",
  "tags": ["frontend", "auth"],
  "due_date": "2026-04-01T00:00:00Z"
}
```

**Response:** `200` — Created item object.

### `GET /projects/{project_id}/items`

List items with optional filters and pagination.

**Query Parameters:**
| Param | Type | Description |
|-------|------|-------------|
| `status` | string | Filter by status name |
| `item_type` | string | Filter by type |
| `priority` | string | Filter by priority |
| `sprint_id` | uuid | Filter by sprint |
| `parent_id` | uuid | Filter by parent |
| `page` | integer | Page number (default: 1) |
| `per_page` | integer | Items per page (default: 100, max: 500) |

**Example:**
```
GET /api/projects/{id}/items?status=In%20Progress&priority=high&page=1&per_page=25
```

**Response:** `200` — Array of item objects, sorted by `sort_order`.

### `GET /projects/{project_id}/items/tree`

Get all items in hierarchical order (root items first, then children).

**Response:** `200` — Array of all items. Root items (`parent_id: null`) come first.

### `GET /items/{id}`

Get a single item with its roles and dependencies.

**Response:**
```json
{
  "item": { ... },
  "roles": [ ... ],
  "dependencies": [ ... ]
}
```

### `PATCH /items/{id}`

Update an item. Status changes are validated against the workflow.

**Body (all fields optional):**
| Field | Type | Description |
|-------|------|-------------|
| `title` | string | New title |
| `description` | string | New description |
| `item_type` | string | Change type |
| `status` | string | Move to new status (validated) |
| `priority` | string | Change priority |
| `estimate` | number | Update estimate |
| `estimate_unit` | string | Change unit |
| `tags` | string[] | Replace tags |
| `due_date` | string | Update due date |
| `sprint_id` | uuid | Assign to sprint |
| `sort_order` | integer | Manual sort position |

**Status change validation:**
- The source and target statuses must exist in the project's workflow
- If the workflow has explicit transitions, only allowed transitions work
- WIP limits are checked on the target column — returns `400` if exceeded

**Error example:**
```json
{
  "error": {
    "status": 400,
    "message": "WIP limit exceeded for column 'In Progress': limit is 5, current is 5"
  }
}
```

### `DELETE /items/{id}`

Delete an item. Children have their `parent_id` set to NULL (not deleted).

**Response:** `200` `{"deleted": true}` or `404`

### `GET /projects/{project_id}/search?q={query}`

Full-text search across item titles, descriptions, and tags using SQLite FTS5.

**Query Parameters:**
| Param | Type | Required | Description |
|-------|------|----------|-------------|
| `q` | string | yes | Search query (supports FTS5 syntax) |

**FTS5 query examples:**
- `login` — items containing "login"
- `login OR signup` — either term
- `"user authentication"` — exact phrase
- `auth*` — prefix match

**Response:** `200` — Array of matching items, ranked by relevance.

---

## Sprints

### `POST /projects/{project_id}/sprints`

Create a new sprint (starts in `planning` status).

**Body:**
| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | yes | Sprint name |
| `goal` | string | no | Sprint goal |
| `start_date` | string | no | ISO 8601 start date |
| `end_date` | string | no | ISO 8601 end date |

### `GET /projects/{project_id}/sprints`

List all sprints for a project, ordered by creation date (newest first).

### `GET /sprints/{id}`

Get a single sprint.

### `PATCH /sprints/{id}/status`

Update sprint status.

**Body:**
```json
{"status": "active"}
```

Valid statuses: `planning` -> `active` -> `review` -> `closed`

---

## Roles

Roles represent **specialties** or **types of work**, not user accounts.

### `POST /projects/{project_id}/roles`

**Body:**
| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `name` | string | yes | | Role name (e.g., "Frontend Dev") |
| `color` | string | no | `#6366f1` | Hex color |
| `icon` | string | no | `null` | Icon identifier |

### `GET /projects/{project_id}/roles`

List all roles for a project.

### `PUT /items/{item_id}/roles/{role_id}`

Assign a role to an item.

**Response:** `200` `{"assigned": true}`

### `DELETE /items/{item_id}/roles/{role_id}`

Remove a role from an item.

**Response:** `200` `{"removed": true}`

### `DELETE /roles/{id}`

Delete a role definition (also removes all assignments).

---

## Dependencies

### `POST /items/{item_id}/dependencies`

Create a dependency. The system validates against cycles (no circular dependencies).

**Body:**
| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `target_item_id` | uuid | yes | The other item |
| `dependency_type` | string | yes | `blocks`, `is_blocked_by`, `relates_to`, `duplicates` |

**Example:** "Item A blocks Item B"
```bash
POST /api/items/<A_id>/dependencies
{"target_item_id": "<B_id>", "dependency_type": "blocks"}
```

**Cycle detection:** If adding the dependency would create a circular chain
(A blocks B, B blocks C, C blocks A), the request returns `400`:
```json
{"error": {"status": 400, "message": "Dependency cycle detected involving item <uuid>"}}
```

### `GET /items/{item_id}/dependencies`

List all dependencies involving this item (both directions).

### `DELETE /items/{item_id}/dependencies/{dep_id}`

Remove a dependency.

---

## Comments

### `POST /items/{item_id}/comments`

**Body:**
| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `content` | string | yes | Comment text (markdown) |
| `author` | string | no | Author name |

### `GET /items/{item_id}/comments`

List all comments for an item, ordered chronologically.

---

## Error Responses

All errors follow this format:

```json
{
  "error": {
    "status": 400,
    "message": "Human-readable error description"
  }
}
```

| Status | Meaning |
|--------|---------|
| 400 | Bad request (validation, workflow violation, cycle) |
| 404 | Resource not found |
| 500 | Internal server error (details logged server-side) |
