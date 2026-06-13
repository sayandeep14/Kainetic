use kainetic_tools::ToolError;
use schemars::JsonSchema;
use serde::Serialize;

#[derive(Serialize, JsonSchema)]
struct Output { x: i32 }

#[kainetic_macros::tool(description = "No parameters at all.")]
async fn no_params() -> Result<Output, ToolError> {
    Ok(Output { x: 0 })
}

fn main() {}
