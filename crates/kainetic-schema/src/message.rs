//! Internal conversation representation used across provider boundaries.
//!
//! [`Message`], [`MessageRole`], and [`MessageContent`] are Kainetic's
//! provider-agnostic conversation types. Every [`ModelProvider`] implementation
//! translates between these types and its own wire format.
//!
//! [`ModelProvider`]: crate::ModelProvider

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The participant role of a [`Message`] in a conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum MessageRole {
    /// System prompt that sets the agent's context, persona, and constraints.
    System,
    /// Input from the human / calling application.
    User,
    /// Output produced by the model.
    Assistant,
    /// The result of a tool invocation, returned to the model.
    Tool,
}

/// A single piece of content within a [`Message`].
///
/// Messages can contain multiple content parts — for example, an assistant
/// turn may contain a text preamble followed by one or more tool-use requests.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum MessageContent {
    /// Plain text content.
    Text {
        /// The text body.
        text: String,
    },
    /// A request from the model to invoke a named tool.
    ToolUse {
        /// Provider-assigned identifier for this specific tool call.
        ///
        /// The same `id` must appear in the corresponding [`MessageContent::ToolResult`]
        /// so the model can match results to requests.
        id: String,
        /// The registered name of the tool to invoke.
        name: String,
        /// Raw JSON input to pass to the tool, conforming to its `input_schema`.
        input: serde_json::Value,
    },
    /// The result of a tool invocation, returned to the model.
    ToolResult {
        /// The `id` from the corresponding [`MessageContent::ToolUse`].
        tool_use_id: String,
        /// Serialised output from the tool (success) or error description (failure).
        content: String,
        /// `true` when the tool call resulted in an error.
        ///
        /// Some providers use this flag to adjust their response strategy.
        is_error: bool,
    },
}

impl MessageContent {
    /// Constructs a [`MessageContent::Text`] part.
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    /// Returns the text body if this is a [`MessageContent::Text`] part.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text { text } => Some(text),
            _ => None,
        }
    }
}

/// A single turn in a conversation, consisting of a role and one or more
/// content parts.
///
/// Kainetic uses a multi-part content model (matching Anthropic's Messages API)
/// rather than a single string, because a single assistant turn may request
/// multiple tool calls simultaneously.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Message {
    /// Who produced this message.
    pub role: MessageRole,
    /// The content parts of this message.
    ///
    /// Most turns contain a single [`MessageContent::Text`] part, but tool
    /// calling turns may contain multiple [`MessageContent::ToolUse`] parts.
    pub content: Vec<MessageContent>,
}

impl Message {
    /// Creates a single-part user text message.
    #[must_use]
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: vec![MessageContent::text(text)],
        }
    }

    /// Creates a single-part system message.
    #[must_use]
    pub fn system(text: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: vec![MessageContent::text(text)],
        }
    }

    /// Creates a single-part assistant text message.
    #[must_use]
    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: vec![MessageContent::text(text)],
        }
    }

    /// Returns the concatenated text of all [`MessageContent::Text`] parts,
    /// separated by `"\n"`.
    ///
    /// Returns `None` if the message contains no text parts.
    #[must_use]
    pub fn text(&self) -> Option<String> {
        let parts: Vec<&str> = self
            .content
            .iter()
            .filter_map(MessageContent::as_text)
            .collect();
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("\n"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_message_role_and_text() {
        let m = Message::user("hello");
        assert_eq!(m.role, MessageRole::User);
        assert_eq!(m.text().as_deref(), Some("hello"));
    }

    #[test]
    fn system_message_role() {
        let m = Message::system("You are a helpful assistant.");
        assert_eq!(m.role, MessageRole::System);
    }

    #[test]
    fn assistant_message_role() {
        let m = Message::assistant("Here is your answer.");
        assert_eq!(m.role, MessageRole::Assistant);
    }

    #[test]
    fn message_with_no_text_parts_returns_none() {
        let m = Message {
            role: MessageRole::Assistant,
            content: vec![MessageContent::ToolUse {
                id: "call_1".into(),
                name: "web_search".into(),
                input: serde_json::json!({"query": "rust"}),
            }],
        };
        assert!(m.text().is_none());
    }

    #[test]
    fn message_text_concatenates_multiple_parts() {
        let m = Message {
            role: MessageRole::Assistant,
            content: vec![
                MessageContent::text("Part one."),
                MessageContent::text("Part two."),
            ],
        };
        assert_eq!(m.text().as_deref(), Some("Part one.\nPart two."));
    }

    #[test]
    fn message_serde_round_trip() {
        let m = Message::user("what time is it?");
        let json = serde_json::to_string(&m).unwrap();
        let m2: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(m, m2);
    }

    #[test]
    fn tool_use_content_serde_round_trip() {
        let content = MessageContent::ToolUse {
            id: "tu_abc123".into(),
            name: "current_datetime".into(),
            input: serde_json::json!({}),
        };
        let json = serde_json::to_string(&content).unwrap();
        let content2: MessageContent = serde_json::from_str(&json).unwrap();
        assert_eq!(content, content2);
    }

    #[test]
    fn tool_result_content_serde_round_trip() {
        let content = MessageContent::ToolResult {
            tool_use_id: "tu_abc123".into(),
            content: "2026-06-10T12:00:00Z".into(),
            is_error: false,
        };
        let json = serde_json::to_string(&content).unwrap();
        let content2: MessageContent = serde_json::from_str(&json).unwrap();
        assert_eq!(content, content2);
    }

    #[test]
    fn message_role_serialises_lowercase() {
        let json = serde_json::to_string(&MessageRole::User).unwrap();
        assert_eq!(json, r#""user""#);
        let json = serde_json::to_string(&MessageRole::Assistant).unwrap();
        assert_eq!(json, r#""assistant""#);
    }

    #[test]
    fn schema_generated_for_message() {
        let schema = schemars::schema_for!(Message);
        let value = serde_json::to_value(schema).unwrap();
        assert_eq!(value["type"], "object");
        assert!(value["properties"]["role"].is_object());
        assert!(value["properties"]["content"].is_object());
    }
}
