use async_trait::async_trait;

use crate::RunnerError;

/// Boundary for harness child-process ownership. It intentionally has no
/// launch method until a real adapter owns a concrete subprocess contract.
#[async_trait]
pub trait ProcessSupervisor: Send + Sync {
    async fn terminate_all(&self) -> Result<(), RunnerError>;
}

/// A process table starts empty in the skeleton. Calling cleanup is safe and
/// explicit; future adapters register only processes they themselves launch.
#[derive(Debug, Default)]
pub struct SystemProcessSupervisor;

#[async_trait]
impl ProcessSupervisor for SystemProcessSupervisor {
    async fn terminate_all(&self) -> Result<(), RunnerError> {
        Ok(())
    }
}
