//! `kainetic run <agent> [--input JSON] [--session SESSION_ID]`.

use std::path::PathBuf;
use std::process::Stdio;

use clap::Args;
use console::style;

use crate::error::CliError;

/// Arguments for the `run` subcommand.
#[derive(Args, Debug)]
pub struct RunArgs {
    /// Name of the agent binary to run (must match a `[[bin]]` in `Cargo.toml`).
    pub agent: String,

    /// JSON-encoded input to pass to the agent's stdin.
    #[arg(long, short)]
    pub input: Option<String>,

    /// Session ID to associate this run with (for episodic memory).
    #[arg(long)]
    pub session: Option<String>,

    /// Pass `--release` to `cargo run`.
    #[arg(long)]
    pub release: bool,

    /// Extra arguments forwarded verbatim after `--` to the agent binary.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub extra: Vec<String>,
}

/// Runs `kainetic run`.
///
/// Compiles the project with `cargo run --bin <agent>` and streams output.
///
/// # Errors
///
/// - [`CliError::NotFound`] if no `Cargo.toml` exists in the current directory.
/// - [`CliError::CommandFailed`] if the cargo invocation fails.
/// - [`CliError::Io`] on I/O errors.
pub async fn run(args: RunArgs) -> Result<(), CliError> {
    ensure_cargo_toml()?;

    let mut cmd = cargo_command(&args);

    println!(
        "{} Running agent `{}`…",
        style("▶").cyan().bold(),
        style(&args.agent).cyan()
    );

    let status = cmd.status().map_err(|e| CliError::Io(e))?;

    if !status.success() {
        return Err(CliError::CommandFailed {
            cmd: format!("cargo run --bin {}", args.agent),
            status: status.code().unwrap_or(-1),
        });
    }

    Ok(())
}

fn cargo_command(args: &RunArgs) -> std::process::Command {
    let mut cmd = std::process::Command::new("cargo");
    cmd.arg("run").arg("--bin").arg(&args.agent);

    if args.release {
        cmd.arg("--release");
    }

    // Forward extra args after `--` to the binary.
    if args.input.is_some() || !args.extra.is_empty() {
        cmd.arg("--");
    }

    // The agent binary reads its query from the first positional arg.
    if let Some(ref input) = args.input {
        cmd.arg(input);
    }
    for arg in &args.extra {
        cmd.arg(arg);
    }

    if let Some(ref session) = args.session {
        cmd.env("KAINETIC_SESSION_ID", session);
    }

    cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    cmd
}

fn ensure_cargo_toml() -> Result<(), CliError> {
    if !PathBuf::from("Cargo.toml").exists() {
        return Err(CliError::NotFound(
            "Cargo.toml not found — run this command from a Kainetic project directory".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_cargo_toml_returns_not_found() {
        // Change to a temp dir that has no Cargo.toml.
        let tmp = tempfile::tempdir().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let result = ensure_cargo_toml();
        std::env::set_current_dir(orig).unwrap();

        assert!(matches!(result, Err(CliError::NotFound(_))));
    }
}
