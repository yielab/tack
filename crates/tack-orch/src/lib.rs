//! `tack-orch` — the pure part of the runner-v1 execution domain: the runner
//! [`scheduler`] (selection and batching over plain values) and per-request
//! [`model_policy`] resolution.
//!
//! No I/O, no clock, no store: depends inward on `tack-protocol` only. The live
//! callers that read rows and write the outcome are in `tack-app`
//! (`scheduler_wiring`, `model_policy_wiring`).

pub mod model_policy;
pub mod scheduler;
