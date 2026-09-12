//! Test binary entry point for `claude-code`'s opt-in live tests. The
//! actual test bodies live in `live/claude_code.rs`, named after the
//! adapter they exercise: a top-level `tests/*.rs` file is what cargo
//! auto-discovers as a separate test binary, while a same-named file
//! under `tests/live/` is only reachable by `#[path]` — that split
//! keeps each adapter's live tests in their own binary without any two
//! adapters' entry points colliding.
#[path = "live/claude_code.rs"]
mod claude_code;
