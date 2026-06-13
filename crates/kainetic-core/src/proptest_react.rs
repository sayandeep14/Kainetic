//! Property-based tests for `ReActLoop` invariants.
//!
//! Uses [`proptest`] to drive the loop with randomised inputs and verify that
//! the stated contracts hold regardless of input shape:
//!
//! - The loop **always terminates** (either with a result or a bounded error).
//! - `max_iterations` is an **exact hard cap**: the loop never takes more LLM
//!   calls than `max_iterations`.
//! - A cancelled token causes an **immediate `AgentError::Cancelled`** return
//!   with zero LLM calls issued.
//! - The loop returns **`AgentError::MaxIterationsExceeded`** when the mock
//!   provider keeps requesting tool calls indefinitely.
//!
//! # Running
//!
//! ```bash
//! cargo test -p kainetic-core proptest
//! ```

#![cfg(test)]

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;

use async_trait::async_trait;
use proptest::prelude::*;

use kainetic_providers::{
    BoxStream, CompletionChunk, CompletionRequest, CompletionResponse, ModelProvider, ProviderError,
    StopReason,
};
use kainetic_schema::{MessageContent, TokenUsage};
use kainetic_tools::ToolRegistry;

use crate::{AgentConfig, AgentContext, ReActLoop};

// ── Mock provider that counts calls ──────────────────────────────────────────

struct CountingProvider {
    calls: Arc<AtomicU32>,
    responses: Mutex<VecDeque<CompletionResponse>>,
}

impl CountingProvider {
    fn new(responses: impl IntoIterator<Item = CompletionResponse>) -> Self {
        Self {
            calls: Arc::new(AtomicU32::new(0)),
            responses: Mutex::new(responses.into_iter().collect()),
        }
    }

    fn text(text: &str) -> CompletionResponse {
        CompletionResponse {
            content: vec![MessageContent::Text { text: text.to_owned() }],
            stop_reason: StopReason::EndTurn,
            usage: TokenUsage::new(5, 5),
            model: "prop-mock".to_owned(),
        }
    }

    /// A tool-use response that always requests a non-existent tool.
    ///
    /// The registry will return `ToolError::ExecutionFailed`, but the loop
    /// continues — the error is fed back as a tool result message with
    /// `is_error: true`.
    fn forever_tool() -> CompletionResponse {
        CompletionResponse {
            content: vec![MessageContent::ToolUse {
                id: "id_01".to_owned(),
                name: "no_such_tool".to_owned(),
                input: serde_json::json!({}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: TokenUsage::new(5, 5),
            model: "prop-mock".to_owned(),
        }
    }
}

#[async_trait]
impl ModelProvider for CountingProvider {
    fn name(&self) -> &'static str {
        "prop-mock"
    }

    fn default_model(&self) -> &'static str {
        "prop-mock-model"
    }

    fn cost_usd(&self, _usage: &kainetic_schema::TokenUsage, _model: &str) -> f64 {
        0.0
    }

    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| ProviderError::ApiError {
                status: 500,
                message: "queue empty".into(),
            })
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<BoxStream<Result<CompletionChunk, ProviderError>>, ProviderError> {
        unimplemented!()
    }
}

fn make_ctx(provider: Arc<dyn ModelProvider>) -> AgentContext {
    AgentContext::for_testing(provider, Arc::new(ToolRegistry::new()))
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

// ── Property tests ────────────────────────────────────────────────────────────

proptest! {
    /// The loop always terminates and returns `Ok` when the provider answers
    /// with a direct text response, regardless of the input string content.
    #[test]
    fn prop_loop_terminates_on_text_response(input in ".*") {
        let rt = runtime();
        rt.block_on(async {
            let provider = Arc::new(CountingProvider::new([CountingProvider::text("done")]));
            let ctx = make_ctx(provider);
            let loop_ = ReActLoop::new(AgentConfig::builder().build());
            let result = loop_.execute(input, ctx).await;
            prop_assert!(result.is_ok(), "expected Ok, got {:?}", result);
            Ok(())
        })?;
    }

    /// The loop issues exactly `max_iterations` LLM calls when the provider
    /// keeps requesting a non-existent tool, then returns
    /// `AgentError::MaxIterationsExceeded`.
    #[test]
    fn prop_max_iterations_is_exact_cap(max_iter in 1u32..=10) {
        let rt = runtime();
        rt.block_on(async {
            let responses: Vec<_> =
                std::iter::repeat_with(CountingProvider::forever_tool)
                    .take(max_iter as usize + 5)
                    .collect();
            let provider = Arc::new(CountingProvider::new(responses));
            let calls_ref = Arc::clone(&provider.calls);

            let ctx = make_ctx(provider);
            let loop_ = ReActLoop::new(
                AgentConfig::builder()
                    .max_iterations(max_iter)
                    .build(),
            );
            let err = loop_.execute("go", ctx).await.unwrap_err();

            prop_assert!(
                matches!(err, crate::AgentError::MaxIterationsExceeded(_)),
                "expected MaxIterationsExceeded, got {:?}", err
            );
            let actual_calls = calls_ref.load(std::sync::atomic::Ordering::SeqCst);
            prop_assert_eq!(
                actual_calls,
                max_iter,
                "expected exactly {} LLM calls, got {}",
                max_iter,
                actual_calls
            );
            Ok(())
        })?;
    }

    /// A pre-cancelled token causes an immediate `AgentError::Cancelled`
    /// with zero LLM calls issued.
    #[test]
    fn prop_cancelled_token_short_circuits(input in ".*") {
        let rt = runtime();
        rt.block_on(async {
            let provider = Arc::new(CountingProvider::new([CountingProvider::text("never")]));
            let calls_ref = Arc::clone(&provider.calls);

            let ctx = make_ctx(provider);
            // Cancel the token before executing so the loop short-circuits.
            ctx.cancellation_token.cancel();

            let loop_ = ReActLoop::new(AgentConfig::builder().build());
            let err = loop_.execute(input, ctx).await.unwrap_err();

            prop_assert!(
                matches!(err, crate::AgentError::Cancelled),
                "expected Cancelled, got {:?}", err
            );
            prop_assert_eq!(
                calls_ref.load(std::sync::atomic::Ordering::SeqCst),
                0,
                "cancelled loop must not call the provider"
            );
            Ok(())
        })?;
    }
}
