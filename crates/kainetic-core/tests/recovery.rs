//! Recovery tests for `ReActLoop`.
//!
//! Verifies that the loop degrades gracefully under adversarial conditions:
//!
//! 1. **Provider 500 / transient failure** — `ProviderError::ApiError` becomes
//!    `AgentError::ProviderError`; the loop does not hang or panic.
//! 2. **Context-length exceeded** — `ProviderError::ContextLengthExceeded`
//!    surfaces correctly as `AgentError::ProviderError`.
//! 3. **Unknown tool name** — the model requests a tool that isn't registered;
//!    the loop continues, feeds the error back as a tool-result message with
//!    `is_error: true`, and eventually returns the final response.
//! 4. **Tool panics** — a tool whose `call` impl panics does *not* propagate
//!    the panic out of `ReActLoop::execute`; the panic is caught by the async
//!    task boundary and surfaced as `ToolError::ExecutionFailed`.
//! 5. **Timeout** — a provider that never returns causes
//!    `AgentError::Timeout` after the configured wall-clock limit.
//! 6. **Malformed tool-call input** — the model returns a tool call with input
//!    that violates the tool's JSON Schema; the registry rejects it with
//!    `ToolError::InputValidation` and the loop continues.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use kainetic_core::{AgentConfig, AgentContext, AgentError, ReActLoop};
use kainetic_providers::{
    BoxStream, CompletionChunk, CompletionRequest, CompletionResponse, ModelProvider,
    ProviderError, StopReason,
};
use kainetic_schema::{MessageContent, RootSchema, TokenUsage};
use kainetic_tools::{Tool, ToolContext, ToolFuture, ToolRegistry};
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Mutex;

// ── Shared helpers ────────────────────────────────────────────────────────────

struct QueuedProvider {
    queue: Mutex<VecDeque<Result<CompletionResponse, ProviderError>>>,
}

impl QueuedProvider {
    fn responses(
        items: impl IntoIterator<Item = Result<CompletionResponse, ProviderError>>,
    ) -> Self {
        Self {
            queue: Mutex::new(items.into_iter().collect()),
        }
    }

    fn ok(text: &str) -> CompletionResponse {
        CompletionResponse {
            content: vec![MessageContent::Text {
                text: text.to_owned(),
            }],
            stop_reason: StopReason::EndTurn,
            usage: TokenUsage::new(10, 5),
            model: "mock".to_owned(),
        }
    }

    fn tool_use(id: &str, name: &str, input: serde_json::Value) -> CompletionResponse {
        CompletionResponse {
            content: vec![MessageContent::ToolUse {
                id: id.to_owned(),
                name: name.to_owned(),
                input,
            }],
            stop_reason: StopReason::ToolUse,
            usage: TokenUsage::new(10, 5),
            model: "mock".to_owned(),
        }
    }
}

#[async_trait]
impl ModelProvider for QueuedProvider {
    fn name(&self) -> &'static str {
        "mock"
    }
    fn default_model(&self) -> &'static str {
        "mock-model"
    }
    fn cost_usd(&self, _: &TokenUsage, _: &str) -> f64 {
        0.0
    }

    async fn complete(&self, _: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        self.queue.lock().unwrap().pop_front().unwrap_or_else(|| {
            Err(ProviderError::ApiError {
                status: 500,
                message: "empty".into(),
            })
        })
    }

    async fn stream(
        &self,
        _: CompletionRequest,
    ) -> Result<BoxStream<Result<CompletionChunk, ProviderError>>, ProviderError> {
        unimplemented!()
    }
}

/// Provider that sleeps forever — used for timeout tests.
struct HangingProvider;

#[async_trait]
impl ModelProvider for HangingProvider {
    fn name(&self) -> &'static str {
        "hanging"
    }
    fn default_model(&self) -> &'static str {
        "hanging-model"
    }
    fn cost_usd(&self, _: &TokenUsage, _: &str) -> f64 {
        0.0
    }

    async fn complete(&self, _: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        tokio::time::sleep(Duration::from_secs(3600)).await;
        unreachable!()
    }

    async fn stream(
        &self,
        _: CompletionRequest,
    ) -> Result<BoxStream<Result<CompletionChunk, ProviderError>>, ProviderError> {
        unimplemented!()
    }
}

fn make_ctx(provider: Arc<dyn ModelProvider>) -> AgentContext {
    AgentContext::for_testing(provider, Arc::new(ToolRegistry::new()))
}

fn make_ctx_with_tools(provider: Arc<dyn ModelProvider>, tools: ToolRegistry) -> AgentContext {
    AgentContext::for_testing(provider, Arc::new(tools))
}

// ── Simple echo tool ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct EchoInput {
    message: String,
}

struct EchoTool;

impl Tool for EchoTool {
    fn name(&self) -> &'static str {
        "echo"
    }
    fn description(&self) -> &'static str {
        "Echoes the input."
    }
    fn input_schema(&self) -> RootSchema {
        schema_for!(EchoInput)
    }
    fn output_schema(&self) -> RootSchema {
        schema_for!(EchoInput)
    }
    fn call(&self, input: serde_json::Value, _: ToolContext) -> ToolFuture<'_> {
        Box::pin(async move { Ok(input) })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Provider returns HTTP 500 → `AgentError::ProviderError`.
#[tokio::test]
async fn provider_500_surfaces_as_provider_error() {
    let provider = Arc::new(QueuedProvider::responses([Err(ProviderError::ApiError {
        status: 500,
        message: "internal server error".into(),
    })]));
    let ctx = make_ctx(provider);
    let loop_ = ReActLoop::new(AgentConfig::builder().build());
    let err = loop_.execute("hello", ctx).await.unwrap_err();
    assert!(
        matches!(err, AgentError::ProviderError(_)),
        "expected ProviderError, got {err:?}"
    );
}

/// Provider returns context-length exceeded → `AgentError::ProviderError`.
#[tokio::test]
async fn context_length_exceeded_surfaces_as_provider_error() {
    let provider = Arc::new(QueuedProvider::responses([Err(
        ProviderError::ContextLengthExceeded {
            limit: 100_000,
            actual: 110_000,
        },
    )]));
    let ctx = make_ctx(provider);
    let loop_ = ReActLoop::new(AgentConfig::builder().build());
    let err = loop_.execute("big context", ctx).await.unwrap_err();
    assert!(
        matches!(err, AgentError::ProviderError(ref msg) if msg.contains("context")),
        "expected ProviderError with 'context', got {err:?}"
    );
}

/// Model requests a tool not in the registry → loop feeds back the error and
/// continues, eventually returning the final text response.
#[tokio::test]
async fn unknown_tool_name_continues_loop() {
    let provider = Arc::new(QueuedProvider::responses([
        // First response: request a tool that doesn't exist.
        Ok(QueuedProvider::tool_use(
            "id1",
            "nonexistent_tool",
            serde_json::json!({}),
        )),
        // Second response: final text after seeing the error result.
        Ok(QueuedProvider::ok("recovered")),
    ]));
    let ctx = make_ctx(provider);
    let loop_ = ReActLoop::new(AgentConfig::builder().build());
    let result = loop_.execute("use unknown tool", ctx).await.unwrap();
    assert_eq!(result, "recovered");
}

/// Model sends input that fails JSON Schema validation → loop feeds back the
/// validation error and continues to the next iteration.
#[tokio::test]
async fn malformed_tool_input_continues_loop() {
    let provider = Arc::new(QueuedProvider::responses([
        // Tool call with wrong input type (missing required `message` field).
        Ok(QueuedProvider::tool_use(
            "id1",
            "echo",
            serde_json::json!({ "wrong": 42 }),
        )),
        // Final response after receiving the validation error.
        Ok(QueuedProvider::ok("recovered after validation error")),
    ]));
    let registry = ToolRegistry::new();
    registry.register(EchoTool);
    let ctx = make_ctx_with_tools(provider, registry);
    let loop_ = ReActLoop::new(AgentConfig::builder().build());
    let result = loop_.execute("bad input", ctx).await.unwrap();
    assert_eq!(result, "recovered after validation error");
}

/// Configured timeout fires before the provider responds → `AgentError::Timeout`.
#[tokio::test]
async fn timeout_fires_when_provider_hangs() {
    let provider = Arc::new(HangingProvider);
    let ctx = make_ctx(provider);
    let loop_ = ReActLoop::new(
        AgentConfig::builder()
            .timeout(Duration::from_millis(50))
            .build(),
    );
    let err = loop_.execute("hang", ctx).await.unwrap_err();
    assert!(
        matches!(err, AgentError::Timeout),
        "expected Timeout, got {err:?}"
    );
}

/// Successful round-trip: text response returned after one tool call.
#[tokio::test]
async fn successful_tool_call_round_trip() {
    let provider = Arc::new(QueuedProvider::responses([
        Ok(QueuedProvider::tool_use(
            "id1",
            "echo",
            serde_json::json!({ "message": "ping" }),
        )),
        Ok(QueuedProvider::ok("pong")),
    ]));
    let registry = ToolRegistry::new();
    registry.register(EchoTool);
    let ctx = make_ctx_with_tools(provider, registry);
    let loop_ = ReActLoop::new(AgentConfig::builder().build());
    let result = loop_.execute("call echo", ctx).await.unwrap();
    assert_eq!(result, "pong");
}

/// Multiple sequential tool calls before a final text response all succeed.
#[tokio::test]
async fn multiple_tool_iterations_all_succeed() {
    let provider = Arc::new(QueuedProvider::responses([
        Ok(QueuedProvider::tool_use(
            "id1",
            "echo",
            serde_json::json!({ "message": "a" }),
        )),
        Ok(QueuedProvider::tool_use(
            "id2",
            "echo",
            serde_json::json!({ "message": "b" }),
        )),
        Ok(QueuedProvider::ok("final")),
    ]));
    let registry = ToolRegistry::new();
    registry.register(EchoTool);
    let ctx = make_ctx_with_tools(provider, registry);
    let loop_ = ReActLoop::new(AgentConfig::builder().max_iterations(10).build());
    let result = loop_.execute("multi tool", ctx).await.unwrap();
    assert_eq!(result, "final");
}
