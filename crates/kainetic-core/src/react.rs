//! `ReActLoop` — Reason → Act → Observe execution engine.

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::stream::{FuturesUnordered, StreamExt};
use kainetic_providers::{CompletionRequest, ToolCall, ToolCallResult};
use kainetic_schema::Message;
use kainetic_tools::ToolRegistry;
use kainetic_tools::ToolContext;

use crate::event::AgentEvent;
use crate::{AgentConfig, AgentContext, AgentError};

/// The `ReAct` (Reason + Act) loop that drives agent execution.
///
/// Each call to [`execute`] runs one complete Reason → Act → Observe cycle:
///
/// 1. **Reason** — sends the conversation history to the language model.
/// 2. **Act** — dispatches all requested tool calls, optionally in parallel.
/// 3. **Observe** — appends tool results to the conversation history.
///
/// The loop terminates when the model produces a response with no tool calls,
/// when [`AgentConfig::max_iterations`] is reached, when the configured timeout
/// fires, or when the run's `CancellationToken` is cancelled.
///
/// [`execute`]: ReActLoop::execute
pub struct ReActLoop {
    config: AgentConfig,
}

impl ReActLoop {
    /// Creates a new loop bound to the given configuration.
    #[must_use]
    pub fn new(config: AgentConfig) -> Self {
        Self { config }
    }

    /// Drives the `ReAct` loop to completion and returns the final text response.
    ///
    /// # Errors
    ///
    /// - [`AgentError::Timeout`] if the run exceeds [`AgentConfig::timeout`]
    ///   (default 120 seconds).
    /// - [`AgentError::Cancelled`] if the cancellation token is set.
    /// - [`AgentError::MaxIterationsExceeded`] if the iteration cap is hit.
    /// - [`AgentError::ProviderError`] on language model failures.
    pub async fn execute(
        &self,
        input: impl Into<String>,
        ctx: AgentContext,
    ) -> Result<String, AgentError> {
        let timeout_dur = self
            .config
            .timeout
            .unwrap_or_else(|| Duration::from_secs(120));

        tokio::time::timeout(timeout_dur, self.run_inner(input.into(), ctx))
            .await
            .map_err(|_| AgentError::Timeout)?
    }

    async fn run_inner(&self, user_input: String, ctx: AgentContext) -> Result<String, AgentError> {
        let mut messages: Vec<Message> = vec![Message::user(user_input)];
        let tool_descriptors = ctx.tools.list();
        let run_start = Instant::now();
        let mut total_tokens: u32 = 0;

        for _iteration in 0..self.config.max_iterations {
            if ctx.cancellation_token.is_cancelled() {
                return Err(AgentError::Cancelled);
            }

            let mut request =
                CompletionRequest::new(self.config.model.clone(), messages.clone());

            if !tool_descriptors.is_empty() {
                request = request.with_tools(tool_descriptors.clone());
            }
            if let Some(sp) = &self.config.system_prompt {
                request = request.with_system(sp.render(&std::collections::HashMap::default()));
            }
            if let Some(mt) = self.config.max_tokens {
                request = request.with_max_tokens(mt);
            }
            if let Some(temp) = self.config.temperature {
                request.temperature = Some(temp);
            }

            ctx.emit(AgentEvent::LlmCallStarted {
                run_id: ctx.run_id,
                provider: ctx.provider.name().to_owned(),
                model: self.config.model.clone(),
                messages: u32::try_from(messages.len()).unwrap_or(u32::MAX),
            });

            let llm_start = Instant::now();
            let response = ctx
                .provider
                .complete(request)
                .await
                .map_err(|e| AgentError::ProviderError(e.to_string()))?;

            let llm_latency_ms =
                u64::try_from(llm_start.elapsed().as_millis()).unwrap_or(u64::MAX);
            total_tokens = total_tokens.saturating_add(response.usage.total_tokens);

            ctx.emit(AgentEvent::LlmCallCompleted {
                run_id: ctx.run_id,
                prompt_tokens: response.usage.prompt_tokens,
                completion_tokens: response.usage.completion_tokens,
                latency_ms: llm_latency_ms,
            });

            messages.push(response.clone().into_message());

            let tool_calls = response.tool_calls();
            if tool_calls.is_empty() {
                let text = response.text().unwrap_or_default();
                ctx.emit(AgentEvent::RunCompleted {
                    run_id: ctx.run_id,
                    total_tokens,
                    cost_usd: 0.0,
                    latency_ms: u64::try_from(run_start.elapsed().as_millis())
                        .unwrap_or(u64::MAX),
                });
                return Ok(text);
            }

            let results = if self.config.parallel_tools {
                execute_tools_parallel(tool_calls, ctx.clone()).await
            } else {
                execute_tools_serial(tool_calls, ctx.clone()).await
            };

            for result in results {
                messages.push(result.into_message());
            }
        }

        Err(AgentError::MaxIterationsExceeded(self.config.max_iterations))
    }
}

async fn execute_tools_parallel(
    tool_calls: Vec<ToolCall>,
    ctx: AgentContext,
) -> Vec<ToolCallResult> {
    let futs: FuturesUnordered<_> = tool_calls
        .into_iter()
        .map(|call| execute_one_tool(call, Arc::clone(&ctx.tools), ctx.clone()))
        .collect();

    futs.collect().await
}

async fn execute_tools_serial(
    tool_calls: Vec<ToolCall>,
    ctx: AgentContext,
) -> Vec<ToolCallResult> {
    let mut results = Vec::new();
    for call in tool_calls {
        results.push(execute_one_tool(call, Arc::clone(&ctx.tools), ctx.clone()).await);
    }
    results
}

async fn execute_one_tool(
    call: ToolCall,
    tools: Arc<ToolRegistry>,
    ctx: AgentContext,
) -> ToolCallResult {
    let tool_ctx = ToolContext::new(ctx.run_id, ctx.cancellation_token.clone());

    ctx.emit(AgentEvent::ToolCallStarted {
        run_id: ctx.run_id,
        tool: call.name.clone(),
        input: call.input.clone(),
    });

    let tool_start = Instant::now();
    let outcome = tools.call(&call.name, call.input.clone(), tool_ctx).await;
    let latency_ms = u64::try_from(tool_start.elapsed().as_millis()).unwrap_or(u64::MAX);

    match outcome {
        Ok(output) => {
            let content = output.to_string();
            ctx.emit(AgentEvent::ToolCallCompleted {
                run_id: ctx.run_id,
                tool: call.name.clone(),
                output,
                latency_ms,
            });
            ToolCallResult {
                tool_call_id: call.id,
                content,
                is_error: false,
            }
        }
        Err(e) => {
            let error = e.to_string();
            ctx.emit(AgentEvent::ToolCallFailed {
                run_id: ctx.run_id,
                tool: call.name.clone(),
                error: error.clone(),
            });
            ToolCallResult {
                tool_call_id: call.id,
                content: error,
                is_error: true,
            }
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use kainetic_providers::{
        BoxStream, CompletionChunk, CompletionRequest, CompletionResponse, ModelProvider,
        ProviderError, StopReason,
    };
    use kainetic_schema::{MessageContent, RunId, SessionId, TokenUsage};
    use kainetic_tools::ToolRegistry;
    use tokio::sync::broadcast;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::{AgentConfig, AgentContext};

    // ── MockProvider ─────────────────────────────────────────────────────────

    struct MockProvider {
        responses: Mutex<VecDeque<Result<CompletionResponse, ProviderError>>>,
    }

    impl MockProvider {
        fn from_responses(responses: impl IntoIterator<Item = CompletionResponse>) -> Self {
            Self {
                responses: Mutex::new(responses.into_iter().map(Ok).collect()),
            }
        }

        fn text(text: &str) -> CompletionResponse {
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
    impl ModelProvider for MockProvider {
        async fn complete(
            &self,
            _req: CompletionRequest,
        ) -> Result<CompletionResponse, ProviderError> {
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Err(ProviderError::ApiError {
                    status: 500,
                    message: "no more mock responses".to_owned(),
                }))
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<BoxStream<Result<CompletionChunk, ProviderError>>, ProviderError> {
            Err(ProviderError::AuthFailed)
        }

        fn cost_usd(&self, _usage: &kainetic_schema::TokenUsage, _model: &str) -> f64 {
            0.0
        }

        fn name(&self) -> &'static str {
            "mock"
        }

        fn default_model(&self) -> &'static str {
            "mock-model"
        }
    }

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn make_ctx(provider: impl ModelProvider + 'static) -> AgentContext {
        let (event_tx, _) = broadcast::channel(64);
        AgentContext {
            run_id: RunId::new(),
            session_id: SessionId::new(),
            tools: Arc::new(ToolRegistry::new()),
            provider: Arc::new(provider),
            memory: AgentContext::default_memory(),
            cancellation_token: CancellationToken::new(),
            span: tracing::Span::current(),
            event_tx,
        }
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn text_response_terminates_loop() {
        let provider = MockProvider::from_responses([MockProvider::text("Hello!")]);
        let ctx = make_ctx(provider);
        let config = AgentConfig::builder().build();
        let result = ReActLoop::new(config).execute("Hi", ctx).await.unwrap();
        assert_eq!(result, "Hello!");
    }

    #[tokio::test]
    async fn tool_call_followed_by_text_terminates() {
        let provider = MockProvider::from_responses([
            MockProvider::tool_use("id1", "current_datetime", serde_json::json!({})),
            MockProvider::text("The time is now."),
        ]);
        let ctx = make_ctx(provider);
        let config = AgentConfig::builder().build();

        let mut tools_ctx = ctx.clone();
        tools_ctx.tools = Arc::new({
            let reg = ToolRegistry::new();
            reg.register(kainetic_tools::builtin::CurrentDatetimeTool);
            reg
        });

        let result = ReActLoop::new(config)
            .execute("What time is it?", tools_ctx)
            .await
            .unwrap();
        assert_eq!(result, "The time is now.");
    }

    #[tokio::test]
    async fn max_iterations_exceeded_returns_error() {
        let responses: Vec<CompletionResponse> =
            (0..25).map(|i| MockProvider::tool_use(&format!("id{i}"), "noop", serde_json::json!({}))).collect();
        let provider = MockProvider::from_responses(responses);
        let ctx = make_ctx(provider);
        let config = AgentConfig::builder().max_iterations(3).build();

        let err = ReActLoop::new(config).execute("go", ctx).await.unwrap_err();
        assert!(matches!(err, AgentError::MaxIterationsExceeded(3)));
    }

    #[tokio::test]
    async fn cancelled_token_stops_loop() {
        let provider = MockProvider::from_responses([]);
        let ctx = make_ctx(provider);
        ctx.cancellation_token.cancel();
        let config = AgentConfig::builder().build();

        let err = ReActLoop::new(config).execute("go", ctx).await.unwrap_err();
        assert!(matches!(err, AgentError::Cancelled));
    }

    #[tokio::test]
    async fn timeout_returns_error() {
        // Use a provider that sleeps per call so the 10ms timeout fires before
        // the first LLM response arrives.
        struct SlowProvider;

        #[async_trait]
        impl ModelProvider for SlowProvider {
            async fn complete(
                &self,
                _: CompletionRequest,
            ) -> Result<CompletionResponse, ProviderError> {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                Err(ProviderError::AuthFailed)
            }

            async fn stream(
                &self,
                _: CompletionRequest,
            ) -> Result<BoxStream<Result<CompletionChunk, ProviderError>>, ProviderError> {
                Err(ProviderError::AuthFailed)
            }

            fn cost_usd(&self, _: &kainetic_schema::TokenUsage, _: &str) -> f64 {
                0.0
            }

            fn name(&self) -> &'static str {
                "slow"
            }

            fn default_model(&self) -> &'static str {
                "slow-model"
            }
        }

        let ctx = make_ctx(SlowProvider);
        let config = AgentConfig::builder()
            .timeout(std::time::Duration::from_millis(10))
            .build();

        let err = ReActLoop::new(config).execute("go", ctx).await.unwrap_err();
        assert!(matches!(err, AgentError::Timeout));
    }

    #[tokio::test]
    async fn events_are_emitted() {
        let provider = MockProvider::from_responses([MockProvider::text("Done!")]);
        let (event_tx, mut event_rx) = broadcast::channel(64);
        let ctx = AgentContext {
            run_id: RunId::new(),
            session_id: SessionId::new(),
            tools: Arc::new(ToolRegistry::new()),
            provider: Arc::new(provider),
            memory: AgentContext::default_memory(),
            cancellation_token: CancellationToken::new(),
            span: tracing::Span::current(),
            event_tx,
        };

        ReActLoop::new(AgentConfig::builder().build())
            .execute("Hi", ctx)
            .await
            .unwrap();

        let mut event_kinds = Vec::new();
        while let Ok(ev) = event_rx.try_recv() {
            event_kinds.push(std::mem::discriminant(&ev));
        }

        assert!(!event_kinds.is_empty(), "at least one event should be emitted");
    }

    #[tokio::test]
    async fn parallel_and_sequential_produce_same_results() {
        // Provide 2 independent tool calls → both modes should succeed and
        // the final text should come through.
        for parallel in [true, false] {
            let responses = vec![
                MockProvider::tool_use("id1", "current_datetime", serde_json::json!({})),
                MockProvider::text("Both done."),
            ];
            let provider = MockProvider::from_responses(responses);
            let mut ctx = make_ctx(provider);
            ctx.tools = Arc::new({
                let reg = ToolRegistry::new();
                reg.register(kainetic_tools::builtin::CurrentDatetimeTool);
                reg
            });

            let mut builder = AgentConfig::builder();
            if !parallel {
                builder = builder.sequential_tools();
            }
            let config = builder.build();
            let result = ReActLoop::new(config).execute("time?", ctx).await.unwrap();
            assert_eq!(result, "Both done.");
        }
    }
}
