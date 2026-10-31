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

// sqlx (Rust) — you write the SQL; sqlx maps the rows to your type:
let rows = sqlx::query_as::<_, ItemRow>(
    "SELECT id, project_id, title, status FROM items WHERE project_id = ?"
)
.bind(project_id.to_string())
.fetch_all(pool)
.await?;
```

sqlx can check queries against a real database at compile time (its `query!` macros). Tack does not use them: its queries are plain strings run through `query`/`query_as`, so a wrong column name or a wrong type shows up when the query runs, not when the code compiles. That is why the repository tests run against a real in-memory database (see below).

This comes at a cost: you write more SQL. The payoff: you have full SQL power — CTEs, `WITH RECURSIVE`, `json_each`, FTS5 `MATCH`, window functions — without fighting an ORM's abstraction layer.

---

## The Repository pattern

All database operations live in `crates/tack-db`, the only crate that names `sqlx`. The structure:

```
crates/tack-db/src/
├── lib.rs           # init_pool; re-exports Repository and StoreError
├── error.rs         # StoreError
├── decode.rs        # reads a stored id, instant, enum or JSON value back, or fails with StoreError::Corrupt
├── migrations.rs    # the migrations, embedded as strings
├── repo.rs          # declares the Repository struct
├── repo/            # commands: one file or directory per aggregate
│   ├── items.rs
│   ├── projects.rs
│   ├── sprints.rs
│   ├── execution/
│   └── ...
└── queries/         # read models: one file per table
```

The `Repository` struct is a thin wrapper around `SqlitePool`:

```rust
pub struct Repository {
    pool: SqlitePool,
}
```

All commands are methods on `Repository`, defined in `impl Repository` blocks spread across the `repo/` modules. There is no object instance with mutable state — the pool handles connection management internally. The pool is reachable from outside the crate only through `Repository::pool()` behind the `raw-pool` feature, which `tack-test-support` alone enables. Every method fails with `StoreError` (`NotFound`, `Conflict`, `Corrupt` or `Sql`), so no `sqlx` type crosses the crate's edge.

Compare to patterns you know:

- **Java/Spring**: this is the DAO layer. Each `repo/` module is a DAO class. The key difference: there are no singleton beans, no `@Repository` annotation, no dependency injection container — just methods on a struct.
- **Django**: equivalent to Manager methods on a model (`Item.objects.filter(...)`), but in a separate layer instead of on the model class.
- **Laravel**: equivalent to a Repository class that the service layer injects.

---

## A typical query

```rust
// From crates/tack-db/src/repo/items.rs

pub async fn get_item(&self, id: Uuid) -> Result<Option<Item>, StoreError> {
    let columns = item_columns("");
    sqlx::query_as::<_, ItemRow>(AssertSqlSafe(format!(
        "SELECT {columns} FROM items WHERE id = ?"
    )))
    .bind(id.to_string())
    .fetch_optional(self.pool())
    .await?
    .map(ItemRow::into_item)
    .transpose()
}
```

Breaking this down:

- `sqlx::query_as::<_, ItemRow>(sql)` — execute SQL and map each row to `ItemRow` (a raw DB struct with string fields)
- `.bind(id.to_string())` — bind the `?` placeholder; UUIDs are stored as TEXT in SQLite, so `.to_string()` converts before binding
- `fetch_optional` — returns `Option<ItemRow>`: `Some(row)` if found, `None` if not, `Err` if the query itself fails
- `.await?` — await the async operation; `?` turns a `sqlx::Error` into `StoreError::Sql` and returns it to the caller
- `.map(ItemRow::into_item).transpose()` — convert the raw DB row into the domain `Item` struct; the conversion can fail, so the `Option<Result<..>>` is flipped into a `Result<Option<..>>`
- `item_columns("")` — the column list is written once (`ITEM_COLUMNS`), and `AssertSqlSafe` marks a query string built from such a constant

The three fetch methods:

| Method | Returns | Use when |
|--------|---------|---------|
| `fetch_all` | `Vec<T>` | Listing queries |
| `fetch_one` | `T` | Exactly one row expected; errors if missing |
| `fetch_optional` | `Option<T>` | Zero or one row; missing is a valid state |

**UUIDs as TEXT**: SQLite has no native UUID type. Tack stores them as TEXT (`id.to_string()` on write, `decode::uuid(table, column, &text)` on read). `ItemRow::into_item()` converts raw row strings into typed domain structs through `decode`, so a stored value that cannot be read becomes `StoreError::Corrupt { table, column }` instead of a panic or a silent default.

---

## Dynamic query building

Some queries build SQL dynamically based on optional filters. The `list_items` query is a good example:

```rust
pub async fn list_items(
    &self,
    project_id: Uuid,
    filter: &ItemFilter,
) -> Result<Vec<ItemCard>, StoreError> {
    // The WHERE clause and its bound values come from the filter's optional fields.
    let (where_clause, binds) = item_filter_clause(project_id, filter);
    let columns = card_columns();
    let mut query =
        format!("SELECT {columns} FROM items{where_clause} ORDER BY sort_order ASC");

    let per_page = filter.effective_per_page() as i64;
    let page = filter.effective_page() as i64;
    query.push_str(&format!(" LIMIT {per_page} OFFSET {}", (page - 1) * per_page));

    let mut q = sqlx::query_as::<_, ItemCardRow>(AssertSqlSafe(query));
    for bind in &binds {
        q = q.bind(bind);
    }
    let rows = q.fetch_all(self.pool()).await?;
    rows.into_iter().map(ItemCardRow::into_card).collect()
}
```

The SQL text is assembled at runtime. Only fixed fragments are concatenated into it (the column list, the `WHERE` clauses, two integers); every value from a caller is bound with `?`. The base SQL is always fixed and only the `WHERE` clauses vary.

---

## Migrations

Tack's migrations are embedded as constant string arrays in `crates/tack-db/src/migrations.rs`. They run automatically on server startup via `migrations::run_all(&pool)`.

```rust
// From crates/tack-db/src/migrations.rs

pub async fn run_all(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    ensure_migrations_table(pool).await?;
    let migrations = all_migrations();                  // the ordered list, each with a name and its statements
    verify_applied_migration_invariant(pool, &migrations).await?;   // the applied history matches by name and checksum
    create_pre_upgrade_backup_if_needed(pool, &migrations).await?; // VACUUM INTO before a table rebuild or drop
    apply_migrations(pool, &migrations).await?;        // each pending one in its own transaction
    Ok(())
}

fn all_migrations() -> Vec<Migration> {
    vec![
        ordinary("001_workspaces", &MIGRATION_001[..]),
        ordinary("002_projects",   &MIGRATION_002[..]),
        // ...
        ordinary("010_fts",        &MIGRATION_010[..]),  // FTS5 virtual table + triggers
        // ...
    ]
}
```

This is exactly the same concept as:
- Django's `python manage.py migrate` (tracks in `django_migrations`)
- Flyway / Liquibase (tracks in `flyway_schema_history`)
- Knex / node-pg-migrate (tracks in `knex_migrations`)

The tracking table is `_migrations` (not `__django_migrations`, but same idea). If a migration name is in the table, it is skipped. This makes startup idempotent — safe to call on every startup. Unlike most of these tools, Tack also refuses to start when the applied history differs from the binary's list by name, order or checksum, so a shipped migration is never edited.

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
tags: decode::json("items", "tags", &self.tags)?,
```

For projects, the workflow and vocabulary are deserialized from the `TEXT` column into typed structs:

```rust
let workflow: WorkflowConfig = decode::json("projects", "workflow", &row.workflow)?;
```

If the JSON is malformed (e.g. a migration corrupted it), this returns `StoreError::Corrupt { table, column }`, which is logged with the table and column (never the value) and surfaced as a 500 from the API. In practice this should not happen because writes go through `serde_json::to_string`, which cannot fail for well-typed structs.

---

## Auto-complete: check_and_update_parent_status

When an item moves to a Done status, Tack automatically checks if all siblings are also done. If so, the parent is updated to Done too. This logic lives in the data layer because it requires querying sibling state:

```rust
// From repo/items.rs

pub async fn siblings_all_done(
    &self,
    parent_id: Uuid,
    done_status: &str,
) -> Result<bool, StoreError> {
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

Because every repository method reads its pool from `self`, a test can hand it an in-memory SQLite database. `tack-test-support` does that in one call:

```rust
// In test code:
let repo = tack_test_support::setup_test_db().await;   // in-memory pool, migrations run

// Make the rows the test needs, then call the code under test:
let workspace_id = tack_test_support::create_test_workspace(&repo).await;
let project = tack_test_support::make_project(&repo, workspace_id).await;
```

There is no mocking framework involved. The in-memory database runs the real migrations, creates real tables, and the repository code runs against it unchanged. This makes integration tests both simple and high-confidence. The helper's current functions are in `crates/tack-test-support/src/`; [Testing](../testing.md) says where each kind of test belongs.
