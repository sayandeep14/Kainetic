//! The `Tool` trait and the `ToolFuture` type alias.

use std::{future::Future, pin::Pin};

use kainetic_schema::{RootSchema, ToolDescriptor};

use crate::{ToolContext, ToolError};

/// A boxed, heap-allocated, `Send` future returned by [`Tool::call`].
///
/// Using a concrete return type makes `Tool` object-safe so it can be stored
/// as `Arc<dyn Tool>` in the [`crate::ToolRegistry`].
pub type ToolFuture<'a> =
    Pin<Box<dyn Future<Output = Result<serde_json::Value, ToolError>> + Send + 'a>>;

/// The core abstraction for any capability an agent can invoke.
///
/// Implement this trait to expose a new capability to the agent runtime.
/// Input and output are always [`serde_json::Value`]; use `serde_json::from_value`
/// / `serde_json::to_value` inside `call` for typed access.
///
/// The [`crate::ToolRegistry`] validates the raw JSON input against
/// [`input_schema`](Tool::input_schema) before forwarding to `call`, so
/// implementations can assume a schema-valid input.
///
/// # Example
///
/// ```rust,no_run
/// use kainetic_tools::{Tool, ToolContext, ToolError, ToolFuture};
/// use kainetic_schema::{RootSchema, ToolDescriptor};
/// use schemars::schema_for;
/// use serde::{Deserialize, Serialize};
/// use schemars::JsonSchema;
///
/// #[derive(Deserialize, JsonSchema)]
/// struct PingInput {}
///
/// #[derive(Serialize, JsonSchema)]
/// struct PingOutput { message: String }
///
/// struct PingTool;
///
/// impl Tool for PingTool {
///     fn name(&self) -> &'static str { "ping" }
///     fn description(&self) -> &'static str { "Replies with pong." }
///     fn input_schema(&self) -> RootSchema { schema_for!(PingInput) }
///     fn output_schema(&self) -> RootSchema { schema_for!(PingOutput) }
///     fn call(&self, _input: serde_json::Value, _ctx: ToolContext) -> ToolFuture<'_> {
///         Box::pin(async move {
///             serde_json::to_value(PingOutput { message: "pong".to_owned() })
///                 .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
///         })
///     }
/// }
/// ```
pub trait Tool: Send + Sync + 'static {
    /// Short, snake-case identifier used to look up and invoke the tool.
    ///
    /// Must be unique within a [`crate::ToolRegistry`]. Panics on duplicate
    /// registration.
    #[must_use]
    fn name(&self) -> &'static str;

    /// Human-readable description forwarded to the model as the tool's purpose.
    #[must_use]
    fn description(&self) -> &'static str;

    /// JSON Schema that describes valid inputs for this tool.
    ///
    /// Used by [`crate::ToolRegistry`] to validate the raw JSON before
    /// dispatching to [`call`](Tool::call).
    #[must_use]
    fn input_schema(&self) -> RootSchema;

    /// JSON Schema that describes the shape of a successful output value.
    #[must_use]
    fn output_schema(&self) -> RootSchema;

    /// Executes the tool with a schema-validated JSON input.
    ///
    /// # Errors
    ///
    /// Returns [`ToolError::ExecutionFailed`] for domain-level failures.
    /// Returns [`ToolError::Cancelled`] if `ctx.cancellation_token` fires.
    /// Returns [`ToolError::Timeout`] if the implementation enforces a budget.
    fn call(&self, input: serde_json::Value, ctx: ToolContext) -> ToolFuture<'_>;

    /// Builds a [`ToolDescriptor`] from the tool's metadata methods.
    ///
    /// Provided as a convenience; override only if you need custom logic.
    #[must_use]
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: self.name().to_owned(),
            description: self.description().to_owned(),
            input_schema: self.input_schema(),
            output_schema: self.output_schema(),
        }
    }
}
