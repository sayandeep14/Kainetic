//! `kainetic bench <agent> [--runs N] [--concurrency C]`.

use clap::Args;

use crate::error::CliError;

/// Arguments for the `bench` subcommand.
#[derive(Args, Debug)]
pub struct BenchArgs {
    /// Name of the agent binary to benchmark.
    pub agent: String,

    /// Number of runs (default 100).
    #[arg(long, default_value_t = 100)]
    pub runs: u32,

    /// Concurrency level (default 1).
    #[arg(long, default_value_t = 1)]
    pub concurrency: u32,
}

/// Runs `kainetic bench`.
///
/// # Errors
///
/// Returns [`CliError::NotImplemented`] (stub).
pub async fn run(_args: BenchArgs) -> Result<(), CliError> {
    Err(CliError::NotImplemented(
        "`kainetic bench` (coming in Part 13 — Hardening & Performance)".into(),
    ))
}
