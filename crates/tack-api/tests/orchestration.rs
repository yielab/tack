//! The docket control-plane integration surface: control-plane and settings
//! administration, the dispatch endpoints (single-item, auto-dispatch, and
//! sprint-level batch), read-facing reporting, the reconciler/store seam
//! exercised without HTTP, and fleet/template administration.

mod common;

#[path = "orchestration/auto_dispatch.rs"]
mod auto_dispatch;
#[path = "orchestration/control_plane.rs"]
mod control_plane;
#[path = "orchestration/dispatch.rs"]
mod dispatch;
#[path = "orchestration/fleet_templates.rs"]
mod fleet_templates;
#[path = "orchestration/reconciler.rs"]
mod reconciler;
#[path = "orchestration/reporting.rs"]
mod reporting;
