# API Reference

**Base URL:** `http://127.0.0.1:3210/api`  
**WebSocket:** `ws://127.0.0.1:3210/api/projects/{id}/boards/live`

The full endpoint reference lives in [docs/API-REFERENCE.md](../../../API-REFERENCE.md). This
page summarizes the endpoint surface for quick orientation.

---

## Authentication

When `TACK_API_TOKEN` is set, all requests need:

```
Authorization: Bearer <token>
```

`GET /api/health` is always public. Without a token configured, no auth is required.

---

## Endpoints by Group

### Projects (5 endpoints)

| Method | Path | Description |
|---|---|---|
| `GET` | `/projects` | List all projects |
| `POST` | `/projects` | Create project |
| `GET` | `/projects/{id}` | Get project |
| `PATCH` | `/projects/{id}` | Update project (workflow, vocabulary, name, etc.) |
| `DELETE` | `/projects/{id}` | Delete project |

### Boards (4 endpoints + WebSocket)

| Method | Path | Description |
|---|---|---|
| `GET` | `/projects/{id}/boards` | List boards for project |
| `POST` | `/projects/{id}/boards` | Create board |
| `GET` | `/projects/{id}/boards/{board_id}` | Get board with items grouped by column |
| `PATCH` | `/projects/{id}/boards/{board_id}` | Update board config |
| `GET` | `/projects/{id}/boards/live` | **WebSocket** — real-time board events |

### Items (6 endpoints)

| Method | Path | Description |
|---|---|---|
| `GET` | `/projects/{id}/items` | List items (filters: status, priority, type, assignee, sprint) |
| `POST` | `/projects/{id}/items` | Create item |
| `GET` | `/items/{id}` | Get item |
| `PATCH` | `/items/{id}` | Update item (status change validated by workflow) |
| `DELETE` | `/items/{id}` | Delete item |
| `GET` | `/items/{id}/history` | Item activity log |

### Dependencies (3 endpoints)

| Method | Path | Description |
|---|---|---|
| `GET` | `/items/{id}/dependencies` | List dependencies |
| `POST` | `/items/{id}/dependencies` | Add dependency (cycle detection applied) |
| `DELETE` | `/items/{id}/dependencies/{dep_id}` | Remove dependency |

### Attachments (4 endpoints)

| Method | Path | Description |
|---|---|---|
| `GET` | `/items/{id}/attachments` | List attachments |
| `POST` | `/items/{id}/attachments` | Upload file (multipart/form-data, max 50 MB) |
| `GET` | `/attachments/{id}` | Download attachment |
| `DELETE` | `/attachments/{id}` | Delete attachment |

### Sprints (4 endpoints)

| Method | Path | Description |
|---|---|---|
| `GET` | `/projects/{id}/sprints` | List sprints |
| `POST` | `/projects/{id}/sprints` | Create sprint |
| `PATCH` | `/sprints/{id}` | Update sprint (advance lifecycle state) |
| `DELETE` | `/sprints/{id}` | Delete sprint |

### Roles (5 endpoints)

| Method | Path | Description |
|---|---|---|
| `GET` | `/projects/{id}/roles` | List roles |
| `POST` | `/projects/{id}/roles` | Create role |
| `GET` | `/roles/{id}` | Get role |
| `PATCH` | `/roles/{id}` | Update role |
| `DELETE` | `/roles/{id}` | Delete role |

### Comments (2 endpoints)

| Method | Path | Description |
|---|---|---|
| `GET` | `/items/{id}/comments` | List comments |
| `POST` | `/items/{id}/comments` | Add comment |

### Search (2 endpoints)

| Method | Path | Description |
|---|---|---|
| `GET` | `/projects/{id}/search?q=term` | FTS5 search within project |
| `GET` | `/search?q=term` | Global FTS5 search across all projects |

### Export / Import (3 endpoints)

| Method | Path | Description |
|---|---|---|
| `GET` | `/projects/{id}/export?format=json\|csv` | Export project |
| `POST` | `/projects/import` | Import project from JSON |
| `GET` | `/backup` | Download full database backup |
| `POST` | `/restore` | Stage a database restore |

### Templates (3 endpoints)

| Method | Path | Description |
|---|---|---|
| `GET` | `/templates` | List project templates |
| `POST` | `/templates` | Save project as template |
| `POST` | `/templates/{id}/create` | Create project from template |

### Custom Fields (4 endpoints)

| Method | Path | Description |
|---|---|---|
| `GET` | `/projects/{id}/fields` | List custom field definitions |
| `POST` | `/projects/{id}/fields` | Create custom field (9 types) |
| `PATCH` | `/fields/{id}` | Update field definition |
| `DELETE` | `/fields/{id}` | Delete field |

### Debug / Health (3 endpoints)

| Method | Path | Description |
|---|---|---|
| `GET` | `/health` | `{"status":"ok","version":"…","migrations_applied":N}` |
| `GET` | `/debug/info` | Build info, config summary |
| `GET` | `/debug/db-stats` | Table row counts |

### Integrations & operations

Import, export, backup, settings, and the Alexa endpoint round out the surface to
**68 REST endpoints + 1 WebSocket**. They are documented with full request/response
schemas in the canonical [API Reference](../../../API-REFERENCE.md):

| Group | Paths |
|---|---|
| Import | `POST /projects/{id}/import-github`, `POST /projects/{id}/import-linear`, `POST /projects/import` |
| Export | `GET /projects/{id}/export?format=json\|yaml\|csv` |
| Backup | `GET /backup`, `POST /restore`, `POST/GET /backup/remote`, `POST /backup/remote/restore` |
| Settings | `GET/PUT /settings/backup` |
| Voice | `POST /alexa` |

---

## WebSocket Events

Connect to `ws://127.0.0.1:3210/api/projects/{id}/boards/live` with a standard WebSocket client. Events are JSON objects:

| Type | Payload |
|---|---|
| `ItemCreated` | Full item object |
| `ItemUpdated` | Full item object |
| `ItemDeleted` | `{"id":"…"}` |
| `BoardConfigUpdated` | Updated board config |
| `SprintUpdated` | Full sprint object |
| `Ping` | `{}` — keepalive, sent periodically |

---

## Error Responses

All errors return JSON:

```json
{"error": "Item not found"}
{"error": "WIP limit exceeded for column 'In Progress'"}
{"error": "Transition from 'Permit' to 'Handover' is not allowed"}
```

| Status | Cause |
|---|---|
| `400` | Bad request (validation failure) |
| `401` | Missing or invalid API token |
| `404` | Resource not found |
| `409` | Conflict (e.g., dependency cycle detected) |
| `422` | Workflow transition rejected |
| `500` | Internal server error |

---

## Pagination and Filtering

`GET /projects/{id}/items` supports:

| Param | Type | Description |
|---|---|---|
| `status` | string | Filter by column name |
| `priority` | string | `high`, `medium`, `low` |
| `item_type` | string | `task`, `epic`, `bug`, etc. |
| `assignee` | string | Assignee string match |
| `sprint_id` | UUID | Items in a specific sprint |
| `parent_id` | UUID | Children of a specific item |
| `limit` | int | Page size (default 50) |
| `offset` | int | Pagination offset |
