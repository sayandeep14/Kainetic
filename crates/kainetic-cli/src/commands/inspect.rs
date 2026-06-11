//! `kainetic inspect <run-id>` — pretty-print a past run's event log.

use clap::Args;

use crate::error::CliError;

/// Arguments for the `inspect` subcommand.
#[derive(Args, Debug)]
pub struct InspectArgs {
    /// The run ID to inspect (UUID format).
    pub run_id: String,
}

/// Runs `kainetic inspect`.
///
/// # Errors
///
/// Returns [`CliError::NotImplemented`] (stub).
pub async fn run(_args: InspectArgs) -> Result<(), CliError> {
    Err(CliError::NotImplemented(
        "`kainetic inspect` (coming in Part 11 — Cloud Backend)".into(),
    ))
}
