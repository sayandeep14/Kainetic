# Parallel Tool Execution

Kainetic dispatches multiple tool calls concurrently by default. This page explains when it matters and how to control it.

## When does parallelism apply?

The model sometimes requests multiple tool calls in a single response. For example, Claude might request `web_search("topic A")` and `web_search("topic B")` simultaneously. Kainetic dispatches both with `FuturesUnordered`:

```
LLM response: [ToolUse("search", "topic A"), ToolUse("search", "topic B")]
      ↓ FuturesUnordered dispatch
       ┌──────────────────┐  ┌──────────────────┐
       │ search("topic A")│  │ search("topic B")│   ← run concurrently
       └──────────────────┘  └──────────────────┘
              ↓ results appended as ToolResult messages
       LLM call with both results
```

Total latency = `max(search_A, search_B)`, not `search_A + search_B`.

## Parallelism is on by default

```rust
AgentConfig::builder()
    .parallel_tools(true)   // default — no need to set this
    .build()
```

## Disabling parallel execution

During debugging, sequential execution makes logs easier to follow:

```rust
AgentConfig::builder()
    .sequential_tools()   // alias for .parallel_tools(false)
    .build()
```

## Tool dependencies

If tool B depends on the output of tool A, that dependency is expressed through the model's reasoning — it will naturally call A first, then B in the next iteration. You do not need to encode dependencies in the registry.

For compile-time-checked dependencies (future feature), see `#[tool(depends_on = [...])]` in the roadmap.

## Cancellation during parallel dispatch

When the parent run's cancellation token is cancelled mid-dispatch:

1. All in-flight tool futures receive their own cancelled child tokens.
2. Tools that check `ctx.cancellation_token.is_cancelled()` return `ToolError::Cancelled` immediately.
3. `FuturesUnordered` collects all results (error or success) and the loop returns `AgentError::Cancelled`.

No goroutine-style leaks — every spawned task completes before `run()` returns.
