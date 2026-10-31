//! The one clock that stands still, for a test that compares times exactly.

use chrono::{DateTime, Utc};
use tack_protocol::Clock;

/// A clock that always reads the instant it was built with.
#[derive(Debug, Clone, Copy)]
pub struct FixedClock(pub DateTime<Utc>);

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}
