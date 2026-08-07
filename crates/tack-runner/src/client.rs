use async_trait::async_trait;

use crate::{RunnerError, Shutdown};

/// Runner protocol boundary. The concrete HTTP client belongs to the protocol
/// vertical slice, not to this skeleton.
#[async_trait]
pub trait RunnerProtocolClient: Send + Sync {
    async fn serve(&self, shutdown: Shutdown) -> Result<(), RunnerError>;
}

/// Fails explicitly until the pull-protocol implementation is supplied.
#[derive(Debug, Default)]
pub struct UnavailableProtocolClient;

#[async_trait]
impl RunnerProtocolClient for UnavailableProtocolClient {
    async fn serve(&self, _shutdown: Shutdown) -> Result<(), RunnerError> {
        Err(RunnerError::ProtocolUnavailable)
    }
}
