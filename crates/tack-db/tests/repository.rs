//! Repository-layer integration tests: CRUD and query coverage for every
//! entity (`integration`), the execution-domain protocol repository (split by
//! operation under test into `execution_enrollment`,
//! `execution_claim_lease_heartbeat`, `execution_events`,
//! `execution_transitions_completion`, `execution_recovery_requeue`,
//! `execution_decisions_artifacts`, `execution_enqueue`, and the
//! concurrent-duplicate-writer race family in `execution_races`, plus
//! `execution_retention` and `event_artifact_retention`), and the
//! concurrency-sensitive `version_concurrency` write path.

mod common;

#[path = "repository/event_artifact_retention.rs"]
mod event_artifact_retention;
#[path = "repository/execution_claim_lease_heartbeat.rs"]
mod execution_claim_lease_heartbeat;
#[path = "repository/execution_decisions_artifacts.rs"]
mod execution_decisions_artifacts;
#[path = "repository/execution_enqueue.rs"]
mod execution_enqueue;
#[path = "repository/execution_enrollment.rs"]
mod execution_enrollment;
#[path = "repository/execution_events.rs"]
mod execution_events;
#[path = "repository/execution_races.rs"]
mod execution_races;
#[path = "repository/execution_recovery_requeue.rs"]
mod execution_recovery_requeue;
#[path = "repository/execution_retention.rs"]
mod execution_retention;
#[path = "repository/execution_transitions_completion.rs"]
mod execution_transitions_completion;
#[path = "repository/integration.rs"]
mod integration;
#[path = "repository/version_concurrency.rs"]
mod version_concurrency;
