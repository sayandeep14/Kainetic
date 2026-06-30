//! Agent trait, `ReActLoop` execution engine, and `KaineticRuntime` for Kainetic.
//!
//! This crate is the execution heart of the Kainetic framework:
//!
//! - [`Agent`] — the trait every agent implements (or derives via
//!   `#[kainetic_macros::agent]`).
//! - [`ReActLoop`] — the Reason → Act → Observe loop that drives LLM
//!   calls and tool dispatch.
//! - [`KaineticRuntime`] — the top-level entry point that wires provider,
//!   tools, and agents together.
//! - [`AgentContext`] — the per-run bag of dependencies threaded through every
//!   layer.
//! - [`AgentConfig`] / [`SystemPrompt`] — configuration primitives.
//! - [`AgentEvent`] — lifecycle events streamed via a `broadcast` channel.
//! - [`AgentError`] — the unified error type for agent operations.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use kainetic_core::{Agent, AgentConfig, AgentContext, AgentError, AgentFuture, KaineticRuntime};
//! use kainetic_providers::AnthropicProvider;
//! use kainetic_tools::builtin::CurrentDatetimeTool;
//!
//! struct DateAgent { config: AgentConfig }
//!
//! impl Agent for DateAgent {
//!     type Input  = String;
//!     type Output = String;
//!     type Error  = AgentError;
//!
//!     fn name(&self) -> &'static str { "date_agent" }
//!     fn description(&self) -> &'static str { "Answers questions about the current date." }
//!     fn config(&self) -> &AgentConfig { &self.config }
//!
//!     fn run<'a>(&'a self, input: String, ctx: AgentContext) -> AgentFuture<'a, String, AgentError> {
//!         let loop_ = kainetic_core::ReActLoop::new(self.config.clone());
//!         Box::pin(async move { loop_.execute(input, ctx).await })
//!     }
//! }
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let runtime = KaineticRuntime::builder()
//!     .provider(AnthropicProvider::from_env()?)
//!     .tool(CurrentDatetimeTool)
//!     .build();
//!
//! let agent = DateAgent { config: AgentConfig::builder().build() };
//! let answer = runtime.run(&agent, "What day is it?".to_owned()).await?;
//! println!("{answer}");
//! # Ok(())
//! # }
//! ```
#![deny(clippy::all, clippy::pedantic, missing_docs, unsafe_code)]

mod agent;
mod config;
mod context;
mod error;
mod event;
mod proptest_react;
mod react;
mod runtime;

pub use agent::{Agent, AgentFuture};
pub use config::{AgentConfig, AgentConfigBuilder, SystemPrompt};
pub use context::AgentContext;
pub use error::AgentError;
pub use event::AgentEvent;
pub use react::ReActLoop;
pub use runtime::{KaineticRuntime, KaineticRuntimeBuilder};
