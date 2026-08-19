use thiserror::Error;

/// Configuration failures intentionally do not retain parser text, because a
/// parser diagnostic can echo a line containing an enrollment credential.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ConfigError {
    #[error("runner configuration is invalid")]
    Invalid,
    #[error("runner configuration could not be read")]
    Unreadable,
    #[error("runner id must not be empty")]
    EmptyRunnerId,
}

#[derive(Debug, Error)]
pub enum RunnerError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("runner enrollment credential is required")]
    MissingEnrollmentCredential,
    #[error("harness {harness:?} is unsupported by this runner")]
    UnsupportedHarness { harness: String },
    #[error("runner protocol client is not configured")]
    ProtocolUnavailable,
    /// The HTTP transport could not be built or the enrollment exchange did
    /// not complete. Deliberately carries no detail: a `reqwest` error's
    /// `Display` includes the request URL and can include a redacted-looking
    /// but complete header context, and this value is printed by `main`.
    #[error("runner protocol transport failed")]
    ProtocolTransport,
    #[error("runner client stopped before shutdown was requested")]
    ClientStopped,
    #[error("runner client task could not be joined")]
    ClientTaskJoin,
    #[error("runner filesystem preparation failed")]
    Filesystem,
    #[error("runner process cleanup failed")]
    ProcessCleanup,
}
