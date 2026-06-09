pub mod config;
pub mod debug;
pub mod error;
pub mod handlers;
pub mod middleware;
pub mod router;

// Re-export commonly used items
pub use router::AppState;
