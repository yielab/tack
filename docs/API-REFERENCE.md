# FlexPM API Reference

**Version:** 1.0
**Base URL:** `http://localhost:3210/api`
**WebSocket URL:** `ws://localhost:3210/api/projects/{id}/board/live`

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
12. [WebSocket Events](#websocket-events)
13. [Error Responses](#error-responses)

---

## Authentication

**Current Status:** No authentication required (solo/small team focus).

Future versions may add optional authentication for multi-workspace deployments.

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
const ws = new WebSocket('ws://localhost:3210/api/projects/{id}/board/live');

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
const ws = new WebSocket(`ws://localhost:3210/api/projects/${projectId}/board/live`);

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

For production, configure `FLEXPM_CORS_ORIGIN` environment variable to restrict origins.

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
  "database": "sqlite:flexpm.db",
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
