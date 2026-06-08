pub mod config;
pub mod handlers;
pub mod middleware;
pub mod router;
pub mod error;
pub mod debug;

// Re-export commonly used items
pub use router::AppState;
