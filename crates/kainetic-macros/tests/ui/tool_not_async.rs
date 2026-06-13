use kainetic_tools::{ToolContext, ToolError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, JsonSchema)]
struct Input { x: i32 }

#[derive(Serialize, JsonSchema)]
struct Output { y: i32 }

#[kainetic_macros::tool(description = "Not async.")]
fn not_async(input: Input, _ctx: ToolContext) -> Result<Output, ToolError> {
    Ok(Output { y: input.x })
}

fn main() {}
