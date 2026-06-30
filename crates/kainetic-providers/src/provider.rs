//! The `ModelProvider` trait that every LLM backend implements.

use async_trait::async_trait;
use kainetic_schema::TokenUsage;

use crate::{
    error::ProviderError,
    types::{BoxStream, CompletionChunk, CompletionRequest, CompletionResponse},
};

/// Abstraction over a language model provider.
///
/// Implement this trait to add a new backend (e.g. Gemini, Mistral, Ollama).
/// All methods are object-safe via `async_trait`, so providers can be stored
/// as `Arc<dyn ModelProvider>`.
///
/// # Example
///
/// ```rust,no_run
/// use kainetic_providers::{ModelProvider, CompletionRequest};
/// use kainetic_schema::Message;
///
/// # async fn example(provider: impl ModelProvider) -> anyhow::Result<()> {
/// let request = CompletionRequest::new(
///     provider.default_model(),
///     vec![Message::user("Hello!")],
/// );
/// let response = provider.complete(request).await?;
/// println!("{}", response.text().unwrap_or_default());
/// # Ok(())
/// # }
/// ```
#[async_trait]
pub trait ModelProvider: Send + Sync + 'static {
    /// Sends a completion request and waits for the full response.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::RateLimited`] on HTTP 429/529 after all retries
    /// are exhausted. Returns [`ProviderError::AuthFailed`] on HTTP 401.
    /// Returns [`ProviderError::NetworkError`] for transport failures.
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError>;

    /// Sends a completion request and returns a stream of incremental chunks.
    ///
    /// The stream ends with a [`crate::ChunkDelta::Done`] chunk that carries
    /// the final [`crate::StopReason`] and token usage.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::NetworkError`] if the initial connection fails.
    /// Returns [`ProviderError::AuthFailed`] on HTTP 401.
    async fn stream(
        &self,
        request: CompletionRequest,
    ) -> Result<BoxStream<Result<CompletionChunk, ProviderError>>, ProviderError>;

    /// Estimates the cost in US dollars for the given token usage and model.
    ///
    /// Uses hard-coded per-token prices that may lag actual provider invoices.
    /// Use as an estimate only.
    fn cost_usd(&self, usage: &TokenUsage, model: &str) -> f64;

    /// Returns the provider's short identifier (e.g. `"anthropic"`, `"openai"`).
    fn name(&self) -> &'static str;

    /// Returns a sensible default model for this provider.
    fn default_model(&self) -> &'static str;
}
