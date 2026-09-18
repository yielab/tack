//! The production retention and health-watch background tasks, driving the
//! real `execution_retention`/`execution_observability` machinery against a
//! real, file-backed `tack_db::Repository` rather than the fake-store unit
//! tests each module keeps under its own `#[cfg(test)]`.

#[path = "ingestion/retention.rs"]
mod retention;
