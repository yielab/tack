//! The crate's one integration-test binary. Each file under `tests/` is a module here;
//! filter one with `-E 'test(/^<module>::/)'`.

mod common;

mod check_solutions;
mod graders;
mod report;
mod synthetic;
mod task_loading;
mod trials;
