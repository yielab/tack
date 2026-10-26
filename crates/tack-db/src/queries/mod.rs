//! Read-model queries: one file per table, one `pub async fn` per question the
//! rest of the program asks of it. Each takes the [`Repository`](crate::Repository),
//! returns `Result<_, StoreError>`, and answers with a row struct whose fields
//! are the columns, under the columns' names.

pub mod agent_runners;
pub mod execution_attempts;
pub mod execution_decisions;
pub mod execution_requests;
