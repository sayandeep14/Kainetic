//! Ollama local model provider.
//!
//! Ollama exposes an OpenAI-compatible `/v1/chat/completions` endpoint on
//! `http://localhost:11434` by default.  No API key is required.
//!
//! # Usage
//!
//! ```rust,no_run
//! use kainetic_providers::{OllamaProvider, CompletionRequest, ModelProvider};
//! use kainetic_schema::Message;
//!
//! # #[tokio::main]
//! # async fn main() -> anyhow::Result<()> {
//! let provider = OllamaProvider::new();
//! let response = provider
//!     .complete(CompletionRequest::new("llama3.2", vec![Message::user("Hello!")]))
//!     .await?;
//! println!("{}", response.text().unwrap_or_default());
//! # Ok(())
//! # }
//! ```

use async_trait::async_trait;
use kainetic_schema::TokenUsage;

use crate::{
    error::ProviderError,
    openai_compat::{retry_complete, OaiCompatClient},
    provider::ModelProvider,
    types::{BoxStream, CompletionChunk, CompletionRequest, CompletionResponse},
};

const DEFAULT_BASE_URL: &str = "http://localhost:11434";
const CHAT_PATH: &str = "/v1/chat/completions";
const MAX_RETRIES: u32 = 3;

/// Ollama local model provider.
///
/// Connects to a locally-running Ollama instance.  All costs are zero since
/// inference is local.
#[derive(Clone)]
pub struct OllamaProvider {
    client: std::sync::Arc<OaiCompatClient>,
    model: String,
}

impl OllamaProvider {
    /// Creates a provider connecting to `http://localhost:11434`.
    #[must_use]
    pub fn new() -> Self {
        Self::with_model("llama3.2")
    }

    /// Creates a provider with a specific default model.
    #[must_use]
    pub fn with_model(model: impl Into<String>) -> Self {
        Self {
            client: std::sync::Arc::new(OaiCompatClient::no_auth(DEFAULT_BASE_URL)),
            model: model.into(),
        }
    }

    /// Creates a provider with a custom base URL and model.
    #[must_use]
    pub fn with_base_url(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            client: std::sync::Arc::new(OaiCompatClient::no_auth(base_url)),
            model: model.into(),
        }
    }
}

impl Default for OllamaProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ModelProvider for OllamaProvider {
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let client = std::sync::Arc::clone(&self.client);
        retry_complete("ollama", MAX_RETRIES, || {
            let client = std::sync::Arc::clone(&client);
            let request = request.clone();
            async move { client.do_complete(CHAT_PATH, &request).await }
        })
        .await
    }

    async fn stream(
        &self,
        request: CompletionRequest,
    ) -> Result<BoxStream<Result<CompletionChunk, ProviderError>>, ProviderError> {
        self.client.do_stream(CHAT_PATH, &request).await
    }

    fn cost_usd(&self, _usage: &TokenUsage, _model: &str) -> f64 {
        // Local inference — no cost.
        0.0
    }

    fn name(&self) -> &'static str {
        "ollama"
    }

    fn default_model(&self) -> &'static str {
        Box::leak(self.model.clone().into_boxed_str())
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use kainetic_schema::Message;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn text_body() -> serde_json::Value {
        serde_json::json!({
            "id": "gen-abc",
            "object": "chat.completion",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "Hello from llama!"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 5, "completion_tokens": 4, "total_tokens": 9},
            "model": "llama3.2"
        })
    }

    #[tokio::test]
    async fn complete_returns_text() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(text_body()))
            .mount(&server)
            .await;

        let provider = OllamaProvider::with_base_url(server.uri(), "llama3.2");
        let resp = provider
            .complete(CompletionRequest::new(
                "llama3.2",
                vec![Message::user("Hi")],
            ))
            .await
            .unwrap();

        assert_eq!(resp.text().as_deref(), Some("Hello from llama!"));
    }

    #[test]
    fn cost_is_zero() {
        let p = OllamaProvider::new();
        let usage = TokenUsage::new(1_000_000, 1_000_000);
        assert_eq!(p.cost_usd(&usage, "llama3.2"), 0.0);
    }

    #[test]
    fn name_and_default_model() {
        let p = OllamaProvider::new();
        assert_eq!(p.name(), "ollama");
        assert_eq!(p.default_model(), "llama3.2");
    }

    #[cfg(feature = "integration")]
    #[tokio::test]
    async fn integration_complete() {
        let provider = OllamaProvider::new();
        let resp = match provider
            .complete(
                CompletionRequest::new(
                    "llama3.2",
                    vec![Message::user("Reply with exactly the word 'pong'.")],
                )
                .with_max_tokens(10),
            )
            .await
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Ollama not running locally — skipping ({e})");
                return;
            }
        };
        assert!(resp.text().is_some());
    }
}
