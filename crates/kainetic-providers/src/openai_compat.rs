//! Shared wire types and HTTP client for OpenAI-compatible APIs.
//!
//! Used by [`super::MistralProvider`], [`super::OllamaProvider`], and
//! [`super::AzureOpenAiProvider`].  The existing [`super::OpenAiProvider`]
//! keeps its own private copies to remain self-contained.

use std::time::Duration;

use async_stream::stream;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use kainetic_schema::{Message, MessageContent, MessageRole, ToolDescriptor, TokenUsage};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::{
    error::ProviderError,
    types::{
        BoxStream, ChunkDelta, CompletionChunk, CompletionRequest, CompletionResponse, StopReason,
    },
};

// ─── Wire request ──────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub(crate) struct OaiRequest {
    pub(crate) model: String,
    pub(crate) messages: Vec<OaiMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) tools: Vec<OaiTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) stream: bool,
}

#[derive(Serialize)]
pub(crate) struct OaiMessage {
    pub(crate) role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_calls: Option<Vec<OaiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_call_id: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct OaiToolCall {
    pub(crate) id: String,
    #[serde(rename = "type")]
    pub(crate) call_type: &'static str,
    pub(crate) function: OaiToolCallFunction,
}

#[derive(Serialize)]
pub(crate) struct OaiToolCallFunction {
    pub(crate) name: String,
    pub(crate) arguments: String,
}

#[derive(Serialize)]
pub(crate) struct OaiTool {
    #[serde(rename = "type")]
    pub(crate) tool_type: &'static str,
    pub(crate) function: OaiToolFunction,
}

#[derive(Serialize)]
pub(crate) struct OaiToolFunction {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) parameters: serde_json::Value,
}

// ─── Wire response ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct OaiResponse {
    pub(crate) choices: Vec<OaiChoice>,
    pub(crate) usage: OaiUsage,
    pub(crate) model: String,
}

#[derive(Deserialize)]
pub(crate) struct OaiChoice {
    pub(crate) message: OaiResponseMessage,
    pub(crate) finish_reason: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct OaiResponseMessage {
    pub(crate) content: Option<String>,
    pub(crate) tool_calls: Option<Vec<OaiResponseToolCall>>,
}

#[derive(Deserialize)]
pub(crate) struct OaiResponseToolCall {
    pub(crate) id: String,
    pub(crate) function: OaiResponseFunction,
}

#[derive(Deserialize)]
pub(crate) struct OaiResponseFunction {
    pub(crate) name: String,
    pub(crate) arguments: String,
}

#[derive(Deserialize)]
#[allow(clippy::struct_field_names)]
pub(crate) struct OaiUsage {
    pub(crate) prompt_tokens: u32,
    pub(crate) completion_tokens: u32,
    pub(crate) total_tokens: u32,
}

// ─── Wire streaming ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct OaiStreamChunk {
    pub(crate) choices: Vec<OaiStreamChoice>,
    #[serde(default)]
    pub(crate) usage: Option<OaiUsage>,
}

#[derive(Deserialize)]
pub(crate) struct OaiStreamChoice {
    pub(crate) delta: OaiStreamDelta,
    pub(crate) finish_reason: Option<String>,
}

#[derive(Deserialize, Default)]
pub(crate) struct OaiStreamDelta {
    pub(crate) content: Option<String>,
    pub(crate) tool_calls: Option<Vec<OaiStreamToolCall>>,
}

#[derive(Deserialize)]
pub(crate) struct OaiStreamToolCall {
    pub(crate) index: usize,
    pub(crate) id: Option<String>,
    pub(crate) function: OaiStreamFunction,
}

#[derive(Deserialize)]
pub(crate) struct OaiStreamFunction {
    pub(crate) name: Option<String>,
    pub(crate) arguments: Option<String>,
}

// ─── Auth strategy ─────────────────────────────────────────────────────────────

/// Authentication strategy for OpenAI-compatible providers.
pub(crate) enum AuthStrategy {
    /// Standard `Authorization: Bearer <key>` used by Mistral, OpenAI, etc.
    Bearer(String),
    /// Custom header/value pair (e.g., Azure `api-key: <key>`).
    #[allow(dead_code)]
    Header { name: &'static str, value: String },
    /// No authentication (local Ollama).
    None,
}

// ─── OaiCompatClient ───────────────────────────────────────────────────────────

/// A reusable HTTP client for OpenAI-compatible `/v1/chat/completions` APIs.
pub(crate) struct OaiCompatClient {
    pub(crate) client: reqwest::Client,
    pub(crate) base_url: String,
    pub(crate) auth: AuthStrategy,
}

impl OaiCompatClient {
    pub(crate) fn bearer(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into(),
            auth: AuthStrategy::Bearer(api_key.into()),
        }
    }

    pub(crate) fn no_auth(base_url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into(),
            auth: AuthStrategy::None,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn header_auth(
        header_name: &'static str,
        header_value: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into(),
            auth: AuthStrategy::Header {
                name: header_name,
                value: header_value.into(),
            },
        }
    }

    /// Send a request to `path` (e.g. `/v1/chat/completions`) on `self.base_url`.
    pub(crate) async fn send_request(
        &self,
        path: &str,
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
            messages.push(OaiMessage {
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

        let url = format!("{}{path}", self.base_url);
        let mut builder = self.client.post(&url).header("content-type", "application/json");

        builder = match &self.auth {
            AuthStrategy::Bearer(key) => builder.bearer_auth(key),
            AuthStrategy::Header { name, value } => builder.header(*name, value.as_str()),
            AuthStrategy::None => builder,
        };

        let response = builder
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::NetworkError(e.to_string()))?;

        if response.status().is_success() {
            return Ok(response);
        }
        Err(map_error_response(response).await)
    }

    /// Non-streaming completion.
    pub(crate) async fn do_complete(
        &self,
        path: &str,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let response = self.send_request(path, request, false).await?;
        let body: OaiResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::DeserializationError(e.to_string()))?;
        wire_to_response(body).map_err(ProviderError::DeserializationError)
    }

    /// Streaming completion — returns an SSE stream.
    pub(crate) async fn do_stream(
        &self,
        path: &str,
        request: &CompletionRequest,
    ) -> Result<BoxStream<Result<CompletionChunk, ProviderError>>, ProviderError> {
        let response = self.send_request(path, request, true).await?;
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
        Ok(Box::pin(s))
    }
}

// ─── Conversion helpers ─────────────────────────────────────────────────────────

pub(crate) fn messages_to_wire(messages: &[Message]) -> Vec<OaiMessage> {
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
                out.push(OaiMessage {
                    role: "user".into(),
                    content: Some(text),
                    tool_calls: None,
                    tool_call_id: None,
                });
            }
            MessageRole::Assistant => {
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

                let tool_calls: Vec<OaiToolCall> = msg
                    .content
                    .iter()
                    .filter_map(|c| {
                        if let MessageContent::ToolUse { id, name, input } = c {
                            let arguments = serde_json::to_string(input).unwrap_or_default();
                            Some(OaiToolCall {
                                id: id.clone(),
                                call_type: "function",
                                function: OaiToolCallFunction {
                                    name: name.clone(),
                                    arguments,
                                },
                            })
                        } else {
                            None
                        }
                    })
                    .collect();

                out.push(OaiMessage {
                    role: "assistant".into(),
                    content: if text.is_empty() { None } else { Some(text) },
                    tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
                    tool_call_id: None,
                });
            }
            MessageRole::Tool => {
                for part in &msg.content {
                    if let MessageContent::ToolResult { tool_use_id, content, .. } = part {
                        out.push(OaiMessage {
                            role: "tool".into(),
                            content: Some(content.clone()),
                            tool_calls: None,
                            tool_call_id: Some(tool_use_id.clone()),
                        });
                    }
                }
            }
            _ => {}
        }
    }
    out
}

pub(crate) fn tool_to_wire(t: &ToolDescriptor) -> Result<OaiTool, ProviderError> {
    let parameters = serde_json::to_value(&t.input_schema)
        .map_err(|e| ProviderError::DeserializationError(e.to_string()))?;
    Ok(OaiTool {
        tool_type: "function",
        function: OaiToolFunction {
            name: t.name.clone(),
            description: t.description.clone(),
            parameters,
        },
    })
}

pub(crate) fn wire_to_response(body: OaiResponse) -> Result<CompletionResponse, String> {
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

pub(crate) fn parse_sse_event(data: &str) -> Result<Option<CompletionChunk>, ProviderError> {
    let chunk: OaiStreamChunk = serde_json::from_str(data)
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

// ─── Error helpers ─────────────────────────────────────────────────────────────

pub(crate) async fn map_error_response(response: reqwest::Response) -> ProviderError {
    let status = response.status().as_u16();
    let retry_after = parse_retry_after(&response);
    let body = response.text().await.unwrap_or_default();
    match status {
        401 => ProviderError::AuthFailed,
        429 => ProviderError::RateLimited { retry_after },
        404 => ProviderError::ModelNotFound(body),
        _ => ProviderError::ApiError { status, message: body },
    }
}

pub(crate) fn parse_retry_after(response: &reqwest::Response) -> Option<Duration> {
    response
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_secs)
}

pub(crate) fn jitter() -> Duration {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    Duration::from_millis(u64::from(nanos % 500))
}

// ─── Retry helper ───────────────────────────────────────────────────────────────

/// Run `complete_fn` up to `max_retries` times, backing off on `RateLimited`.
pub(crate) async fn retry_complete<F, Fut>(
    provider_name: &'static str,
    max_retries: u32,
    mut complete_fn: F,
) -> Result<CompletionResponse, ProviderError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<CompletionResponse, ProviderError>>,
{
    for attempt in 0..max_retries {
        match complete_fn().await {
            Err(ProviderError::RateLimited { retry_after }) => {
                let delay =
                    retry_after.unwrap_or_else(|| Duration::from_secs(1u64 << attempt));
                warn!(attempt, ?delay, "{}: rate limited, retrying", provider_name);
                tokio::time::sleep(delay + jitter()).await;
            }
            result => return result,
        }
    }
    complete_fn().await
}
