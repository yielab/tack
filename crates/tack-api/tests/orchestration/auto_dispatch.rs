//! Dispatch triggered by something other than a direct call to the dispatch
//! endpoint: the `PATCH /api/items/{id}` hook that auto-dispatches on an
//! eligible status change, the gate that keeps that hook reading the live
//! `effective_orch_enabled` setting rather than the static startup-only
//! `TACK_ORCH_ENABLE` env value, and the sprint-level batch endpoint
//! (`POST /api/sprints/{id}/dispatch` and its dry-run) that dispatches a
//! whole dependency graph at once.

#[path = "auto_dispatch/gate.rs"]
mod gate;
#[path = "auto_dispatch/hook.rs"]
mod hook;
#[path = "auto_dispatch/sprint.rs"]
mod sprint;
