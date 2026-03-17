# FlexPM v1.2 - New Features Implementation Plan

## Overview

This document outlines the implementation of three major new features for FlexPM v1.2:

1. **Project Templates** - Reusable project blueprints
2. **Custom Fields** - User-defined metadata for items
3. **Multiple Boards per Project** - Different views/filters within a single project

---

## 1. Project Templates

### Purpose
Allow users to create and reuse project configurations, reducing setup time for similar projects.

### Backend Implementation

#### Models (`flexpm-core/src/models.rs`)
```rust
pub struct ProjectTemplate {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub project_type: ProjectType,
    pub vocabulary: VocabularyMap,
    pub workflow: WorkflowConfig,
    pub custom_fields: Vec<CustomFieldDefinition>,
    pub default_boards: Vec<BoardTemplate>,
    pub is_builtin: bool,  // System vs. user-created
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

#### Database Schema (`MIGRATION_011`)
- **Table:** `project_templates`
- **Columns:** id, name, description, project_type, vocabulary, workflow, custom_fields, default_boards, is_builtin, created_at, updated_at
- **Indexes:** project_type

#### Repository (`flexpm-db/src/repo/templates.rs`)
- `create_template(pool, data)` - Create user template
- `get_template(pool, id)` - Get template by ID
- `list_templates(pool, project_type?)` - List all templates (optionally filtered)
- `delete_template(pool, id)` - Delete user template (not builtin)

#### API Endpoints (TODO)
```
POST   /api/templates                  - Create template
GET    /api/templates                  - List all templates
GET    /api/templates/:id              - Get template
DELETE /api/templates/:id              - Delete template
POST   /api/projects/from-template/:id - Create project from template
```

### Frontend Implementation (TODO)

#### Template Gallery (`/templates`)
- Grid of template cards
- Filter by project type
- Preview template details
- "Use Template" button → create project flow

#### Template Creator (`/templates/new`)
- Form to define template properties
- Workflow editor
- Vocabulary editor
- Custom fields configuration
- Board templates configuration

#### Template in Project Creation
- Add "From Template" tab to create project modal
- Template selector dropdown
- Preview shows what will be configured

### Use Cases
1. **Agency**: "Web Project" template with standard phases, roles, custom fields for client info
2. **Construction**: "Building Project" template with permit/inspection workflows
3. **Education**: "Course" template with assignment types, grading fields
4. **Personal**: "Goal Tracking" template with habit fields, milestone boards

---

## 2. Custom Fields

### Purpose
Allow users to add project-specific or template-level metadata fields beyond the standard item properties.

### Backend Implementation

#### Models (`flexpm-core/src/models.rs`)
```rust
pub struct CustomFieldDefinition {
    pub id: Uuid,
    pub project_id: Option<Uuid>,  // None for template-level
    pub name: String,
    pub field_type: CustomFieldType,
    pub description: Option<String>,
    pub required: bool,
    pub default_value: Option<serde_json::Value>,
    pub options: Option<Vec<String>>,  // For select types
    pub validation: Option<serde_json::Value>,  // JSON schema/regex
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub enum CustomFieldType {
    Text,
    Number,
    Date,
    Boolean,
    Select,       // Dropdown
    MultiSelect,  // Multiple selection
    Url,
    Email,
    LongText,     // Textarea
}

pub struct CustomFieldValue {
    pub id: Uuid,
    pub item_id: Uuid,
    pub field_id: Uuid,
    pub value: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

#### Database Schema (`MIGRATION_012`)
- **Tables:**
  - `custom_field_definitions` - Field schemas
  - `custom_field_values` - Field values per item
- **Unique Constraint:** (item_id, field_id) - One value per field per item
- **Cascade Delete:** Delete values when item or field is deleted

#### Repository (`flexpm-db/src/repo/custom_fields.rs`)
**Field Definitions:**
- `create_field(pool, project_id, data)` - Add field to project
- `get_field(pool, id)` - Get field definition
- `list_fields_for_project(pool, project_id)` - Get all fields
- `update_field(pool, id, data)` - Update field definition
- `delete_field(pool, id)` - Remove field (cascades to values)

**Field Values:**
- `set_field_value(pool, item_id, field_id, value)` - Upsert value
- `get_field_value(pool, item_id, field_id)` - Get single value
- `get_all_field_values_for_item(pool, item_id)` - Get all values for item
- `delete_field_value(pool, item_id, field_id)` - Remove value

#### API Endpoints (TODO)
```
# Field Definitions
POST   /api/projects/:id/custom-fields       - Create field
GET    /api/projects/:id/custom-fields       - List project fields
GET    /api/custom-fields/:id                - Get field
PATCH  /api/custom-fields/:id                - Update field
DELETE /api/custom-fields/:id                - Delete field

# Field Values
PUT    /api/items/:id/custom-fields/:field_id - Set field value
GET    /api/items/:id/custom-fields           - Get all item field values
DELETE /api/items/:id/custom-fields/:field_id - Remove field value
```

### Frontend Implementation (TODO)

#### Custom Fields Manager (`/projects/:id/settings/fields`)
- List of project custom fields
- "Add Field" button → field creation modal
- Field type selector with preview
- Validation rules editor (for each type)
- Drag-to-reorder fields
- Edit/Delete actions

#### Item Form Integration
- Display custom fields in create/edit item modal
- Render appropriate input based on field_type
  - Text → `<input type="text">`
  - Number → `<input type="number">`
  - Date → `<input type="date">`
  - Boolean → `<input type="checkbox">`
  - Select → `<select>` with options
  - MultiSelect → Multi-select component
  - Url → `<input type="url">`
  - Email → `<input type="email">`
  - LongText → `<textarea>`
- Show required indicator
- Validate on submit

#### List/Board View Display
- Add custom field columns to list view (optional toggle)
- Show custom field values in item cards (configurable)
- Filter by custom field values

### Use Cases
1. **Software**: Custom fields for "Reviewed By", "Deployed To", "Feature Flag"
2. **Construction**: "Contractor", "Material Cost", "Inspection Date"
3. **Marketing**: "Campaign ID", "Budget", "Target Audience", "Platform"
4. **Sales**: "Deal Size", "Close Probability", "Account Manager"

---

## 3. Multiple Boards per Project

### Purpose
Allow a single project to have multiple board views with different filters, groupings, and configurations.

### Backend Implementation

#### Models (`flexpm-core/src/models.rs`)
```rust
pub struct Board {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub filters: Option<serde_json::Value>,  // Filter criteria
    pub grouping: Option<BoardGrouping>,
    pub is_default: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub enum BoardGrouping {
    Status,              // Standard Kanban
    Priority,            // Columns: Critical, High, Medium, Low
    ItemType,            // Columns: Epic, Feature, Task, Bug
    Sprint,              // Columns per sprint
    Assignee,            // Columns per team member
    CustomField(Uuid),   // Group by custom field value
}
```

#### Database Schema (`MIGRATION_013`)
- **Table:** `boards`
- **Columns:** id, project_id, name, description, filters, grouping, is_default, created_at, updated_at
- **Indexes:** project_id, (project_id, is_default)

#### Repository (`flexpm-db/src/repo/boards.rs`)
- `create_board(pool, project_id, data)` - Create board
- `get_board(pool, id)` - Get board by ID
- `get_default_board(pool, project_id)` - Get default board
- `list_boards(pool, project_id)` - Get all boards for project
- `update_board(pool, id, data)` - Update board
- `delete_board(pool, id)` - Delete board

#### API Endpoints (TODO)
```
POST   /api/projects/:id/boards      - Create board
GET    /api/projects/:id/boards      - List project boards
GET    /api/boards/:id               - Get board
PATCH  /api/boards/:id               - Update board
DELETE /api/boards/:id               - Delete board
GET    /api/boards/:id/view          - Get board state (items grouped/filtered)
```

### Frontend Implementation (TODO)

#### Board Selector
- Dropdown in Board view header
- Shows all boards for current project
- Default board auto-selected
- Quick-switch between boards

#### Board Manager (`/projects/:id/settings/boards`)
- List of all boards with preview cards
- "Create Board" button → board creation wizard
- Board configuration:
  - Name and description
  - Grouping type selector
  - Filter builder (status, priority, type, sprint, custom fields)
  - Set as default checkbox
- Edit/Delete/Duplicate actions

#### Board Creation Wizard
1. **Name & Description** - Basic info
2. **Grouping** - Choose how to organize columns
3. **Filters** - Which items to show
4. **Preview** - See what the board will look like

#### Smart Board Presets
- "Active Sprint" - Filter: current sprint, Group: status
- "By Priority" - Group: priority
- "Bugs Only" - Filter: type=bug, Group: status
- "My Tasks" - Filter: assignee=me (future)
- "Backlog" - Filter: sprint=null, Group: priority

### Use Cases
1. **Development Team**:
   - "Sprint Board" - Current sprint items by status
   - "Backlog" - Unassigned items by priority
   - "Bugs" - All bugs grouped by priority

2. **Marketing Campaign**:
   - "Content Calendar" - Group by custom field "Publishing Month"
   - "By Channel" - Group by custom field "Platform"
   - "Priority Tasks" - Filter: priority≥high, Group: status

3. **Construction**:
   - "By Phase" - Group by status (standard workflow)
   - "By Trade" - Group by custom field "Contractor Type"
   - "This Month" - Filter: due_date this month, Group: priority

---

## Testing Strategy

### Unit Tests
- Core models serialization/deserialization
- Repository CRUD operations
- Validation logic (custom field types, board filters)

### Integration Tests
- Full API endpoint workflows
- Template → Project creation
- Custom field value persistence
- Board filtering/grouping logic

### E2E Tests (Playwright/Cypress)
- Create template → Create project from template
- Add custom field → Set values on items → View in list
- Create multiple boards → Switch between them → Apply filters

---

## Migration Path

### For Existing Projects
1. **Default Board Creation** - On first access, create a "Main Board" (is_default=true) for each project with current workflow
2. **Backward Compatibility** - `/projects/:id/board` routes to default board
3. **Zero Downtime** - Migrations add tables without affecting existing data

### For Existing Items
- Custom field values start empty
- No required fields initially
- Users can gradually populate custom fields

---

## Timeline Estimate

| Phase | Tasks | Estimate |
|-------|-------|----------|
| **Backend** | Models + Migrations + Repositories | ✅ Complete |
| **Backend** | API Handlers + Routes | 4-6 hours |
| **Backend** | Testing (unit + integration) | 2-3 hours |
| **Frontend** | Templates UI (gallery + creator) | 6-8 hours |
| **Frontend** | Custom Fields UI (manager + form integration) | 6-8 hours |
| **Frontend** | Multiple Boards UI (selector + manager) | 4-6 hours |
| **Frontend** | Testing (E2E + manual QA) | 3-4 hours |
| **Documentation** | API docs + user guides | 2-3 hours |
| **Total** | | **27-38 hours** (~1 week) |

---

## Success Criteria

✅ **Project Templates**
- [ ] Users can create custom templates
- [ ] Built-in templates exist for common project types
- [ ] Projects created from templates inherit all configuration
- [ ] Templates can be shared/exported (future: template marketplace)

✅ **Custom Fields**
- [ ] Support all 9 field types with proper validation
- [ ] Fields can be required or optional
- [ ] Field values persist and display correctly
- [ ] List view can filter/sort by custom fields
- [ ] Board view can group by custom fields

✅ **Multiple Boards**
- [ ] Projects can have unlimited boards
- [ ] Boards can group by 6 different criteria
- [ ] Boards can filter items by any property
- [ ] Easy board switching in UI
- [ ] Default board concept works intuitively

---

## Future Enhancements (v1.3+)

1. **Template Marketplace** - Share templates publicly
2. **Field Formulas** - Calculated fields (e.g., "Total Cost" = sum of item costs)
3. **Conditional Fields** - Show field X only if field Y = value Z
4. **Board Automation** - Auto-move items based on rules
5. **Saved Filters** - Reusable filter presets
6. **Board Permissions** - Some boards visible only to certain roles
7. **Custom Field Types** - File upload, relationship (link to other item), user selector

---

## Notes

- All three features are designed to work together
- Templates can include custom fields and default boards
- Custom fields enable richer board grouping options
- This brings FlexPM closer to feature parity with tools like Jira, Asana, ClickUp
