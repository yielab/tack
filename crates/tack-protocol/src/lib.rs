//! The runner-v1 contract: the types the board and a runner exchange, the attempt
//! lifecycle, capability negotiation, usage provenance, the merge-readiness pack and the
//! prepare step. No I/O, no database: the server and the runner both build on this and on
//! nothing else they share.

pub mod clock;
pub mod execution;
pub mod mrp;
pub mod prepare;
pub mod usage_provenance;

pub use clock::{Clock, SystemClock};
