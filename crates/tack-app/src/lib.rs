//! The application layer: what Tack does, written over the store and a clock and nothing
//! else. It names no HTTP type and no SQL; `tack-api` calls it and maps its errors, and
//! `tack-cli` schedules its loops.
//!
//! # Use cases
//!
//! A use case is one `pub async fn <verb>(repo: &Repository, clock: &dyn Clock, <args>) ->
//! Result<T, AppError>` in `runs/<verb>.rs`, taking plain values and holding the rule the
//! handler used to hold (idempotency keys, fencing checks, `validate_transition`). A handler
//! extracts, calls, maps the error and responds. A test of a rule calls it directly on
//! `setup_test_db()`.
//!
//! # Ticks
//!
//! Every background loop is a module with two functions:
//!
//! ```ignore
//! /// One pass. Tests call this with a fake clock; nothing in a test sleeps.
//! pub async fn tick(repo: &Repository, clock: &dyn Clock, settings: &Settings)
//!     -> Result<Outcome, AppError>;
//!
//! /// The loop `tack serve` spawns: wait `interval`, `tick`, log a failure, repeat.
//! pub async fn run(repo: Repository, clock: Arc<dyn Clock>, settings: Settings, interval: Duration);
//! ```
//!
//! `run` owns the timing and the logging of a failed pass and nothing else; `tick` owns the
//! rule. `tack-cli`'s `serve.rs` does the `tokio::spawn(tack_app::<module>::run(..))`.

mod error;
pub mod local_runner;
pub mod runs;

pub use error::AppError;
