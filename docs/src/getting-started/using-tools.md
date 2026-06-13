# Using Tools

Tools are functions the language model can call during a run. Kainetic validates all tool inputs against a JSON Schema before execution, so the model can never pass unexpected data to your code.

## Built-in tools

| Tool | Crate | Description |
|---|---|---|
| `CurrentDatetimeTool` | `kainetic-tools` | Returns the current UTC date/time |
| `HttpRequestTool` | `kainetic-tools` | Performs HTTP GET/POST requests |
| `WebSearchTool` | `kainetic-tools` | Searches the web via Brave Search API |
| `WebFetchTool` | `kainetic-tools` | Fetches a URL and returns readable text |
| `FileReadTool` | `kainetic-tools` | Reads a file (path allowlist enforced) |
| `FileWriteTool` | `kainetic-tools` | Writes a file (path allowlist enforced) |
| `SqlQueryTool` | `kainetic-tools` | Executes read-only SQL against SQLite |

## Defining a custom tool with `#[tool]`

```rust
use kainetic_macros::tool;
use kainetic_tools::{ToolContext, ToolError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, JsonSchema)]
pub struct AddInput {
    /// First operand.
    pub a: f64,
    /// Second operand.
    pub b: f64,
}

#[derive(Serialize, JsonSchema)]
pub struct AddOutput {
    /// The sum of `a` and `b`.
    pub sum: f64,
}

#[tool(description = "Adds two numbers and returns their sum.")]
pub async fn add(input: AddInput, _ctx: ToolContext) -> Result<AddOutput, ToolError> {
    Ok(AddOutput { sum: input.a + input.b })
}
```

This generates a `pub struct Add;` that implements `Tool`. Register it:

```rust
let runtime = KaineticRuntime::builder()
    .provider(provider)
    .tool(Add)   // generated struct name is PascalCase of the function name
    .build();
```

## Implementing `Tool` manually

For full control — custom timeout logic, conditional execution — implement the trait directly:

```rust
use kainetic_schema::RootSchema;
use kainetic_tools::{Tool, ToolContext, ToolError, ToolFuture};
use schemars::schema_for;

pub struct MyTool;

impl Tool for MyTool {
    fn name(&self) -> &'static str { "my_tool" }
    fn description(&self) -> &'static str { "Does something useful." }
    fn input_schema(&self) -> RootSchema { schema_for!(MyInput) }
    fn output_schema(&self) -> RootSchema { schema_for!(MyOutput) }

    fn call(&self, input: serde_json::Value, ctx: ToolContext) -> ToolFuture<'_> {
        Box::pin(async move {
            let typed: MyInput = serde_json::from_value(input)
                .map_err(|e| ToolError::InputValidation(e.to_string()))?;
            // … your logic here
            Ok(serde_json::to_value(MyOutput { /* … */ })
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?)
        })
    }
}
```

## Timeouts

Add a per-tool timeout using the `timeout` attribute:

```rust
#[tool(description = "Slow external call.", timeout = "5s")]
async fn slow_api(input: ApiInput, ctx: ToolContext) -> Result<ApiOutput, ToolError> {
    // …
}
```

`timeout = "5s"` or `timeout = "500ms"` are both valid. The generated code wraps the call in `tokio::time::timeout` and returns `ToolError::Timeout` on expiry.
