//! The crate's one integration-test binary. Each file under `tests/` is a module here;
//! filter one with `-E 'test(/^<module>::/)'`.

mod common;
mod fake_docket;
mod usage_fixture;

mod agent_boundaries;
mod bootstrap_entrypoint;
mod briefing_schema;
mod cli;
mod crash_matrix;
mod docket_v5;
mod evidence_contract;
mod g2_journal_corruption_test;
mod h3_checkout;
mod live;
mod plan_contract;
