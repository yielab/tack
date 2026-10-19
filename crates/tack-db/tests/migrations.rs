//! Schema-layer tests: the additive columns riding alongside the item-source
//! backfill (`item_source_migration`) and the migrations that drop the
//! legacy Docket bridge's tables (`bridge_tables_dropped`), and the tables the
//! factory loop reads (`factory_loop_tables`).

mod common;

#[path = "migrations/automation_tables.rs"]
mod automation_tables;
#[path = "migrations/autonomy.rs"]
mod autonomy;
#[path = "migrations/bridge_tables_dropped.rs"]
mod bridge_tables_dropped;
#[path = "migrations/factory_loop_tables.rs"]
mod factory_loop_tables;
#[path = "migrations/item_recurrence.rs"]
mod item_recurrence;
#[path = "migrations/item_source_migration.rs"]
mod item_source_migration;
#[path = "migrations/one_model_string.rs"]
mod one_model_string;
#[path = "migrations/provisional_limit.rs"]
mod provisional_limit;
#[path = "migrations/role_profile_fleet.rs"]
mod role_profile_fleet;
#[path = "migrations/template_bundle.rs"]
mod template_bundle;
