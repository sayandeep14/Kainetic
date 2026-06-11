//! Developer CLI for Kainetic.
//!
//! # Usage
//!
//! ```text
//! kainetic init <name>               Scaffold a new Kainetic project
//! kainetic new agent <name>          Generate an agent skeleton file
//! kainetic new tool <name>           Generate a tool skeleton file
//! kainetic run <agent> [--input …]   Compile and run a named agent
//! kainetic validate                  Check project config and schema
//! kainetic inspect <run-id>          Pretty-print a past run's event log
//! kainetic bench <agent> [--runs N]  Latency and cost benchmarks
//! kainetic logs [--tail]             Stream agent logs
//! kainetic deploy                    Deploy to Kainetic Cloud
//! ```
#![deny(clippy::all, clippy::pedantic, unsafe_code)]
// Command handlers share a consistent `async fn run(...)` interface even when
// they don't currently await anything — this keeps the dispatch uniform and
// makes it easy to add async operations (HTTP, tokio::fs, etc.) later.
#![allow(clippy::unused_async, clippy::module_name_repetitions)]

mod commands;
mod error;

use clap::{Parser, Subcommand};

/// The Kainetic developer CLI.
#[derive(Parser)]
#[command(
    name = "kainetic",
    about = "Kainetic — production-grade Rust runtime for agentic AI",
    version,
    propagate_version = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scaffold a new Kainetic project in a new directory.
    Init(commands::init::InitArgs),

    /// Code-generation helpers.
    New(commands::new::NewArgs),

    /// Compile and run a named agent.
    Run(commands::run::RunArgs),

    /// Validate project config, tool references, and pipeline graph.
    Validate(commands::validate::ValidateArgs),

    /// Pretty-print the full event log for a past run.
    Inspect(commands::inspect::InspectArgs),

    /// Run latency and cost benchmarks for an agent.
    Bench(commands::bench::BenchArgs),

    /// Stream agent logs from the local log store.
    Logs(commands::logs::LogsArgs),

    /// Deploy this project to Kainetic Cloud.
    Deploy(commands::deploy::DeployArgs),
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("kainetic=info".parse().expect("valid directive")),
        )
        .init();

    let cli = Cli::parse();

    let result = match cli.command {
        Command::Init(args) => commands::init::run(args).await,
        Command::New(args) => commands::new::run(args).await,
        Command::Run(args) => commands::run::run(args).await,
        Command::Validate(args) => commands::validate::run(args).await,
        Command::Inspect(args) => commands::inspect::run(args).await,
        Command::Bench(args) => commands::bench::run(args).await,
        Command::Logs(args) => commands::logs::run(args).await,
        Command::Deploy(args) => commands::deploy::run(args).await,
    };

    if let Err(e) = result {
        eprintln!("{} {e}", console::style("error:").red().bold());
        std::process::exit(1);
    }
}
