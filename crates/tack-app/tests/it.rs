//! The crate's one integration-test binary. Each file under `tests/` is a module here;
//! filter one with `-E 'test(/^<module>::/)'`.

mod common;
mod decisions;
mod logs;
mod support;

mod background;
mod cancel_run;
mod claim;
mod complete;
mod create_decision;
mod due_soon;
mod enroll;
mod expire_decisions;
mod heartbeat;
mod ingestion;
mod local_runner;
mod mark_decision_viewed;
mod observe_recovery;
mod ports;
mod refresh;
mod report_events;
mod requeue;
mod resolve_decision;
mod scheduling;
mod start_run;
mod submit_artifacts;
