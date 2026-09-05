//! Two places orchestration's shape is written or validated outside the
//! dispatch path itself: populating a fleet's roster
//! (`agent_fleet_members`) and proving a fleet-targeted request actually
//! schedules onto one of its members, and a template's `orchestration`
//! block being validated (`status_map`, `pipeline_yaml`) at template-save
//! time.

#[path = "fleet_templates/fleet_membership.rs"]
mod fleet_membership;
#[path = "fleet_templates/templates.rs"]
mod templates;
