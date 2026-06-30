//! Provider-agnostic completion request and response types.

use futures::Stream;
use kainetic_schema::{Message, MessageContent, MessageRole, TokenUsage, ToolDescriptor};
use serde::{Deserialize, Serialize};
use std::pin::Pin;

/// A boxed, heap-allocated, `Send` stream of completion chunks.
pub type BoxStream<T> = Pin<Box<dyn Stream<Item = T> + Send + 'static>>;

/// The reason a model stopped generating tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum StopReason {
    /// The model reached a natural stopping point.
    EndTurn,
    /// The model requested one or more tool calls.
    ToolUse,
    /// The `max_tokens` budget was exhausted.
    MaxTokens,
    /// A stop sequence was matched.
    StopSequence,
}

/// A tool call requested by the model within a completion.
#[derive(Debug, Clone)]
pub struct ToolCall {
    /// Provider-assigned unique identifier for this call.
    ///
    /// Must be echoed back in the corresponding [`ToolCallResult`].
    pub id: String,
    /// The registered name of the tool to invoke.
    pub name: String,
    /// The parsed JSON input to pass to the tool.
    pub input: serde_json::Value,
}

/// The result of a tool invocation, ready to be inserted into the conversation.
#[derive(Debug, Clone)]
pub struct ToolCallResult {
    /// The `id` from the corresponding [`ToolCall`].
    pub tool_call_id: String,
    /// Serialised output from the tool on success, or error description on failure.
    pub content: String,
    /// `true` when the tool call resulted in an error.
    pub is_error: bool,
}

impl ToolCallResult {
    /// Wraps this result in a [`Message`] suitable for appending to the conversation.
    #[must_use]
    pub fn into_message(self) -> Message {
        Message {
            role: MessageRole::Tool,
            content: vec![MessageContent::ToolResult {
                tool_use_id: self.tool_call_id,
                content: self.content,
                is_error: self.is_error,
            }],
        }
    }
}

/// A provider-agnostic chat completion request.
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    /// The model identifier (e.g. `"claude-sonnet-4-6"`, `"gpt-4o"`).
    pub model: String,
    /// Conversation history. Should contain only user, assistant, and tool messages.
    ///
    /// System prompts should be passed via the `system` field, not as messages.
    pub messages: Vec<Message>,
    /// Tools the model may call during this completion.
    pub tools: Vec<ToolDescriptor>,
    /// Sampling temperature. `0.0` is deterministic, `1.0` is maximally creative.
    pub temperature: Option<f32>,
    /// Maximum tokens to generate. Each provider applies its own default when `None`.
    pub max_tokens: Option<u32>,
    /// System prompt, sent as a top-level parameter on providers that support it.
    pub system: Option<String>,
}

impl CompletionRequest {
    /// Creates a minimal request with only a model and message list.
    #[must_use]
    pub fn new(model: impl Into<String>, messages: Vec<Message>) -> Self {
        Self {
            model: model.into(),
            messages,
            tools: vec![],
            temperature: None,
            max_tokens: None,
            system: None,
        }
    }

    /// Attaches a system prompt.
    #[must_use]
    pub fn with_system(mut self, system: impl Into<String>) -> Self {
        self.system = Some(system.into());
        self
    }

    /// Attaches the available tools.
    #[must_use]
    pub fn with_tools(mut self, tools: Vec<ToolDescriptor>) -> Self {
        self.tools = tools;
        self
    }

    /// Sets the maximum tokens to generate.
    #[must_use]
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }
}

/// A fully-resolved completion response.
#[derive(Debug, Clone)]
pub struct CompletionResponse {
    /// The generated content parts (text and/or tool calls).
    pub content: Vec<MessageContent>,
    /// Why the model stopped.
    pub stop_reason: StopReason,
    /// Token usage for cost accounting.
    pub usage: TokenUsage,
    /// The exact model identifier that processed the request (may differ from requested).
    pub model: String,
}

impl CompletionResponse {
    /// Extracts all tool calls from the response content.
    #[must_use]
    pub fn tool_calls(&self) -> Vec<ToolCall> {
        self.content
            .iter()
            .filter_map(|c| {
                if let MessageContent::ToolUse { id, name, input } = c {
                    Some(ToolCall {
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    /// Returns the concatenated text from all text content parts.
    ///
    /// Returns `None` if the response contains no text parts.
    #[must_use]
    pub fn text(&self) -> Option<String> {
        let parts: Vec<&str> = self
            .content
            .iter()
            .filter_map(|c| {
                if let MessageContent::Text { text } = c {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect();
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(""))
        }
    }

    /// Converts this response into a [`Message`] suitable for appending to the conversation.
    #[must_use]
    pub fn into_message(self) -> Message {
        Message {
            role: MessageRole::Assistant,
            content: self.content,
        }
    }
}

/// A single chunk from a streaming completion.
#[derive(Debug, Clone)]
pub struct CompletionChunk {
    /// The content delta in this chunk.
    pub delta: ChunkDelta,
    /// Token usage, populated only in the final chunk.
    pub usage: Option<TokenUsage>,
}

/// The payload of a single [`CompletionChunk`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ChunkDelta {
    /// A text fragment.
    Text(String),
    /// The beginning of a tool call (name and id are now known).
    ToolCallStart {
        /// Provider-assigned identifier for the tool call.
        id: String,
        /// The tool being called.
        name: String,
    },
    /// A partial JSON fragment for an in-progress tool call.
    ToolCallDelta {
        /// Index of the tool call (matches a prior [`ChunkDelta::ToolCallStart`]).
        index: usize,
        /// Partial JSON fragment to append to the tool call's argument buffer.
        partial_json: String,
    },
    /// The stream is complete.
    Done {
        /// Why the model stopped.
        stop_reason: StopReason,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use kainetic_schema::{Message, MessageRole};

    #[test]
    fn completion_request_builder() {
        let req = CompletionRequest::new("claude-sonnet-4-6", vec![Message::user("hi")])
            .with_system("Be helpful.")
            .with_max_tokens(1024);
        assert_eq!(req.model, "claude-sonnet-4-6");
        assert_eq!(req.system.as_deref(), Some("Be helpful."));
        assert_eq!(req.max_tokens, Some(1024));
    }

    #[test]
    fn completion_response_tool_calls() {
        let response = CompletionResponse {
            content: vec![
                MessageContent::Text {
                    text: "Searching...".into(),
                },
                MessageContent::ToolUse {
                    id: "tu_1".into(),
                    name: "web_search".into(),
                    input: serde_json::json!({"query": "rust"}),
                },
            ],
            stop_reason: StopReason::ToolUse,
            usage: TokenUsage::new(50, 20),
            model: "claude-sonnet-4-6-20251001".into(),
        };

        let calls = response.tool_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "web_search");
        assert_eq!(calls[0].id, "tu_1");
    }

    #[test]
    fn completion_response_text() {
        let response = CompletionResponse {
            content: vec![MessageContent::Text {
                text: "Hello!".into(),
            }],
            stop_reason: StopReason::EndTurn,
            usage: TokenUsage::new(10, 5),
            model: "claude-sonnet-4-6-20251001".into(),
        };
        assert_eq!(response.text().as_deref(), Some("Hello!"));
    }

    #[test]
    fn completion_response_into_message() {
        let response = CompletionResponse {
            content: vec![MessageContent::text("Hi")],
            stop_reason: StopReason::EndTurn,
            usage: TokenUsage::default(),
            model: "gpt-4o".into(),
        };
        let msg = response.into_message();
        assert_eq!(msg.role, MessageRole::Assistant);
    }

    #[test]
    fn tool_call_result_into_message() {
        let result = ToolCallResult {
            tool_call_id: "tu_1".into(),
            content: "result".into(),
            is_error: false,
        };
        let msg = result.into_message();
        assert_eq!(msg.role, MessageRole::Tool);
        assert_eq!(msg.content.len(), 1);
    }
}
