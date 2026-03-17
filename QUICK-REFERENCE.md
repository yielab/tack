# FlexPM Quick Reference Guide

**Version:** 1.0 | **Last Updated:** 2026-03-16

---

## 🚀 Quick Start (30 seconds)

```bash
# Clone and start
git clone <repo-url> flexpm
cd flexpm
docker compose up -d

# Access
# Backend:  http://localhost:3210
# Frontend: http://localhost:8080
```

---

## ⌨️ Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+K` / `Cmd+K` | Open command palette |
| `Ctrl+/` / `Cmd+/` | Open global search |
| `Esc` | Close modals/palettes |
| `R` | Refresh board (in command palette) |
| `↑` `↓` | Navigate lists |
| `Enter` | Select/Submit |

---

## 🗂️ Project Types & Workflows

| Type | Best For | Default Workflow |
|------|----------|------------------|
| **Software** | Web/app development | Scrum (Backlog → Sprint → In Progress → Review → Done) |
| **Construction** | Building projects | Linear phases (Permit → Procurement → Build → Inspect → Handover) |
| **Personal** | Life tasks | Simple (Todo → In Progress → Done) |
| **Homework** | Student assignments | Assignment flow (Not Started → Research → Writing → Review → Submitted) |
| **Maintenance** | Repair work | Ticket system (Reported → Triaged → Scheduled → In Progress → Done) |

---

## 📋 Views

FlexPM includes 6 views for different project management needs:

### Board View (Kanban)
- **URL:** `/projects/:id/board`
- **Best for:** Visual workflow, drag-and-drop
- **Features:**
  - Drag items between columns
  - WIP limits per column
  - Real-time updates
  - Add items to specific columns
  - Color-coded priorities

### List View (Table)
- **URL:** `/projects/:id/list`
- **Best for:** Bulk operations, filtering, sorting
- **Features:**
  - Sort by any column (click header)
  - Filter by status/priority/type
  - Multi-select with checkboxes
  - Bulk status change
  - Bulk delete
  - Search across title/description/tags

### Dashboard View
- **URL:** `/projects/:id/dashboard`
- **Best for:** Project overview, analytics
- **Features:**
  - Total/completed items, completion rate
  - Status distribution chart
  - Priority distribution chart
  - Type distribution chart
  - Story points progress

### Sprint View
- **URL:** `/projects/:id/sprints`
- **Best for:** Scrum workflow, sprint planning
- **Features:**
  - Create/edit/delete sprints
  - Sprint lifecycle (planning → active → review → closed)
  - Progress tracking (items, story points)
  - Backlog management
  - Sprint items preview

### Calendar View
- **URL:** `/projects/:id/calendar`
- **Best for:** Due date tracking, deadlines
- **Features:**
  - Month-based calendar grid
  - Items on due dates
  - Color-coded by priority
  - Today highlighted
  - Items without due dates section

### Timeline View (Gantt)
- **URL:** `/projects/:id/timeline`
- **Best for:** Date ranges, project timeline
- **Features:**
  - Gantt-style horizontal bars
  - Three view modes (week/month/quarter)
  - Month markers and today line
  - Color-coded by priority
  - Date range visualization

**Switch views:** Navigation buttons in header or `Ctrl+K` → "Switch to [View Name]"

---

## 🎨 Item Priorities

| Priority | Color | Use For |
|----------|-------|---------|
| **Critical** | Red | Production down, blocking issues |
| **High** | Orange | Important features, urgent fixes |
| **Medium** | Yellow | Regular work items |
| **Low** | Green | Nice-to-haves, polish |

---

## 🏷️ Item Types

- **Epic** - Large feature or theme (contains multiple features)
- **Feature** - User-facing functionality (contains tasks)
- **Task** - Single unit of work
- **Subtask** - Breakdown of a task
- **Bug** - Defect or error
- **Story** - User story (Agile)
- **Chore** - Maintenance work

**Custom Types:** Add via API or database

---

## 🔄 Workflow Transitions

### Scrum/Kanban (Flexible)
- Any status → Any status (no restrictions)

### Construction (Linear)
- Permit → Procurement → Build → Inspect → Handover
- Cannot skip phases (enforced)

### Custom Workflows
- Define in `project.workflow.transitions`
- Leave empty for unrestricted movement

---

## 🌐 Real-Time Collaboration

**WebSocket Events:**
- Item created/updated/deleted
- Board config changed
- Sprint updated
- Auto-reconnect on disconnect

**Visual Indicators:**
- 🟢 Green dot = Connected
- 🟡 Yellow dot = Reconnecting
- 🔴 Red dot = Disconnected

---

## 🔍 Search & Filtering

### Global Search (`Ctrl+/`)
- Searches across ALL projects
- Includes: titles, descriptions, tags
- Click result to jump to item's board

### List View Filters
- **Search:** Real-time filter by text
- **Status:** Dropdown filter
- **Priority:** Dropdown filter
- **Type:** Dropdown filter
- **Clear All:** Reset all filters

### Advanced Search (via API)
```bash
curl "http://localhost:3210/api/search?q=authentication"
curl "http://localhost:3210/api/projects/PROJECT_ID/search?q=bug"
```

---

## 📦 Bulk Operations (List View)

1. Select items with checkboxes
2. Choose action:
   - **Change Status:** Dropdown menu
   - **Delete:** Red delete button
3. Confirm if needed

**Tip:** Use filters first to narrow down selection

---

## 🎯 Sprints & Planning

### Create Sprint
1. Go to project
2. Click "Sprints" (future UI)
3. Set name, start date, end date
4. Choose status (Planning/Active/Review/Closed)

### Assign Items to Sprint
- Edit item → "Sprint" dropdown
- Items can only be in active or planning sprints

### Sprint Lifecycle
```
Planning → Active → Review → Closed
```

---

## 📎 Attachments

**Upload:**
- Via API: `POST /api/items/{id}/attachments`
- Max size: 50MB per file
- Stored in `./storage/items/{item_id}/`

**Download:**
- `GET /api/attachments/{attachment_id}`

**Supported:** Any file type

---

## 📊 Export & Backup

### Export Project (JSON)
```bash
curl "http://localhost:3210/api/projects/PROJECT_ID/export?format=json" > backup.json
```

### Export Project (CSV)
```bash
curl "http://localhost:3210/api/projects/PROJECT_ID/export?format=csv" > items.csv
```

### Full Database Backup
```bash
docker compose exec flexpm cat /data/flexpm.db > flexpm-backup.db
```

---

## 🔧 Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `FLEXPM_HOST` | `127.0.0.1` | Server bind address |
| `FLEXPM_PORT` | `3210` | Server port |
| `FLEXPM_DATABASE_URL` | `sqlite:flexpm.db?mode=rwc` | Database path |
| `FLEXPM_LOG_LEVEL` | `info` | Log verbosity |
| `FLEXPM_STORAGE_DIR` | `./storage` | File storage path |

### Config File (flexpm.toml)
```toml
[server]
host = "0.0.0.0"
port = 3210

[database]
url = "sqlite:flexpm.db?mode=rwc"

[logging]
level = "info"
json = false

[storage]
dir = "./storage"
max_file_size = 52428800  # 50MB
```

---

## 🩺 Health Checks

```bash
# Backend health
curl http://localhost:3210/api/health

# Frontend health
curl http://localhost:8080

# Database info
curl http://localhost:3210/api/debug/info

# Database stats
curl http://localhost:3210/api/debug/db-stats
```

---

## 🐛 Troubleshooting

### Frontend not loading
```bash
docker compose logs frontend
docker compose restart frontend
```

### Backend not responding
```bash
docker compose logs flexpm
curl http://localhost:3210/api/health
```

### Database errors
```bash
# Check database file
ls -lh flexpm.db

# View migrations
sqlite3 flexpm.db "SELECT * FROM _migrations"

# Reset database (DESTRUCTIVE)
docker compose down -v
docker compose up -d
```

### WebSocket not connecting
- Check firewall allows port 3210
- Verify backend is running: `curl http://localhost:3210/api/health`
- Check browser console for errors

---

## 📚 Documentation Index

| Document | Description |
|----------|-------------|
| [README.md](README.md) | Getting started, features, quick start |
| [CLAUDE.md](CLAUDE.md) | Developer guide, architecture, patterns |
| [docs/API-REFERENCE.md](docs/API-REFERENCE.md) | Complete API documentation (34 endpoints) |
| [docs/API-EXAMPLES.md](docs/API-EXAMPLES.md) | Example API workflows with curl |
| [docs/DEPLOYMENT-GUIDE.md](docs/DEPLOYMENT-GUIDE.md) | Production deployment instructions |
| [docs/FRONTEND-FEATURES.md](docs/FRONTEND-FEATURES.md) | Complete frontend feature list |
| [docs/KEYBOARD-SHORTCUTS.md](docs/KEYBOARD-SHORTCUTS.md) | All keyboard shortcuts |
| [docs/TESTING.md](docs/TESTING.md) | Testing guide (unit, integration, E2E) |
| [PROJECT-SUMMARY.md](PROJECT-SUMMARY.md) | Executive summary & metrics |
| [TODO-ARCHITECTURE.md](TODO-ARCHITECTURE.md) | Architecture roadmap & completion status |

---

## 🆘 Getting Help

1. **Check Documentation:** Start with [README.md](README.md)
2. **Search Issues:** Look for similar problems
3. **Check Logs:** `docker compose logs -f`
4. **API Errors:** Check response body for error details
5. **Browser Console:** F12 → Console tab for frontend errors

---

## 🎉 Quick Wins

**Power User Tips:**

1. **Keyboard-Driven Workflow:**
   - `Ctrl+K` → Type "create" → Enter → Fill form
   - Never touch mouse!

2. **Bulk Status Updates:**
   - Switch to List view
   - Filter by criteria
   - Select all → Change status
   - Process 100s of items in seconds

3. **Custom Workflows:**
   - Create project as "Custom" type
   - Define your own statuses via API
   - Add transitions for enforcement

4. **Search Shortcuts:**
   - `Ctrl+/` → Type → Click result
   - Fastest way to find anything

5. **Real-Time Collaboration:**
   - Open same board in multiple browsers
   - See updates instantly
   - Perfect for pair programming or remote teams

---

**Happy Project Managing! 🚀**

For support: See documentation above or check [GitHub Issues](https://github.com/user/flexpm/issues)
