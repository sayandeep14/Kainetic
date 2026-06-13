//! Fuzz target: `ToolRegistry::call` with arbitrary tool names and inputs.
//!
//! Exercises the lookup-and-dispatch path with random name strings, verifying
//! that unknown names and any JSON input never cause a panic.
//!
//! Run with:
//! ```bash
//! cargo +nightly fuzz run fuzz_tool_call -- -max_total_time=60
//! ```

#![no_main]

use kainetic_schema::{RootSchema, RunId};
use kainetic_tools::{Tool, ToolContext, ToolFuture, ToolRegistry};
use libfuzzer_sys::fuzz_target;
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
struct AnyInput {
    value: serde_json::Value,
}

struct EchoTool;

impl Tool for EchoTool {
    fn name(&self) -> &'static str {
        "echo"
    }
    fn description(&self) -> &'static str {
        "Echo"
    }
    fn input_schema(&self) -> RootSchema {
        schema_for!(AnyInput)
    }
    fn output_schema(&self) -> RootSchema {
        schema_for!(AnyInput)
    }
    fn call(&self, input: serde_json::Value, _ctx: ToolContext) -> ToolFuture<'_> {
        Box::pin(async move { Ok(input) })
    }
}

fuzz_target!(|data: &[u8]| {
    // Split data in two: first half is the tool name, second half is the JSON input.
    let mid = data.len() / 2;
    let name_bytes = &data[..mid];
    let input_bytes = &data[mid..];

    let Ok(name) = std::str::from_utf8(name_bytes) else { return };
    let Ok(s) = std::str::from_utf8(input_bytes) else { return };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(s) else { return };

    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("tokio runtime");

    let registry = ToolRegistry::new();
    registry.register(EchoTool);

    let ctx = ToolContext::new(RunId::new(), CancellationToken::new());

    // Any outcome is acceptable — what must NOT happen is a panic.
    let _ = rt.block_on(registry.call(name, value, ctx));
});
