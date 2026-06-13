# Kainetic

**Kainetic** is a production-grade Rust runtime for agentic AI systems — think of it as what Tokio is to async Rust, but for AI agents.

```rust
use kainetic_core::{AgentConfig, KaineticRuntime};
use kainetic_providers::AnthropicProvider;
use kainetic_tools::builtin::CurrentDatetimeTool;

#[kainetic_macros::agent(description = "Answers questions about today's date.")]
async fn date_agent(input: String, ctx: AgentContext) -> Result<String, AgentError> {
    ReActLoop::new(ctx.config().clone()).execute(input, ctx).await
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let runtime = KaineticRuntime::builder()
        .provider(AnthropicProvider::from_env()?)
        .tool(CurrentDatetimeTool)
        .build();

    let agent = DateAgent::new();
    let answer = runtime.run(&agent, "What day is it?".into()).await?;
    println!("{answer}");
    Ok(())
}
```

## Why Kainetic?

| | Kainetic | Python frameworks |
|---|---|---|
| Cold start | < 5 ms | 60–140 ms |
| Memory at rest | < 15 MB | 180–250 MB |
| P99 overhead | < 50 ms | +800 ms |
| Type safety | Compile-time | Runtime |
| Cancellation | Structural (`CancellationToken`) | Best-effort |

## Core design principles

- **Type-safe everywhere.** Tool inputs/outputs and provider responses are typed Rust structs. JSON only exists at wire boundaries, validated via `schemars`.
- **Async-first, cancellation-safe.** All execution runs on Tokio. A cancelled parent immediately cascades to every in-flight tool call and child agent.
- **Parallel by default.** When the model returns N independent tool calls, they are dispatched with `FuturesUnordered` — not as a configuration option, but as the fundamental execution model.
- **Actor model.** Each agent instance is an actor communicating via typed channels. No shared mutable state.
- **No unsafe code.** `#![deny(unsafe_code)]` is enforced in every core crate via CI.

## Quick links

- [Installation](./getting-started/installation.md)
- [Your first agent](./getting-started/first-agent.md)
- [API reference](https://docs.rs/kainetic)
- [GitHub](https://github.com/kainetic/kainetic)
