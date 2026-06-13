# Your First Agent

This tutorial walks through building a date-aware assistant that answers questions about the current time.

## 1. Define the agent function

```rust
use kainetic_core::{AgentConfig, AgentContext, AgentError, ReActLoop};
use kainetic_macros::agent;

#[agent(description = "A helpful assistant that knows the current date and time.")]
async fn assistant(input: String, ctx: AgentContext) -> Result<String, AgentError> {
    ReActLoop::new(ctx.config().clone()).execute(input, ctx).await
}
```

The `#[agent]` macro generates:

- A `pub struct Assistant { pub config: AgentConfig }` with `new()` and `with_config()` constructors.
- A complete `Agent` trait implementation that wires `run()` to your function.

## 2. Register tools

```rust
use kainetic_tools::builtin::CurrentDatetimeTool;

let runtime = KaineticRuntime::builder()
    .provider(AnthropicProvider::from_env()?)
    .tool(CurrentDatetimeTool)   // gives the model access to the current date
    .build();
```

## 3. Run

```rust
let agent = Assistant::new();
let answer = runtime.run(&agent, "What day of the week is it?".into()).await?;
println!("{answer}");
```

## Full example

```rust
use anyhow::Result;
use kainetic_core::{AgentContext, AgentError, KaineticRuntime, ReActLoop};
use kainetic_macros::agent;
use kainetic_providers::AnthropicProvider;
use kainetic_tools::builtin::CurrentDatetimeTool;

#[agent(description = "A date-aware assistant.")]
async fn assistant(input: String, ctx: AgentContext) -> Result<String, AgentError> {
    ReActLoop::new(ctx.config().clone()).execute(input, ctx).await
}

#[tokio::main]
async fn main() -> Result<()> {
    let runtime = KaineticRuntime::builder()
        .provider(AnthropicProvider::from_env()?)
        .tool(CurrentDatetimeTool)
        .build();

    let answer = runtime
        .run(&Assistant::new(), "What day is today?".into())
        .await?;
    println!("{answer}");
    Ok(())
}
```

Run it:

```bash
ANTHROPIC_API_KEY=sk-... cargo run
# Today is Thursday, June 12th, 2026.
```

## What happens under the hood

1. `KaineticRuntime::run` creates an `AgentContext` carrying the provider, tools, memory backend, and a fresh `RunId`.
2. `ReActLoop::execute` sends your message to Claude along with the tool descriptors.
3. Claude responds with a `ToolUse` block requesting `current_datetime`.
4. The loop dispatches the tool call, appends the result as a `ToolResult` message, and calls Claude again.
5. Claude produces a final text response; the loop returns it.

All of this happens in a single `await` — no callbacks, no polling.
