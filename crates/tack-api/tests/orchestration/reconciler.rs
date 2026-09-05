//! The reconciler/store seam, exercised directly rather than through HTTP:
//! polling a real docket instance and persisting health
//! (`tack_orch::reconciler::spawn_reconcilers`), applying a workflow status
//! transition when a run reaches a terminal state — unless a human has moved
//! the card since dispatch — and broadcasting a board event exactly once per
//! real change to the underlying `orch_runs`/`orch_approvals` rows.

#[path = "reconciler/broadcast.rs"]
mod broadcast;
#[path = "reconciler/terminal_status.rs"]
mod terminal_status;
#[path = "reconciler/wiring.rs"]
mod wiring;
