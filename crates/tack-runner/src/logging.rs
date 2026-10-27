//! The one place a standalone runner process installs its logger. `tack-runner` and
//! `tack runner start` both call it, so a runner on a machine of its own writes the same lines.

use tracing_subscriber::EnvFilter;

/// What a runner logs when `RUST_LOG` is not set: its own events from `info` up, everything else
/// from `warn`. Without a default the filter shows only errors, and a runner that stopped
/// heartbeating would say nothing.
const DEFAULT_FILTER: &str = "warn,tack_runner=info";

/// Installs the process-wide subscriber, writing to stderr. A subscriber already installed stays;
/// that is the outcome this call wants, so the `Err` is not propagated.
pub fn init() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_second_call_keeps_the_first_subscriber_and_does_not_panic() {
        super::init();
        super::init();
    }
}
