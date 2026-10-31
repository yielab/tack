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
//! /// Adds the loop to the server's [`background::Loops`]: `tick` every `interval`, a failure
//! /// logged, the next pass awaited.
//! pub fn spawn(loops: &mut Loops, repo: Repository, clock: Arc<dyn Clock>, settings: Settings, interval: Duration);
//! ```
//!
//! `spawn` owns the logging of a failed pass and nothing else; [`background::Loops`] owns the
//! timing, the stop signal and the panic log, and `tick` owns the rule. `tack-cli`'s `serve.rs`
//! owns the `Loops`, hands [`loop_config::LoopConfig`] the settings to start the ones that are
//! on, and stops them all on shutdown.

pub mod auto_start;
pub mod autonomy;
pub mod background;
pub mod due_soon;
mod error;
pub mod github_sync;
pub mod local_runner;
pub mod loop_config;
pub mod merge;
pub mod model_policy_wiring;
pub mod mrp;
pub mod observability;
pub mod ports;
pub mod pull_request;
pub mod remote_backup;
pub mod retention;
pub mod runs;
pub mod scheduler_wiring;

pub use error::AppError;
