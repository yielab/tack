# FlexPM - Flexible Project Management Tool
## Architecture & Implementation Roadmap

> A lightweight, versatile project management tool for solo developers and small teams.
> Supports Scrum, Kanban, mixed workflows, and fully customizable terminology.
> Handles everything from software sprints to construction builds to homework tracking.

---

## Phase 0: Core Principles & Constraints

- [ ] **Simplicity first** - Low learning curve, minimal UI clutter
- [ ] **Solo/small team focus** - No complex user management, no seat-based licensing logic
- [ ] **Workflow agnostic** - Scrum, Kanban, mixed, or fully custom workflows
- [ ] **Domain agnostic** - Software, web, mobile, construction, personal, homework, maintenance
- [ ] **Lightweight** - Fast startup, low resource usage, offline-capable
- [ ] **Customizable vocabulary** - Users rename "Epic" to "Phase", "Sprint" to "Cycle", etc.

---

## Phase 1: Data Model & Domain Design

### 1.1 Core Entities

- [ ] **Workspace** — Top-level container (one per installation or account)
  - name, description, default vocabulary map, default workflow template
- [ ] **Project** — A single project within a workspace
  - name, description, project type (preset or custom), vocabulary overrides, workflow config
  - project types: `software`, `web`, `mobile`, `construction`, `personal`, `homework`, `maintenance`, `custom`
- [ ] **Item** (the universal work unit) — Hierarchical and flexible
  - id, title, description (rich text / markdown)
  - type: configurable label (default: `epic > feature > task > subtask`)
  - status: mapped to workflow columns
  - priority: `critical | high | medium | low | none`
  - estimate: story points, hours, or custom unit
  - tags/labels: free-form
  - assignee: role/specialty (not necessarily a "user")
  - dates: created, updated, due, started, completed
  - parent_item_id (nullable — enables unlimited nesting)
  - sort_order (for manual ordering within columns/backlogs)
- [ ] **Dependency** — Relationships between items
  - type: `blocks | is_blocked_by | relates_to | duplicates | parent_of | child_of`
  - source_item_id, target_item_id
- [ ] **Role / Specialty** — Lightweight role tagging per item
  - name (e.g., "Frontend Dev", "Electrician", "Designer", "Student")
  - color, icon
  - assigned per item, NOT per user (since solo/small team)
- [ ] **Comment / Activity Log** — Per item
  - text (markdown), author, timestamp, type (`comment | status_change | edit`)
- [ ] **Attachment** — Files linked to items
  - filename, mime_type, storage_path, size, uploaded_at
- [ ] **Sprint / Iteration** (optional, Scrum mode)
  - name, goal, start_date, end_date, status (`planning | active | review | closed`)
  - linked items
- [ ] **Board View Config** — Saved view configurations
  - columns (mapped to statuses), filters, sort, grouping, WIP limits

### 1.2 Vocabulary System

- [ ] Design **vocabulary map** per project:
  ```
  default_vocabulary = {
    "epic": "Epic",
    "feature": "Feature",
    "task": "Task",
    "subtask": "Subtask",
    "sprint": "Sprint",
    "backlog": "Backlog",
    "board": "Board",
    "blocker": "Blocker",
    "story_points": "Story Points",
    "assignee": "Assignee",
    "requirement": "Requirement",
    "deliverable": "Deliverable"
  }
  ```
- [ ] Allow full override per project (e.g., construction project renames "Sprint" → "Phase", "Epic" → "Building", "Task" → "Work Order")
- [ ] Provide **preset vocabulary packs**:
  - `agile-software` (default agile terms)
  - `construction` (Phase, Work Order, Inspection, Permit, etc.)
  - `academic` (Assignment, Module, Deadline, Grade, etc.)
  - `personal` (Goal, Action, Habit, etc.)
  - `maintenance` (Ticket, Job, Schedule, Inspection, etc.)

### 1.3 Workflow Engine

- [ ] **Status pipeline** — Ordered list of statuses per project
  - Default: `Backlog → To Do → In Progress → In Review → Done`
  - Fully customizable: add, remove, rename, reorder statuses
- [ ] **Workflow templates**:
  - `scrum`: Backlog + Sprint Planning + Sprint Board + Review + Retro
  - `kanban`: Continuous flow board with WIP limits
  - `mixed`: Sprint-based with Kanban board (Scrumban)
  - `simple`: To Do → Doing → Done (for personal/homework)
  - `construction`: Permit → Procurement → Build → Inspect → Handover
  - `custom`: Start from scratch
- [ ] **Transition rules** (optional, per workflow):
  - Allowed transitions (e.g., can't go from "Backlog" straight to "Done")
  - Auto-transitions (e.g., all subtasks done → parent moves to "Review")
- [ ] **WIP limits** — Per column, optional, with visual warning

### 1.4 Dependencies & Requirements

- [ ] Dependency graph (DAG — directed acyclic graph validation)
- [ ] Visual dependency lines on board view
- [ ] Blocked item highlighting
- [ ] **Requirements tracking** — Items can be tagged as requirements
  - Traceability: link requirements → implementation items → verification items
- [ ] **Critical path detection** (nice-to-have, Phase 3)

---

## Phase 2: Tech Stack Selection

### 2.1 Backend

- [ ] **Runtime**: Rust (with Axum framework)
  - Why: Extreme performance, low memory footprint, single binary deployment
  - Alternative considered: Go (Fiber), Node (Fastify)
- [ ] **API**: REST + optional GraphQL (async-graphql)
  - REST for simplicity, GraphQL for complex frontend queries
- [ ] **Database**: SQLite (via `rusqlite` or `sqlx`)
  - Why: Zero config, single file, perfect for solo/small teams, portable
  - Scales to millions of items easily
  - Enable WAL mode for concurrent read performance
- [ ] **Real-time**: WebSocket (tokio-tungstenite) for live board updates
- [ ] **Search**: SQLite FTS5 (full-text search built-in)
- [ ] **File storage**: Local filesystem with optional S3-compatible backend
- [ ] **Auth** (minimal):
  - Single-user mode: no auth needed (default for local)
  - Multi-user mode: simple token-based auth, max ~10 users
  - Optional: passkey/WebAuthn for security without passwords

### 2.2 Frontend

- [ ] **Framework**: SolidJS or Svelte 5
  - Why: Tiny bundle size, fast rendering, reactive without virtual DOM
  - Low learning curve for contributors
- [ ] **Styling**: TailwindCSS 4 + headless UI components
- [ ] **State management**: Built-in reactivity (signals/runes)
- [ ] **Drag & drop**: @dnd-kit equivalent or native HTML DnD
- [ ] **Offline support**: Service Worker + IndexedDB cache
- [ ] **Mobile**: Responsive web-first, then Tauri mobile or Capacitor wrapper
- [ ] **Desktop**: Tauri v2 (uses the Rust backend directly)
  - Single binary, ~5MB installed size
  - Native OS integration (system tray, notifications)

### 2.3 Deployment Options

- [ ] **Local desktop app** (Tauri) — Primary target
- [ ] **Self-hosted server** — Single Docker container or binary
- [ ] **CLI companion** — Terminal-based task management (`flexpm add task "Fix login bug"`)
- [ ] **Cloud hosted** (future) — Optional managed service

---

## Phase 3: Backend Implementation

### 3.1 Project Setup

- [ ] Initialize Rust workspace with Cargo
- [ ] Set up Axum HTTP server with graceful shutdown
- [ ] Configure SQLite with migrations (sqlx or refinery)
- [ ] Set up structured logging (tracing crate)
- [ ] Error handling strategy (thiserror + anyhow)
- [ ] Configuration management (TOML config file + env vars)

### 3.2 Database Schema & Migrations

- [ ] Migration 001: workspaces table
- [ ] Migration 002: projects table (with vocabulary JSON column, workflow JSON column)
- [ ] Migration 003: items table (with parent_item_id self-reference, type, status, priority)
- [ ] Migration 004: dependencies table (source_id, target_id, dependency_type)
- [ ] Migration 005: roles table + item_roles junction
- [ ] Migration 006: comments table
- [ ] Migration 007: attachments table
- [ ] Migration 008: sprints table + sprint_items junction
- [ ] Migration 009: board_views table (saved view configurations)
- [ ] Migration 010: FTS5 virtual table for full-text search on items
- [ ] Seed data: default vocabulary packs, workflow templates

### 3.3 API Endpoints

- [ ] **Projects**
  - `POST /api/projects` — Create project (with type, vocabulary, workflow)
  - `GET /api/projects` — List projects
  - `GET /api/projects/:id` — Get project details
  - `PATCH /api/projects/:id` — Update project settings/vocabulary/workflow
  - `DELETE /api/projects/:id` — Archive/delete project
- [ ] **Items**
  - `POST /api/projects/:id/items` — Create item (task, epic, etc.)
  - `GET /api/projects/:id/items` — List items (with filters, pagination, sorting)
  - `GET /api/projects/:id/items/tree` — Get hierarchical item tree
  - `GET /api/items/:id` — Get single item with children & dependencies
  - `PATCH /api/items/:id` — Update item (status, fields, etc.)
  - `DELETE /api/items/:id` — Delete item
  - `PATCH /api/items/:id/move` — Reorder / move between columns
  - `POST /api/items/:id/dependencies` — Add dependency
  - `DELETE /api/items/:id/dependencies/:dep_id` — Remove dependency
- [ ] **Board**
  - `GET /api/projects/:id/board` — Get board state (columns + items)
  - `PATCH /api/projects/:id/board` — Update board config (columns, WIP limits)
  - `WS /api/projects/:id/board/live` — WebSocket for real-time updates
- [ ] **Sprints** (when using Scrum/mixed workflow)
  - `POST /api/projects/:id/sprints` — Create sprint
  - `GET /api/projects/:id/sprints` — List sprints
  - `PATCH /api/sprints/:id` — Update sprint (start, close, etc.)
  - `POST /api/sprints/:id/items` — Add items to sprint
- [ ] **Roles**
  - `GET /api/projects/:id/roles` — List roles for project
  - `POST /api/projects/:id/roles` — Create role
  - `PATCH /api/items/:id/roles` — Assign/remove roles on item
- [ ] **Search**
  - `GET /api/projects/:id/search?q=` — Full-text search within project
  - `GET /api/search?q=` — Global search across projects
- [ ] **Comments**
  - `POST /api/items/:id/comments` — Add comment
  - `GET /api/items/:id/comments` — List comments
- [ ] **Attachments**
  - `POST /api/items/:id/attachments` — Upload file
  - `GET /api/attachments/:id` — Download file
- [ ] **Export/Import**
  - `GET /api/projects/:id/export` — Export project (JSON/CSV)
  - `POST /api/projects/import` — Import from JSON/CSV/Jira/Trello

### 3.4 Business Logic Services

- [ ] **Workflow engine service** — Validate transitions, enforce WIP limits
- [ ] **Dependency resolver** — DAG validation, cycle detection, blocked-item computation
- [ ] **Auto-status propagation** — Parent status updates when children change
- [ ] **Vocabulary resolver** — Map internal keys to display labels per project
- [ ] **Template service** — Apply project templates with preset items
- [ ] **Notification service** — Due date alerts, blocked item alerts (local notifications)

---

## Phase 4: Frontend Implementation

### 4.1 Core Layout & Navigation

- [ ] App shell with responsive sidebar
- [ ] Project switcher
- [ ] Global search bar
- [ ] Command palette (Ctrl+K) for quick actions
- [ ] Dark/light theme toggle
- [ ] Breadcrumb navigation

### 4.2 Views

- [ ] **Board view** (Kanban) — Drag-and-drop columns
  - Column headers with WIP count/limit
  - Item cards (title, priority badge, role tags, dependency indicator)
  - Quick-add item inline
  - Swimlanes (group by: role, priority, epic, or none)
- [ ] **List view** — Flat or hierarchical sortable table
  - Inline editing
  - Bulk actions (move, assign role, change priority)
  - Column customization
- [ ] **Timeline view** (Gantt-style)
  - Date-based bars with dependency arrows
  - Drag to adjust dates
  - Critical path highlight
- [ ] **Calendar view** — Due dates on calendar
- [ ] **Sprint view** (Scrum mode)
  - Sprint backlog vs product backlog
  - Sprint planning drag-and-drop
  - Burndown chart (simple)
  - Sprint review/retrospective notes
- [ ] **Dashboard view** — Project overview
  - Progress summary (items by status)
  - Velocity chart (if using sprints)
  - Upcoming deadlines
  - Blocked items alert

### 4.3 Item Detail Panel

- [ ] Slide-over or modal for item detail
- [ ] Rich text / markdown editor for description
- [ ] Subtask checklist (inline create)
- [ ] Dependency viewer (what blocks this / what this blocks)
- [ ] Role assignment (multi-select)
- [ ] Comment thread
- [ ] Activity history
- [ ] File attachments (drag-and-drop upload)
- [ ] Custom fields display

### 4.4 Project Settings UI

- [ ] Workflow editor — Visual column/status manager
- [ ] Vocabulary editor — Rename all terms
- [ ] Role manager — Create/edit roles with colors/icons
- [ ] Template selector (on project creation)
- [ ] Import/export buttons

### 4.5 Mobile-Specific Optimizations

- [ ] Bottom tab navigation
- [ ] Swipe gestures for status transitions
- [ ] Compact card view for board
- [ ] Pull-to-refresh
- [ ] Offline indicator + sync status

---

## Phase 5: CLI Companion

- [ ] `flexpm init` — Initialize a project in current directory
- [ ] `flexpm add <type> "<title>"` — Quick add item
- [ ] `flexpm list` — Show items (with filters)
- [ ] `flexpm move <id> <status>` — Change item status
- [ ] `flexpm board` — ASCII board view in terminal
- [ ] `flexpm sprint start|close` — Sprint management
- [ ] `flexpm sync` — Sync with desktop/server instance
- [ ] Integration with git hooks (auto-link commits to items)

---

## Phase 6: Project Templates Library

### 6.1 Built-in Templates

- [ ] **Software Development** (Scrum)
  - Epics → Features → Tasks → Subtasks
  - Sprints, backlog grooming, code review status
  - Roles: Backend Dev, Frontend Dev, QA, DevOps, PM
- [ ] **Web Development** (Kanban)
  - Design → Develop → Test → Deploy
  - Roles: Designer, Developer, Content Writer, SEO
- [ ] **Mobile App** (Mixed)
  - Platform-specific epics (iOS, Android, Shared)
  - Roles: iOS Dev, Android Dev, UI/UX, QA
- [ ] **Construction / Building** (Phase-based)
  - Phases: Permits → Design → Foundation → Structure → MEP → Finishing → Inspection
  - Roles: Architect, Engineer, Foreman, Electrician, Plumber, Inspector
  - Vocabulary: Work Order, Phase, Inspection, Permit, Material, Blueprint
- [ ] **Homework / Academic** (Simple Kanban)
  - Subjects as categories
  - Roles: Student, Study Group
  - Vocabulary: Assignment, Module, Deadline, Grade
  - Calendar-centric view default
- [ ] **Personal Projects** (Simple)
  - Goals → Actions → Habits
  - Minimal columns: To Do → Doing → Done
  - No sprints, no roles
- [ ] **Maintenance** (Ticket-based)
  - Ticket → Diagnose → Fix → Verify → Close
  - Roles: Technician, Supervisor
  - Recurring items support
  - Vocabulary: Ticket, Job, Scheduled Maintenance

### 6.2 Custom Template Creation

- [ ] Save current project config as template
- [ ] Export/import templates as JSON
- [ ] Community template sharing (future)

---

## Phase 7: Advanced Features (Post-MVP)

- [ ] **Recurring items** — Auto-create items on schedule (maintenance, habits)
- [ ] **Time tracking** — Optional stopwatch per item
- [ ] **Custom fields** — User-defined fields per project (text, number, date, dropdown)
- [ ] **Automations** — Simple if/then rules (e.g., "when status = Done, notify")
- [ ] **Multiple boards per project** — Different views for different contexts
- [ ] **Cross-project dependencies** — Link items across projects
- [ ] **Reporting** — Velocity, cycle time, lead time, cumulative flow
- [ ] **AI assistance** — Smart task breakdown, priority suggestions, blocker detection
- [ ] **Plugin system** — Extend with custom integrations
- [ ] **Import from**:
  - Jira (JSON export)
  - Trello (JSON export)
  - Asana (CSV)
  - GitHub Issues (API)
  - Todoist (API)
- [ ] **Backup & restore** — Automated SQLite backups
- [ ] **Multi-language UI** — i18n support

---

## Phase 8: Testing & Quality

- [ ] Unit tests for workflow engine & dependency resolver
- [ ] Integration tests for all API endpoints
- [ ] E2E tests for critical user flows (create project → add items → move through board)
- [ ] Performance benchmarks (10K+ items per project)
- [ ] Accessibility audit (WCAG 2.1 AA)
- [ ] Mobile responsiveness testing
- [ ] Offline/sync reliability testing
- [ ] SQLite stress testing (concurrent reads/writes)

---

## Phase 9: Documentation & Launch

- [ ] User guide (getting started, workflows, templates)
- [ ] API reference (auto-generated from OpenAPI spec)
- [ ] CLI reference
- [ ] Video walkthrough (2-3 min)
- [ ] Contributing guide
- [ ] License selection (MIT or Apache 2.0)
- [ ] Landing page
- [ ] GitHub repository setup with CI/CD

---

## Architecture Summary

```
┌─────────────────────────────────────────────────────────┐
│                    FlexPM Architecture                   │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌──────────┐  │
│  │ Desktop │  │ Mobile  │  │   Web   │  │   CLI    │  │
│  │ (Tauri) │  │ (Tauri  │  │ (SPA)   │  │ (Rust)   │  │
│  │         │  │  Mobile │  │         │  │          │  │
│  │ SolidJS │  │  /PWA)  │  │ SolidJS │  │ Terminal │  │
│  └────┬────┘  └────┬────┘  └────┬────┘  └─────┬────┘  │
│       │            │            │              │        │
│       └────────────┴─────┬──────┴──────────────┘        │
│                          │                              │
│                   ┌──────┴──────┐                       │
│                   │  REST API   │                       │
│                   │  WebSocket  │                       │
│                   │   (Axum)    │                       │
│                   └──────┬──────┘                       │
│                          │                              │
│          ┌───────────────┼───────────────┐              │
│          │               │               │              │
│   ┌──────┴──────┐ ┌──────┴─────┐ ┌──────┴──────┐      │
│   │  Workflow   │ │ Dependency │ │  Vocabulary  │      │
│   │  Engine     │ │  Resolver  │ │  Resolver    │      │
│   └─────────────┘ └────────────┘ └─────────────┘      │
│          │               │               │              │
│          └───────────────┼───────────────┘              │
│                          │                              │
│                   ┌──────┴──────┐                       │
│                   │   SQLite    │                       │
│                   │  (WAL mode) │                       │
│                   │  + FTS5     │                       │
│                   └─────────────┘                       │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

---

## Key Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Backend language | Rust | Performance, single binary, low memory |
| Database | SQLite | Zero-config, portable, perfect for small teams |
| Frontend framework | SolidJS/Svelte 5 | Tiny bundle, fast, simple reactivity |
| Desktop wrapper | Tauri v2 | Native feel, tiny footprint (~5MB) |
| API style | REST + WebSocket | Simple + real-time updates |
| Auth model | Optional/minimal | Solo-first, no complexity by default |
| Work item model | Single "Item" entity with types | Flexible hierarchy, no rigid Epic/Story/Task |
| Vocabulary | Per-project override map | Same engine, different domain language |
| Workflow | Per-project status pipeline | One system fits all methodologies |

---

## Non-Goals (Explicitly Out of Scope)

- Enterprise SSO / SAML / LDAP
- Complex permission matrices
- Multi-tenant SaaS (initially)
- Real-time collaborative editing (Google Docs-style)
- Built-in chat / messaging
- Billing / invoicing
- Resource capacity planning
- Portfolio management across organizations

---

*This document serves as the north star for FlexPM development.
Start with Phase 1-3 (data model + backend), then Phase 4 (frontend), iterate.*
