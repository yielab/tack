//! Concrete [`crate::ControlPlane`] implementations (`github_actions` is
//! compile-only, never registered); adding one only needs a `pub mod` line here.

pub mod docket;
pub mod github_actions;
pub mod legacy_bridge;
pub mod prometheus;
pub mod registry;
