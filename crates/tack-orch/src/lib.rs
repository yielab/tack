//! `tack-orch` — the runner-v1 execution domain: capability negotiation and
//! request/attempt lifecycle types ([`execution`]), the fleet health and
//! retention background tasks that run over them
//! ([`execution_observability`], [`execution_retention`]), the runner
//! [`scheduler`], per-request [`model_policy`] resolution, and
//! [`usage_provenance`].
//!
//! Depends inward on `tack-core`/`tack-db` only, never on `tack-api` (which
//! depends on it instead) — a `tack-api` type needed here belongs in this crate.

pub mod execution;
// A sibling of `execution`, not a submodule, since both are I/O-bearing
// background tasks — see `execution_retention`'s module doc.
pub mod execution_observability;
pub mod execution_retention;
pub mod model_policy;
pub mod scheduler;
pub mod usage_provenance;

/// Everything that can go wrong in a background task talking to the
/// repository behind it.
#[derive(Debug, thiserror::Error)]
pub enum OrchError {
    /// Configured but not reachable right now.
    #[error("unavailable: {0}")]
    Unavailable(String),
}
