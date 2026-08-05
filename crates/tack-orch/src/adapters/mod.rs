//! Concrete [`crate::ControlPlane`] implementations.
//!
//! `docket.rs` (the `DocketAdapter`) lands in Wave 1, card A1 — this module is
//! declared now, empty, so that card can add its file without editing this one
//! (and without touching `lib.rs`, which is frozen after Wave 0). See
//! `TODO.md` §2 (file-ownership map) and §Wave 1, card A1.

pub mod docket;

/// The Prometheus text-exposition parser card A1 built for `/metrics`
/// (TODO.md §Wave 1, card A1, step 3). Declared here — rather than as a
/// crate-root `pub mod prometheus;` in `lib.rs` — only because `lib.rs` is
/// frozen after Wave 0 and doesn't already have a placeholder for it (unlike
/// `adapters`/`reconciler`, which W0-A pre-declared). Card B3 (Wave 2)
/// reuses this module as-is: `tack_orch::adapters::prometheus::parse`.
pub mod prometheus;
