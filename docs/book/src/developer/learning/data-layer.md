# The Data Layer (sqlx & Repository Pattern)

Tack's data layer looks nothing like Sequelize, SQLAlchemy, Hibernate, or ActiveRecord. It uses `sqlx`, which is not an ORM. This chapter explains how it works and why it is structured the way it is.

---

## sqlx is not an ORM

Traditional ORMs generate SQL from model definitions. sqlx goes the other direction: you write SQL, sqlx validates it and generates the type mapping.

```rust
// SQLAlchemy (Python) — ORM generates the SQL:
items = session.query(Item).filter(Item.project_id == project_id).all()

// Sequelize (Node) — ORM generates the SQL:
const items = await Item.findAll({ where: { projectId } })

// sqlx (Rust) — you write the SQL; sqlx validates it at compile time:
let rows = sqlx::query_as::<_, ItemRow>(
    "SELECT id, project_id, title, status FROM items WHERE project_id = ?"
)
.bind(project_id.to_string())
.fetch_all(pool)
.await?;
```

The key feature: **sqlx checks your SQL against a real database at compile time**. If the column does not exist, the type is wrong, or the parameter count is off, it will not compile. This is enforced by a cached database schema checked in at `.sqlx/`. If you add a column to a migration and forget to update a query that reads that table, `cargo build` fails with a clear error before you can ever run the broken code.

This comes at a cost: you write more SQL. The payoff: you have full SQL power — CTEs, `WITH RECURSIVE`, `json_each`, FTS5 `MATCH`, window functions — without fighting an ORM's abstraction layer.

---

## The Repository pattern

All database operations live in `crates/tack-db/src/repo/`. The structure:

```
crates/tack-db/src/
├── lib.rs           # declares Repository struct; re-exports all repo modules
├── migrations.rs    # 17 migrations embedded as strings
└── repo/
    ├── items.rs
    ├── projects.rs
    ├── sprints.rs
    ├── boards.rs
    ├── comments.rs
    ├── dependencies.rs
    ├── roles.rs
    ├── attachments.rs
    └── ...
```

The `Repository` struct is a thin wrapper around `SqlitePool`:

```rust
pub struct Repository {
    pool: SqlitePool,
}

impl Repository {
    pub fn new(pool: SqlitePool) -> Self { Self { pool } }
    pub fn pool(&self) -> &SqlitePool { &self.pool }
}
```

All database functions are methods on `Repository`, defined in `impl Repository` blocks spread across the repo modules. There is no object instance with mutable state — the pool handles connection management internally.

Compare to patterns you know:

- **Java/Spring**: this is the DAO layer. Each `repo/` module is a DAO class. The key difference: there are no singleton beans, no `@Repository` annotation, no dependency injection container — just methods on a struct.
- **Django**: equivalent to Manager methods on a model (`Item.objects.filter(...)`), but in a separate layer instead of on the model class.
- **Laravel**: equivalent to a Repository class that the service layer injects.

---

## A typical query

```rust
// From crates/tack-db/src/repo/items.rs

pub async fn get_item(&self, id: Uuid) -> Result<Option<Item>, sqlx::Error> {
    let row = sqlx::query_as::<_, ItemRow>(
        "SELECT id, project_id, parent_id, title, description, item_type,
                status, priority, estimate, estimate_unit, tags, sort_order,
                sprint_id, assignee, due_date, started_at, completed_at,
                created_at, updated_at
         FROM items WHERE id = ?"
    )
    .bind(id.to_string())
    .fetch_optional(self.pool())
    .await?;

    Ok(row.map(|r| r.into_item()))
}
```

Breaking this down:

- `sqlx::query_as::<_, ItemRow>(sql)` — execute SQL and map each row to `ItemRow` (a raw DB struct with string fields)
- `.bind(id.to_string())` — bind the `?` placeholder; UUIDs are stored as TEXT in SQLite, so `.to_string()` converts before binding
- `fetch_optional` — returns `Option<ItemRow>`: `Some(row)` if found, `None` if not, `Err` if the query itself fails
- `.await?` — await the async operation; `?` propagates `sqlx::Error` up to the caller
- `.map(|r| r.into_item())` — convert the raw DB row into the domain `Item` struct

The three fetch methods:

| Method | Returns | Use when |
|--------|---------|---------|
| `fetch_all` | `Vec<T>` | Listing queries |
| `fetch_one` | `T` | Exactly one row expected; errors if missing |
| `fetch_optional` | `Option<T>` | Zero or one row; missing is a valid state |

**UUIDs as TEXT**: SQLite has no native UUID type. Tack stores them as TEXT (`id.to_string()` on write, `Uuid::parse_str(&row.id)` on read). The conversion is handled in the `ItemRow::into_item()` method that converts raw row strings into typed domain structs.

---

## Dynamic query building

Some queries build SQL dynamically based on optional filters. The `list_items` query is a good example:

```rust
pub async fn list_items(
    &self,
    project_id: Uuid,
    filter: &ItemFilter,
) -> Result<Vec<Item>, sqlx::Error> {
    let mut query = String::from(
        "SELECT ... FROM items WHERE project_id = ?"
    );
    let mut binds: Vec<String> = vec![project_id.to_string()];

    if let Some(ref status) = filter.status {
        query.push_str(" AND status = ?");
        binds.push(status.clone());
    }
    if let Some(ref priority) = filter.priority {
        query.push_str(" AND priority = ?");
        binds.push(priority.to_string());
    }
    // ... more optional filters ...

    let per_page = filter.per_page.unwrap_or(100).min(500) as i64;
    let page = filter.page.unwrap_or(1).max(1) as i64;
    let offset = (page - 1) * per_page;
    query.push_str(&format!(" LIMIT {per_page} OFFSET {offset}"));

    let mut q = sqlx::query_as::<_, ItemRow>(&query);
    for bind in &binds {
        q = q.bind(bind);
    }
    let rows = q.fetch_all(self.pool()).await?;
    Ok(rows.into_iter().map(|r| r.into_item()).collect())
}
```

This dynamic approach is necessary because `sqlx`'s compile-time checking only works for static string literals. For dynamic queries you build the SQL string at runtime and fall back to runtime checking. The tradeoff is acceptable here — the base SQL is always fixed; only the `WHERE` clauses vary.

---

## Migrations

Tack has 17 migrations, embedded as constant string arrays in `crates/tack-db/src/migrations.rs`. They run automatically on server startup via `migrations::run_all(&pool)`.

```rust
// From crates/tack-db/src/migrations.rs

pub async fn run_all(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // Create the tracking table if it does not exist
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS _migrations (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        )"
    )
    .execute(pool).await?;

    let migrations: Vec<(&str, &[&str])> = vec![
        ("001_workspaces",       &MIGRATION_001[..]),
        ("002_projects",         &MIGRATION_002[..]),
        ("003_sprints",          &MIGRATION_003_SPRINTS[..]),
        ("004_items",            &MIGRATION_004_ITEMS[..]),
        ("005_dependencies",     &MIGRATION_005[..]),
        // ...
        ("010_fts",              &MIGRATION_010[..]),  // FTS5 virtual table + triggers
        // ...
        ("016_perf_indexes",     &MIGRATION_016[..]),
    ];

    for (name, statements) in migrations {
        let already_applied = /* check _migrations table */;
        if !already_applied {
            for statement in statements {
                sqlx::query(statement).execute(pool).await?;
            }
            // Record as applied
        }
    }
}
```

This is exactly the same concept as:
- Django's `python manage.py migrate` (tracks in `django_migrations`)
- Flyway / Liquibase (tracks in `flyway_schema_history`)
- Knex / node-pg-migrate (tracks in `knex_migrations`)

The tracking table is `_migrations` (not `__django_migrations`, but same idea). If a migration name is in the table, it is skipped. This makes startup idempotent — safe to call on every startup.

Each migration is an array of SQL statements because SQLite does not support multi-statement strings in all contexts. Migration 010 creates the FTS5 full-text search table and three triggers:

```sql
-- items_fts is a virtual table — SQLite handles the inverted index internally
CREATE VIRTUAL TABLE IF NOT EXISTS items_fts
USING fts5(title, description, tags, content='items', content_rowid='rowid');

-- Triggers keep the FTS index in sync with the items table
CREATE TRIGGER IF NOT EXISTS items_fts_insert
AFTER INSERT ON items BEGIN
  INSERT INTO items_fts(rowid, title, description, tags)
  VALUES (new.rowid, new.title, new.description, new.tags);
END;
```

The FTS search query that uses this:

```rust
// From repo/items.rs
"SELECT i.* FROM items i
 JOIN items_fts fts ON i.rowid = fts.rowid
 WHERE i.project_id = ? AND items_fts MATCH ?
 ORDER BY rank
 LIMIT 50"
```

`MATCH` is FTS5 syntax; `rank` orders by relevance. This is pure SQLite FTS5 — no search library needed.

---

## JSON fields in SQLite

Some columns store structured data as JSON text. The main cases in Tack:

- `projects.workflow` — a `WorkflowConfig` struct, serialized to JSON
- `projects.vocabulary` — a `HashMap<String, String>`, serialized to JSON
- `items.tags` — a `Vec<String>`, serialized to JSON

SQLite stores these as `TEXT`. sqlx reads them back as `String`, and the row conversion methods deserialize them:

```rust
// In ItemRow::into_item():
let tags: Vec<String> = serde_json::from_str(&row.tags).unwrap_or_default();
```

For projects, the workflow and vocabulary are deserialized from the `TEXT` column into typed structs:

```rust
let workflow: WorkflowConfig = serde_json::from_str(&row.workflow)
    .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
```

If the JSON is malformed (e.g. a migration corrupted it), this returns a `sqlx::Error::Decode` — surfaced as a 500 from the API. In practice this should not happen because writes go through `serde_json::to_string` which is infallible for well-typed structs.

---

## Auto-complete: check_and_update_parent_status

When an item moves to a Done status, Tack automatically checks if all siblings are also done. If so, the parent is updated to Done too. This logic lives in the data layer because it requires querying sibling state:

```rust
// From repo/items.rs

pub async fn siblings_all_done(
    &self,
    parent_id: Uuid,
    done_status: &str,
) -> Result<bool, sqlx::Error> {
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM items WHERE parent_id = ?"
    )
    .bind(parent_id.to_string())
    .fetch_one(self.pool()).await?;

    if total == 0 { return Ok(false); }

    let not_done: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM items WHERE parent_id = ? AND status != ?"
    )
    .bind(parent_id.to_string())
    .bind(done_status)
    .fetch_one(self.pool()).await?;

    Ok(not_done == 0)
}
```

The handler in `items.rs` calls this after every successful status update:

```rust
// After update_item succeeds:
if let Some(parent_id) = item.parent_id
    && item.status != old_status
    && proj.workflow.is_done_status(&item.status)
    && let Ok(all_done) = state.repo.siblings_all_done(parent_id, done_status).await
    && all_done
{
    let _ = state.repo.check_and_update_parent_status(parent_id, done_status).await;
}
```

The `let _` and the fact that errors are ignored (`let _ = ...`) is intentional — this is a best-effort feature. If the parent update fails for any reason, the primary operation (the child's status change) still succeeds. That is the correct tradeoff for an auto-complete feature.

---

## Testing with in-memory SQLite

Because every repository function takes a pool as input (via `&self` where `self` contains the pool), you can swap in an in-memory SQLite database for tests:

```rust
// In test code:
let pool = sqlx::SqlitePool::connect("sqlite::memory:").await?;
migrations::run_all(&pool).await?;
let repo = Repository::new(pool);

// Now use repo exactly as production code does — no mocking needed
let item = repo.create_item(project_id, "todo", input).await?;
assert_eq!(item.title, "My test item");
```

There is no mocking framework involved. The in-memory database runs migrations, creates real tables, and the repository code runs against it unchanged. This makes integration tests both simple and high-confidence.
