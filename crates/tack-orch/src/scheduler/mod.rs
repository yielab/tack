//! The deterministic fleet scheduler.
//!
//! A pure decision library, no I/O: given a candidate runner pool and a request, it
//! returns a selected runner or a typed reason none qualify. It never grants the
//! authoritative lease itself — only the repository's fenced claim
//! (`docs/contracts/runner-v1/`) can. [`select::select_runner`] decides one request;
//! [`batch::schedule`] schedules several sharing a pool, priority then FIFO.

pub mod batch;
pub mod select;
pub mod types;
pub mod wiring;

pub use batch::schedule;
pub use select::{SchedulingError, SchedulingPolicy, select_runner};
pub use types::{
    IneligibleReason, ModelSelector, Priority, RunnerCandidate, RunnerState, SchedulingRequest,
    Selection, SelectionOutcome,
};
pub use wiring::choose_request_for_runner;
