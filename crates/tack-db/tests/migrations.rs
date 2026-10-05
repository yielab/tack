//! Schema-layer tests: the additive columns riding alongside the item-source
//! backfill (`item_source_migration`) and the migrations that drop the
//! legacy Docket bridge's tables (`bridge_tables_dropped`), and the tables the
//! factory loop reads (`factory_loop_tables`).

mod common;

#[path = "migrations/bridge_tables_dropped.rs"]
mod bridge_tables_dropped;
#[path = "migrations/factory_loop_tables.rs"]
mod factory_loop_tables;
#[path = "migrations/item_source_migration.rs"]
mod item_source_migration;
