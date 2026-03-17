# FlexPM API Examples

This document provides practical examples for using the FlexPM REST API.

**Base URL:** `http://localhost:3210/api` (or your deployment URL)

---

## Table of Contents

- [Projects](#projects)
- [Items](#items)
- [Board View](#board-view)
- [Sprints](#sprints)
- [Dependencies](#dependencies)
- [Roles](#roles)
- [Comments](#comments)
- [Attachments](#attachments)
- [Search](#search)
- [Export/Import](#exportimport)
- [WebSocket](#websocket)

---

## Projects

### Create a Project

```bash
curl -X POST http://localhost:3210/api/projects \
  -H "Content-Type: application/json" \
  -d '{
    "name": "My Web App",
    "description": "E-commerce platform",
    "template": "agile-software"
  }'
```

**Templates:** `agile-software`, `construction`, `homework`, `personal`, `maintenance`, `custom`

**Response:**
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000"
}
```

### List All Projects

```bash
curl http://localhost:3210/api/projects
```

### Get Project Details

```bash
curl http://localhost:3210/api/projects/{project_id}
```

### Update Project

```bash
curl -X PATCH http://localhost:3210/api/projects/{project_id} \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Updated Project Name",
    "description": "New description"
  }'
```

### Delete Project

```bash
curl -X DELETE http://localhost:3210/api/projects/{project_id}
```

---

## Items

### Create an Item

```bash
curl -X POST http://localhost:3210/api/projects/{project_id}/items \
  -H "Content-Type: application/json" \
  -d '{
    "title": "Implement user authentication",
    "description": "Add JWT-based authentication",
    "item_type": "feature",
    "priority": "high",
    "estimate": 8,
    "estimate_unit": "story_points",
    "tags": ["backend", "security"]
  }'
```

**Item Types:** `epic`, `feature`, `task`, `subtask`, `bug`, `requirement`, or `{"custom": "YourType"}`

**Priorities:** `critical`, `high`, `medium`, `low`, `none`

### List Items with Filters

```bash
# All items
curl http://localhost:3210/api/projects/{project_id}/items

# Filter by status
curl http://localhost:3210/api/projects/{project_id}/items?status=in_progress

# Filter by priority
curl http://localhost:3210/api/projects/{project_id}/items?priority=high

# Filter by type
curl http://localhost:3210/api/projects/{project_id}/items?item_type=bug

# Pagination
curl http://localhost:3210/api/projects/{project_id}/items?page=1&per_page=20
```

### Get Item Tree (Hierarchical)

```bash
curl http://localhost:3210/api/projects/{project_id}/items/tree
```

### Get Item Details

```bash
curl http://localhost:3210/api/items/{item_id}
```

Returns item with roles and dependencies.

### Update Item

```bash
# Update status (will validate workflow transitions)
curl -X PATCH http://localhost:3210/api/items/{item_id} \
  -H "Content-Type: application/json" \
  -d '{
    "status": "in_review"
  }'

# Update multiple fields
curl -X PATCH http://localhost:3210/api/items/{item_id} \
  -H "Content-Type: application/json" \
  -d '{
    "title": "Updated title",
    "priority": "critical",
    "estimate": 13,
    "tags": ["backend", "security", "urgent"]
  }'

# Move item to different column (reorder)
curl -X PATCH http://localhost:3210/api/items/{item_id} \
  -H "Content-Type: application/json" \
  -d '{
    "sort_order": 5
  }'

# Assign to sprint
curl -X PATCH http://localhost:3210/api/items/{item_id} \
  -H "Content-Type: application/json" \
  -d '{
    "sprint_id": "{sprint_id}"
  }'
```

### Delete Item

```bash
curl -X DELETE http://localhost:3210/api/items/{item_id}
```

---

## Board View

### Get Board State

```bash
curl http://localhost:3210/api/projects/{project_id}/board
```

**Response:**
```json
{
  "project_id": "...",
  "columns": [
    {
      "status": "To Do",
      "category": "backlog",
      "wip_limit": null,
      "order": 0,
      "items": [...],
      "item_count": 5,
      "wip_exceeded": false
    },
    {
      "status": "In Progress",
      "category": "in_progress",
      "wip_limit": 3,
      "order": 1,
      "items": [...],
      "item_count": 2,
      "wip_exceeded": false
    }
  ],
  "total_items": 15
}
```

### Update Board Configuration (WIP Limits)

```bash
curl -X PATCH http://localhost:3210/api/projects/{project_id}/board \
  -H "Content-Type: application/json" \
  -d '{
    "columns": [
      {
        "status": "In Progress",
        "wip_limit": 5
      },
      {
        "status": "In Review",
        "wip_limit": 3
      }
    ]
  }'
```

---

## Sprints

### Create Sprint

```bash
curl -X POST http://localhost:3210/api/projects/{project_id}/sprints \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Sprint 1",
    "goal": "Implement core authentication features",
    "start_date": "2026-03-15T00:00:00Z",
    "end_date": "2026-03-29T23:59:59Z"
  }'
```

### List Sprints

```bash
curl http://localhost:3210/api/projects/{project_id}/sprints
```

### Get Sprint Details

```bash
curl http://localhost:3210/api/sprints/{sprint_id}
```

### Update Sprint Status

```bash
curl -X PATCH http://localhost:3210/api/sprints/{sprint_id}/status \
  -H "Content-Type: application/json" \
  -d '{
    "status": "active"
  }'
```

**Sprint Statuses:** `planning`, `active`, `review`, `closed`

---

## Dependencies

### Add Dependency

```bash
curl -X POST http://localhost:3210/api/items/{item_id}/dependencies \
  -H "Content-Type: application/json" \
  -d '{
    "target_id": "{other_item_id}",
    "dependency_type": "blocks"
  }'
```

**Dependency Types:** `blocks`, `is_blocked_by`, `relates_to`, `duplicates`

Note: The system validates for circular dependencies using DAG.

### List Dependencies

```bash
curl http://localhost:3210/api/items/{item_id}/dependencies
```

### Delete Dependency

```bash
curl -X DELETE http://localhost:3210/api/items/{item_id}/dependencies/{dependency_id}
```

---

## Roles

### Create Role

```bash
curl -X POST http://localhost:3210/api/projects/{project_id}/roles \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Backend Developer",
    "color": "#3B82F6",
    "icon": "code"
  }'
```

### List Roles

```bash
curl http://localhost:3210/api/projects/{project_id}/roles
```

### Assign Role to Item

```bash
curl -X PUT http://localhost:3210/api/items/{item_id}/roles/{role_id}
```

### Remove Role from Item

```bash
curl -X DELETE http://localhost:3210/api/items/{item_id}/roles/{role_id}
```

### Delete Role

```bash
curl -X DELETE http://localhost:3210/api/roles/{role_id}
```

---

## Comments

### Add Comment

```bash
curl -X POST http://localhost:3210/api/items/{item_id}/comments \
  -H "Content-Type: application/json" \
  -d '{
    "text": "This looks good, but we need to add error handling",
    "author": "John Doe"
  }'
```

### List Comments

```bash
curl http://localhost:3210/api/items/{item_id}/comments
```

---

## Attachments

### Upload File

```bash
curl -X POST http://localhost:3210/api/items/{item_id}/attachments \
  -F "file=@/path/to/document.pdf"
```

**Max file size:** 50MB

### List Attachments

```bash
curl http://localhost:3210/api/items/{item_id}/attachments
```

### Download File

```bash
curl -O http://localhost:3210/api/attachments/{attachment_id}
```

### Delete Attachment

```bash
curl -X DELETE http://localhost:3210/api/attachments/{attachment_id}
```

---

## Search

### Search Within Project

```bash
curl "http://localhost:3210/api/projects/{project_id}/search?q=authentication"
```

### Global Search (All Projects)

```bash
curl "http://localhost:3210/api/search?q=bug+critical"
```

Uses SQLite FTS5 for full-text search across titles, descriptions, and tags.

---

## Export/Import

### Export Project (JSON)

```bash
curl "http://localhost:3210/api/projects/{project_id}/export?format=json" \
  -o project-export.json
```

### Export Project (CSV)

```bash
curl "http://localhost:3210/api/projects/{project_id}/export?format=csv" \
  -o project-export.csv
```

### Import Project

```bash
curl -X POST http://localhost:3210/api/projects/import \
  -H "Content-Type: application/json" \
  -d @project-export.json
```

---

## WebSocket

### Connect to Board Live Updates

```javascript
const ws = new WebSocket('ws://localhost:3210/api/projects/{project_id}/board/live');

ws.onmessage = (event) => {
  const boardEvent = JSON.parse(event.data);
  console.log('Received event:', boardEvent);

  switch (boardEvent.type) {
    case 'item_created':
      console.log('New item:', boardEvent.item_id, 'in', boardEvent.status);
      break;
    case 'item_updated':
      console.log('Item updated:', boardEvent.item_id);
      console.log('Status changed from', boardEvent.old_status, 'to', boardEvent.new_status);
      break;
    case 'item_deleted':
      console.log('Item deleted:', boardEvent.item_id);
      break;
    case 'board_config_updated':
      console.log('Board config changed');
      break;
    case 'ping':
      console.log('Keepalive ping');
      break;
  }
};

ws.onopen = () => console.log('WebSocket connected');
ws.onerror = (error) => console.error('WebSocket error:', error);
ws.onclose = () => console.log('WebSocket disconnected');
```

**Event Types:**
- `item_created` - New item added
- `item_updated` - Item modified (status, fields, etc.)
- `item_deleted` - Item removed
- `board_config_updated` - WIP limits or columns changed
- `sprint_updated` - Sprint status changed
- `ping` - Keepalive

---

## Complete Workflow Example

Here's a complete example of creating a project, adding items, and moving them through the board:

```bash
# 1. Create project
PROJECT_ID=$(curl -s -X POST http://localhost:3210/api/projects \
  -H "Content-Type: application/json" \
  -d '{"name": "My App", "template": "agile-software"}' | jq -r '.id')

# 2. Create an item
ITEM_ID=$(curl -s -X POST http://localhost:3210/api/projects/$PROJECT_ID/items \
  -H "Content-Type: application/json" \
  -d '{
    "title": "Build login page",
    "item_type": "task",
    "priority": "high"
  }' | jq -r '.id')

# 3. Move item to "In Progress"
curl -X PATCH http://localhost:3210/api/items/$ITEM_ID \
  -H "Content-Type: application/json" \
  -d '{"status": "In Progress"}'

# 4. Add a comment
curl -X POST http://localhost:3210/api/items/$ITEM_ID/comments \
  -H "Content-Type: application/json" \
  -d '{"text": "Working on this now", "author": "Developer"}'

# 5. Mark as done
curl -X PATCH http://localhost:3210/api/items/$ITEM_ID \
  -H "Content-Type: application/json" \
  -d '{"status": "Done"}'

# 6. View board state
curl http://localhost:3210/api/projects/$PROJECT_ID/board
```

---

## Error Handling

All endpoints return appropriate HTTP status codes:

- `200 OK` - Success
- `201 Created` - Resource created
- `400 Bad Request` - Invalid input or validation error
- `404 Not Found` - Resource doesn't exist
- `409 Conflict` - Circular dependency detected, WIP limit exceeded, etc.
- `500 Internal Server Error` - Server error

Error response format:
```json
{
  "error": "Detailed error message"
}
```

---

## Rate Limiting

Currently no rate limiting. For production deployments, consider adding rate limiting at the reverse proxy level (Caddy, Nginx, etc.).

---

## Authentication

Currently FlexPM is designed for single-user or small team use without authentication. For production use with multiple users, you can:

1. Add authentication middleware in Axum
2. Use JWT tokens
3. Configure authentication at the reverse proxy level

---

For more details, see:
- [Architecture Documentation](../TODO-ARCHITECTURE.md)
- [Development Guide](../CLAUDE.md)
- [API Source Code](../crates/flexpm-api/src/)
