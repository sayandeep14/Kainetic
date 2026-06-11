//! `kainetic deploy` — deploy to Kainetic Cloud (stub).

use clap::Args;

use crate::error::CliError;

/// Arguments for the `deploy` subcommand.
#[derive(Args, Debug)]
pub struct DeployArgs {
    /// Target environment (e.g. `staging`, `production`).
    #[arg(long, default_value = "production")]
    pub env: String,
}

/// Runs `kainetic deploy`.
///
/// # Errors
///
/// Returns [`CliError::NotImplemented`] (stub; full implementation in Part 10).
pub async fn run(_args: DeployArgs) -> Result<(), CliError> {
    Err(CliError::NotImplemented(
        "`kainetic deploy` (coming in Part 10 — Cloud Backend)".into(),
    ))
}
