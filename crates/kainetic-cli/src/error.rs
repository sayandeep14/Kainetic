//! CLI-specific error type.

use thiserror::Error;

/// Top-level error returned by every CLI command handler.
#[derive(Debug, Error)]
pub enum CliError {
    /// An I/O error while reading or writing files.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// A path or directory already exists and `--force` was not passed.
    #[error("already exists: {0}")]
    AlreadyExists(String),

    /// A required file or directory was not found.
    #[error("not found: {0}")]
    NotFound(String),

    /// The `.kainetic.toml` config is invalid.
    #[error("invalid config: {0}")]
    InvalidConfig(String),

    /// A child process (e.g. `cargo`) exited with a non-zero status.
    #[error("command failed with status {status}: {cmd}")]
    CommandFailed { cmd: String, status: i32 },

    /// A feature not yet implemented.
    #[error("{0} is not yet implemented — coming in a future release")]
    NotImplemented(String),

    /// An unexpected internal error (e.g. network failure, JSON decode error).
    #[error("{0}")]
    Internal(String),
}
