//! Google Gemini API provider.

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

// ─── Constants ────────────────────────────────────────────────────────────────

const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com";
const MAX_RETRIES: u32 = 3;

// ─── Wire request types ───────────────────────────────────────────────────────

#[derive(Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "systemInstruction")]
    system_instruction: Option<GeminiSystemInstruction>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<GeminiToolSet>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "generationConfig")]
    generation_config: Option<GeminiGenerationConfig>,
}

/// Shared content type used in both request history and response candidates.
#[derive(Serialize, Deserialize)]
struct GeminiContent {
    role: String,
    /// Gemini 2.5 thinking models may omit `parts` when the token budget is
    /// exhausted by thinking tokens; default to empty to avoid a parse error.
    #[serde(default)]
    parts: Vec<GeminiPart>,
}

/// A single part within a [`GeminiContent`].
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
enum GeminiPart {
    /// Plain text.
    Text { text: String },
    /// A function call emitted by the model (in response) or replayed in history.
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: GeminiFunctionCall,
    },
    /// A function execution result sent back to the model.
    FunctionResponse {
        #[serde(rename = "functionResponse")]
        function_response: GeminiFunctionResponse,
    },
    /// Unrecognised part type; silently skipped during conversion.
    Unknown(serde_json::Value),
}

#[derive(Serialize, Deserialize)]
struct GeminiFunctionCall {
    name: String,
    args: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
struct GeminiFunctionResponse {
    name: String,
    response: serde_json::Value,
}

#[derive(Serialize)]
struct GeminiSystemInstruction {
    parts: Vec<GeminiTextPart>,
}

#[derive(Serialize)]
struct GeminiTextPart {
    text: String,
}

#[derive(Serialize)]
struct GeminiToolSet {
    #[serde(rename = "functionDeclarations")]
    function_declarations: Vec<GeminiFunctionDeclaration>,
}

#[derive(Serialize)]
struct GeminiFunctionDeclaration {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Serialize)]
struct GeminiGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none", rename = "maxOutputTokens")]
    max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

// ─── Wire response types ──────────────────────────────────────────────────────

#[derive(Deserialize)]
struct GeminiResponse {
    #[serde(default)]
    candidates: Vec<GeminiCandidate>,
    #[serde(rename = "usageMetadata", default)]
    usage_metadata: Option<GeminiUsageMetadata>,
    #[serde(rename = "modelVersion")]
    model_version: Option<String>,
}

#[derive(Deserialize)]
struct GeminiCandidate {
    content: Option<GeminiContent>,
    #[serde(rename = "finishReason")]
    finish_reason: Option<String>,
}

#[derive(Deserialize, Default)]
#[allow(clippy::struct_field_names)]
struct GeminiUsageMetadata {
    #[serde(rename = "promptTokenCount", default)]
    prompt_token_count: u32,
    #[serde(rename = "candidatesTokenCount", default)]
    candidates_token_count: u32,
    #[serde(rename = "totalTokenCount", default)]
    total_token_count: u32,
}

#[derive(Deserialize)]
struct GeminiErrorBody {
    error: GeminiErrorDetail,
}

#[derive(Deserialize)]
struct GeminiErrorDetail {
    message: String,
}

// ─── Cost table ───────────────────────────────────────────────────────────────

// Prices in USD per 1 million tokens (input, output).
fn price_per_million(model: &str) -> (f64, f64) {
    if model.contains("gemini-2.5-flash") {
        (0.30, 2.50)
    } else if model.contains("gemini-2.5-pro") {
        (1.25, 10.00)
    } else if model.contains("gemini-2.0-flash") {
        (0.10, 0.40)
    } else if model.contains("gemini-1.5-pro") {
        (1.25, 5.00)
    } else if model.contains("gemini-1.5-flash") {
        (0.075, 0.30)
    } else {
        (0.30, 2.50)
    }
}

// ─── GeminiProvider ───────────────────────────────────────────────────────────

/// Google Gemini API provider.
///
/// Supports `complete()` and `stream()` with automatic retry on rate limits
/// (HTTP 429), tool calling via function declarations, and per-model cost
/// estimation.
///
/// # Authentication
///
/// The API key is transmitted as a query parameter. Set the `GEMINI_API_KEY`
/// environment variable, or pass the key directly to [`GeminiProvider::new`].
///
/// # Tool calling
///
/// Gemini does not assign unique IDs to function calls; the function name is
/// used as the call ID. A given tool may therefore be called at most once per
/// model turn when using the `ReActLoop`.
#[derive(Clone)]
pub struct GeminiProvider {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl GeminiProvider {
    /// Creates a provider using an explicit API key.
    #[must_use]
    pub fn new(api_key: impl Into<String>) -> Self {
        Self::with_base_url(api_key, DEFAULT_BASE_URL)
    }

    /// Creates a provider using the `GEMINI_API_KEY` environment variable.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::AuthFailed`] if `GEMINI_API_KEY` is not set.
    pub fn from_env() -> Result<Self, ProviderError> {
        let api_key = std::env::var("GEMINI_API_KEY").map_err(|_| ProviderError::AuthFailed)?;
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

    fn endpoint(&self, model: &str, stream: bool) -> String {
        if stream {
            format!(
                "{}/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
                self.base_url, model, self.api_key
            )
        } else {
            format!(
                "{}/v1beta/models/{}:generateContent?key={}",
                self.base_url, model, self.api_key
            )
        }
    }

    async fn send_request(
        &self,
        request: &CompletionRequest,
        stream: bool,
    ) -> Result<reqwest::Response, ProviderError> {
        let declarations = request
            .tools
            .iter()
            .map(tool_to_declaration)
            .collect::<Result<Vec<_>, _>>()?;

        let contents = messages_to_contents(&request.messages);

        let system_instruction = request.system.as_ref().map(|s| GeminiSystemInstruction {
            parts: vec![GeminiTextPart { text: s.clone() }],
        });

        let generation_config = if request.max_tokens.is_some() || request.temperature.is_some() {
            Some(GeminiGenerationConfig {
                max_output_tokens: request.max_tokens,
                temperature: request.temperature,
            })
        } else {
            None
        };

        let tools = if declarations.is_empty() {
            vec![]
        } else {
            vec![GeminiToolSet {
                function_declarations: declarations,
            }]
        };

        let body = GeminiRequest {
            contents,
            system_instruction,
            tools,
            generation_config,
        };

        let response = self
            .client
            .post(self.endpoint(&request.model, stream))
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
        let model = request.model.clone();
        let body: GeminiResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::DeserializationError(e.to_string()))?;
        wire_to_response(body, &model).map_err(ProviderError::DeserializationError)
    }
}

// ─── ModelProvider impl ───────────────────────────────────────────────────────

#[async_trait]
impl ModelProvider for GeminiProvider {
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        for attempt in 0..MAX_RETRIES {
            match self.do_complete(&request).await {
                Err(ProviderError::RateLimited { retry_after }) => {
                    let delay =
                        retry_after.unwrap_or_else(|| Duration::from_secs(1u64 << attempt));
                    warn!(attempt, ?delay, "gemini: rate limited, retrying");
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
                match parse_sse_event(&event.data) {
                    Ok(Some(chunks)) => {
                        for chunk in chunks {
                            yield Ok(chunk);
                        }
                    }
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
        "google"
    }

    fn default_model(&self) -> &'static str {
        "gemini-2.5-flash"
    }
}

// ─── Conversion helpers ───────────────────────────────────────────────────────

/// Converts a message list to Gemini `contents`, merging consecutive same-role
/// entries (e.g. multiple `ToolResult` messages emitted by the `ReActLoop`).
fn messages_to_contents(messages: &[Message]) -> Vec<GeminiContent> {
    let mut contents: Vec<GeminiContent> = Vec::new();

    for msg in messages {
        let role = match msg.role {
            MessageRole::User | MessageRole::Tool => "user".to_owned(),
            MessageRole::Assistant => "model".to_owned(),
            // System prompts flow through `CompletionRequest::system`; skip here.
            _ => continue,
        };

        let parts = message_to_parts(msg);
        if parts.is_empty() {
            continue;
        }

        // Merge consecutive entries with the same role (Gemini requires alternating turns).
        if let Some(last) = contents.last_mut() {
            if last.role == role {
                last.parts.extend(parts);
                continue;
            }
        }

        contents.push(GeminiContent { role, parts });
    }

    contents
}

fn message_to_parts(msg: &Message) -> Vec<GeminiPart> {
    let mut parts = Vec::new();
    for content in &msg.content {
        match content {
            MessageContent::Text { text } => {
                parts.push(GeminiPart::Text { text: text.clone() });
            }
            MessageContent::ToolUse { name, input, .. } => {
                parts.push(GeminiPart::FunctionCall {
                    function_call: GeminiFunctionCall {
                        name: name.clone(),
                        args: input.clone(),
                    },
                });
            }
            MessageContent::ToolResult {
                tool_use_id,
                content,
                ..
            } => {
                let response = serde_json::from_str::<serde_json::Value>(content)
                    .unwrap_or_else(|_| serde_json::json!({ "output": content }));
                parts.push(GeminiPart::FunctionResponse {
                    function_response: GeminiFunctionResponse {
                        name: tool_use_id.clone(),
                        response,
                    },
                });
            }
            _ => {}
        }
    }
    parts
}

fn tool_to_declaration(t: &ToolDescriptor) -> Result<GeminiFunctionDeclaration, ProviderError> {
    let parameters = serde_json::to_value(&t.input_schema)
        .map_err(|e| ProviderError::DeserializationError(e.to_string()))?;
    Ok(GeminiFunctionDeclaration {
        name: t.name.clone(),
        description: t.description.clone(),
        parameters,
    })
}

fn wire_to_response(body: GeminiResponse, model: &str) -> Result<CompletionResponse, String> {
    let GeminiResponse {
        candidates,
        usage_metadata,
        model_version,
    } = body;

    let candidate = candidates
        .into_iter()
        .next()
        .ok_or_else(|| "response contained no candidates".to_string())?;

    let mut content = Vec::new();

    if let Some(gemini_content) = candidate.content {
        for part in gemini_content.parts {
            match part {
                GeminiPart::Text { text } if !text.is_empty() => {
                    content.push(MessageContent::Text { text });
                }
                GeminiPart::FunctionCall { function_call } => {
                    // Gemini omits call IDs; use the function name as the ID so that
                    // `ToolCallResult.tool_call_id` can be round-tripped as the
                    // `functionResponse.name` on the next turn.
                    content.push(MessageContent::ToolUse {
                        id: function_call.name.clone(),
                        name: function_call.name,
                        input: function_call.args,
                    });
                }
                _ => {}
            }
        }
    }

    let has_tool_calls = content
        .iter()
        .any(|c| matches!(c, MessageContent::ToolUse { .. }));

    let stop_reason = if has_tool_calls {
        StopReason::ToolUse
    } else {
        match candidate.finish_reason.as_deref() {
            Some("MAX_TOKENS") => StopReason::MaxTokens,
            _ => StopReason::EndTurn,
        }
    };

    let usage = usage_metadata.map_or_else(
        || TokenUsage::new(0, 0),
        |u| TokenUsage {
            prompt_tokens: u.prompt_token_count,
            completion_tokens: u.candidates_token_count,
            total_tokens: u.total_token_count,
        },
    );

    let resolved_model = model_version.unwrap_or_else(|| model.to_owned());

    Ok(CompletionResponse {
        content,
        stop_reason,
        usage,
        model: resolved_model,
    })
}

fn parse_sse_event(data: &str) -> Result<Option<Vec<CompletionChunk>>, ProviderError> {
    let GeminiResponse {
        candidates,
        usage_metadata,
        model_version: _,
    } = serde_json::from_str(data)
        .map_err(|e| ProviderError::DeserializationError(e.to_string()))?;

    let Some(candidate) = candidates.into_iter().next() else {
        return Ok(None);
    };

    let mut chunks: Vec<CompletionChunk> = Vec::new();

    if let Some(content) = candidate.content {
        for (idx, part) in content.parts.into_iter().enumerate() {
            match part {
                GeminiPart::Text { text } if !text.is_empty() => {
                    chunks.push(CompletionChunk {
                        delta: ChunkDelta::Text(text),
                        usage: None,
                    });
                }
                GeminiPart::FunctionCall { function_call } => {
                    chunks.push(CompletionChunk {
                        delta: ChunkDelta::ToolCallStart {
                            id: function_call.name.clone(),
                            name: function_call.name.clone(),
                        },
                        usage: None,
                    });
                    let partial_json =
                        serde_json::to_string(&function_call.args).unwrap_or_default();
                    chunks.push(CompletionChunk {
                        delta: ChunkDelta::ToolCallDelta {
                            index: idx,
                            partial_json,
                        },
                        usage: None,
                    });
                }
                _ => {}
            }
        }
    }

    if let Some(reason) = candidate.finish_reason {
        let has_tool = chunks
            .iter()
            .any(|c| matches!(c.delta, ChunkDelta::ToolCallStart { .. }));
        let stop_reason = if has_tool {
            StopReason::ToolUse
        } else {
            match reason.as_str() {
                "MAX_TOKENS" => StopReason::MaxTokens,
                _ => StopReason::EndTurn,
            }
        };
        let usage = usage_metadata.map(|u| TokenUsage {
            prompt_tokens: u.prompt_token_count,
            completion_tokens: u.candidates_token_count,
            total_tokens: u.total_token_count,
        });
        chunks.push(CompletionChunk {
            delta: ChunkDelta::Done { stop_reason },
            usage,
        });
    }

    if chunks.is_empty() {
        Ok(None)
    } else {
        Ok(Some(chunks))
    }
}

async fn map_error_response(response: reqwest::Response) -> ProviderError {
    let status = response.status().as_u16();
    let retry_after = parse_retry_after(&response);
    let body = response.text().await.unwrap_or_default();

    let message = serde_json::from_str::<GeminiErrorBody>(&body)
        .ok()
        .map(|b| b.error.message)
        .unwrap_or(body);

    match status {
        401 | 403 => ProviderError::AuthFailed,
        429 => ProviderError::RateLimited { retry_after },
        404 => ProviderError::ModelNotFound(message),
        _ => ProviderError::ApiError { status, message },
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

    fn provider(base_url: &str) -> GeminiProvider {
        GeminiProvider::with_base_url("test-key", base_url)
    }

    fn text_response_body() -> serde_json::Value {
        serde_json::json!({
            "candidates": [{
                "content": {"role": "model", "parts": [{"text": "Hello, world!"}]},
                "finishReason": "STOP",
                "index": 0
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 5,
                "totalTokenCount": 15
            }
        })
    }

    fn tool_use_response_body() -> serde_json::Value {
        serde_json::json!({
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{"functionCall": {"name": "web_search", "args": {"query": "rust"}}}]
                },
                "finishReason": "STOP",
                "index": 0
            }],
            "usageMetadata": {
                "promptTokenCount": 50,
                "candidatesTokenCount": 20,
                "totalTokenCount": 70
            }
        })
    }

    #[tokio::test]
    async fn complete_text_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1beta/models/gemini-2.5-flash:generateContent"))
            .respond_with(ResponseTemplate::new(200).set_body_json(text_response_body()))
            .mount(&server)
            .await;

        let response = provider(&server.uri())
            .complete(CompletionRequest::new(
                "gemini-2.5-flash",
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
            .and(path("/v1beta/models/gemini-2.5-flash:generateContent"))
            .respond_with(ResponseTemplate::new(200).set_body_json(tool_use_response_body()))
            .mount(&server)
            .await;

        let response = provider(&server.uri())
            .complete(CompletionRequest::new(
                "gemini-2.5-flash",
                vec![Message::user("Search")],
            ))
            .await
            .unwrap();

        assert_eq!(response.stop_reason, StopReason::ToolUse);
        let calls = response.tool_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "web_search");
        assert_eq!(calls[0].id, "web_search");
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
                "gemini-2.5-flash",
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
                "gemini-2.5-flash",
                vec![Message::user("Hello")],
            ))
            .await
            .unwrap_err();

        assert!(matches!(err, ProviderError::AuthFailed));
    }

    #[tokio::test]
    async fn stream_yields_text_chunks() {
        let server = MockServer::start().await;
        // Two SSE events: one with text, one with finishReason.
        let sse_body = concat!(
            "data: {\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"Hi\"}]},\"index\":0}]}\n\n",
            "data: {\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[]},\"finishReason\":\"STOP\",\"index\":0}],",
            "\"usageMetadata\":{\"promptTokenCount\":5,\"candidatesTokenCount\":2,\"totalTokenCount\":7}}\n\n",
        );

        Mock::given(method("POST"))
            .and(path("/v1beta/models/gemini-2.5-flash:streamGenerateContent"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_body),
            )
            .mount(&server)
            .await;

        let mut stream = provider(&server.uri())
            .stream(CompletionRequest::new(
                "gemini-2.5-flash",
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
    fn cost_usd_gemini_25_flash() {
        let provider = GeminiProvider::new("key");
        let usage = TokenUsage::new(1_000_000, 1_000_000);
        let cost = provider.cost_usd(&usage, "gemini-2.5-flash");
        // $0.30 input + $2.50 output = $2.80 per million tokens each
        assert!((cost - 2.80).abs() < 0.01, "expected ~$2.80, got {cost}");
    }

    #[test]
    fn name_and_default_model() {
        let provider = GeminiProvider::new("key");
        assert_eq!(provider.name(), "google");
        assert_eq!(provider.default_model(), "gemini-2.5-flash");
    }

    #[test]
    fn messages_to_contents_merges_consecutive_tool_results() {
        let messages = vec![
            Message::user("query"),
            Message {
                role: MessageRole::Assistant,
                content: vec![
                    MessageContent::ToolUse {
                        id: "tool_a".into(),
                        name: "tool_a".into(),
                        input: serde_json::json!({}),
                    },
                    MessageContent::ToolUse {
                        id: "tool_b".into(),
                        name: "tool_b".into(),
                        input: serde_json::json!({}),
                    },
                ],
            },
            Message {
                role: MessageRole::Tool,
                content: vec![MessageContent::ToolResult {
                    tool_use_id: "tool_a".into(),
                    content: "result_a".into(),
                    is_error: false,
                }],
            },
            Message {
                role: MessageRole::Tool,
                content: vec![MessageContent::ToolResult {
                    tool_use_id: "tool_b".into(),
                    content: "result_b".into(),
                    is_error: false,
                }],
            },
        ];
        let contents = messages_to_contents(&messages);
        // [user, model, user(2 tool responses merged)]
        assert_eq!(contents.len(), 3);
        assert_eq!(contents[0].role, "user");
        assert_eq!(contents[1].role, "model");
        assert_eq!(contents[2].role, "user");
        assert_eq!(contents[2].parts.len(), 2);
    }

    #[cfg(feature = "integration")]
    #[tokio::test]
    async fn integration_complete() {
        let provider = match GeminiProvider::from_env() {
            Ok(p) => p,
            Err(e) => { eprintln!("GEMINI_API_KEY not set — skipping ({e})"); return; }
        };
        // Use a generous token limit — gemini-2.5-flash is a thinking model and
        // consumes thinking tokens before output tokens, so a very small budget
        // produces an empty content body.
        let request = CompletionRequest::new(
            "gemini-2.5-flash",
            vec![Message::user("Reply with exactly the word 'pong'.")],
        )
        .with_max_tokens(512);
        let response = match provider.complete(request).await {
            Ok(r) => r,
            Err(e) => { eprintln!("Gemini API unavailable — skipping ({e})"); return; }
        };
        assert!(
            matches!(
                response.stop_reason,
                StopReason::EndTurn | StopReason::MaxTokens
            ),
            "unexpected stop reason: {:?}",
            response.stop_reason
        );
        assert!(response.text().is_some(), "expected non-empty text response");
    }
}
