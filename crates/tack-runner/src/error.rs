use thiserror::Error;

/// Configuration failures intentionally do not retain parser text, because a
/// parser diagnostic can echo a line containing an enrollment credential.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ConfigError {
    #[error("runner configuration is invalid")]
    Invalid,
    /// Named by its key alone, never its contents: the table is checked
    /// before any parser text could be produced.
    #[error(
        "runner configuration has a [provider] table, which this version no longer reads: \
         each agent signs in with its own login, so remove the table"
    )]
    ProviderTable,
    /// A top-level key the file's schema does not have, named by the key alone: never a value.
    /// `is_table` is set when the file wrote it as a `[table]`.
    #[error(
        "{}",
        if *.is_table {
            format!(
                "runner configuration has an unknown table [{}]: the runner's settings go at the top of the file",
                .key
            )
        } else {
            format!("runner configuration has an unknown key {}", .key)
        }
    )]
    UnknownKey { key: String, is_table: bool },
    /// The file's kind of failure only: the operating system's own text can name the path.
    #[error("runner configuration could not be read: {0}")]
    Unreadable(IoKind),
    #[error("runner id must not be empty")]
    EmptyRunnerId,
    /// A setting that is present and wrong, named so the operator knows
    /// which one to fix.
    #[error("runner configuration field {field} is invalid: {reason}")]
    InvalidField { field: &'static str, reason: String },
}

#[derive(Debug, Error)]
pub enum RunnerError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("runner enrollment credential is required")]
    MissingEnrollmentCredential,
    /// The server no longer accepts the saved session (it expired or was revoked) and there is
    /// no enrollment credential to enroll again with.
    #[error(
        "the runner's saved session was refused by the server (expired or revoked) and no \
         enrollment credential is configured to enroll again"
    )]
    SessionRefused,
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
    /// `panicked` says whether the task panicked or was cancelled; the panic's own message is
    /// not carried, it can quote what the task was holding.
    #[error("runner client task could not be joined (panicked: {panicked})")]
    ClientTaskJoin { panicked: bool },
    #[error("runner filesystem preparation failed: {0}")]
    Filesystem(IoKind),
    #[error("runner could not listen for the shutdown signal: {0}")]
    ShutdownSignal(IoKind),
    #[error("runner process cleanup failed")]
    ProcessCleanup,
}

/// The kind of an I/O failure, printed as the words the standard library uses for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IoKind(pub std::io::ErrorKind);

impl std::fmt::Display for IoKind {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(out, "{}", self.0)
    }
}
