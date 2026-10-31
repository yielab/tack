//! The deterministic fleet scheduler: a pure decision library, no I/O.
//! Given a candidate pool and a request it returns a selected runner or a
//! typed reason none qualify — never the authoritative lease itself, only
//! the repository's fenced claim can.

pub mod batch;
pub mod select;
pub mod types;

pub use batch::schedule;
pub use select::{SchedulingPolicy, select_runner};
pub use types::{
    BudgetKind, IneligibleReason, Priority, RunnerCandidate, RunnerState, SchedulingRequest,
    Selection, SelectionOutcome,
};
