//! The crate's one integration-test binary. Each file under `tests/` is a module here;
//! filter one with `-E 'test(/^<module>::/)'`.

mod common;

mod handlers;
mod openapi_contract;
mod runner_protocol;
mod runner_vertical_slice;
mod security;
mod wiring;
