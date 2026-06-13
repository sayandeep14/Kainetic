# KaineticError Reference

`KaineticError` is the top-level error type returned by the Kainetic runtime. Every subsystem error converts to it via `From` implementations.

## Variants

### `Provider(String)`

An error returned by a model provider (Anthropic, OpenAI, etc.).

Common causes:
- `authentication failed: check your API key` — API key is missing or invalid.
- `rate limited; retry after …` — rate limit hit; the runtime retries automatically but eventually gives up.
- `context length exceeded` — the conversation has grown beyond the model's context window. Use `EpisodicMemory` with a summarisation strategy to prevent this.
- `model not found: …` — the model name in `AgentConfig::model` is invalid for this provider.

### `Tool(String)`

An error raised during tool execution or registry operations.

Common causes:
- `tool 'name' not found in registry` — the model requested a tool that wasn't registered. This can happen if the model hallucinates a tool name; the `ReActLoop` feeds the error back to the model rather than aborting.
- `input validation: …` — the model supplied input that violates the tool's JSON Schema.
- `execution failed: …` — the tool's `call` implementation returned an error.

### `Memory(String)`

An error raised by a memory backend.

Common causes:
- `connection refused` / `timed out` — Redis or PostgreSQL backend is unavailable.
- `backend: usearch index mutex was poisoned` — a previous tool panicked while holding the index lock. Restart the agent.

### `Validation(String)`

Input failed JSON Schema validation before reaching a handler (used at API boundaries in `kainetic-cloud`).

### `Timeout(u64)`

The run exceeded its configured wall-clock budget.

- The `u64` is the budget in **seconds** (not the actual elapsed time).
- Configure via `AgentConfig::builder().timeout(Duration::from_secs(N))`.

### `Cancelled`

The run's `CancellationToken` was cancelled before the agent completed.

- No partial result is available.
- All in-flight tool calls have been dropped.
- This is a normal termination path, not a bug.

### `Orchestration(String)`

An error raised during multi-agent pipeline execution.

Common causes:
- `unreachable node: 'name'` — a `Pipeline::build()` validation failure (node defined but not reachable from the start node).
- `cycle detected` — the pipeline graph contains a cycle with no conditional exit.

## Actionability guide

| Error | First thing to check |
|---|---|
| `Provider: authentication failed` | `echo $ANTHROPIC_API_KEY` — is it set and non-empty? |
| `Provider: context length exceeded` | Use `EpisodicMemory` with a lower `max_history` |
| `Tool: not found in registry` | Check `registry.list()` — is the tool registered? |
| `Tool: input validation` | Print the raw tool call JSON from the `ToolCallStarted` event |
| `Timeout` | Increase `AgentConfig::timeout` or reduce `max_iterations` |
| `Cancelled` | Check who is calling `.cancel()` on the `CancellationToken` |
