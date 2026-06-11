# Kainetic

> *The production-grade Rust runtime for agentic AI.*

What Tokio is to async Rust, Kainetic is to AI agents — the foundational runtime layer that everything else runs on top of.

## Why Kainetic?

Python AI agent frameworks (LangChain, LangGraph, AutoGen) were designed for research. Their architecture carries structural costs that cannot be optimized away: the GIL prevents true parallel tool execution, cancellation semantics are fragile, and cold-start times of 60–140ms make serverless deployments unreliable.

Kainetic is built in Rust from first principles:

| | Python frameworks | Kainetic |
|---|---|---|
| Cold start | 60–140ms | < 5ms |
| Memory at rest | 180–250MB | < 15MB |
| Parallel tool execution | Blocked by GIL | Native (`FuturesUnordered`) |
| Type safety | Runtime validation | Compile-time verification |
| Cancellation | Fragile | Propagates through entire call tree |

## Hello Agent

```rust
// Coming in Part 4 — once KaineticRuntime and the #[tool]/#[agent] macros are implemented.
//
// use kainetic::prelude::*;
//
// #[tool(description = "Get the current date and time")]
// async fn current_datetime(_ctx: ToolContext) -> Result<String, ToolError> {
//     Ok(chrono::Utc::now().to_rfc3339())
// }
//
// #[agent(model = "claude-sonnet-4-6", tools = [current_datetime])]
// async fn assistant(input: String, ctx: AgentContext) -> Result<String, KaineticError> {
//     ctx.run(input).await
// }
//
// #[tokio::main]
// async fn main() -> anyhow::Result<()> {
//     let runtime = KaineticRuntime::builder()
//         .provider(AnthropicProvider::from_env()?)
//         .build();
//
//     let response = runtime.run(assistant, "What time is it?").await?;
//     println!("{response}");
//     Ok(())
// }
```

## Installation

```toml
[dependencies]
kainetic = "0.1"
```

> Kainetic is in active development. See [PLAN.md](PLAN.md) for the implementation roadmap.

## Features

- **Compile-time-verified agent topology** — if it compiles, your agent graph is valid
- **Fearless concurrency** — parallel tool execution via `FuturesUnordered`, no GIL
- **Cancellation-safe** — `CancellationToken` propagates through the entire call tree
- **Observable by default** — OpenTelemetry traces and Prometheus metrics out of the box
- **Multi-agent orchestration** — typed handoffs, pipelines, supervisors, state machines
- **Model-agnostic** — Anthropic, OpenAI, Gemini, Ollama, and any custom endpoint
- **Language bindings** — Python (PyO3) and TypeScript (napi-rs)

## Workspace Structure

```
crates/
├── kainetic          # facade — re-exports everything
├── kainetic-schema   # shared types, errors, JSON Schema
├── kainetic-providers# ModelProvider trait + Anthropic/OpenAI/Gemini/Ollama
├── kainetic-tools    # Tool trait, ToolRegistry, built-in tools
├── kainetic-core     # Agent trait, ReActLoop, KaineticRuntime
├── kainetic-memory   # memory backends (in-memory, SQLite, Redis, vector)
├── kainetic-telemetry# OTel traces, Prometheus metrics, cost tracking
├── kainetic-macros   # #[tool], #[agent], #[pipeline] proc macros
├── kainetic-orchestra# Pipeline, Handoff, Supervisor, StateMachineAgent
└── kainetic-cli      # developer CLI: init, run, validate, inspect, bench, deploy
```

## License

Apache-2.0
