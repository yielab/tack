use sqlx::SqlitePool;
use tracing::{info, instrument};

/// The full, ordered migration list. Order matters: e.g. sprints before items
/// (items references sprints), and control_planes before the orch_* tables that
/// reference it.
fn all_migrations() -> Vec<(&'static str, &'static [&'static str])> {
    vec![
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
        ("014_consolidate_boards", &MIGRATION_014[..]),
        ("015_item_assignee", &MIGRATION_015[..]),
        ("016_perf_indexes", &MIGRATION_016[..]),
        ("017_app_meta", &MIGRATION_017[..]),
        ("018_github_links", &MIGRATION_018[..]),
        ("019_control_planes", &MIGRATION_019[..]),
        ("020_orch_links", &MIGRATION_020[..]),
        ("021_orch_tasks", &MIGRATION_021[..]),
        ("022_orch_runs", &MIGRATION_022[..]),
        ("023_orch_events", &MIGRATION_023[..]),
        ("024_orch_approvals", &MIGRATION_024[..]),
        ("025_orch_metrics", &MIGRATION_025[..]),
        ("026_orch_events_daily", &MIGRATION_026[..]),
        ("027_orch_metrics_daily", &MIGRATION_027[..]),
        ("028_orch_trace_cursors", &MIGRATION_028[..]),
        ("029_item_source", &MIGRATION_029[..]),
        ("030_template_orchestration", &MIGRATION_030[..]),
        ("031_items_completed_at_index", &MIGRATION_031[..]),
    ]
}

/// Run all migrations in order.
#[instrument(skip(pool))]
pub async fn run_all(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    info!("Running database migrations...");
    ensure_migrations_table(pool).await?;
    apply_migrations(pool, &all_migrations()).await?;
    info!("All migrations applied");
    Ok(())
}

/// Apply migrations only up to and including `cutoff` (by name). Exists so tests can
/// simulate an existing database stopped at a known migration boundary (e.g. an
/// installed `tack.db` still on "018_github_links") and then call [`run_all`] again to
/// prove the upgrade-in-place path works, without needing a real on-disk fixture file.
///
/// Panics if `cutoff` does not match a known migration name.
pub async fn run_up_to(pool: &SqlitePool, cutoff: &str) -> Result<(), sqlx::Error> {
    ensure_migrations_table(pool).await?;
    let migrations = all_migrations();
    let idx = migrations
        .iter()
        .position(|(name, _)| *name == cutoff)
        .unwrap_or_else(|| panic!("run_up_to: unknown migration name {cutoff:?}"));
    apply_migrations(pool, &migrations[..=idx]).await
}

async fn ensure_migrations_table(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS _migrations (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn apply_migrations(
    pool: &SqlitePool,
    migrations: &[(&str, &[&str])],
) -> Result<(), sqlx::Error> {
    for (name, statements) in migrations {
        let already_applied: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM _migrations WHERE name = ?)")
                .bind(*name)
                .fetch_one(pool)
                .await?;

        if !already_applied {
            info!(migration = *name, "Applying migration");
            for statement in *statements {
                sqlx::query(statement).execute(pool).await.map_err(|e| {
                    tracing::error!(migration = *name, statement, error = %e, "Migration failed");
                    e
                })?;
            }
            sqlx::query("INSERT INTO _migrations (name) VALUES (?)")
                .bind(*name)
                .execute(pool)
                .await?;
            info!(migration = *name, "Migration applied successfully");
        }
    }
    Ok(())
}

// Each migration is an array of individual SQL statements.

const MIGRATION_001: [&str; 1] = ["CREATE TABLE IF NOT EXISTS workspaces (
        id TEXT PRIMARY KEY NOT NULL,
        name TEXT NOT NULL,
        description TEXT,
        default_vocabulary TEXT NOT NULL DEFAULT '{}',
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )"];

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

// Backfill a Default Board for every project that has no boards yet, then retire board_views.
// The UUID-like id is generated from randomblob(16) — unique and compatible with how the app
// stores UUIDs as TEXT.
const MIGRATION_014: [&str; 2] = [
    "INSERT INTO boards (id, project_id, name, is_default, created_at, updated_at)
     SELECT
       lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(2))) || '-' || lower(hex(randomblob(6))),
       id,
       'Default Board',
       1,
       datetime('now'),
       datetime('now')
     FROM projects
     WHERE id NOT IN (SELECT DISTINCT project_id FROM boards)",
    "DROP TABLE IF EXISTS board_views",
];

const MIGRATION_015: [&str; 2] = [
    "ALTER TABLE items ADD COLUMN assignee TEXT",
    "CREATE INDEX IF NOT EXISTS idx_items_assignee ON items(project_id, assignee)",
];

const MIGRATION_016: [&str; 1] = [
    // Sprint-grouping board view filters items by (project_id, sprint_id); no composite index existed.
    "CREATE INDEX IF NOT EXISTS idx_items_sprint ON items(project_id, sprint_id)",
];

const MIGRATION_017: [&str; 1] = [
    // Generic key/value store for app-level settings (e.g. install_id, cloud-backup
    // config edited from the UI). Created IF NOT EXISTS because install_id may have
    // lazily created it on an older install.
    "CREATE TABLE IF NOT EXISTS app_meta (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL)",
];

const MIGRATION_018: [&str; 1] = [
    // Links a Tack item to a GitHub issue for push-only status sync.
    // One issue per item; removed automatically when the item is deleted.
    "CREATE TABLE IF NOT EXISTS github_links (
        item_id TEXT PRIMARY KEY NOT NULL REFERENCES items(id) ON DELETE CASCADE,
        repo TEXT NOT NULL,
        issue_number INTEGER NOT NULL,
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
];

// ─── Agent-Factory Control Center (Phases 33–38) ──────────────────────────
//
// Schema for the tack-orch integration: Tack holds desired state, an external
// "control plane" (docket) executes agent work, and a reconciler mirrors progress
// back through these tables. See docs/book/src/roadmap.md → "Next — Agent-Factory
// Control Center" for the full design; that section's schema table is authoritative
// for these six migrations.
//
// Every FK to items/projects/control_planes is ON DELETE CASCADE, matching the
// github_links precedent: deleting the Tack-side row must not leave orphaned
// orchestration rows behind. Timestamps are TEXT RFC3339 (via datetime('now') at
// insert time, same as the rest of this file); all IDs are TEXT UUIDs.

const MIGRATION_019: [&str; 1] = [
    // One row per registered control plane (currently always "docket", but the
    // ControlPlane trait is written to allow other kinds later). `token` is the
    // docket Bearer credential: nullable, and write-only over the API (never
    // returned to clients — same discipline as the S3 backup secret key).
    // `health` is the reconciler's state machine: healthy | degraded | unreachable.
    "CREATE TABLE IF NOT EXISTS control_planes (
        id TEXT PRIMARY KEY NOT NULL,
        name TEXT NOT NULL,
        kind TEXT NOT NULL DEFAULT 'docket',
        base_url TEXT NOT NULL,
        token TEXT,
        api_version TEXT,
        health TEXT NOT NULL DEFAULT 'unknown',
        last_seen_at TEXT,
        consecutive_failures INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
];

const MIGRATION_020: [&str; 2] = [
    // Links a Tack project to a pod on a control plane — one link per project
    // (mirrors the one-issue-per-item shape of github_links). `status_map` is a JSON
    // object (validated against the project's WorkflowConfig at save time, in the API
    // layer, not here) mapping remote run states to Tack statuses. `budget_usd` is a
    // user-set cap, not a computed spend figure, so it does not need the
    // "_estimated" suffix reserved for derived cost numbers.
    "CREATE TABLE IF NOT EXISTS orch_links (
        project_id TEXT PRIMARY KEY NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        control_plane_id TEXT NOT NULL REFERENCES control_planes(id) ON DELETE CASCADE,
        remote_project TEXT NOT NULL,
        pipeline_file TEXT,
        blueprint TEXT,
        auto_dispatch INTEGER NOT NULL DEFAULT 0,
        budget_usd REAL,
        status_map TEXT NOT NULL DEFAULT '{}',
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_orch_links_control_plane ON orch_links(control_plane_id)",
];

const MIGRATION_021: [&str; 2] = [
    // item ↔ docket task. PK is (item_id, remote_task_id) rather than a single-column
    // key: an item can be redispatched (retries, reruns), and each dispatch gets its
    // own task row rather than overwriting the last one. `remote_run_id` correlates to
    // orch_runs.run_id but is deliberately not a hard FK — orch_runs is populated by a
    // separate poll step and a task may briefly (or permanently, pre-Phase-35) have no
    // known run. Token counts are the primary measure; cost_usd_estimated is derived
    // and named accordingly per the "never present an estimate as spend" rule.
    "CREATE TABLE IF NOT EXISTS orch_tasks (
        item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
        remote_task_id TEXT NOT NULL,
        remote_run_id TEXT,
        remote_status TEXT NOT NULL DEFAULT 'pending',
        attempt INTEGER NOT NULL DEFAULT 1,
        tokens_in INTEGER NOT NULL DEFAULT 0,
        tokens_out INTEGER NOT NULL DEFAULT 0,
        cost_usd_estimated REAL,
        dispatched_at TEXT NOT NULL DEFAULT (datetime('now')),
        trusted INTEGER NOT NULL DEFAULT 1,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now')),
        PRIMARY KEY (item_id, remote_task_id)
    )",
    "CREATE INDEX IF NOT EXISTS idx_orch_tasks_remote_run ON orch_tasks(remote_run_id)",
];

const MIGRATION_022: [&str; 2] = [
    // Mirror of docket's GET /runs. `item_id` is nullable: a run dispatched from the
    // docket CLI (not through Tack) mirrors "unattributed" until a matching orch_tasks
    // row correlates it — that is the normal case pre-Phase-35 and must not error.
    "CREATE TABLE IF NOT EXISTS orch_runs (
        run_id TEXT PRIMARY KEY NOT NULL,
        control_plane_id TEXT NOT NULL REFERENCES control_planes(id) ON DELETE CASCADE,
        item_id TEXT REFERENCES items(id) ON DELETE CASCADE,
        remote_project TEXT NOT NULL,
        source TEXT NOT NULL DEFAULT 'cli',
        state TEXT NOT NULL DEFAULT 'queued',
        started_at TEXT,
        ended_at TEXT,
        error TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_orch_runs_plane_state ON orch_runs(control_plane_id, state)",
];

const MIGRATION_023: [&str; 3] = [
    // Append-only telemetry: hops, tool calls, verdicts, rework, status_map_rejected,
    // etc. `event_type` stores docket's event type verbatim (including types Tack does
    // not yet recognise) so a docket upgrade degrades to "shown as-is" rather than a
    // dropped or errored row. Two indexes: one for an item's timeline, one for the
    // Phase 34.6 retention sweep (delete-by-age scans occurred_at across all items).
    "CREATE TABLE IF NOT EXISTS orch_events (
        id TEXT PRIMARY KEY NOT NULL,
        control_plane_id TEXT NOT NULL REFERENCES control_planes(id) ON DELETE CASCADE,
        item_id TEXT REFERENCES items(id) ON DELETE CASCADE,
        run_id TEXT,
        event_type TEXT NOT NULL,
        payload TEXT NOT NULL DEFAULT '{}',
        occurred_at TEXT NOT NULL DEFAULT (datetime('now')),
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_orch_events_item_occurred ON orch_events(item_id, occurred_at)",
    "CREATE INDEX IF NOT EXISTS idx_orch_events_occurred ON orch_events(occurred_at)",
];

const MIGRATION_024: [&str; 3] = [
    // Mirror of docket's GET /approvals. `token` (docket's approval token, not a
    // credential) is the natural PK. Correlated to an item via
    // record.context.taskId → orch_tasks.remote_task_id at ingest time in the
    // repository layer; `remote_task_id` is stored for that correlation but, like
    // orch_tasks.remote_run_id, is not a hard FK. Uncorrelated records keep
    // item_id = NULL and must still surface in the fleet-wide approvals inbox.
    "CREATE TABLE IF NOT EXISTS orch_approvals (
        token TEXT PRIMARY KEY NOT NULL,
        control_plane_id TEXT NOT NULL REFERENCES control_planes(id) ON DELETE CASCADE,
        item_id TEXT REFERENCES items(id) ON DELETE CASCADE,
        remote_task_id TEXT,
        agent TEXT,
        action TEXT,
        state TEXT NOT NULL DEFAULT 'pending',
        requested_at TEXT NOT NULL DEFAULT (datetime('now')),
        decided_at TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_orch_approvals_item ON orch_approvals(item_id)",
    "CREATE INDEX IF NOT EXISTS idx_orch_approvals_state ON orch_approvals(state)",
];

// ─── Phase 34 — metrics ingestion + retention (card B3, tasks 34.3/34.6/34.7) ─────
//
// `orch_metrics` was deliberately *not* part of the Wave 0 batch above: the roadmap's
// own schema table for migrations 019-024 lists six tables, not seven, and assigns
// `orch_metrics` to Phase 34 (see W0-B's TODO.md §6 handoff, which flagged this
// explicitly for whoever picked up metrics ingestion). It gets its own migration here,
// 025, plus two new rollup tables (026, 027) for the 34.6 retention sweep.

const MIGRATION_025: [&str; 4] = [
    // One row per scrape per metric per label set. Append-only time series: unlike
    // orch_tasks/orch_runs/orch_events/orch_approvals, a sample has no natural key
    // across scrapes — the same name+labels recorded at two different scrape times are
    // two distinct data points, not a correction of one another — so this table has no
    // ON CONFLICT upsert path (see repo/orch.rs's upsert_orch_metrics doc comment).
    // `labels` is a canonical (BTreeMap-key-sorted) JSON object so the same logical
    // label set always serializes identically; the retention rollup (026/027) and the
    // "latest sample per metric" query (GET /api/metrics) both depend on that.
    // `value` is nullable, not `REAL NOT NULL`: SQLite has no native NaN representation,
    // and sqlx's SQLite driver silently binds an `f64::NAN` value as SQL NULL (verified
    // directly — an `f64::NAN` bind against a `NOT NULL` column fails with a constraint
    // violation, while `+Inf`/`-Inf` bind and round-trip fine as ordinary REAL values).
    // The Prometheus parser (`adapters::prometheus::parse_value`) deliberately preserves
    // `NaN`/`Inf` rather than dropping them, so this column must accept whatever it hands
    // back rather than erroring the whole scrape over one exotic sample.
    "CREATE TABLE IF NOT EXISTS orch_metrics (
        id TEXT PRIMARY KEY NOT NULL,
        control_plane_id TEXT NOT NULL REFERENCES control_planes(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        labels TEXT NOT NULL DEFAULT '{}',
        value REAL,
        scraped_at TEXT NOT NULL DEFAULT (datetime('now')),
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    // Retention sweep scan (34.6): oldest-first, unscoped by plane.
    "CREATE INDEX IF NOT EXISTS idx_orch_metrics_scraped ON orch_metrics(scraped_at)",
    // Per-plane timeline / bounded-retention-batch queries.
    "CREATE INDEX IF NOT EXISTS idx_orch_metrics_plane_scraped ON orch_metrics(control_plane_id, scraped_at)",
    // "Latest sample per (plane, name, labels)" — GET /api/metrics's merge query.
    "CREATE INDEX IF NOT EXISTS idx_orch_metrics_plane_name_labels ON orch_metrics(control_plane_id, name, labels)",
];

const MIGRATION_026: [&str; 2] = [
    // Per-day rollup of orch_events, written by the retention sweep before the
    // corresponding raw rows are deleted (34.6). Grouped at (day, control_plane_id,
    // event_type) — deliberately NOT per-item: SQLite treats NULL as distinct from
    // every other NULL in a UNIQUE constraint, so a nullable item_id in the uniqueness
    // key would silently allow duplicate aggregate rows for uncorrelated events.
    // Per-item event history is expected to age out along with the raw rows once
    // retention passes; the day/plane/type total (what Phase 38's unit economics
    // needs) is what survives.
    "CREATE TABLE IF NOT EXISTS orch_events_daily (
        id TEXT PRIMARY KEY NOT NULL,
        day TEXT NOT NULL,
        control_plane_id TEXT NOT NULL REFERENCES control_planes(id) ON DELETE CASCADE,
        event_type TEXT NOT NULL,
        event_count INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now')),
        UNIQUE(day, control_plane_id, event_type)
    )",
    "CREATE INDEX IF NOT EXISTS idx_orch_events_daily_plane_day ON orch_events_daily(control_plane_id, day)",
];

const MIGRATION_027: [&str; 2] = [
    // Per-day rollup of orch_metrics: count/sum/min/max per (day, control_plane_id,
    // metric_name, labels), written before the corresponding raw scrape rows are
    // deleted (34.6). `labels` is non-nullable (default '{}'), so — unlike the item_id
    // concern on orch_events_daily above — including it in the UNIQUE key is safe:
    // SQLite's NULL-distinctness quirk only bites nullable columns.
    "CREATE TABLE IF NOT EXISTS orch_metrics_daily (
        id TEXT PRIMARY KEY NOT NULL,
        day TEXT NOT NULL,
        control_plane_id TEXT NOT NULL REFERENCES control_planes(id) ON DELETE CASCADE,
        metric_name TEXT NOT NULL,
        labels TEXT NOT NULL DEFAULT '{}',
        sample_count INTEGER NOT NULL DEFAULT 0,
        value_sum REAL NOT NULL DEFAULT 0,
        value_min REAL,
        value_max REAL,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now')),
        UNIQUE(day, control_plane_id, metric_name, labels)
    )",
    "CREATE INDEX IF NOT EXISTS idx_orch_metrics_daily_plane_day ON orch_metrics_daily(control_plane_id, day)",
];

// ─── Phase 34 — trace ingestion (card B2, task 34.4) ──────────────────────────────
//
// docket's `/traces/{project}?since=` cursor (`serve.py`'s `_traces_page`) is a
// compound `"<ts>Z:<n>"` token, not a bare timestamp or offset — see
// `crates/tack-orch/src/reconciler.rs`'s module doc for the full mechanics. It must
// resume correctly per *docket* project, independent of which (if any) Tack project
// is currently linked to it, so this is its own table keyed on
// `(control_plane_id, remote_project)` rather than a column bolted onto `orch_links`
// (whose PK is `project_id` — Tack's side of the link, which can be unlinked/relinked
// without docket's own trace history caring).

const MIGRATION_028: [&str; 1] = ["CREATE TABLE IF NOT EXISTS orch_trace_cursors (
        control_plane_id TEXT NOT NULL REFERENCES control_planes(id) ON DELETE CASCADE,
        remote_project TEXT NOT NULL,
        cursor TEXT NOT NULL,
        updated_at TEXT NOT NULL DEFAULT (datetime('now')),
        PRIMARY KEY (control_plane_id, remote_project)
    )"];

// ─── Item provenance / trust boundary (card C2, task 35.7) ────────────────────
//
// Tack imports items from GitHub Issues and Linear — text written by anyone who
// can file an issue on a linked repo. Once Phase 35's dispatcher (card C1) sends
// an item's title/description to docket as agent input, that text becomes
// instructions to an autonomous agent. `source` is a sticky, creation-time-only
// marker of where an item's text came from, read by the dispatcher
// (`tack-api::dispatcher::resolve_default_trust`, and the auto-dispatch hook in
// `handlers::items`) to decide whether to pass `trusted: true` or `trusted: false`
// to docket's `pre_input` policy gate. See `tack_core::models::ItemSource` for the
// full enum and the trust rule (`is_trusted()`).
//
// Default is deliberately `'unknown'`, not `'manual'`: every *existing* row in
// every existing install — including items imported via GitHub before this Phase
// 35 cycle even existed (migration 018 predates it) — backfills to `'unknown'`,
// which `ItemSource::is_trusted()` treats as untrusted. `'manual'` is a value only
// ever written explicitly, by the ordinary create-item code path, never assumed.
// This is the "unsafe state is never the accidental default" rule from this
// card's brief, applied literally: a NULL/legacy/unrecognised source resolves to
// "do not trust this text with operator privileges," not the reverse.
const MIGRATION_029: [&str; 1] =
    ["ALTER TABLE items ADD COLUMN source TEXT NOT NULL DEFAULT 'unknown'"];

// Phase 37 / card D3, task 37.1: `TemplateOrchestration` (tack-core). A
// nullable column, not `NOT NULL DEFAULT '{}'` — `NULL` means "this template
// has no orchestration block," distinct from `'{}'` ("an orchestration block
// with every field at its default"). Every row that predates this migration
// backfills to `NULL`, so `repo::templates::get_template`/`list_templates`
// deserialize it to `orchestration: None`, matching `ProjectTemplate`'s own
// `#[serde(default)]` — the same "absent means nothing, existing templates
// are untouched" rule migration 029 established for `items.source`, applied
// here to a column instead of a JSON payload key.
const MIGRATION_030: [&str; 1] = ["ALTER TABLE project_templates ADD COLUMN orchestration TEXT"];

// Phase 38 / card D5, task 38.1: unit economics. `GET /api/economics/summary` and
// `GET /api/economics/items` both start from "every item where completed_at IS NOT
// NULL" — a query the existing indexes don't cover: `idx_items_status` and friends are
// all `(project_id, ...)` composites, but this scan is deliberately NOT scoped to one
// project (it slices by `project_type`/`item_type` across the whole instance), so
// there is no leading `project_id` predicate for those indexes to serve. Partial
// (`WHERE completed_at IS NOT NULL`) because most items in a healthy board are not yet
// done, so the index only needs to cover the minority that are.
const MIGRATION_031: [&str; 1] = [
    "CREATE INDEX IF NOT EXISTS idx_items_completed_at ON items(completed_at) \
     WHERE completed_at IS NOT NULL",
];
