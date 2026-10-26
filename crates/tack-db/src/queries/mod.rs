//! Read-model queries: one file per table, one `pub async fn` per question the
//! rest of the program asks of it. Each takes the [`Repository`](crate::Repository),
//! returns `Result<_, StoreError>`, and answers with a row struct whose fields
//! are the columns, under the columns' names.

pub mod agent_enrollment_tokens;
pub mod agent_fleets;
pub mod agent_profiles;
pub mod agent_runners;
pub mod app_meta;
pub mod attempt_reviews;
pub mod autonomy;
pub mod backup;
pub mod budgets;
pub mod database;
pub mod execution_artifacts;
pub mod execution_attempts;
pub mod execution_claim_replays;
pub mod execution_completion_replays;
pub mod execution_decisions;
pub mod execution_events;
pub mod execution_heartbeat_replays;
pub mod execution_recovery_audits;
pub mod execution_request_rows;
pub mod execution_requests;
pub mod github_links;
pub mod items;
pub mod migrations;
pub mod prill_records;
pub mod projects;
pub mod pull_requests;
pub mod rate_card;
