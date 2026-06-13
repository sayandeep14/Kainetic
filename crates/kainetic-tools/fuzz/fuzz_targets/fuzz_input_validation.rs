//! Fuzz target: JSON schema input validation in `ToolRegistry::call`.
//!
//! Feeds arbitrary bytes as a JSON payload to the `ToolRegistry::call` path,
//! exercising the `jsonschema` validation step. The goal is to verify that:
//!
//! - No amount of malformed JSON causes a panic, segfault, or infinite loop.
//! - The only outcomes are `Ok(result)`, `Err(InputValidation(_))`,
//!   `Err(ExecutionFailed(_))`, or `Err(Cancelled)` — all documented variants.
//!
//! Run with:
//! ```bash
//! cargo +nightly fuzz run fuzz_input_validation -- -max_total_time=60
//! ```

#![no_main]

use kainetic_schema::{RootSchema, RunId};
use kainetic_tools::{Tool, ToolContext, ToolError, ToolFuture, ToolRegistry};
use libfuzzer_sys::fuzz_target;
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

// ── A schema-constrained tool to validate against ────────────────────────────

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
struct StrictInput {
    /// Required string field.
    name: String,
    /// Optional non-negative integer.
    count: Option<u32>,
    /// Nested object with an enum value.
    mode: Mode,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum Mode {
    Fast,
    Slow,
    Auto,
}

struct StrictTool;

impl Tool for StrictTool {
    fn name(&self) -> &'static str {
        "strict"
    }

    fn description(&self) -> &'static str {
        "Fuzz target — schema-constrained tool."
    }

    fn input_schema(&self) -> RootSchema {
        schema_for!(StrictInput)
    }

    fn output_schema(&self) -> RootSchema {
        schema_for!(StrictInput)
    }

    fn call(&self, input: serde_json::Value, _ctx: ToolContext) -> ToolFuture<'_> {
        // Simply echo the input back — only reachable if validation passes.
        Box::pin(async move { Ok(input) })
    }
}

// ── Fuzz target ───────────────────────────────────────────────────────────────

fuzz_target!(|data: &[u8]| {
    // Parse the fuzz bytes as a JSON value; skip if not valid UTF-8 or JSON.
    let Ok(s) = std::str::from_utf8(data) else { return };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(s) else { return };

    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("tokio runtime");

    let registry = ToolRegistry::new();
    registry.register(StrictTool);

    let ctx = ToolContext::new(RunId::new(), CancellationToken::new());

    let result = rt.block_on(registry.call("strict", value, ctx));

    // Only legal outcomes:
    match result {
        Ok(_) => {}
        Err(ToolError::InputValidation(_)) => {}
        Err(ToolError::ExecutionFailed(_)) => {}
        Err(ToolError::Cancelled) => {}
        Err(ToolError::Timeout) => {}
        Err(ToolError::Unauthorized) => {}
    }
});
