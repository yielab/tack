use sqlx::SqlitePool;
use tracing::{info, instrument};

/// Run all migrations in order.
#[instrument(skip(pool))]
pub async fn run_all(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    info!("Running database migrations...");

    // Create migrations tracking table
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS _migrations (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        )"
    )
    .execute(pool)
    .await?;

    // Order matters: sprints before items (items references sprints)
    let migrations: Vec<(&str, &[&str])> = vec![
        ("001_workspaces", &MIGRATION_001[..]),
        ("002_projects", &MIGRATION_002[..]),
        ("003_sprints", &MIGRATION_003_SPRINTS[..]),
        ("004_items", &MIGRATION_004_ITEMS[..]),
        ("005_dependencies", &MIGRATION_005[..]),
        ("006_roles", &MIGRATION_006[..]),
        ("007_comments", &MIGRATION_007[..]),
        ("008_attachments", &MIGRATION_008[..]),
        ("009_board_views", &MIGRATION_009[..]),
        ("010_fts", &MIGRATION_010[..]),
        ("011_project_templates", &MIGRATION_011[..]),
        ("012_custom_fields", &MIGRATION_012[..]),
        ("013_boards", &MIGRATION_013[..]),
    ];

    for (name, statements) in migrations {
        let already_applied: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM _migrations WHERE name = ?)"
        )
        .bind(name)
        .fetch_one(pool)
        .await?;

        if !already_applied {
            info!(migration = name, "Applying migration");
            for statement in statements {
                sqlx::query(statement)
                    .execute(pool)
                    .await
                    .map_err(|e| {
                        tracing::error!(migration = name, statement, error = %e, "Migration failed");
                        e
                    })?;
            }
            sqlx::query("INSERT INTO _migrations (name) VALUES (?)")
                .bind(name)
                .execute(pool)
                .await?;
            info!(migration = name, "Migration applied successfully");
        }
    }

    info!("All migrations applied");
    Ok(())
}

// Each migration is an array of individual SQL statements.

const MIGRATION_001: [&str; 1] = [
    "CREATE TABLE IF NOT EXISTS workspaces (
        id TEXT PRIMARY KEY NOT NULL,
        name TEXT NOT NULL,
        description TEXT,
        default_vocabulary TEXT NOT NULL DEFAULT '{}',
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
];

const MIGRATION_002: [&str; 3] = [
    "CREATE TABLE IF NOT EXISTS projects (
        id TEXT PRIMARY KEY NOT NULL,
        workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        description TEXT,
        project_type TEXT NOT NULL DEFAULT 'software',
        vocabulary TEXT NOT NULL DEFAULT '{}',
        workflow TEXT NOT NULL DEFAULT '{}',
        archived INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_projects_workspace ON projects(workspace_id)",
    "CREATE INDEX IF NOT EXISTS idx_projects_archived ON projects(archived)",
];

const MIGRATION_003_SPRINTS: [&str; 3] = [
    "CREATE TABLE IF NOT EXISTS sprints (
        id TEXT PRIMARY KEY NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        goal TEXT,
        start_date TEXT,
        end_date TEXT,
        status TEXT NOT NULL DEFAULT 'planning',
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_sprints_project ON sprints(project_id)",
    "CREATE INDEX IF NOT EXISTS idx_sprints_status ON sprints(project_id, status)",
];

const MIGRATION_004_ITEMS: [&str; 7] = [
    "CREATE TABLE IF NOT EXISTS items (
        id TEXT PRIMARY KEY NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        parent_id TEXT REFERENCES items(id) ON DELETE SET NULL,
        title TEXT NOT NULL,
        description TEXT,
        item_type TEXT NOT NULL DEFAULT 'task',
        status TEXT NOT NULL,
        priority TEXT NOT NULL DEFAULT 'medium',
        estimate REAL,
        estimate_unit TEXT NOT NULL DEFAULT 'story_points',
        tags TEXT NOT NULL DEFAULT '[]',
        sort_order INTEGER NOT NULL DEFAULT 0,
        sprint_id TEXT REFERENCES sprints(id) ON DELETE SET NULL,
        due_date TEXT,
        started_at TEXT,
        completed_at TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_items_project ON items(project_id)",
    "CREATE INDEX IF NOT EXISTS idx_items_parent ON items(parent_id)",
    "CREATE INDEX IF NOT EXISTS idx_items_status ON items(project_id, status)",
    "CREATE INDEX IF NOT EXISTS idx_items_type ON items(project_id, item_type)",
    "CREATE INDEX IF NOT EXISTS idx_items_priority ON items(project_id, priority)",
    "CREATE INDEX IF NOT EXISTS idx_items_sort ON items(project_id, sort_order)",
];

const MIGRATION_005: [&str; 3] = [
    "CREATE TABLE IF NOT EXISTS dependencies (
        id TEXT PRIMARY KEY NOT NULL,
        source_item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
        target_item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
        dependency_type TEXT NOT NULL DEFAULT 'blocks',
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        UNIQUE(source_item_id, target_item_id, dependency_type)
    )",
    "CREATE INDEX IF NOT EXISTS idx_deps_source ON dependencies(source_item_id)",
    "CREATE INDEX IF NOT EXISTS idx_deps_target ON dependencies(target_item_id)",
];

const MIGRATION_006: [&str; 3] = [
    "CREATE TABLE IF NOT EXISTS roles (
        id TEXT PRIMARY KEY NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        color TEXT NOT NULL DEFAULT '#6366f1',
        icon TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_roles_project ON roles(project_id)",
    "CREATE TABLE IF NOT EXISTS item_roles (
        item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
        role_id TEXT NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
        PRIMARY KEY (item_id, role_id)
    )",
];

const MIGRATION_007: [&str; 2] = [
    "CREATE TABLE IF NOT EXISTS comments (
        id TEXT PRIMARY KEY NOT NULL,
        item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
        author TEXT,
        content TEXT NOT NULL,
        comment_type TEXT NOT NULL DEFAULT 'comment',
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_comments_item ON comments(item_id)",
];

const MIGRATION_008: [&str; 2] = [
    "CREATE TABLE IF NOT EXISTS attachments (
        id TEXT PRIMARY KEY NOT NULL,
        item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
        filename TEXT NOT NULL,
        mime_type TEXT NOT NULL,
        storage_path TEXT NOT NULL,
        size_bytes INTEGER NOT NULL DEFAULT 0,
        uploaded_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_attachments_item ON attachments(item_id)",
];

const MIGRATION_009: [&str; 2] = [
    "CREATE TABLE IF NOT EXISTS board_views (
        id TEXT PRIMARY KEY NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        columns TEXT NOT NULL DEFAULT '[]',
        filters TEXT,
        grouping TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_board_views_project ON board_views(project_id)",
];

const MIGRATION_010: [&str; 4] = [
    "CREATE VIRTUAL TABLE IF NOT EXISTS items_fts USING fts5(
        title,
        description,
        tags,
        content='items',
        content_rowid='rowid'
    )",
    "CREATE TRIGGER IF NOT EXISTS items_fts_insert AFTER INSERT ON items BEGIN
        INSERT INTO items_fts(rowid, title, description, tags)
        VALUES (new.rowid, new.title, COALESCE(new.description, ''), new.tags);
    END",
    "CREATE TRIGGER IF NOT EXISTS items_fts_delete AFTER DELETE ON items BEGIN
        INSERT INTO items_fts(items_fts, rowid, title, description, tags)
        VALUES ('delete', old.rowid, old.title, COALESCE(old.description, ''), old.tags);
    END",
    "CREATE TRIGGER IF NOT EXISTS items_fts_update AFTER UPDATE ON items BEGIN
        INSERT INTO items_fts(items_fts, rowid, title, description, tags)
        VALUES ('delete', old.rowid, old.title, COALESCE(old.description, ''), old.tags);
        INSERT INTO items_fts(rowid, title, description, tags)
        VALUES (new.rowid, new.title, COALESCE(new.description, ''), new.tags);
    END",
];

const MIGRATION_011: [&str; 2] = [
    "CREATE TABLE IF NOT EXISTS project_templates (
        id TEXT PRIMARY KEY NOT NULL,
        name TEXT NOT NULL,
        description TEXT,
        project_type TEXT NOT NULL DEFAULT 'software',
        vocabulary TEXT NOT NULL DEFAULT '{}',
        workflow TEXT NOT NULL DEFAULT '{}',
        custom_fields TEXT NOT NULL DEFAULT '[]',
        default_boards TEXT NOT NULL DEFAULT '[]',
        is_builtin INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_templates_type ON project_templates(project_type)",
];

const MIGRATION_012: [&str; 4] = [
    "CREATE TABLE IF NOT EXISTS custom_field_definitions (
        id TEXT PRIMARY KEY NOT NULL,
        project_id TEXT REFERENCES projects(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        field_type TEXT NOT NULL,
        description TEXT,
        required INTEGER NOT NULL DEFAULT 0,
        default_value TEXT,
        options TEXT,
        validation TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_custom_fields_project ON custom_field_definitions(project_id)",
    "CREATE TABLE IF NOT EXISTS custom_field_values (
        id TEXT PRIMARY KEY NOT NULL,
        item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
        field_id TEXT NOT NULL REFERENCES custom_field_definitions(id) ON DELETE CASCADE,
        value TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now')),
        UNIQUE(item_id, field_id)
    )",
    "CREATE INDEX IF NOT EXISTS idx_custom_values_item ON custom_field_values(item_id)",
];

const MIGRATION_013: [&str; 3] = [
    "CREATE TABLE IF NOT EXISTS boards (
        id TEXT PRIMARY KEY NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        description TEXT,
        filters TEXT,
        grouping TEXT,
        is_default INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_boards_project ON boards(project_id)",
    "CREATE INDEX IF NOT EXISTS idx_boards_default ON boards(project_id, is_default)",
];
