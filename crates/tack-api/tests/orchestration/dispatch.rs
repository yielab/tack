//! The single-item `POST /api/items/{id}/dispatch` endpoint, and the guard
//! that keeps it from colliding with the neutral runner-v1 scheduling plane
//! (`execution_requests`) when both are eligible to claim the same item.

#[path = "dispatch/dual_scheduling.rs"]
mod dual_scheduling;
#[path = "dispatch/item.rs"]
mod item;
