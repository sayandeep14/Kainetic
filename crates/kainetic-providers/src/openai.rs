//! OpenAI Chat Completions API provider.

use std::time::Duration;

use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use kainetic_schema::{Message, MessageContent, MessageRole, TokenUsage, ToolDescriptor};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::{
    error::ProviderError,
    provider::ModelProvider,
    types::{
        BoxStream, ChunkDelta, CompletionChunk, CompletionRequest, CompletionResponse, StopReason,
    },
};

// ─── Constants ────────────────────────────────────────────────────────────────

const DEFAULT_BASE_URL: &str = "https://api.openai.com";
const MAX_RETRIES: u32 = 3;

// ─── Wire request types ───────────────────────────────────────────────────────

#[derive(Serialize)]
struct OpenAiRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<OpenAiTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
}

#[derive(Serialize)]
struct OpenAiMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Serialize)]
struct OpenAiToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: &'static str,
    function: OpenAiToolCallFunction,
}

#[derive(Serialize)]
struct OpenAiToolCallFunction {
    name: String,
    arguments: String,
}

#[derive(Serialize)]
struct OpenAiTool {
    #[serde(rename = "type")]
    tool_type: &'static str,
    function: OpenAiToolFunction,
}

#[derive(Serialize)]
struct OpenAiToolFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

// ─── Wire response types ──────────────────────────────────────────────────────

#[derive(Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
    usage: OpenAiUsage,
    model: String,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiResponseMessage {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAiResponseToolCall>>,
}

#[derive(Deserialize)]
struct OpenAiResponseToolCall {
    id: String,
    function: OpenAiResponseFunction,
}

#[derive(Deserialize)]
struct OpenAiResponseFunction {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
#[allow(clippy::struct_field_names)]
struct OpenAiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

// ─── Wire streaming event types ───────────────────────────────────────────────

#[derive(Deserialize)]
struct OpenAiStreamChunk {
    choices: Vec<OpenAiStreamChoice>,
    #[serde(default)]
    usage: Option<OpenAiUsage>,
}

#[derive(Deserialize)]
struct OpenAiStreamChoice {
    delta: OpenAiStreamDelta,
    finish_reason: Option<String>,
}

#[derive(Deserialize, Default)]
struct OpenAiStreamDelta {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAiStreamToolCall>>,
}

#[derive(Deserialize)]
struct OpenAiStreamToolCall {
    index: usize,
    id: Option<String>,
    function: OpenAiStreamFunction,
}

#[derive(Deserialize)]
struct OpenAiStreamFunction {
    name: Option<String>,
    arguments: Option<String>,
}

// ─── Cost table ───────────────────────────────────────────────────────────────

// Prices in USD per 1 million tokens (input, output).
fn price_per_million(model: &str) -> (f64, f64) {
    if model.contains("gpt-4o-mini") {
        (0.15, 0.60)
    } else if model.contains("gpt-4o") {
        (2.50, 10.00)
    } else if model.starts_with("o3-mini") {
        (1.10, 4.40)
    } else if model.starts_with("o1") || model.starts_with("o3") {
        (15.00, 60.00)
    } else {
        // Default to gpt-4o pricing for unknown models.
        (2.50, 10.00)
    }
}

// ─── OpenAiProvider ───────────────────────────────────────────────────────────

/// OpenAI Chat Completions API provider.
///
/// Supports `complete()` and `stream()` with automatic retry on rate limits
/// (HTTP 429), tool calling, and per-model cost estimation.
///
/// # Authentication
///
/// Set the `OPENAI_API_KEY` environment variable, or pass the key directly
/// to [`OpenAiProvider::new`].
#[derive(Clone)]
pub struct OpenAiProvider {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl OpenAiProvider {
    /// Creates a provider using an explicit API key.
    #[must_use]
    pub fn new(api_key: impl Into<String>) -> Self {
        Self::with_base_url(api_key, DEFAULT_BASE_URL)
    }

    /// Creates a provider using the `OPENAI_API_KEY` environment variable.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::AuthFailed`] if `OPENAI_API_KEY` is not set.
    pub fn from_env() -> Result<Self, ProviderError> {
        let api_key = std::env::var("OPENAI_API_KEY").map_err(|_| ProviderError::AuthFailed)?;
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
        let tools: Vec<OpenAiTool> = request
            .tools
            .iter()
            .map(tool_to_wire)
            .collect::<Result<_, _>>()?;

        let mut messages = Vec::new();
        if let Some(system) = &request.system {
            messages.push(OpenAiMessage {
                role: "system".into(),
                content: Some(system.clone()),
                tool_calls: None,
                tool_call_id: None,
            });
        }
        for msg in messages_to_wire(&request.messages) {
            messages.push(msg);
        }

        let body = OpenAiRequest {
            model: request.model.clone(),
            messages,
            tools,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream,
        };

        let response = self
            .client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
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
        let body: OpenAiResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::DeserializationError(e.to_string()))?;

        wire_to_response(body).map_err(|e| ProviderError::DeserializationError(e.clone()))
    }
}

// ─── ModelProvider impl ───────────────────────────────────────────────────────

#[async_trait]
impl ModelProvider for OpenAiProvider {
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        for attempt in 0..MAX_RETRIES {
            match self.do_complete(&request).await {
                Err(ProviderError::RateLimited { retry_after }) => {
                    let delay = retry_after.unwrap_or_else(|| Duration::from_secs(1u64 << attempt));
                    warn!(attempt, ?delay, "openai: rate limited, retrying");
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
        "openai"
    }

    fn default_model(&self) -> &'static str {
        "gpt-4o"
    }
}

// ─── Conversion helpers ───────────────────────────────────────────────────────

fn messages_to_wire(messages: &[Message]) -> Vec<OpenAiMessage> {
    let mut out = Vec::new();
    for msg in messages {
        match msg.role {
            MessageRole::User => {
                let text = msg
                    .content
                    .iter()
                    .filter_map(|c| {
                        if let MessageContent::Text { text } = c {
                            Some(text.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                out.push(OpenAiMessage {
                    role: "user".into(),
                    content: Some(text),
                    tool_calls: None,
                    tool_call_id: None,
                });
            }
            MessageRole::Assistant => {
                let text: String = msg
                    .content
                    .iter()
                    .filter_map(|c| {
                        if let MessageContent::Text { text } = c {
                            Some(text.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                let tool_calls: Vec<OpenAiToolCall> = msg
                    .content
                    .iter()
                    .filter_map(|c| {
                        if let MessageContent::ToolUse { id, name, input } = c {
                            let arguments = serde_json::to_string(input).unwrap_or_default();
                            Some(OpenAiToolCall {
                                id: id.clone(),
                                call_type: "function",
                                function: OpenAiToolCallFunction {
                                    name: name.clone(),
                                    arguments,
                                },
                            })
                        } else {
                            None
                        }
                    })
                    .collect();

                out.push(OpenAiMessage {
                    role: "assistant".into(),
                    content: if text.is_empty() { None } else { Some(text) },
                    tool_calls: if tool_calls.is_empty() {
                        None
                    } else {
                        Some(tool_calls)
                    },
                    tool_call_id: None,
                });
            }
            MessageRole::Tool => {
                for part in &msg.content {
                    if let MessageContent::ToolResult {
                        tool_use_id,
                        content,
                        ..
                    } = part
                    {
                        out.push(OpenAiMessage {
                            role: "tool".into(),
                            content: Some(content.clone()),
                            tool_calls: None,
                            tool_call_id: Some(tool_use_id.clone()),
                        });
                    }
                }
            }
            // System messages handled via CompletionRequest.system; ignore future variants.
            _ => {}
        }
    }
    out
}

fn tool_to_wire(t: &ToolDescriptor) -> Result<OpenAiTool, ProviderError> {
    let parameters = serde_json::to_value(&t.input_schema)
        .map_err(|e| ProviderError::DeserializationError(e.to_string()))?;
    Ok(OpenAiTool {
        tool_type: "function",
        function: OpenAiToolFunction {
            name: t.name.clone(),
            description: t.description.clone(),
            parameters,
        },
    })
}

fn wire_to_response(body: OpenAiResponse) -> Result<CompletionResponse, String> {
    let choice = body
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| "response contained no choices".to_string())?;

    let mut content = Vec::new();

    if let Some(text) = choice.message.content {
        if !text.is_empty() {
            content.push(MessageContent::Text { text });
        }
    }

    if let Some(tool_calls) = choice.message.tool_calls {
        for tc in tool_calls {
            let input: serde_json::Value = serde_json::from_str(&tc.function.arguments)
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
            content.push(MessageContent::ToolUse {
                id: tc.id,
                name: tc.function.name,
                input,
            });
        }
    }

    let stop_reason = match choice.finish_reason.as_deref() {
        Some("tool_calls") => StopReason::ToolUse,
        Some("length") => StopReason::MaxTokens,
        _ => StopReason::EndTurn,
    };

    Ok(CompletionResponse {
        content,
        stop_reason,
        usage: TokenUsage {
            prompt_tokens: body.usage.prompt_tokens,
            completion_tokens: body.usage.completion_tokens,
            total_tokens: body.usage.total_tokens,
        },
        model: body.model,
    })
}

fn parse_sse_event(data: &str) -> Result<Option<CompletionChunk>, ProviderError> {
    let chunk: OpenAiStreamChunk = serde_json::from_str(data)
        .map_err(|e| ProviderError::DeserializationError(e.to_string()))?;

    let Some(choice) = chunk.choices.into_iter().next() else {
        return Ok(None);
    };

    if let Some(reason) = choice.finish_reason {
        let stop_reason = match reason.as_str() {
            "tool_calls" => StopReason::ToolUse,
            "length" => StopReason::MaxTokens,
            _ => StopReason::EndTurn,
        };
        let usage = chunk.usage.map(|u| TokenUsage {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        });
        return Ok(Some(CompletionChunk {
            delta: ChunkDelta::Done { stop_reason },
            usage,
        }));
    }

    if let Some(text) = choice.delta.content {
        return Ok(Some(CompletionChunk {
            delta: ChunkDelta::Text(text),
            usage: None,
        }));
    }

    if let Some(tool_calls) = choice.delta.tool_calls {
        for tc in tool_calls {
            if let (Some(id), Some(name)) = (tc.id, tc.function.name) {
                return Ok(Some(CompletionChunk {
                    delta: ChunkDelta::ToolCallStart { id, name },
                    usage: None,
                }));
            }
            if let Some(partial_json) = tc.function.arguments {
                return Ok(Some(CompletionChunk {
                    delta: ChunkDelta::ToolCallDelta {
                        index: tc.index,
                        partial_json,
                    },
                    usage: None,
                }));
            }
        }
    }

    Ok(None)
}

async fn map_error_response(response: reqwest::Response) -> ProviderError {
    let status = response.status().as_u16();
    let retry_after = parse_retry_after(&response);
    let body = response.text().await.unwrap_or_default();

    match status {
        401 => ProviderError::AuthFailed,
        429 => ProviderError::RateLimited { retry_after },
        404 => ProviderError::ModelNotFound(body),
        _ => ProviderError::ApiError {
            status,
            message: body,
        },
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
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn provider(base_url: &str) -> OpenAiProvider {
        OpenAiProvider::with_base_url("test-key", base_url)
    }

    fn text_response_body() -> serde_json::Value {
        serde_json::json!({
            "id": "chatcmpl-abc",
            "object": "chat.completion",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "Hello, world!"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15},
            "model": "gpt-4o-2024-11-20"
        })
    }

    fn tool_use_response_body() -> serde_json::Value {
        serde_json::json!({
            "id": "chatcmpl-def",
            "object": "chat.completion",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_123",
                        "type": "function",
                        "function": {
                            "name": "web_search",
                            "arguments": "{\"query\":\"rust\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"prompt_tokens": 50, "completion_tokens": 20, "total_tokens": 70},
            "model": "gpt-4o-2024-11-20"
        })
    }

    #[tokio::test]
    async fn complete_text_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(text_response_body()))
            .mount(&server)
            .await;

        let response = provider(&server.uri())
            .complete(CompletionRequest::new(
                "gpt-4o",
                vec![Message::user("Hello")],
            ))
            .await
            .unwrap();

        assert_eq!(response.stop_reason, StopReason::EndTurn);
        assert_eq!(response.text().as_deref(), Some("Hello, world!"));
        assert_eq!(response.usage.prompt_tokens, 10);
    }

    #[tokio::test]
    async fn complete_tool_use_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(tool_use_response_body()))
            .mount(&server)
            .await;

        let response = provider(&server.uri())
            .complete(CompletionRequest::new(
                "gpt-4o",
                vec![Message::user("Search")],
            ))
            .await
            .unwrap();

        assert_eq!(response.stop_reason, StopReason::ToolUse);
        let calls = response.tool_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "web_search");
    }

    #[tokio::test]
    async fn complete_retries_on_429() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(429)
                    .insert_header("retry-after", "0")
                    .set_body_string("rate limited"),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(text_response_body()))
            .mount(&server)
            .await;

        let response = provider(&server.uri())
            .complete(CompletionRequest::new(
                "gpt-4o",
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
                "gpt-4o",
                vec![Message::user("Hello")],
            ))
            .await
            .unwrap_err();

        assert!(matches!(err, ProviderError::AuthFailed));
    }

    #[tokio::test]
    async fn stream_yields_text_chunks() {
        let server = MockServer::start().await;
        let sse_body = concat!(
            "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hi\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n",
        );

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_body),
            )
            .mount(&server)
            .await;

        let mut stream = provider(&server.uri())
            .stream(CompletionRequest::new(
                "gpt-4o",
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
        assert_eq!(text, "Hi");
        assert!(got_done);
    }

    #[test]
    fn cost_usd_gpt4o() {
        let provider = OpenAiProvider::new("key");
        let usage = TokenUsage::new(1_000_000, 1_000_000);
        let cost = provider.cost_usd(&usage, "gpt-4o");
        assert!((cost - 12.5).abs() < 0.01, "expected ~$12.50, got {cost}");
    }

    #[test]
    fn name_and_default_model() {
        let provider = OpenAiProvider::new("key");
        assert_eq!(provider.name(), "openai");
        assert_eq!(provider.default_model(), "gpt-4o");
    }

    #[cfg(feature = "integration")]
    #[tokio::test]
    async fn integration_complete() {
        let provider = match OpenAiProvider::from_env() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("OPENAI_API_KEY not set — skipping ({e})");
                return;
            }
        };
        let request = CompletionRequest::new(
            "gpt-4o-mini",
            vec![Message::user("Reply with exactly the word 'pong'.")],
        )
        .with_max_tokens(10);
        let response = match provider.complete(request).await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("OpenAI API unavailable — skipping ({e})");
                return;
            }
        };
        assert_eq!(response.stop_reason, StopReason::EndTurn);
        assert!(response.text().is_some());
    }
}
