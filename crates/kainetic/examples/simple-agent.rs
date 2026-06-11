//! A minimal Kainetic agent that answers questions about the current date and time.
//!
//! # Running
//!
//! ```bash
//! export ANTHROPIC_API_KEY=sk-ant-…
//! cargo run --example simple-agent -- "What day of the week is it?"
//! ```
//!
//! If `ANTHROPIC_API_KEY` is not set the binary prints usage instructions and
//! exits successfully.
#![deny(clippy::all, unsafe_code)]

use kainetic_core::{AgentConfig, AgentContext, AgentError, KaineticRuntime, ReActLoop};
use kainetic_providers::AnthropicProvider;
use kainetic_tools::builtin::CurrentDatetimeTool;

#[kainetic_macros::agent(description = "Answers questions about the current date and time.")]
async fn date_agent(input: String, ctx: AgentContext) -> Result<String, AgentError> {
    ReActLoop::new(AgentConfig::builder().build())
        .execute(input, ctx)
        .await
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("simple_agent=info".parse().unwrap()),
        )
        .init();

    let provider = match AnthropicProvider::from_env() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("ANTHROPIC_API_KEY is not set: {e}");
            eprintln!();
            eprintln!("Usage:");
            eprintln!("  export ANTHROPIC_API_KEY=sk-ant-...");
            eprintln!("  cargo run --example simple-agent -- \"What day is it?\"");
            std::process::exit(0);
        }
    };

    let query = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "What is the current date and time in ISO 8601 format?".to_owned());

    println!("Query: {query}");
    println!();

    let runtime = KaineticRuntime::builder()
        .provider(provider)
        .tool(CurrentDatetimeTool)
        .build();

    match runtime.run(&DateAgent::new(), query).await {
        Ok(answer) => {
            println!("Answer: {answer}");
        }
        Err(e) => {
            eprintln!("Agent failed: {e}");
            std::process::exit(1);
        }
    }
}
