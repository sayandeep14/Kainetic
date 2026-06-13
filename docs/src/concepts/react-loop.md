# The ReAct Loop

`ReActLoop` is the execution engine that drives the Reason → Act → Observe cycle described in the [ReAct paper](https://arxiv.org/abs/2210.03629).

## One iteration

```
┌─────────────────────────────────────────────────────────────────┐
│ Reason  │  Send messages + tool descriptors to the LLM          │
├─────────────────────────────────────────────────────────────────┤
│ Act     │  Dispatch all returned tool calls (parallel by default)│
├─────────────────────────────────────────────────────────────────┤
│ Observe │  Append all tool results as ToolResult messages        │
└─────────────────────────────────────────────────────────────────┘
```

The loop repeats until the model returns a `stop_reason` of `end_turn` (no more tool calls), `max_iterations` is hit, the timeout fires, or the cancellation token is set.

## Parallel tool dispatch

When the model requests N tool calls in a single response, Kainetic dispatches all N using `FuturesUnordered`:

```rust
// All N tools run concurrently; total latency = max(individual latencies)
let results: Vec<ToolCallResult> = tool_calls
    .into_iter()
    .map(|call| execute_one_tool(call, tools.clone(), ctx.clone()))
    .collect::<FuturesUnordered<_>>()
    .collect()
    .await;
```

This is not a configuration option — it is the fundamental model. Use `AgentConfig::builder().sequential_tools()` only during debugging.

## Error recovery

A tool failure does **not** abort the loop. The error message is fed back to the model as a `ToolResult` with `is_error: true`. The model can then decide to retry with corrected parameters, choose a different tool, or produce a final response explaining the failure.

This mirrors how humans use computers: a shell command returning an error doesn't end your session.

## Configuration

```rust
AgentConfig::builder()
    .model("claude-sonnet-4-6")
    .max_iterations(20)              // default
    .timeout(Duration::from_secs(120)) // default
    .parallel_tools(true)            // default
    .system_prompt("You are …")
    .build()
```
