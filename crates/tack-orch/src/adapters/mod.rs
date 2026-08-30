//! Concrete [`crate::ControlPlane`] implementations.
//!
//! `docket.rs` (the `DocketAdapter`) lands in Wave 1, card A1 — this module is
//! declared now, empty, so that card can add its file without editing this one
//! (and without touching `lib.rs`, which is frozen after Wave 0). See
//! `TODO.md` §2 (file-ownership map) and §Wave 1, card A1.

pub mod docket;

/// A compile-only second [`crate::ControlPlane`] implementor (TODO.md card
/// G1, Wave B / Phase 40). Never registered — see the module's own doc
/// comment and `registry::build`'s.
pub mod github_actions;

/// The Prometheus text-exposition parser card A1 built for `/metrics`
/// Declared here — rather than as a
/// crate-root `pub mod prometheus;` in `lib.rs` — only because `lib.rs` is
/// frozen after Wave 0 and doesn't already have a placeholder for it (unlike
/// `adapters`/`reconciler`, which W0-A pre-declared). Card B3 (Wave 2)
/// reuses this module as-is: `tack_orch::adapters::prometheus::parse`.
pub mod prometheus;

/// One place every caller builds a live adapter from a `control_planes`
/// row's `kind` — see the module's own
/// doc comment for what it replaces and why it lives here, not in
/// `tack-api`.
pub mod registry;

/// The Part III legacy-bridge compatibility policy (card **III-G1**, Wave 6,
/// Phase 57 — not to be confused with the older "card G1, Wave B / Phase 40"
/// referenced above, a Part II card that predates and is unrelated to this
/// one). See the module's own doc comment for the maintain/export/deprecate
/// decision and its evidence.
pub mod legacy_bridge;
