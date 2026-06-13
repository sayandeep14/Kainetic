//! Criterion benchmarks for `ReActLoop`.
//!
//! Covers:
//! - Cold-start latency: time from context creation to first response with a
//!   zero-tool, single-iteration mock provider.
//! - Parallel vs serial tool execution: N instant echo tools dispatched via
//!   `FuturesUnordered` vs sequential loop.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use kainetic_core::{AgentConfig, AgentContext, ReActLoop};
use kainetic_providers::{
    BoxStream, CompletionChunk, CompletionRequest, CompletionResponse, ModelProvider, ProviderError,
    StopReason,
};
use kainetic_schema::{MessageContent, RootSchema, TokenUsage};
use kainetic_tools::{Tool, ToolContext, ToolFuture, ToolRegistry};
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use tokio::runtime::Runtime;

// ── Mock provider ─────────────────────────────────────────────────────────────

struct QueuedProvider {
    responses: Mutex<VecDeque<CompletionResponse>>,
}

impl QueuedProvider {
    fn new(responses: impl IntoIterator<Item = CompletionResponse>) -> Self {
        Self {
            responses: Mutex::new(responses.into_iter().collect()),
        }
    }

    fn text_response(text: &str) -> CompletionResponse {
        CompletionResponse {
            content: vec![MessageContent::Text { text: text.to_owned() }],
            stop_reason: StopReason::EndTurn,
            usage: TokenUsage::new(10, 5),
            model: "bench-mock".to_owned(),
        }
    }

    fn tool_calls_response(calls: Vec<(String, String, serde_json::Value)>) -> CompletionResponse {
        let content = calls
            .into_iter()
            .map(|(id, name, input)| MessageContent::ToolUse { id, name, input })
            .collect();
        CompletionResponse {
            content,
            stop_reason: StopReason::ToolUse,
            usage: TokenUsage::new(15, 8),
            model: "bench-mock".to_owned(),
        }
    }
}

#[async_trait]
impl ModelProvider for QueuedProvider {
    fn name(&self) -> &'static str {
        "bench-mock"
    }

    fn default_model(&self) -> &'static str {
        "bench-mock-model"
    }

    fn cost_usd(&self, _usage: &TokenUsage, _model: &str) -> f64 {
        0.0
    }

    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        self.responses
            .lock()
            .expect("mock mutex")
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
        unimplemented!("stream not used in benchmarks")
    }
}

// ── Instant echo tool (no async I/O) ─────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct EchoInput {
    msg: String,
}

struct InstantEchoTool {
    // Static name stored in a leaked Box to satisfy 'static lifetime.
    tool_name: &'static str,
}

impl Tool for InstantEchoTool {
    fn name(&self) -> &'static str {
        self.tool_name
    }

    fn description(&self) -> &'static str {
        "Instant echo — no I/O"
    }

    fn input_schema(&self) -> RootSchema {
        schema_for!(EchoInput)
    }

    fn output_schema(&self) -> RootSchema {
        schema_for!(EchoInput)
    }

    fn call(&self, input: serde_json::Value, _ctx: ToolContext) -> ToolFuture<'_> {
        Box::pin(async move { Ok(input) })
    }
}

// ── Helper: build AgentContext with N instant echo tools ─────────────────────

fn make_context(provider: Arc<dyn ModelProvider>, n_tools: usize) -> AgentContext {
    let registry = ToolRegistry::new();
    for i in 0..n_tools {
        let tool_name: &'static str = Box::leak(format!("echo_{i}").into_boxed_str());
        registry.register(InstantEchoTool { tool_name });
    }
    AgentContext::for_testing(provider, Arc::new(registry))
}

// ── Benchmarks ────────────────────────────────────────────────────────────────

/// Cold-start: create context + run a single-iteration, no-tool response.
///
/// Target: < 5 ms per iteration (SPEC §17).
fn bench_cold_start(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("react_loop_cold_start", |b| {
        b.to_async(&rt).iter(|| async {
            let provider = Arc::new(QueuedProvider::new([QueuedProvider::text_response(
                "done",
            )]));
            let ctx = make_context(provider, 0);
            let loop_ = ReActLoop::new(AgentConfig::builder().build());
            loop_.execute("hi", ctx).await.unwrap()
        });
    });
}

/// Parallel vs serial: N tools dispatched at once vs one-by-one.
fn bench_parallel_vs_serial(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("tool_dispatch");

    for n_tools in [1usize, 4, 8, 16] {
        group.bench_with_input(
            BenchmarkId::new("parallel", n_tools),
            &n_tools,
            |b, &n| {
                b.to_async(&rt).iter(|| async move {
                    let provider = Arc::new(QueuedProvider::new([
                        QueuedProvider::tool_calls_response(
                            (0..n)
                                .map(|i| {
                                    (
                                        format!("call_{i}"),
                                        format!("echo_{i}"),
                                        serde_json::json!({ "msg": format!("hi-{i}") }),
                                    )
                                })
                                .collect(),
                        ),
                        QueuedProvider::text_response("all done"),
                    ]));
                    let ctx = make_context(provider, n);
                    let loop_ = ReActLoop::new(AgentConfig::builder().build());
                    loop_.execute("go", ctx).await.unwrap()
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("serial", n_tools),
            &n_tools,
            |b, &n| {
                b.to_async(&rt).iter(|| async move {
                    let provider = Arc::new(QueuedProvider::new([
                        QueuedProvider::tool_calls_response(
                            (0..n)
                                .map(|i| {
                                    (
                                        format!("call_{i}"),
                                        format!("echo_{i}"),
                                        serde_json::json!({ "msg": format!("hi-{i}") }),
                                    )
                                })
                                .collect(),
                        ),
                        QueuedProvider::text_response("all done"),
                    ]));
                    let ctx = make_context(provider, n);
                    let loop_ = ReActLoop::new(
                        AgentConfig::builder().sequential_tools().build(),
                    );
                    loop_.execute("go", ctx).await.unwrap()
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_cold_start, bench_parallel_vs_serial);
criterion_main!(benches);
