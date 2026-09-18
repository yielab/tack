//! Schema-layer tests: the additive columns riding alongside the item-source
//! backfill (`item_source_migration`) and the migrations that drop the
//! legacy Docket bridge's tables (`bridge_tables_dropped`).

mod common;

#[path = "migrations/bridge_tables_dropped.rs"]
mod bridge_tables_dropped;
#[path = "migrations/item_source_migration.rs"]
mod item_source_migration;
