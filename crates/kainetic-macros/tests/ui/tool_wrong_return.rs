use kainetic_tools::{ToolContext, ToolError};
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
struct Input { x: i32 }

#[kainetic_macros::tool(description = "Returns a plain i32, not Result.")]
async fn wrong_return(input: Input, _ctx: ToolContext) -> i32 {
    input.x
}

fn main() {}
