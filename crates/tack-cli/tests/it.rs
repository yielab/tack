//! The crate's one integration-test binary. Each file under `tests/` is a module here;
//! filter one with `-E 'test(/^<module>::/)'`.

mod common;

mod cli_test;
mod e6_scheduler_e2e_test;
mod embedded_runner_id;
mod embedded_runner_orphaned_credential;
mod embedded_runner_state_scoping;
