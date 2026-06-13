# Tools & the Tool Registry

A tool is any function the language model can invoke to retrieve information or take action. Kainetic represents tools as implementations of the `Tool` trait.

## The `Tool` trait

```rust
pub trait Tool: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn input_schema(&self) -> RootSchema;
    fn output_schema(&self) -> RootSchema;
    fn call(&self, input: serde_json::Value, ctx: ToolContext) -> ToolFuture<'_>;
}
```

`ToolFuture<'_>` is `Pin<Box<dyn Future<Output = Result<serde_json::Value, ToolError>> + Send + '_>>`. The `BoxFuture` pattern makes `Tool` object-safe so it can be stored as `Arc<dyn Tool>`.

## ToolRegistry

`ToolRegistry` is a `DashMap`-backed concurrent registry. Registering a tool twice with the same name panics at startup — this is intentional to catch misconfiguration early.

```rust
let registry = ToolRegistry::new();
registry.register(CurrentDatetimeTool);
registry.register(WebSearchTool::with_key("brave-key"));

// call() validates input before executing the tool
let result = registry.call("current_datetime", json!({}), ctx).await?;
```

## Input validation

Every `registry.call()` validates the JSON input against the tool's `input_schema` **before** calling `tool.call()`. This means:

- Your tool implementation can `unwrap()` required fields without defensive checks — the schema has already verified they exist and have the right type.
- The model can never pass `null` to a required `String` field, for example.
- Validation errors are returned as `ToolError::InputValidation` with a human-readable message listing every violation.

## Parallel execution

When the model returns multiple tool calls simultaneously, `ReActLoop` dispatches all of them via `FuturesUnordered`. Each tool call gets its own `ToolContext` with a child cancellation token, so cancelling the parent run cascades correctly.

## ToolContext

`ToolContext` gives tools access to:

- `run_id: RunId` — the current run identifier (for logging, tracing)
- `cancellation_token: CancellationToken` — check `.is_cancelled()` in long-running tools
- `tracing::Span` — the active span for structured logging

Tools do **not** have access to the `AgentContext` — they cannot modify the conversation history or the provider configuration. This is intentional: tools are side-effectful functions, not agents.
