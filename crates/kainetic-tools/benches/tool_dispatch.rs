//! Criterion benchmarks for `ToolRegistry`.
//!
//! Measures:
//! - `ToolRegistry::call` throughput for a single instant tool.
//! - Schema validation overhead (with vs without a complex schema).
//! - Concurrent `call` throughput under N parallel tasks.

use std::sync::Arc;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use kainetic_schema::{RootSchema, RunId};
use kainetic_tools::{Tool, ToolContext, ToolFuture, ToolRegistry};
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use tokio::runtime::Runtime;
use tokio_util::sync::CancellationToken;

// ── Minimal instant tool ──────────────────────────────────────────────────────

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

    fn call(&self, input: serde_json::Value, _ctx: ToolContext) -> ToolFuture<'_> {
        Box::pin(async move { Ok(input) })
    }
}

fn ctx() -> ToolContext {
    ToolContext::new(RunId::new(), CancellationToken::new())
}

// ── Benchmarks ────────────────────────────────────────────────────────────────

/// Single sequential `call` — pure overhead of lookup + validation + dispatch.
fn bench_single_call(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let registry = Arc::new(ToolRegistry::new());
    registry.register(EchoTool);

    c.bench_function("registry_single_call", |b| {
        b.to_async(&rt).iter(|| {
            let reg = Arc::clone(&registry);
            async move {
                reg.call("echo", serde_json::json!({ "message": "hello" }), ctx())
                    .await
                    .unwrap()
            }
        });
    });
}

/// N concurrent `call`s dispatched with `tokio::spawn`.
///
/// Models the parallel tool execution path inside `ReActLoop`.
fn bench_concurrent_calls(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("registry_concurrent_calls");

    for n in [1usize, 4, 8, 16, 32] {
        let registry = Arc::new(ToolRegistry::new());
        registry.register(EchoTool);

        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            b.to_async(&rt).iter(|| {
                let reg = Arc::clone(&registry);
                async move {
                    let handles: Vec<_> = (0..n)
                        .map(|i| {
                            let reg = Arc::clone(&reg);
                            tokio::spawn(async move {
                                reg.call(
                                    "echo",
                                    serde_json::json!({ "message": format!("msg-{i}") }),
                                    ctx(),
                                )
                                .await
                                .unwrap()
                            })
                        })
                        .collect();
                    for h in handles {
                        h.await.unwrap();
                    }
                }
            });
        });
    }

    group.finish();
}

/// Registry `list()` — returns a snapshot of all tool descriptors.
///
/// Called once per ReActLoop iteration to build the provider request.
fn bench_list_descriptors(c: &mut Criterion) {
    let mut group = c.benchmark_group("registry_list");

    for n_tools in [1usize, 10, 50] {
        // Build a registry with n_tools distinct tools.
        let registry = ToolRegistry::new();
        for i in 0..n_tools {
            // Leak a unique static str for each name — acceptable in benchmarks.
            let name: &'static str = Box::leak(format!("tool_{i}").into_boxed_str());
            registry.register(NamedTool(name));
        }

        group.bench_with_input(BenchmarkId::from_parameter(n_tools), &n_tools, |b, _| {
            b.iter(|| registry.list());
        });
    }

    group.finish();
}

struct NamedTool(&'static str);

impl Tool for NamedTool {
    fn name(&self) -> &'static str {
        self.0
    }

    fn description(&self) -> &'static str {
        "bench placeholder"
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

criterion_group!(
    benches,
    bench_single_call,
    bench_concurrent_calls,
    bench_list_descriptors
);
criterion_main!(benches);
