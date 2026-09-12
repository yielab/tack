//! Opt-in tests that exercise a real, installed harness binary — never a
//! fake fixture. Each module self-skips when its binary (or a further
//! opt-in env var, for the one test that spends real credentials) is
//! absent, and every test in it carries `#[ignore]` so a plain `cargo
//! nextest run` never attempts them: run explicitly with
//! `--run-ignored ignored-only`.

#[path = "../common/mod.rs"]
mod common;

mod codex;
