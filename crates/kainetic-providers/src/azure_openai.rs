//! Azure OpenAI Service provider.
//!
//! Azure wraps the OpenAI API but uses resource-specific URLs and `api-key`
//! header authentication instead of `Authorization: Bearer`.
//!
//! # Authentication
//!
//! Set `AZURE_OPENAI_API_KEY`, `AZURE_OPENAI_RESOURCE`, and
//! `AZURE_OPENAI_DEPLOYMENT` environment variables, or construct the provider
//! directly via [`AzureOpenAiProvider::new`].
//!
//! # URL format
//!
//! ```text
//! https://{resource}.openai.azure.com/openai/deployments/{deployment}/chat/completions?api-version=2024-02-01
//! ```

use std::sync::Arc;
use std::time::Duration;

use async_stream::stream;
use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use kainetic_schema::TokenUsage;
use tracing::warn;

use crate::{
    error::ProviderError,
    openai_compat::{
        jitter, map_error_response, messages_to_wire, parse_sse_event, tool_to_wire,
        wire_to_response, OaiRequest,
    },
    provider::ModelProvider,
    types::{BoxStream, CompletionChunk, CompletionRequest, CompletionResponse},
};

const DEFAULT_API_VERSION: &str = "2024-02-01";
const MAX_RETRIES: u32 = 3;

// Azure uses OpenAI model pricing.
fn price_per_million(model: &str) -> (f64, f64) {
    if model.contains("gpt-4o-mini") {
        (0.15, 0.60)
    } else if model.contains("gpt-4o") {
        (2.50, 10.00)
    } else if model.contains("o1") || model.contains("o3") {
        (15.00, 60.00)
    } else {
        (2.50, 10.00)
    }
}

/// Azure OpenAI Service provider.
///
/// Supports `complete()` and `stream()` with automatic retry on rate limits,
/// tool calling, and per-model cost estimation.
#[derive(Clone)]
pub struct AzureOpenAiProvider {
    client: Arc<reqwest::Client>,
    api_key: String,
    /// Full endpoint URL including path and query string.
    endpoint: String,
    deployment: String,
}

impl AzureOpenAiProvider {
    /// Creates a provider.
    ///
    /// - `resource` — Azure resource name (e.g. `my-openai`)
    /// - `deployment` — deployment name (e.g. `gpt-4o`)
    /// - `api_key` — Azure `api-key` header value
    #[must_use]
    pub fn new(
        resource: impl Into<String>,
        deployment: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Self {
        Self::with_api_version(resource, deployment, api_key, DEFAULT_API_VERSION)
    }

    /// Creates a provider with an explicit API version.
    #[must_use]
    pub fn with_api_version(
        resource: impl Into<String>,
        deployment: impl Into<String>,
        api_key: impl Into<String>,
        api_version: &str,
    ) -> Self {
        let resource = resource.into();
        let deployment = deployment.into();
        let endpoint = format!(
            "https://{resource}.openai.azure.com/openai/deployments/{deployment}/chat/completions?api-version={api_version}"
        );
        Self {
            client: Arc::new(reqwest::Client::new()),
            api_key: api_key.into(),
            endpoint,
            deployment,
        }
    }

    /// Creates a provider from environment variables.
    ///
    /// Requires `AZURE_OPENAI_API_KEY`, `AZURE_OPENAI_RESOURCE`, and
    /// `AZURE_OPENAI_DEPLOYMENT` to be set.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::AuthFailed`] if any variable is missing.
    pub fn from_env() -> Result<Self, ProviderError> {
        let api_key =
            std::env::var("AZURE_OPENAI_API_KEY").map_err(|_| ProviderError::AuthFailed)?;
        let resource =
            std::env::var("AZURE_OPENAI_RESOURCE").map_err(|_| ProviderError::AuthFailed)?;
        let deployment =
            std::env::var("AZURE_OPENAI_DEPLOYMENT").map_err(|_| ProviderError::AuthFailed)?;
        Ok(Self::new(resource, deployment, api_key))
    }

    /// Creates a provider pointing at a custom endpoint URL (for testing).
    #[must_use]
    pub fn with_endpoint(
        endpoint: impl Into<String>,
        deployment: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Self {
        Self {
            client: Arc::new(reqwest::Client::new()),
            api_key: api_key.into(),
            endpoint: endpoint.into(),
            deployment: deployment.into(),
        }
    }

    async fn send_request(
        &self,
        request: &CompletionRequest,
        stream: bool,
    ) -> Result<reqwest::Response, ProviderError> {
        let tools = request
            .tools
            .iter()
            .map(tool_to_wire)
            .collect::<Result<Vec<_>, _>>()?;

        let mut messages = Vec::new();
        if let Some(system) = &request.system {
            messages.push(crate::openai_compat::OaiMessage {
                role: "system".into(),
                content: Some(system.clone()),
                tool_calls: None,
                tool_call_id: None,
            });
        }
        messages.extend(messages_to_wire(&request.messages));

        let body = OaiRequest {
            model: request.model.clone(),
            messages,
            tools,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream,
        };

        let response = self
            .client
            .post(&self.endpoint)
            .header("api-key", &self.api_key)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::NetworkError(e.to_string()))?;

        if response.status().is_success() {
            return Ok(response);
        }
        Err(map_error_response(response).await)
    }

    async fn do_complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let response = self.send_request(request, false).await?;
        let body: crate::openai_compat::OaiResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::DeserializationError(e.to_string()))?;
        wire_to_response(body).map_err(ProviderError::DeserializationError)
    }
}

#[async_trait]
impl ModelProvider for AzureOpenAiProvider {
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        for attempt in 0..MAX_RETRIES {
            match self.do_complete(&request).await {
                Err(ProviderError::RateLimited { retry_after }) => {
                    let delay = retry_after.unwrap_or_else(|| Duration::from_secs(1u64 << attempt));
                    warn!(attempt, ?delay, "azure_openai: rate limited, retrying");
                    tokio::time::sleep(delay + jitter()).await;
                }
                result => return result,
            }
        }
        self.do_complete(&request).await
    }

    async fn stream(
        &self,
        request: CompletionRequest,
    ) -> Result<BoxStream<Result<CompletionChunk, ProviderError>>, ProviderError> {
        let response = self.send_request(&request, true).await?;
        let s = stream! {
            let mut event_stream = response.bytes_stream().eventsource();
            while let Some(event_result) = event_stream.next().await {
                let event = match event_result {
                    Ok(e) => e,
                    Err(e) => {
                        yield Err(ProviderError::NetworkError(e.to_string()));
                        return;
                    }
                };
                if event.data == "[DONE]" { break; }
                match parse_sse_event(&event.data) {
                    Ok(Some(chunk)) => yield Ok(chunk),
                    Ok(None) => {}
                    Err(e) => { yield Err(e); return; }
                }
            }
        };
        Ok(Box::pin(s))
    }

    fn cost_usd(&self, usage: &TokenUsage, model: &str) -> f64 {
        // Use deployment name as model if request model is empty.
        let effective_model = if model.is_empty() {
            &self.deployment
        } else {
            model
        };
        let (in_per_m, out_per_m) = price_per_million(effective_model);
        f64::from(usage.prompt_tokens) * in_per_m / 1_000_000.0
            + f64::from(usage.completion_tokens) * out_per_m / 1_000_000.0
    }

    fn name(&self) -> &'static str {
        "azure_openai"
    }

    fn default_model(&self) -> &'static str {
        "gpt-4o"
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use kainetic_schema::Message;
    use wiremock::matchers::{header, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn text_body() -> serde_json::Value {
        serde_json::json!({
            "id": "chatcmpl-azure",
            "object": "chat.completion",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "Hello from Azure!"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 4, "total_tokens": 14},
            "model": "gpt-4o"
        })
    }

    #[tokio::test]
    async fn complete_sends_api_key_header() {
        let server = MockServer::start().await;
        let endpoint = format!(
            "{}/openai/deployments/gpt-4o/chat/completions?api-version=2024-02-01",
            server.uri()
        );

        Mock::given(method("POST"))
            .and(header("api-key", "test-azure-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(text_body()))
            .mount(&server)
            .await;

        let provider = AzureOpenAiProvider::with_endpoint(endpoint, "gpt-4o", "test-azure-key");
        let resp = provider
            .complete(CompletionRequest::new("gpt-4o", vec![Message::user("Hi")]))
            .await
            .unwrap();

        assert_eq!(resp.text().as_deref(), Some("Hello from Azure!"));
    }

    #[tokio::test]
    async fn auth_failed_on_401() {
        let server = MockServer::start().await;
        let endpoint = format!("{}/chat?api-version=2024-02-01", server.uri());

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&server)
            .await;

        let err = AzureOpenAiProvider::with_endpoint(endpoint, "gpt-4o", "bad")
            .complete(CompletionRequest::new("gpt-4o", vec![Message::user("Hi")]))
            .await
            .unwrap_err();

        assert!(matches!(err, ProviderError::AuthFailed));
    }

    #[test]
    fn cost_gpt4o() {
        let p = AzureOpenAiProvider::new("res", "gpt-4o", "key");
        let usage = TokenUsage::new(1_000_000, 1_000_000);
        let cost = p.cost_usd(&usage, "gpt-4o");
        assert!((cost - 12.5).abs() < 0.01);
    }

    #[test]
    fn name_and_model() {
        let p = AzureOpenAiProvider::new("res", "gpt-4o", "key");
        assert_eq!(p.name(), "azure_openai");
        assert_eq!(p.default_model(), "gpt-4o");
    }
}
