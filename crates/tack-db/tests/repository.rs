//! Repository-layer integration tests: CRUD and query coverage for every
//! entity (`integration`), the execution-domain protocol repository
//! (`execution_repo`, `execution_retention`, `event_artifact_retention`),
//! the agent-fleet control-plane repository (`orch_repo`), and the two
//! concurrency-sensitive write paths (`status_update_checked`,
//! `version_concurrency`).

mod common;

#[path = "repository/event_artifact_retention.rs"]
mod event_artifact_retention;
#[path = "repository/execution_repo.rs"]
mod execution_repo;
#[path = "repository/execution_retention.rs"]
mod execution_retention;
#[path = "repository/integration.rs"]
mod integration;
#[path = "repository/orch_repo.rs"]
mod orch_repo;
#[path = "repository/status_update_checked.rs"]
mod status_update_checked;
#[path = "repository/version_concurrency.rs"]
mod version_concurrency;
