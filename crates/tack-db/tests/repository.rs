//! Repository-layer integration tests: CRUD and query coverage for every
//! entity (`integration`), the execution-domain protocol repository (split by
//! operation under test into `execution_enrollment`,
//! `execution_claim_lease_heartbeat`, `execution_events`,
//! `execution_transitions_completion`, `execution_recovery_requeue`,
//! `execution_decisions_artifacts`, `execution_enqueue`, plus
//! `execution_retention` and `event_artifact_retention`), the agent-fleet
//! control-plane repository (`orch_repo`), and the two concurrency-sensitive
//! write paths (`status_update_checked`, `version_concurrency`).

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
#[path = "repository/execution_recovery_requeue.rs"]
mod execution_recovery_requeue;
#[path = "repository/execution_retention.rs"]
mod execution_retention;
#[path = "repository/execution_transitions_completion.rs"]
mod execution_transitions_completion;
#[path = "repository/integration.rs"]
mod integration;
#[path = "repository/orch_repo.rs"]
mod orch_repo;
#[path = "repository/status_update_checked.rs"]
mod status_update_checked;
#[path = "repository/version_concurrency.rs"]
mod version_concurrency;
