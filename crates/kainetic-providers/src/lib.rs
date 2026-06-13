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
mod azure_openai;
mod error;
mod gemini;
mod mistral;
mod ollama;
mod openai;
mod openai_compat;
mod provider;
mod router;
mod types;

pub use anthropic::AnthropicProvider;
pub use azure_openai::AzureOpenAiProvider;
pub use error::ProviderError;
pub use gemini::GeminiProvider;
pub use mistral::MistralProvider;
pub use ollama::OllamaProvider;
pub use openai::OpenAiProvider;
pub use provider::ModelProvider;
pub use router::{ProviderRouter, ProviderRouterBuilder};
pub use types::{
    BoxStream, ChunkDelta, CompletionChunk, CompletionRequest, CompletionResponse, StopReason,
    ToolCall, ToolCallResult,
};
