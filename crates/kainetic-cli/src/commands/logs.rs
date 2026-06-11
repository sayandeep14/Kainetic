//! `kainetic logs [--tail] [--since DURATION]`.

use clap::Args;

use crate::error::CliError;

/// Arguments for the `logs` subcommand.
#[derive(Args, Debug)]
pub struct LogsArgs {
    /// Keep streaming new log lines as they arrive.
    #[arg(long)]
    pub tail: bool,

    /// Show logs from the last DURATION (e.g. `5m`, `1h`).
    #[arg(long)]
    pub since: Option<String>,
}

/// Runs `kainetic logs`.
///
/// # Errors
///
/// Returns [`CliError::NotImplemented`] (stub).
pub async fn run(_args: LogsArgs) -> Result<(), CliError> {
    Err(CliError::NotImplemented(
        "`kainetic logs` (coming in Part 11 — Cloud Backend)".into(),
    ))
}
