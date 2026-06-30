//! Integration tests for the `#[tool]` proc macro.
//!
//! These live outside the crate so the macro expansion is exercised exactly
//! as an end-user would use it.

use kainetic_schema::RunId;
use kainetic_tools::{Tool, ToolContext, ToolError, ToolRegistry};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

// ─── Test tool definitions ────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
struct EchoInput {
    message: String,
}

#[derive(Debug, Serialize, JsonSchema)]
struct EchoOutput {
    echo: String,
}

#[kainetic_macros::tool(description = "Echoes the input message back unchanged.")]
async fn echo(input: EchoInput, _ctx: ToolContext) -> Result<EchoOutput, ToolError> {
    Ok(EchoOutput {
        echo: input.message,
    })
}

#[derive(Debug, Deserialize, JsonSchema)]
struct AddInput {
    a: f64,
    b: f64,
}

#[derive(Debug, Serialize, JsonSchema)]
struct AddOutput {
    sum: f64,
}

#[kainetic_macros::tool(description = "Adds two numbers.")]
async fn add(input: AddInput, _ctx: ToolContext) -> Result<AddOutput, ToolError> {
    Ok(AddOutput {
        sum: input.a + input.b,
    })
}

#[derive(Debug, Deserialize, JsonSchema)]
struct FailInput {
    should_fail: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
struct FailOutput {}

#[kainetic_macros::tool(description = "Returns an error when should_fail is true.")]
async fn maybe_fail(input: FailInput, _ctx: ToolContext) -> Result<FailOutput, ToolError> {
    if input.should_fail {
        return Err(ToolError::ExecutionFailed("intentional failure".to_owned()));
    }
    Ok(FailOutput {})
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SlowInput {}

#[derive(Debug, Serialize, JsonSchema)]
struct SlowOutput {}

#[kainetic_macros::tool(
    description = "Sleeps briefly, used to test timeout.",
    timeout = "100ms"
)]
async fn slow_tool(input: SlowInput, _ctx: ToolContext) -> Result<SlowOutput, ToolError> {
    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    let _ = input;
    Ok(SlowOutput {})
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn ctx() -> ToolContext {
    ToolContext::new(RunId::new(), CancellationToken::new())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[test]
fn generated_struct_has_correct_name() {
    // The macro derives `Echo` from `echo`, `Add` from `add`, etc.
    let _ = Echo;
    let _ = Add;
    let _ = MaybeFail;
}

#[test]
fn name_matches_function_name() {
    assert_eq!(Echo.name(), "echo");
    assert_eq!(Add.name(), "add");
    assert_eq!(MaybeFail.name(), "maybe_fail");
}

#[test]
fn description_matches_attribute() {
    assert_eq!(
        Echo.description(),
        "Echoes the input message back unchanged."
    );
    assert_eq!(Add.description(), "Adds two numbers.");
}

#[test]
fn descriptor_is_populated() {
    let desc = Echo.descriptor();
    assert_eq!(desc.name, "echo");
    assert_eq!(desc.description, "Echoes the input message back unchanged.");
}

#[tokio::test]
async fn echo_tool_call_returns_correct_output() {
    let result = Echo
        .call(serde_json::json!({"message": "hello"}), ctx())
        .await
        .unwrap();
    assert_eq!(result["echo"], "hello");
}

#[tokio::test]
async fn add_tool_call_returns_sum() {
    let result = Add
        .call(serde_json::json!({"a": 1.5, "b": 2.5}), ctx())
        .await
        .unwrap();
    assert!((result["sum"].as_f64().unwrap() - 4.0).abs() < f64::EPSILON);
}

#[tokio::test]
async fn tool_propagates_execution_error() {
    let err = MaybeFail
        .call(serde_json::json!({"should_fail": true}), ctx())
        .await
        .unwrap_err();
    assert!(matches!(err, ToolError::ExecutionFailed(_)));
}

#[tokio::test]
async fn tool_succeeds_when_not_failing() {
    MaybeFail
        .call(serde_json::json!({"should_fail": false}), ctx())
        .await
        .unwrap();
}

#[tokio::test]
async fn timeout_attribute_fires() {
    let err = SlowTool
        .call(serde_json::json!({}), ctx())
        .await
        .unwrap_err();
    assert!(
        matches!(err, ToolError::Timeout),
        "expected Timeout, got {err:?}"
    );
}

#[tokio::test]
async fn macro_tool_registers_and_calls_via_registry() {
    let registry = ToolRegistry::new();
    registry.register(Echo);
    registry.register(Add);

    let echo_result = registry
        .call("echo", serde_json::json!({"message": "world"}), ctx())
        .await
        .unwrap();
    assert_eq!(echo_result["echo"], "world");

    let add_result = registry
        .call("add", serde_json::json!({"a": 10.0, "b": 5.0}), ctx())
        .await
        .unwrap();
    assert!((add_result["sum"].as_f64().unwrap() - 15.0).abs() < f64::EPSILON);
}

#[tokio::test]
async fn registry_validates_macro_tool_input() {
    let registry = ToolRegistry::new();
    registry.register(Echo);

    // Missing required `message` field.
    let err = registry
        .call("echo", serde_json::json!({}), ctx())
        .await
        .unwrap_err();
    assert!(matches!(err, ToolError::InputValidation(_)));
}
