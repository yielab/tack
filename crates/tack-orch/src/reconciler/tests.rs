//! `reconciler`'s unit tests, split by concern: `health` (the pure
//! `HealthTracker`/`evaluate` state machine), `support` (the
//! `FakeControlPlane`/`FakeStore` fixtures most other submodules share),
//! `spawn` (`spawn_reconcilers`/the supervised task loop), `polling` (one
//! tick's runs/approvals/metrics/trace persistence), and `retention` (the
//! rollup sweep). See each submodule's own header for what it covers.

#[path = "tests/health.rs"]
mod health;
#[path = "tests/polling.rs"]
mod polling;
#[path = "tests/retention.rs"]
mod retention;
#[path = "tests/spawn.rs"]
mod spawn;
#[path = "tests/support.rs"]
mod support;
