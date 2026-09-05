//! Control-plane resource administration: creating/reading/patching/deleting
//! a docket control plane (its auth token write-only and never echoed back),
//! saving and validating a project's `orch-link` against its own workflow,
//! the `/api/fleet` unreachable-vs-zero distinction, and the runtime
//! `GET`/`PUT /api/settings/orchestration` toggle that lets an operator turn
//! orchestration on or off without a restart.

#[path = "control_plane/resource.rs"]
mod resource;
#[path = "control_plane/settings.rs"]
mod settings;
