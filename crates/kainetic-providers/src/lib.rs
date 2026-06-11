//! Model provider abstraction and implementations for Kainetic.
//!
//! This crate defines the [`ModelProvider`] trait and ships three concrete
//! implementations: [`AnthropicProvider`], [`OpenAiProvider`], and
//! [`GeminiProvider`]. All three support `complete()` (blocking full response)
//! and `stream()` (incremental SSE stream), tool calling, cost estimation,
//! and automatic retry on rate limits.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use kainetic_providers::{AnthropicProvider, CompletionRequest, ModelProvider};
//! use kainetic_schema::Message;
//!
//! # #[tokio::main]
//! # async fn main() -> anyhow::Result<()> {
//! let provider = AnthropicProvider::from_env()?;
//! let request = CompletionRequest::new(
//!     "claude-sonnet-4-6",
//!     vec![Message::user("Hello!")],
//! );
//! let response = provider.complete(request).await?;
//! println!("{}", response.text().unwrap_or_default());
//! # Ok(())
//! # }
//! ```
#![deny(clippy::all, clippy::pedantic, missing_docs, unsafe_code)]

mod anthropic;
mod error;
mod gemini;
mod openai;
mod provider;
mod types;

pub use anthropic::AnthropicProvider;
pub use error::ProviderError;
pub use gemini::GeminiProvider;
pub use openai::OpenAiProvider;
pub use provider::ModelProvider;
pub use types::{
    BoxStream, ChunkDelta, CompletionChunk, CompletionRequest, CompletionResponse, StopReason,
    ToolCall, ToolCallResult,
};
