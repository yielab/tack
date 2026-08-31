//! Local, pull-based runner primitives.
//!
//! This crate deliberately has no harness implementation yet.  It owns the
//! process lifecycle boundary and exposes seams that future protocol and
//! harness work can implement without putting credentials in the API server.

pub mod bootstrap;
pub mod client;
pub mod clock;
pub mod config;
pub mod error;
pub mod filesystem;
pub mod harness;
pub mod process;
pub mod registry;
pub mod runtime;

pub use client::{RunnerProtocolClient, UnavailableProtocolClient};
pub use clock::{Clock, SystemClock};
pub use config::{ConfigOverrides, EnrollmentCredential, RunnerConfig, RunnerConfigSources};
pub use error::{ConfigError, RunnerError};
pub use filesystem::{LocalFilesystem, RunnerFilesystem};
pub use process::{ProcessSupervisor, SystemProcessSupervisor};
pub use registry::{HarnessKind, HarnessRegistry};
pub use runtime::{RunnerRuntime, Shutdown, ShutdownHandle};
