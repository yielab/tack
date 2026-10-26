//! The production retention and health-watch background tasks, driving the real
//! `retention`/`observability` machinery against a real `tack_db::Repository`
//! rather than the unit tests each module keeps under its own `#[cfg(test)]`.

mod common;
#[path = "ingestion/retention.rs"]
mod retention;
