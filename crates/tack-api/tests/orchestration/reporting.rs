//! Read-facing endpoints that surface what orchestration has already done:
//! per-item/per-project agent activity, budget/policy cost and denial-rate
//! reporting, and the fleet-wide approvals inbox plus its decision endpoint.

#[path = "reporting/agent_activity.rs"]
mod agent_activity;
#[path = "reporting/approvals.rs"]
mod approvals;
#[path = "reporting/budget_policy.rs"]
mod budget_policy;
