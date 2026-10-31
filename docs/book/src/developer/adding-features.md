# Adding Features

This chapter is for contributors who add to Tack. It gives four recipes: a new entity, a new workflow preset, an extension of the workflow engine, and a new key in the project file. Each section includes a concrete example and explains the reasoning behind the step order. Before you start, read the layering rule in the [Architecture Overview](index.md) and the anti-patterns at the end of this page. The Milestone and `education_workflow` examples are illustrations; neither exists in the codebase.

---

## Adding a New Entity

**Example: Milestone** (illustrative; there is no milestone in the codebase)

A Milestone is a date-anchored checkpoint associated with a project. It has a name, an optional description, a target date, and a status.

### Step 1: Define the model in `tack-core`

Open `crates/tack-core/src/models.rs` and add the struct and any associated DTOs:

```rust
// ─── Milestone ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Milestone {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub target_date: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateMilestone {
    #[validate(length(min = 1, max = 200))]
    pub name: String,
    pub description: Option<String>,
    pub target_date: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateMilestone {
    #[validate(length(min = 1, max = 200))]
    pub name: Option<String>,
    pub description: Option<String>,
    pub target_date: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}
```

`tack-core` is the right home for these types because it is the layer all other crates share. The API crate uses `CreateMilestone` for deserialization; the DB crate uses it as the input to the repository function. Keeping them in one place avoids duplication.

### Step 2: Add the migration in `tack-db`

Open `crates/tack-db/src/migrations.rs`. Migration names are numbered sequentially and
never reused. Find the highest number in `all_migrations()` (`grep -o
'"[0-9]\{3\}_[a-zA-Z0-9_]*"' crates/tack-db/src/migrations.rs | sort -u | tail -1`) and use the next
one, written `NNN` below. Append to the `Vec` in `all_migrations()`:

```rust
ordinary("NNN_milestones", &MIGRATION_NNN[..]),
```

Then add the constant near the end of the file:

```rust
const MIGRATION_NNN: [&str; 2] = [
    "CREATE TABLE IF NOT EXISTS milestones (
        id TEXT PRIMARY KEY NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        description TEXT,
        target_date TEXT,
        completed_at TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_milestones_project ON milestones(project_id)",
];
```

Migrations are append-only and run in order. Never edit a shipped one: startup refuses a database whose applied history differs from the binary's list by name or checksum. To change the schema, add a new migration.

### Step 3: Create the repository module

Create `crates/tack-db/src/repo/milestones.rs`, declare it with `pub mod milestones;` in `repo.rs`, and write its commands as an `impl Repository` block. A method binds its parameters, runs the query, reads the row back through `decode`, and returns `StoreError`; no `sqlx` type may appear in a signature.

```rust
use chrono::Utc;
use uuid::Uuid;

use tack_core::models::{CreateMilestone, Milestone};

use super::Repository;
use crate::StoreError;

impl Repository {
    pub async fn create_milestone(
        &self,
        project_id: Uuid,
        input: CreateMilestone,
    ) -> Result<Milestone, StoreError> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO milestones (id, project_id, name, description, target_date, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(id.to_string())
        .bind(project_id.to_string())
        .bind(&input.name)
        .bind(&input.description)
        .bind(input.target_date.map(|d| d.to_rfc3339()))
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(self.pool())
        .await?;

        Ok(Milestone {
            id,
            project_id,
            name: input.name,
            description: input.description,
            target_date: input.target_date,
            completed_at: None,
            created_at: now,
            updated_at: now,
        })
    }

    // get, update and delete follow the same shape.
}
```

Reading a row back goes through a private row struct and `decode::uuid`, `decode::instant` and the other `decode` functions (see `repo/briefs.rs` for a small, complete example), so a stored value that cannot be read is `StoreError::Corrupt` rather than a panic. A question that only reads, such as a list for a screen, is a `pub async fn` in `crates/tack-db/src/queries/` instead. Its name states the question, and it returns a row struct under the columns' names.

### Step 4: Put any rule in a use case

A handler extracts, calls, maps the error and responds. If the entity has a rule beyond that (an idempotency key, a state check, two writes that must happen together), write it as a `tack-app` use case, `pub async fn <verb>(repo: &Repository, now: DateTime<Utc>, ..) -> Result<T, AppError>`, and have the handler call it. A plain create like this one calls the repository directly.

### Step 5: Create the handler

Create `crates/tack-api/src/handlers/milestones.rs` and declare it in `handlers.rs`. Take `Json`, `Path` and `Query` from `crate::extract`, not from axum, so a request that cannot be read gets the API's error body. Every handler carries a `#[utoipa::path]` attribute: it is what puts the route in `docs/openapi.json`.

```rust
use axum::extract::State;
use axum::http::StatusCode;
use uuid::Uuid;
use validator::Validate;

use tack_core::models::{CreateMilestone, Milestone};

use crate::error::{ApiError, ApiResult};
use crate::extract::{Json, Path};
use crate::router::AppState;

#[utoipa::path(
    post,
    path = "/api/projects/{project_id}/milestones",
    tag = "milestones",
    params(("project_id" = Uuid, Path, description = "Project ID")),
    request_body = CreateMilestone,
    responses(
        (status = 201, description = "The new milestone", body = Milestone),
        (status = 404, description = "Project not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn create_milestone(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Json(input): Json<CreateMilestone>,
) -> ApiResult<(StatusCode, Json<Milestone>)> {
    input.validate().map_err(|e| ApiError::BadRequest(e.to_string()))?;
    state.repo.get_project(project_id).await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {project_id} not found")))?;
    let milestone = state.repo.create_milestone(project_id, input).await?;
    Ok((StatusCode::CREATED, Json(milestone)))
}

// list_milestones, get_milestone, update_milestone, delete_milestone ...
```

The model types need `#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]` so the spec can name them.

### Step 6: Register the routes and the spec

In `crates/tack-api/src/router.rs`, add the routes to the `/api` router builder:

```rust
.route("/projects/{project_id}/milestones", post(milestones::create_milestone).get(milestones::list_milestones))
.route("/milestones/{id}", get(milestones::get_milestone).patch(milestones::update_milestone).delete(milestones::delete_milestone))
```

In `crates/tack-api/src/openapi.rs`, add each handler to the `paths(...)` list of `ApiDoc` and each new type to `components(schemas(...))`. Then run `./scripts/regen-generated.sh`: it regenerates `docs/openapi.json`, `frontend/src/shared/api/schema.gen.ts`, the generated API reference page and the lockfiles, and the OpenAPI contract test fails until you do.

At this point the feature is complete. Add repository tests under `crates/tack-db/tests/repository/` and handler tests under `crates/tack-api/tests/handlers/`. Each crate's `tests/*.rs` files are modules of one test binary, `tests/it.rs`, so add to the module whose subject fits; a new subject is one more `mod` line there (see [Testing](testing.md)).

---

## Adding a New Workflow Preset

**Example: education_workflow for an online-course project type**

### Step 1: Add the preset function in `tack-core`

Open `crates/tack-core/src/workflow.rs` and add the function after the existing presets:

```rust
pub fn education_workflow() -> WorkflowConfig {
    WorkflowConfig {
        workflow_type: WorkflowType::Custom,
        statuses: vec![
            StatusDef {
                name: "Not Started".into(),
                category: StatusCategory::Todo,
                wip_limit: None,
                order: 0,
            },
            StatusDef {
                name: "In Progress".into(),
                category: StatusCategory::InProgress,
                wip_limit: None,
                order: 1,
            },
            StatusDef {
                name: "Under Review".into(),
                category: StatusCategory::InProgress,
                wip_limit: None,
                order: 2,
            },
            StatusDef {
                name: "Completed".into(),
                category: StatusCategory::Done,
                wip_limit: None,
                order: 3,
            },
        ],
        transitions: None,
    }
}
```

### Step 2: Write unit tests

Add tests in the `#[cfg(test)]` module in the same file:

```rust
#[test]
fn education_initial_status_is_not_started() {
    assert_eq!(education_workflow().initial_status().unwrap(), "Not Started");
}

#[test]
fn education_allows_in_progress_to_completed() {
    let wf = education_workflow();
    assert!(wf.validate_transition("In Progress", "Completed").is_ok());
}
```

Unit testing presets here is valuable because these tests run without any database or runtime — they are essentially free to run and will catch any mistake in the status list.

### Step 3: Add the `ProjectType` variant

Open `crates/tack-core/src/models.rs` and add `Education` to the `ProjectType` enum. The
enum is exhaustive — every match on it must add an arm — so this is the full current
list, not an excerpt:

```rust
pub enum ProjectType {
    Software,
    Web,
    Mobile,
    Construction,
    Personal,
    Homework,
    Maintenance,
    Legal,
    Research,
    Event,
    Education,  // ← new
    Custom,
}
```

Also update the `Display` impl:

```rust
Self::Education => write!(f, "education"),
```

### Step 4: Update `workflow_for_type`

In `workflow.rs`, add the new arm to the `match` in `workflow_for_type`:

```rust
pub fn workflow_for_type(project_type: &ProjectType) -> WorkflowConfig {
    match project_type {
        ProjectType::Software | ProjectType::Web | ProjectType::Mobile => scrum_workflow(),
        ProjectType::Construction => construction_workflow(),
        ProjectType::Personal | ProjectType::Homework => simple_workflow(),
        ProjectType::Maintenance => kanban_workflow(),
        ProjectType::Legal => legal_workflow(),
        ProjectType::Research => research_workflow(),
        ProjectType::Event => event_workflow(),
        ProjectType::Education => education_workflow(),  // ← new
        ProjectType::Custom => simple_workflow(),
    }
}
```

To let a project file choose the preset by name (`workflow.preset: education`), add an arm to `preset_workflow` in the same file, and list the name in the project file reference ([The Project File Format](project-file.md)).

### Step 5: Add a vocabulary preset

Open `crates/tack-core/src/vocabulary.rs` and add a case to `vocabulary_for_type`:

```rust
ProjectType::Education => HashMap::from([
    ("epic".into(), "Course".into()),
    ("feature".into(), "Module".into()),
    ("task".into(), "Lesson".into()),
    ("subtask".into(), "Exercise".into()),
    ("bug".into(), "Correction".into()),
    ("sprint".into(), "Week".into()),
    ("milestone".into(), "Exam".into()),
    // … other terms
]),
```

At this point `POST /api/projects` with `"project_type": "education"` will auto-select the new workflow and vocabulary.

---

## Extending the Workflow Engine

Sometimes you need new logic in the workflow engine itself — for example, a rule that prevents moving an item to `Done` if any of its dependencies are still incomplete.

### Step 1: Add the function to `tack-core`

Open `crates/tack-core/src/workflow.rs`. Add a pure function that takes its facts as parameters:

```rust
impl WorkflowConfig {
    /// Refuse to mark an item done while it has unresolved blocking dependencies.
    pub fn check_dependencies_resolved(
        &self,
        item_id: Uuid,
        unresolved_blockers: usize,
        target_status: &str,
    ) -> Result<(), CoreError> {
        if self.is_done_status(target_status) && unresolved_blockers > 0 {
            return Err(CoreError::Validation(format!(
                "Item {item_id} has {unresolved_blockers} unresolved blocker(s)"
            )));
        }
        Ok(())
    }
}
```

The function does not query the database. The caller loads the facts and passes them in.

### Step 2: Write unit tests

Add tests in the `#[cfg(test)]` module of the same file (`workflow/tests.rs`). Test both the passing case (no blockers, or target not Done) and the failing case (blockers present, target is Done). These tests are cheap because there is no I/O to mock: you call the function with constructed data.

### Step 3: Call it where the move is decided

A status change is decided inside the store's transaction: `Repository::update_item_atomically` counts the items already in the target status and asks the workflow, through the `StatusGate` trait (`admit_move`, which runs `validate_transition` and `check_wip_limit`), whether the move is allowed. A rule that needs only what `StatusGate` already receives belongs in `admit_move`. A rule that needs another fact (here, the item's blockers) is checked by the caller before it writes: load the fact with a `Repository` method or a `queries/` function, call the new check, and return its error. Put that call in a `tack-app` use case when other callers (the CLI through the API, the MCP server, the auto-start tick) must obey the same rule; the handler in `tack-api/src/handlers/items.rs` then calls the use case.

### What not to change

No migration is needed. The workflow engine is pure logic: it reads from a `WorkflowConfig` struct that is already stored as JSON. Adding a method to `WorkflowConfig` changes no schema.

If the new logic needs new *configuration* (for example, a per-project flag to enable or disable dependency-blocking), add a field to `WorkflowConfig`, give it a serde default so stored rows that lack it still load, and add a project-file key if a person should be able to set it (next recipe).

---

## Add a key to the project file

**Example: a `review_required` flag on `automation`.** The project file is one type read by every door (the import routes, `tack import`, the MCP tool, Settings → Data → Import), so a key added in the right six places reaches all of them. The reference for every existing key is [The Project File Format](project-file.md).

### Step 1: Add the field to the type

In `crates/tack-core/src/outline.rs`, add it to `BundleAutomation`:

```rust
/// Hold every merge for a person's review.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub review_required: Option<bool>,
```

`skip_serializing_if` keeps an export free of `null`. The type derives `deny_unknown_fields`, so until this step is done the key is refused wherever it is written.

### Step 2: Check it

In `crates/tack-core/src/outline/bundle.rs`, `check_automation` is where the `automation` section is validated. Push a string onto `problems` to refuse the file, or onto `warnings` to let it through. A key that could raise autonomy or switch automation on must be lowered to what the board already allows, as `lowered_level` does.

### Step 3: Write it

In `crates/tack-api/src/handlers/outline.rs`, `write_automation` turns the section into an `UpdateProject`. Add the field to `UpdateProject` and the column to the repository if the board does not store it yet (a migration, see above). Keep the section name in the `at("automation")` error mapping so that a failure says where it happened.

### Step 4: Export it

In `to_bundle` (`outline/bundle.rs`), read the value back from the project. If the key needs rows the export does not gather yet, add them to `Parts` and fill them in `bundle_response` in `crates/tack-api/src/handlers/export.rs`. The test that matters is the round trip: export, import into a new project, export again, and compare the text.

### Step 5: Templates, if they should carry it

A template stores `roles`, `profiles` and `automation` as JSON in `project_templates`. A key inside `automation` is carried for free once Steps 1 to 4 are done, because `build_project_from_template` and `save_project_as_template` pass the whole section. A key in a new section needs a column and both functions changed.

### Step 6: Regenerate, pin and document

1. `./scripts/regen-generated.sh` regenerates `docs/openapi.json`, `frontend/src/shared/api/schema.gen.ts` and the lockfiles. The OpenAPI contract test fails until you do.
2. Add the key to `crates/tack-api/tests/fixtures/bundles/my-saas.yaml` with a comment saying what it does.
3. Add a row to the checking tests in `crates/tack-core/src/outline/bundle/tests.rs` (an accepted case and a refused one) and a case to `crates/tack-api/tests/handlers/bundle.rs`.
4. Add the key to the tables in [The Project File Format](project-file.md), and to [Import, Export & Project Files](../user-guide/import-export.md) if a person writes it.

---

## Anti-Patterns to Avoid

The crate layering is the project's load-bearing constraint. Most review feedback on new features comes down to one of these:

- **Don't put I/O in `tack-core`.** No file access, no HTTP calls, no `sqlx`, no `tokio` runtime needs. Core is pure, synchronous domain logic so it stays trivially testable. If a rule needs data, take it as a function parameter and let the handler load it (as in the dependency check above).
- **Don't scatter validation across handlers.** Transition rules, WIP limits, cycle checks, and field validation belong in `tack-core`, called from the handler. Duplicating a rule inline in a handler means the CLI, API, and MCP server can disagree about what's valid.
- **Don't reach past the repository layer.** Handlers call `Repository` methods; they never build SQL or touch the pool directly. New commands go in the matching `repo/` module and new reads in `queries/`.
- **Don't let `tack-cli` import `tack-db`.** The CLI is an HTTP client — all data access goes through the API so workflow rules are enforced server-side. The same applies to the MCP server. The one exception is `serve.rs`, which builds the `Repository` the server runs on. `scripts/check-crate-graph.sh` fails on any other edge.
- **Don't edit an existing migration.** Migrations are append-only. Changing a shipped migration corrupts databases that already applied it. Add a new numbered migration and wire it into `all_migrations()`.
- **Don't hardcode colors in the frontend.** Components consume `--color-*` design tokens via inline `style`, never raw hex, so the theme/palette system keeps working. See [Frontend & Design System](frontend.md).
- **Don't add an endpoint without a handler test.** Every new route gets at least a success-path and an error-path test in the module of `crates/tack-api/tests/handlers/` that matches the route's subject — see [Testing](testing.md).

When a change feels like it needs to break one of these, that's usually a sign the logic belongs in a different layer — move it rather than bending the boundary.
