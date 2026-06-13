# Agent Execution Model

Kainetic models agent execution as a **single async task** per run. There is no thread pool, no background scheduler, and no polling loop — just `await`.

## The call chain

```
KaineticRuntime::run(agent, input)
  └─► AgentContext::new(run_id, provider, tools, memory, cancellation_token)
        └─► agent.run(input, ctx)
              └─► ReActLoop::execute(input, ctx)
                    ├─► provider.complete(request)          ← LLM call
                    ├─► tools.call(name, input, tool_ctx)   ← parallel dispatch
                    └─► [repeat until end_turn or max_iterations]
```

## Lifecycle events

Every state transition emits an `AgentEvent` to a `broadcast::Sender<AgentEvent>`. Subscribe via `runtime.subscribe_events()` to receive:

| Event | When |
|---|---|
| `RunStarted` | `run()` begins |
| `LlmCallStarted` | Before each provider call |
| `LlmCallCompleted` | After each provider call (with token counts) |
| `ToolCallStarted` | Before each tool invocation |
| `ToolCallCompleted` | After a successful tool call |
| `ToolCallFailed` | After a failed tool call |
| `MemoryRead` / `MemoryWrite` | On memory backend access |
| `RunCompleted` | Final text response returned |
| `RunFailed` | Error or cancellation |

## Cancellation

Pass a `CancellationToken` via `AgentConfig` or `KaineticRuntimeBuilder`. Cancelling it at any point:

1. Stops the ReAct loop at the next iteration boundary.
2. Cancels all in-flight tool calls via their own tokens.
3. Returns `AgentError::Cancelled` immediately.

There are no resource leaks — Tokio's structured concurrency guarantees all spawned tasks complete or are cancelled before `run()` returns.

## Memory and context

`AgentContext` is cheaply clonable (all fields behind `Arc`) and threaded through every layer without explicit parameter passing. Tools receive a `ToolContext` (a subset of `AgentContext`) — they cannot modify the conversation history or the provider configuration.
