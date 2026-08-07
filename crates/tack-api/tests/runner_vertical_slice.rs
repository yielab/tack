//! III-C4 production-repository crash matrix.
//!
//! C5 owns the global production router, so this card keeps its API-side
//! coverage at the repository seam until that router is available.

#[path = "runner_vertical_slice/repository_crash.rs"]
mod repository_crash;
