//! Concrete [`crate::ControlPlane`] implementations.
//!
//! Each adapter is its own file here; adding one only needs a `pub mod` line
//! in this file, never a change to `lib.rs`.

pub mod docket;

/// A compile-only second [`crate::ControlPlane`] implementor. Never
/// registered — see the module's own doc comment and `registry::build`'s.
pub mod github_actions;

/// The Prometheus text-exposition parser for `/metrics`, reused directly as
/// `tack_orch::adapters::prometheus::parse`.
pub mod prometheus;

/// One place every caller builds a live adapter from a `control_planes`
/// row's `kind` — see the module's own
/// doc comment for what it replaces and why it lives here, not in
/// `tack-api`.
pub mod registry;

/// The legacy-bridge compatibility policy. See the module's own doc comment
/// for the maintain/export/deprecate decision and its evidence.
pub mod legacy_bridge;
