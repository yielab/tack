# FlexPM Testing Guide

This document provides comprehensive testing instructions for FlexPM, including unit tests, integration tests, and end-to-end testing workflows.

**Test Coverage:** 32+ tests (22 integration + 10 unit tests)
**v1.2 Features:** Includes tests for Templates, Custom Fields, and Multiple Boards

## Table of Contents

1. [Quick Start](#quick-start)
2. [Backend Testing](#backend-testing)
3. [v1.2 Feature Tests](#v12-feature-tests)
4. [Frontend Testing](#frontend-testing)
5. [End-to-End Testing](#end-to-end-testing)
6. [Docker Testing](#docker-testing)
7. [Manual Testing Workflows](#manual-testing-workflows)

## Quick Start

```bash
# Test backend
cargo test

# Test with coverage
cargo tarpaulin --out Html

# Test frontend
cd frontend
npm test

# Full system test with Docker
docker compose up -d
curl http://localhost:3210/api/health
curl http://localhost:8080/health
```

## Backend Testing

### Unit Tests

FlexPM backend uses Rust's built-in testing framework:

```bash
# Run all tests
cargo test

# Run tests for specific crate
cargo test -p flexpm-core
cargo test -p flexpm-db
cargo test -p flexpm-api

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_create_project

# Run tests in release mode (faster)
cargo test --release
```

### Integration Tests

Integration tests are located in `crates/*/tests/` directories:

```bash
# Run integration tests only
cargo test --test '*'

# Example: Test database operations
cargo test --test db_integration
```

### Coverage

Install and run coverage tools:

```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Generate HTML coverage report
cargo tarpaulin --out Html --output-dir coverage

# Open coverage report
xdg-open coverage/index.html  # Linux
open coverage/index.html      # macOS
```

## v1.2 Feature Tests

FlexPM v1.2 introduces comprehensive tests for three major features: **Templates**, **Custom Fields**, and **Multiple Boards**.

### Test Summary

| Feature | Tests | Coverage |
|---------|-------|----------|
| **Templates** | 3 tests | Create, List with filters, Delete |
| **Custom Fields** | 3 tests | CRUD, Value upsert, Cascade delete |
| **Multiple Boards** | 4 tests | CRUD, Default management, Grouping types |
| **Total v1.2** | 10 tests | Full feature coverage |

### Template Tests

Located in `crates/flexpm-db/tests/integration_test.rs`:

```bash
# Run template-specific tests
cargo test test_create_and_get_template
cargo test test_list_templates_with_filter
cargo test test_delete_template_not_builtin
```

**Test Coverage:**
- ✅ Create custom template with workflow and vocabulary
- ✅ List all templates and filter by project type
- ✅ Protect built-in templates from deletion
- ✅ Delete user-created templates

**Example Test:**
```rust
#[tokio::test]
async fn test_create_and_get_template() {
    let repo = setup_test_db().await;
    let template_data = CreateProjectTemplate {
        name: "My Software Template".into(),
        project_type: ProjectType::Software,
        vocabulary: None,
        workflow: None,
        custom_fields: vec![],
        default_boards: vec![],
        is_builtin: false,
    };
    let template = templates::create_template(repo.pool(), template_data).await.unwrap();
    assert_eq!(template.name, "My Software Template");
}
```

### Custom Fields Tests

```bash
# Run custom field tests
cargo test test_create_and_list_custom_fields
cargo test test_custom_field_value_upsert
cargo test test_custom_field_cascade_delete
```

**Test Coverage:**
- ✅ Create fields with all 9 field types (text, number, date, select, etc.)
- ✅ Set and update field values (upsert logic)
- ✅ Cascade delete field values when field is deleted
- ✅ Validate required fields and options for select types

**Example Test:**
```rust
#[tokio::test]
async fn test_custom_field_value_upsert() {
    let field = custom_fields::create_field(repo.pool(), project.id, CreateCustomField {
        name: "Customer".into(),
        field_type: CustomFieldType::Text,
        required: false,
        // ...
    }).await.unwrap();

    // Set value
    custom_fields::set_field_value(repo.pool(), item.id, field.id, json!("Acme Corp")).await.unwrap();

    // Update value (upsert)
    custom_fields::set_field_value(repo.pool(), item.id, field.id, json!("Updated Corp")).await.unwrap();

    let value = custom_fields::get_field_value(repo.pool(), item.id, field.id).await.unwrap();
    assert_eq!(value.value, json!("Updated Corp"));
}
```

### Multiple Boards Tests

```bash
# Run board tests
cargo test test_create_and_list_boards
cargo test test_default_board_management
cargo test test_board_grouping_types
```

**Test Coverage:**
- ✅ Create unlimited boards per project
- ✅ Auto-manage default board (unset previous default)
- ✅ Test all 6 grouping types (status, priority, type, sprint, assignee, custom field)
- ✅ Get default board for project
- ✅ List all boards with filtering

**Example Test:**
```rust
#[tokio::test]
async fn test_default_board_management() {
    // Create first board as default
    let board1 = boards::create_board(repo.pool(), project.id, CreateBoard {
        name: "Board 1".into(),
        grouping: Some(BoardGrouping::Status),
        is_default: true,
    }).await.unwrap();

    // Create second board and make it default
    let board2 = boards::create_board(repo.pool(), project.id, CreateBoard {
        name: "Board 2".into(),
        grouping: Some(BoardGrouping::Priority),
        is_default: true, // This should unset board1's default
    }).await.unwrap();

    // Board 1 should no longer be default
    let board1_updated = boards::get_board(repo.pool(), board1.id).await.unwrap();
    assert!(!board1_updated.is_default);
}
```

### Running All v1.2 Tests

```bash
# Run all v1.2 tests at once
cargo test test_create_and_get_template \
           test_list_templates_with_filter \
           test_delete_template_not_builtin \
           test_create_and_list_custom_fields \
           test_custom_field_value_upsert \
           test_custom_field_cascade_delete \
           test_create_and_list_boards \
           test_default_board_management \
           test_board_grouping_types

# Or run all integration tests
cargo test --test integration_test
```

### Test Statistics

**Before v1.2:**
- Integration tests: 12
- Unit tests: 10
- Total: 22 tests

**After v1.2:**
- Integration tests: 22 (+10)
- Unit tests: 10
- Total: 32 tests (+45% increase)

**Coverage by Feature:**
- Core features (v1.0): 12 tests
- Templates (v1.2): 3 tests
- Custom Fields (v1.2): 3 tests
- Multiple Boards (v1.2): 4 tests
- Workflow/Vocabulary: 2 tests

## Frontend Testing

### Unit Tests

```bash
cd frontend

# Run tests
npm test

# Run tests in watch mode
npm run test:watch

# Run tests with coverage
npm run test:coverage
```

### Component Tests

```bash
# Run component-specific tests
npm test -- Board.test.tsx

# Update snapshots
npm test -- -u
```

### Type Checking

```bash
# Type check without building
npm run type-check

# Type check in watch mode
npm run type-check:watch
```

## End-to-End Testing

### Setup

1. **Start the full stack:**
   ```bash
   docker compose up -d
   ```

2. **Verify services are healthy:**
   ```bash
   # Check backend health
   curl http://localhost:3210/api/health

   # Check frontend health
   curl http://localhost:8080/health
   ```

### Complete Workflow Test

This workflow tests the entire application from project creation to board management:

#### 1. Create a Workspace and Project

```bash
# Get workspace ID (using default workspace)
WORKSPACE_ID=$(curl -s http://localhost:3210/api/workspaces | jq -r '.[0].id')

# Create a project
PROJECT_RESPONSE=$(curl -s -X POST http://localhost:3210/api/projects \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Test Project",
    "description": "E2E testing project",
    "project_type": "scrum"
  }')

PROJECT_ID=$(echo $PROJECT_RESPONSE | jq -r '.id')
echo "Created project: $PROJECT_ID"
```

#### 2. Create Items

```bash
# Create an epic
EPIC_RESPONSE=$(curl -s -X POST "http://localhost:3210/api/projects/$PROJECT_ID/items" \
  -H "Content-Type: application/json" \
  -d '{
    "title": "User Authentication Epic",
    "description": "Implement complete user authentication system",
    "item_type": "epic",
    "priority": "high"
  }')

EPIC_ID=$(echo $EPIC_RESPONSE | jq -r '.id')
echo "Created epic: $EPIC_ID"

# Create tasks under the epic
TASK1_RESPONSE=$(curl -s -X POST "http://localhost:3210/api/projects/$PROJECT_ID/items" \
  -H "Content-Type: application/json" \
  -d "{
    \"title\": \"Design login page\",
    \"description\": \"Create UI mockups for login\",
    \"item_type\": \"task\",
    \"parent_id\": \"$EPIC_ID\",
    \"priority\": \"medium\"
  }")

TASK1_ID=$(echo $TASK1_RESPONSE | jq -r '.id')

TASK2_RESPONSE=$(curl -s -X POST "http://localhost:3210/api/projects/$PROJECT_ID/items" \
  -H "Content-Type: application/json" \
  -d "{
    \"title\": \"Implement authentication API\",
    \"description\": \"Create backend auth endpoints\",
    \"item_type\": \"task\",
    \"parent_id\": \"$EPIC_ID\",
    \"priority\": \"high\"
  }")

TASK2_ID=$(echo $TASK2_RESPONSE | jq -r '.id')
echo "Created tasks: $TASK1_ID, $TASK2_ID"
```

#### 3. Test Board Operations

```bash
# Get board state
curl -s "http://localhost:3210/api/projects/$PROJECT_ID/board" | jq .

# Move task to "In Progress"
curl -s -X PATCH "http://localhost:3210/api/items/$TASK1_ID" \
  -H "Content-Type: application/json" \
  -d '{
    "status": "in_progress"
  }' | jq .

# Move task to "Done"
curl -s -X PATCH "http://localhost:3210/api/items/$TASK1_ID" \
  -H "Content-Type: application/json" \
  -d '{
    "status": "done"
  }' | jq .
```

#### 4. Test Auto-Status Propagation

This tests the parent status auto-update feature:

```bash
# Complete first task
curl -s -X PATCH "http://localhost:3210/api/items/$TASK1_ID" \
  -H "Content-Type: application/json" \
  -d '{"status": "done"}' | jq .

# Complete second task
curl -s -X PATCH "http://localhost:3210/api/items/$TASK2_ID" \
  -H "Content-Type: application/json" \
  -d '{"status": "done"}' | jq .

# Check if parent epic was auto-completed
curl -s "http://localhost:3210/api/items/$EPIC_ID" | jq '.status'
# Should return: "done"
```

#### 5. Test Sprint Management

```bash
# Create a sprint
SPRINT_RESPONSE=$(curl -s -X POST "http://localhost:3210/api/projects/$PROJECT_ID/sprints" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Sprint 1",
    "goal": "Complete authentication",
    "start_date": "2026-03-15T00:00:00Z",
    "end_date": "2026-03-29T00:00:00Z"
  }')

SPRINT_ID=$(echo $SPRINT_RESPONSE | jq -r '.id')
echo "Created sprint: $SPRINT_ID"

# Assign items to sprint
curl -s -X PATCH "http://localhost:3210/api/items/$TASK1_ID" \
  -H "Content-Type: application/json" \
  -d "{\"sprint_id\": \"$SPRINT_ID\"}" | jq .
```

#### 6. Test Search

```bash
# Search within project
curl -s "http://localhost:3210/api/projects/$PROJECT_ID/search?q=authentication" | jq .

# Global search across all projects
curl -s "http://localhost:3210/api/search?q=login" | jq .
```

#### 7. Test Export/Import

```bash
# Export project
curl -s "http://localhost:3210/api/projects/$PROJECT_ID/export?format=json" > export.json

# View export
cat export.json | jq .

# Validate import (validation only - full import not yet implemented)
curl -s -X POST http://localhost:3210/api/projects/import \
  -H "Content-Type: application/json" \
  -d @export.json | jq .
```

#### 8. Test WebSocket Real-time Updates

```bash
# In one terminal, listen to WebSocket
websocat "ws://localhost:3210/api/projects/$PROJECT_ID/board/live"

# In another terminal, make changes
curl -s -X PATCH "http://localhost:3210/api/items/$TASK1_ID" \
  -H "Content-Type: application/json" \
  -d '{"status": "in_progress"}'

# You should see WebSocket event in first terminal:
# {"type":"item_updated","project_id":"...","item_id":"...","old_status":"done","new_status":"in_progress"}
```

#### 9. Cleanup

```bash
# Delete project and all its items
curl -s -X DELETE "http://localhost:3210/api/projects/$PROJECT_ID"
```

### Browser E2E Testing

1. **Open the frontend:**
   ```bash
   open http://localhost:8080  # macOS
   xdg-open http://localhost:8080  # Linux
   ```

2. **Test workflow:**
   - Click "New Project" button
   - Fill in project details
   - Create project
   - Navigate to Board view
   - Create new items
   - Drag items between columns
   - Verify WebSocket updates work
   - Test search functionality
   - Export project data

### Frontend E2E with Playwright (Optional)

If you want to add automated browser testing:

```bash
cd frontend

# Install Playwright
npm install -D @playwright/test

# Run E2E tests
npm run test:e2e

# Run E2E tests with UI
npm run test:e2e:ui
```

Example Playwright test:

```typescript
// frontend/tests/e2e/board.spec.ts
import { test, expect } from '@playwright/test';

test('create project and add items', async ({ page }) => {
  await page.goto('http://localhost:8080');

  // Create project
  await page.click('text=New Project');
  await page.fill('input[name="name"]', 'Test Project');
  await page.click('button[type="submit"]');

  // Navigate to board
  await page.click('text=Board');

  // Add item
  await page.click('text=+ Add item');
  await page.fill('input[name="title"]', 'Test Task');
  await page.click('button:has-text("Create")');

  // Verify item appears
  await expect(page.locator('text=Test Task')).toBeVisible();
});
```

## Docker Testing

### Build and Test

```bash
# Build all services
docker compose build

# Start services
docker compose up -d

# Check logs
docker compose logs -f flexpm
docker compose logs -f frontend

# Test health endpoints
curl http://localhost:3210/api/health
curl http://localhost:8080/health

# Stop services
docker compose down

# Clean up volumes
docker compose down -v
```

### Production-like Testing

```bash
# Build with production optimizations
docker compose build --no-cache

# Start without development features
FLEXPM_LOG_LEVEL=warn docker compose up -d

# Load test (requires Apache Bench)
ab -n 1000 -c 10 http://localhost:3210/api/health
```

## Manual Testing Workflows

### Complete Feature Test Checklist

- [ ] **Project Management**
  - [ ] Create project with all types (scrum, kanban, construction)
  - [ ] Update project details
  - [ ] Delete project
  - [ ] List all projects

- [ ] **Item Management**
  - [ ] Create items of all types (epic, feature, task, bug)
  - [ ] Create parent-child relationships
  - [ ] Update item details
  - [ ] Delete items
  - [ ] Verify cascade delete of children

- [ ] **Board Operations**
  - [ ] View board with multiple columns
  - [ ] Move items between columns
  - [ ] Test WIP limits
  - [ ] Test workflow transitions
  - [ ] Verify board configuration updates

- [ ] **Auto-Status Propagation**
  - [ ] Create parent with multiple children
  - [ ] Complete all children
  - [ ] Verify parent auto-completes
  - [ ] Test with nested hierarchy (epic > feature > task)

- [ ] **Sprint Management**
  - [ ] Create sprint
  - [ ] Assign items to sprint
  - [ ] Update sprint dates
  - [ ] Complete sprint
  - [ ] View sprint report

- [ ] **Search & Filter**
  - [ ] Search within project
  - [ ] Global search across projects
  - [ ] Filter by status, type, priority
  - [ ] Full-text search

- [ ] **Real-time Updates**
  - [ ] Connect WebSocket
  - [ ] Verify item updates broadcast
  - [ ] Test multiple clients
  - [ ] Handle disconnection/reconnection

- [ ] **Export/Import**
  - [ ] Export project as JSON
  - [ ] Export project as CSV
  - [ ] Validate export format
  - [ ] Test import validation

- [ ] **Dependencies**
  - [ ] Create dependency between items
  - [ ] Test dependency validation
  - [ ] Detect circular dependencies

- [ ] **Comments & Attachments**
  - [ ] Add comment to item
  - [ ] Upload attachment
  - [ ] Download attachment
  - [ ] Delete comment/attachment

- [ ] **v1.2: Project Templates**
  - [ ] Create custom template
  - [ ] List templates with type filter
  - [ ] Create project from template
  - [ ] Verify workflow/vocabulary applied
  - [ ] Delete user template
  - [ ] Verify builtin templates protected

- [ ] **v1.2: Custom Fields**
  - [ ] Create text field
  - [ ] Create select field with options
  - [ ] Create date/number/boolean fields
  - [ ] Set field value on item
  - [ ] Update field value (upsert)
  - [ ] Delete field (cascade delete values)
  - [ ] View all field values for item

- [ ] **v1.2: Multiple Boards**
  - [ ] Create multiple boards per project
  - [ ] Set default board
  - [ ] Switch between boards
  - [ ] Test status grouping
  - [ ] Test priority grouping
  - [ ] Test item type grouping
  - [ ] Test sprint grouping
  - [ ] Delete board

## Continuous Integration

Example GitHub Actions workflow:

```yaml
# .github/workflows/test.yml
name: Test

on: [push, pull_request]

jobs:
  backend:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - run: cargo test
      - run: cargo clippy -- -D warnings

  frontend:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 20
      - run: cd frontend && npm ci
      - run: cd frontend && npm test
      - run: cd frontend && npm run build

  e2e:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: docker compose up -d
      - run: sleep 10  # Wait for services
      - run: curl http://localhost:3210/api/health
      - run: curl http://localhost:8080/health
      - run: docker compose down
```

## Troubleshooting

### Backend Tests Fail

```bash
# Clean and rebuild
cargo clean
cargo build
cargo test

# Check database connection
FLEXPM_DATABASE_URL=sqlite::memory: cargo test
```

### Frontend Tests Fail

```bash
# Clear cache
rm -rf frontend/node_modules
rm frontend/package-lock.json
npm install

# Run with verbose output
npm test -- --verbose
```

### Docker Tests Fail

```bash
# Check service status
docker compose ps

# View logs
docker compose logs

# Restart services
docker compose restart

# Full reset
docker compose down -v
docker compose build --no-cache
docker compose up -d
```

## Performance Testing

### Backend Load Testing

```bash
# Install wrk
sudo apt install wrk  # Ubuntu
brew install wrk      # macOS

# Run load test
wrk -t4 -c100 -d30s http://localhost:3210/api/health

# Test API endpoint
wrk -t4 -c100 -d30s \
  -s scripts/post.lua \
  http://localhost:3210/api/projects
```

### Database Performance

```bash
# Run with profiling
FLEXPM_LOG_LEVEL=debug cargo run --release

# Check slow queries in logs
docker compose logs flexpm | grep "slow query"
```

## Security Testing

```bash
# Check for known vulnerabilities
cargo audit

# Run security checks
cargo deny check

# Frontend security audit
cd frontend
npm audit
npm audit fix
```

## Conclusion

This testing guide covers:
- ✅ Unit and integration tests for backend and frontend
- ✅ Complete end-to-end workflow testing
- ✅ Docker-based testing
- ✅ Manual testing checklists
- ✅ Performance and security testing
- ✅ CI/CD integration

For more information:
- Backend API: See [API-EXAMPLES.md](./API-EXAMPLES.md)
- Development: See [DEVELOPMENT.md](../DEVELOPMENT.md)
- Architecture: See [TODO-ARCHITECTURE.md](../TODO-ARCHITECTURE.md)
