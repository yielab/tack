pub mod config;
pub mod debug;
pub mod error;
// Execution-domain retention sweep + health watch runtime wiring — see the
// module's own doc comment.
pub mod execution_runtime;
pub mod handlers;
pub mod host;
pub mod middleware;
pub mod openapi;
pub mod prill;
pub mod prill_store;
pub mod rate_card;
pub mod router;
pub mod server;
pub mod webhook;

// Re-export commonly used items
pub use router::AppState;
pub use server::serve;
pub use tack_app::local_runner::{
    LocalRunnerControl, LocalRunnerControlError, RuntimeState, RuntimeStatus,
};
