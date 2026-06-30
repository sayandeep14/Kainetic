//! Mistral AI API provider.
//!
//! Mistral uses the OpenAI-compatible `/v1/chat/completions` endpoint with
//! `Authorization: Bearer <key>` authentication.
//!
//! # Authentication
//!
//! Set the `MISTRAL_API_KEY` environment variable, or pass the key directly
//! to [`MistralProvider::new`].

use async_trait::async_trait;
use kainetic_schema::TokenUsage;

use crate::{
    error::ProviderError,
    openai_compat::{retry_complete, OaiCompatClient},
    provider::ModelProvider,
    types::{BoxStream, CompletionChunk, CompletionRequest, CompletionResponse},
};

const BASE_URL: &str = "https://api.mistral.ai";
const CHAT_PATH: &str = "/v1/chat/completions";
const MAX_RETRIES: u32 = 3;

// USD per 1M tokens (input, output).
fn price_per_million(model: &str) -> (f64, f64) {
    if model.contains("medium") {
        (0.70, 2.10)
    } else if model.contains("small") || model.contains("tiny") || model.contains("codestral") {
        (0.20, 0.60)
    } else {
        // large or unknown — use mistral-large pricing
        (2.00, 6.00)
    }
}

/// Mistral AI provider.
///
/// Supports `complete()` and `stream()` with automatic retry on rate limits,
/// tool calling, and per-model cost estimation.
#[derive(Clone)]
pub struct MistralProvider {
    client: std::sync::Arc<OaiCompatClient>,
}

impl MistralProvider {
    /// Creates a provider with an explicit API key.
    #[must_use]
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: std::sync::Arc::new(OaiCompatClient::bearer(api_key, BASE_URL)),
        }
    }

    /// Creates a provider using the `MISTRAL_API_KEY` environment variable.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::AuthFailed`] if `MISTRAL_API_KEY` is not set.
    pub fn from_env() -> Result<Self, ProviderError> {
        let api_key = std::env::var("MISTRAL_API_KEY").map_err(|_| ProviderError::AuthFailed)?;
        Ok(Self::new(api_key))
    }

    /// Creates a provider with a custom base URL (useful for testing).
    #[must_use]
    pub fn with_base_url(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            client: std::sync::Arc::new(OaiCompatClient::bearer(api_key, base_url)),
        }
    }
}

#[async_trait]
impl ModelProvider for MistralProvider {
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let client = std::sync::Arc::clone(&self.client);
        retry_complete("mistral", MAX_RETRIES, || {
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

    fn cost_usd(&self, usage: &TokenUsage, model: &str) -> f64 {
        let (in_per_m, out_per_m) = price_per_million(model);
        f64::from(usage.prompt_tokens) * in_per_m / 1_000_000.0
            + f64::from(usage.completion_tokens) * out_per_m / 1_000_000.0
    }

    fn name(&self) -> &'static str {
        "mistral"
    }

    fn default_model(&self) -> &'static str {
        "mistral-large-latest"
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
            "id": "chat-abc",
            "object": "chat.completion",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "Bonjour!"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 8, "completion_tokens": 3, "total_tokens": 11},
            "model": "mistral-large-latest"
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

        let provider = MistralProvider::with_base_url("key", server.uri());
        let resp = provider
            .complete(CompletionRequest::new(
                "mistral-large-latest",
                vec![Message::user("Hello")],
            ))
            .await
            .unwrap();

        assert_eq!(resp.text().as_deref(), Some("Bonjour!"));
    }

    #[tokio::test]
    async fn returns_auth_failed_on_401() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&server)
            .await;

        let err = MistralProvider::with_base_url("bad-key", server.uri())
            .complete(CompletionRequest::new(
                "mistral-large-latest",
                vec![Message::user("Hi")],
            ))
            .await
            .unwrap_err();

        assert!(matches!(err, ProviderError::AuthFailed));
    }

    #[test]
    fn cost_large() {
        let p = MistralProvider::new("k");
        let usage = TokenUsage::new(1_000_000, 1_000_000);
        let cost = p.cost_usd(&usage, "mistral-large-latest");
        assert!((cost - 8.0).abs() < 0.01, "expected $8.00, got {cost}");
    }

    #[test]
    fn name_and_model() {
        let p = MistralProvider::new("k");
        assert_eq!(p.name(), "mistral");
        assert_eq!(p.default_model(), "mistral-large-latest");
    }

    #[cfg(feature = "integration")]
    #[tokio::test]
    async fn integration_complete() {
        let provider = match MistralProvider::from_env() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("MISTRAL_API_KEY not set — skipping ({e})");
                return;
            }
        };
        let resp = match provider
            .complete(
                CompletionRequest::new(
                    "mistral-small-latest",
                    vec![Message::user("Reply with exactly the word 'pong'.")],
                )
                .with_max_tokens(10),
            )
            .await
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Mistral API unavailable — skipping ({e})");
                return;
            }
        };
        assert!(resp.text().is_some());
    }
}
