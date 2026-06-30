//! Anthropic Messages API provider.

use std::time::Duration;

use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use kainetic_schema::{Message, MessageContent, MessageRole, ToolDescriptor, TokenUsage};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::{
    error::ProviderError,
    provider::ModelProvider,
    types::{
        BoxStream, ChunkDelta, CompletionChunk, CompletionRequest, CompletionResponse, StopReason,
    },
};

// ─── Anthropic API version ───────────────────────────────────────────────────

const API_VERSION: &str = "2023-06-01";
const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const DEFAULT_MAX_TOKENS: u32 = 4096;
const MAX_RETRIES: u32 = 3;

// ─── Wire request types ───────────────────────────────────────────────────────

#[derive(Serialize)]
struct AnthropicRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<AnthropicTool<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: &'static str,
    content: Vec<AnthropicContent>,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicContent {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        is_error: bool,
    },
}

#[derive(Serialize)]
struct AnthropicTool<'a> {
    name: &'a str,
    description: &'a str,
    input_schema: serde_json::Value,
}

// ─── Wire response types ──────────────────────────────────────────────────────

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicResponseContent>,
    stop_reason: String,
    usage: AnthropicUsage,
    model: String,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicResponseContent {
    Text { text: String },
    ToolUse { id: String, name: String, input: serde_json::Value },
}

#[derive(Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

// ─── Wire streaming event types ───────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicStreamEvent {
    // `message` field is intentionally omitted; serde ignores unknown JSON keys.
    MessageStart {},
    ContentBlockStart {
        // `index` omitted — serde ignores it; only `content_block` is used.
        content_block: AnthropicStreamBlock,
    },
    ContentBlockDelta {
        index: usize,
        delta: AnthropicStreamDelta,
    },
    // `index` omitted — unused.
    ContentBlockStop {},
    MessageDelta {
        delta: AnthropicMessageDelta,
        usage: AnthropicStreamUsage,
    },
    MessageStop,
    Ping,
    Error {
        error: AnthropicApiError,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicStreamBlock {
    // `text` field omitted — serde ignores it; we only use `ToolUse`.
    Text {},
    ToolUse { id: String, name: String },
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicStreamDelta {
    TextDelta { text: String },
    InputJsonDelta { partial_json: String },
}

#[derive(Deserialize)]
struct AnthropicMessageDelta {
    stop_reason: String,
}

#[derive(Deserialize)]
struct AnthropicStreamUsage {
    output_tokens: u32,
}

#[derive(Deserialize)]
struct AnthropicApiError {
    message: String,
}

// ─── Cost table ───────────────────────────────────────────────────────────────

// Prices in USD per 1 million tokens (input, output).
// Update when Anthropic adjusts pricing.
fn price_per_million(model: &str) -> (f64, f64) {
    if model.contains("claude-haiku-4-5") {
        (0.25, 1.25)
    } else if model.contains("claude-sonnet-4-6") {
        (3.00, 15.00)
    } else if model.contains("claude-opus-4-8") {
        (15.00, 75.00)
    } else if model.contains("claude-3-5-haiku") {
        (0.80, 4.00)
    } else if model.contains("claude-3-5-sonnet") {
        (3.00, 15.00)
    } else {
        // Default to Sonnet pricing for unknown models.
        (3.00, 15.00)
    }
}

// ─── AnthropicProvider ────────────────────────────────────────────────────────

/// Anthropic Messages API provider.
///
/// Supports `complete()` and `stream()` with automatic retry on rate limits
/// (HTTP 429/529), tool calling, and per-model cost estimation.
///
/// # Authentication
///
/// Set the `ANTHROPIC_API_KEY` environment variable, or pass the key directly
/// to [`AnthropicProvider::new`].
#[derive(Clone)]
pub struct AnthropicProvider {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl AnthropicProvider {
    /// Creates a provider using an explicit API key.
    #[must_use]
    pub fn new(api_key: impl Into<String>) -> Self {
        Self::with_base_url(api_key, DEFAULT_BASE_URL)
    }

    /// Creates a provider using the `ANTHROPIC_API_KEY` environment variable.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::AuthFailed`] if `ANTHROPIC_API_KEY` is not set.
    pub fn from_env() -> Result<Self, ProviderError> {
        let api_key =
            std::env::var("ANTHROPIC_API_KEY").map_err(|_| ProviderError::AuthFailed)?;
        Ok(Self::new(api_key))
    }

    /// Creates a provider with a custom base URL (for testing with mock servers).
    #[must_use]
    pub fn with_base_url(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.into(),
            base_url: base_url.into(),
        }
    }

    async fn send_request(
        &self,
        request: &CompletionRequest,
        stream: bool,
    ) -> Result<reqwest::Response, ProviderError> {
        let tools: Vec<AnthropicTool<'_>> = request
            .tools
            .iter()
            .map(tool_to_wire)
            .collect::<Result<_, _>>()?;

        let body = AnthropicRequest {
            model: &request.model,
            max_tokens: request.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            messages: messages_to_wire(&request.messages),
            tools,
            system: request.system.as_deref(),
            temperature: request.temperature,
            stream,
        };

        let response = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
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

    async fn do_complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        let response = self.send_request(request, false).await?;
        let body: AnthropicResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::DeserializationError(e.to_string()))?;

        Ok(wire_to_response(body))
    }
}

// ─── ModelProvider impl ───────────────────────────────────────────────────────

#[async_trait]
impl ModelProvider for AnthropicProvider {
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        for attempt in 0..MAX_RETRIES {
            match self.do_complete(&request).await {
                Err(ProviderError::RateLimited { retry_after }) => {
                    let delay =
                        retry_after.unwrap_or_else(|| Duration::from_secs(1u64 << attempt));
                    warn!(attempt, ?delay, "anthropic: rate limited, retrying");
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
        let stream = async_stream::stream! {
            let mut event_stream = response.bytes_stream().eventsource();
            while let Some(event_result) = event_stream.next().await {
                let event = match event_result {
                    Ok(e) => e,
                    Err(e) => {
                        yield Err(ProviderError::NetworkError(e.to_string()));
                        return;
                    }
                };
                if event.data == "[DONE]" {
                    break;
                }
                match parse_sse_event(&event.data) {
                    Ok(Some(chunk)) => yield Ok(chunk),
                    Ok(None) => {}
                    Err(e) => {
                        yield Err(e);
                        return;
                    }
                }
            }
        };
        Ok(Box::pin(stream))
    }

    fn cost_usd(&self, usage: &TokenUsage, model: &str) -> f64 {
        let (input_per_m, output_per_m) = price_per_million(model);
        f64::from(usage.prompt_tokens) * input_per_m / 1_000_000.0
            + f64::from(usage.completion_tokens) * output_per_m / 1_000_000.0
    }

    fn name(&self) -> &'static str {
        "anthropic"
    }

    fn default_model(&self) -> &'static str {
        "claude-sonnet-4-6"
    }
}

// ─── Conversion helpers ───────────────────────────────────────────────────────

fn messages_to_wire(messages: &[Message]) -> Vec<AnthropicMessage> {
    messages
        .iter()
        .filter(|m| m.role != MessageRole::System)
        .map(|m| {
            let role: &'static str = match m.role {
                MessageRole::User | MessageRole::Tool => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::System => unreachable!("filtered above"),
                _ => unreachable!("unknown message role"),
            };
            let content = m.content.iter().map(content_to_wire).collect();
            AnthropicMessage { role, content }
        })
        .collect()
}

fn content_to_wire(c: &MessageContent) -> AnthropicContent {
    match c {
        MessageContent::Text { text } => AnthropicContent::Text { text: text.clone() },
        MessageContent::ToolUse { id, name, input } => AnthropicContent::ToolUse {
            id: id.clone(),
            name: name.clone(),
            input: input.clone(),
        },
        MessageContent::ToolResult {
            tool_use_id,
            content,
            is_error,
        } => AnthropicContent::ToolResult {
            tool_use_id: tool_use_id.clone(),
            content: content.clone(),
            is_error: *is_error,
        },
        _ => unreachable!("unknown message content variant"),
    }
}

fn tool_to_wire(t: &ToolDescriptor) -> Result<AnthropicTool<'_>, ProviderError> {
    let input_schema = serde_json::to_value(&t.input_schema)
        .map_err(|e| ProviderError::DeserializationError(e.to_string()))?;
    Ok(AnthropicTool {
        name: &t.name,
        description: &t.description,
        input_schema,
    })
}

fn wire_to_response(body: AnthropicResponse) -> CompletionResponse {
    let content = body
        .content
        .into_iter()
        .map(|c| match c {
            AnthropicResponseContent::Text { text } => MessageContent::Text { text },
            AnthropicResponseContent::ToolUse { id, name, input } => {
                MessageContent::ToolUse { id, name, input }
            }
        })
        .collect();

    let stop_reason = match body.stop_reason.as_str() {
        "tool_use" => StopReason::ToolUse,
        "max_tokens" => StopReason::MaxTokens,
        "stop_sequence" => StopReason::StopSequence,
        _ => StopReason::EndTurn,
    };

    CompletionResponse {
        content,
        stop_reason,
        usage: TokenUsage {
            prompt_tokens: body.usage.input_tokens,
            completion_tokens: body.usage.output_tokens,
            total_tokens: body.usage.input_tokens + body.usage.output_tokens,
        },
        model: body.model,
    }
}

fn parse_sse_event(data: &str) -> Result<Option<CompletionChunk>, ProviderError> {
    let event: AnthropicStreamEvent = serde_json::from_str(data)
        .map_err(|e| ProviderError::DeserializationError(e.to_string()))?;

    let chunk = match event {
        AnthropicStreamEvent::ContentBlockStart {
            content_block: AnthropicStreamBlock::ToolUse { id, name },
        } => Some(CompletionChunk {
            delta: ChunkDelta::ToolCallStart { id, name },
            usage: None,
        }),
        AnthropicStreamEvent::ContentBlockDelta {
            index: _,
            delta: AnthropicStreamDelta::TextDelta { text },
        } => Some(CompletionChunk {
            delta: ChunkDelta::Text(text),
            usage: None,
        }),
        AnthropicStreamEvent::ContentBlockDelta {
            index,
            delta: AnthropicStreamDelta::InputJsonDelta { partial_json },
        } => Some(CompletionChunk {
            delta: ChunkDelta::ToolCallDelta { index, partial_json },
            usage: None,
        }),
        AnthropicStreamEvent::MessageDelta { delta, usage } => Some(CompletionChunk {
            delta: ChunkDelta::Done {
                stop_reason: match delta.stop_reason.as_str() {
                    "tool_use" => StopReason::ToolUse,
                    "max_tokens" => StopReason::MaxTokens,
                    _ => StopReason::EndTurn,
                },
            },
            usage: Some(TokenUsage {
                prompt_tokens: 0,
                completion_tokens: usage.output_tokens,
                total_tokens: usage.output_tokens,
            }),
        }),
        AnthropicStreamEvent::Error { error } => {
            return Err(ProviderError::ApiError {
                status: 200,
                message: error.message,
            });
        }
        _ => None,
    };

    Ok(chunk)
}

async fn map_error_response(response: reqwest::Response) -> ProviderError {
    let status = response.status().as_u16();
    let retry_after = parse_retry_after(&response);
    let body = response.text().await.unwrap_or_default();

    match status {
        401 => ProviderError::AuthFailed,
        429 | 529 => ProviderError::RateLimited { retry_after },
        404 => ProviderError::ModelNotFound(body),
        _ => ProviderError::ApiError { status, message: body },
    }
}

fn parse_retry_after(response: &reqwest::Response) -> Option<Duration> {
    response
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_secs)
}

fn jitter() -> Duration {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    Duration::from_millis(u64::from(nanos % 500))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use kainetic_schema::Message;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn provider(base_url: &str) -> AnthropicProvider {
        AnthropicProvider::with_base_url("test-key", base_url)
    }

    fn text_response_body() -> serde_json::Value {
        serde_json::json!({
            "id": "msg_01abc",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "Hello, world!"}],
            "model": "claude-sonnet-4-6-20251001",
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 10, "output_tokens": 5}
        })
    }

    fn tool_use_response_body() -> serde_json::Value {
        serde_json::json!({
            "id": "msg_02abc",
            "type": "message",
            "role": "assistant",
            "content": [
                {"type": "text", "text": "Let me search that."},
                {
                    "type": "tool_use",
                    "id": "tu_123",
                    "name": "web_search",
                    "input": {"query": "rust programming"}
                }
            ],
            "model": "claude-sonnet-4-6-20251001",
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 50, "output_tokens": 30}
        })
    }

    #[tokio::test]
    async fn complete_text_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "test-key"))
            .and(header("anthropic-version", API_VERSION))
            .respond_with(ResponseTemplate::new(200).set_body_json(text_response_body()))
            .mount(&server)
            .await;

        let response = provider(&server.uri())
            .complete(CompletionRequest::new(
                "claude-sonnet-4-6",
                vec![Message::user("Hello")],
            ))
            .await
            .unwrap();

        assert_eq!(response.stop_reason, StopReason::EndTurn);
        assert_eq!(response.text().as_deref(), Some("Hello, world!"));
        assert_eq!(response.usage.prompt_tokens, 10);
        assert_eq!(response.usage.completion_tokens, 5);
    }

    #[tokio::test]
    async fn complete_tool_use_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(tool_use_response_body()))
            .mount(&server)
            .await;

        let response = provider(&server.uri())
            .complete(CompletionRequest::new(
                "claude-sonnet-4-6",
                vec![Message::user("Search for rust")],
            ))
            .await
            .unwrap();

        assert_eq!(response.stop_reason, StopReason::ToolUse);
        let calls = response.tool_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "web_search");
        assert_eq!(calls[0].input["query"], "rust programming");
    }

    #[tokio::test]
    async fn complete_retries_on_429() {
        let server = MockServer::start().await;

        // First call returns 429.
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(
                ResponseTemplate::new(429)
                    .insert_header("retry-after", "0")
                    .set_body_string("rate limited"),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;

        // Second call succeeds.
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(text_response_body()))
            .mount(&server)
            .await;

        let response = provider(&server.uri())
            .complete(CompletionRequest::new(
                "claude-sonnet-4-6",
                vec![Message::user("Hello")],
            ))
            .await
            .unwrap();

        assert_eq!(response.stop_reason, StopReason::EndTurn);
    }

    #[tokio::test]
    async fn complete_returns_auth_failed_on_401() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&server)
            .await;

        let err = provider(&server.uri())
            .complete(CompletionRequest::new(
                "claude-sonnet-4-6",
                vec![Message::user("Hello")],
            ))
            .await
            .unwrap_err();

        assert!(matches!(err, ProviderError::AuthFailed));
    }

    #[tokio::test]
    async fn complete_returns_deserialization_error_on_bad_json() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;

        let err = provider(&server.uri())
            .complete(CompletionRequest::new(
                "claude-sonnet-4-6",
                vec![Message::user("Hello")],
            ))
            .await
            .unwrap_err();

        assert!(matches!(err, ProviderError::DeserializationError(_)));
    }

    #[tokio::test]
    async fn stream_yields_text_chunks() {
        let server = MockServer::start().await;
        let sse_body = concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_01\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":1}}}\n\n",
            "event: content_block_start\n",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n",
            "event: content_block_stop\n",
            "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "event: message_delta\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":5}}\n\n",
            "event: message_stop\n",
            "data: {\"type\":\"message_stop\"}\n\n",
        );

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_body),
            )
            .mount(&server)
            .await;

        let mut stream = provider(&server.uri())
            .stream(CompletionRequest::new(
                "claude-sonnet-4-6",
                vec![Message::user("Hello")],
            ))
            .await
            .unwrap();

        let mut text = String::new();
        let mut got_done = false;
        while let Some(chunk) = stream.next().await {
            match chunk.unwrap().delta {
                ChunkDelta::Text(t) => text.push_str(&t),
                ChunkDelta::Done { .. } => got_done = true,
                _ => {}
            }
        }
        assert_eq!(text, "Hello");
        assert!(got_done);
    }

    #[test]
    fn cost_usd_sonnet() {
        let provider = AnthropicProvider::new("key");
        let usage = TokenUsage::new(1_000_000, 1_000_000);
        let cost = provider.cost_usd(&usage, "claude-sonnet-4-6");
        assert!((cost - 18.0).abs() < 0.01, "expected ~$18, got {cost}");
    }

    #[test]
    fn name_and_default_model() {
        let provider = AnthropicProvider::new("key");
        assert_eq!(provider.name(), "anthropic");
        assert_eq!(provider.default_model(), "claude-sonnet-4-6");
    }

    #[cfg(feature = "integration")]
    #[tokio::test]
    async fn integration_complete() {
        let provider = match AnthropicProvider::from_env() {
            Ok(p) => p,
            Err(e) => { eprintln!("ANTHROPIC_API_KEY not set — skipping ({e})"); return; }
        };
        let request = CompletionRequest::new(
            "claude-haiku-4-5-20251001",
            vec![Message::user("Reply with exactly the word 'pong'.")],
        )
        .with_max_tokens(10);
        let response = match provider.complete(request).await {
            Ok(r) => r,
            Err(e) => { eprintln!("Anthropic API unavailable — skipping ({e})"); return; }
        };
        assert_eq!(response.stop_reason, StopReason::EndTurn);
        assert!(response.text().is_some());
    }
}
