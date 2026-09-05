//! Schema-layer tests: the `orch_*` migrations and the additive columns that
//! ride alongside them (`orch_migrations`, `orch_metrics`,
//! `item_source_migration`), plus the reconciliation sweep that keeps a
//! control plane's rows from sticking in a non-terminal state forever
//! (`stale_reconcile`).

mod common;

#[path = "migrations/item_source_migration.rs"]
mod item_source_migration;
#[path = "migrations/orch_metrics.rs"]
mod orch_metrics;
#[path = "migrations/orch_migrations.rs"]
mod orch_migrations;
#[path = "migrations/stale_reconcile.rs"]
mod stale_reconcile;
