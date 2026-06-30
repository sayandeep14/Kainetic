//! `ToolRegistry` — lock-free concurrent storage for registered tools.

use std::sync::Arc;

use dashmap::DashMap;
use kainetic_schema::{RootSchema, ToolDescriptor};
use tracing::instrument;

use crate::{Tool, ToolContext, ToolError};

/// A lock-free, concurrent registry of [`Tool`] implementations.
///
/// Backed by a [`DashMap`] so multiple agent threads can look up and invoke
/// tools in parallel without contention. Registration is expected at startup
/// (single-threaded) and panics on duplicate names to catch misconfiguration
/// as early as possible.
///
/// # Example
///
/// ```rust,no_run
/// use kainetic_tools::{ToolRegistry, ToolContext};
/// use kainetic_schema::RunId;
/// use tokio_util::sync::CancellationToken;
///
/// # use kainetic_tools::builtin::CurrentDatetimeTool;
/// let registry = ToolRegistry::new();
/// registry.register(CurrentDatetimeTool);
///
/// # async fn run(registry: ToolRegistry) -> Result<(), Box<dyn std::error::Error>> {
/// let ctx = ToolContext::new(RunId::new(), CancellationToken::new());
/// let result = registry.call("current_datetime", serde_json::json!({}), ctx).await?;
/// # Ok(())
/// # }
/// ```
#[derive(Default)]
pub struct ToolRegistry {
    tools: DashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    /// Creates an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tools: DashMap::new(),
        }
    }

    /// Registers a tool.
    ///
    /// # Panics
    ///
    /// Panics if a tool with the same [`Tool::name`] is already registered.
    /// This is intentional: name collisions indicate a misconfiguration that
    /// should be caught at startup, not at runtime.
    pub fn register(&self, tool: impl Tool) -> &Self {
        let name = tool.name().to_owned();
        assert!(
            !self.tools.contains_key(&name),
            "tool '{name}' is already registered — name collision detected at startup"
        );
        self.tools.insert(name, Arc::new(tool));
        self
    }

    /// Returns the tool registered under `name`, or `None` if absent.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).map(|r| Arc::clone(r.value()))
    }

    /// Returns a snapshot of descriptors for all registered tools.
    ///
    /// Iteration order is unspecified (depends on the underlying `DashMap`
    /// shard layout).
    #[must_use]
    pub fn list(&self) -> Vec<ToolDescriptor> {
        self.tools.iter().map(|r| r.value().descriptor()).collect()
    }

    /// Validates `input` against the tool's schema and then calls it.
    ///
    /// Steps:
    /// 1. Look up the tool by `name`; return `ExecutionFailed` if missing.
    /// 2. Short-circuit with `Cancelled` if `ctx.cancellation_token` is set.
    /// 3. Validate `input` against [`Tool::input_schema`]; return
    ///    `InputValidation` on failure.
    /// 4. Dispatch to [`Tool::call`].
    ///
    /// # Errors
    ///
    /// Returns [`ToolError::ExecutionFailed`] if `name` is not registered.
    /// Returns [`ToolError::Cancelled`] if the cancellation token is set.
    /// Returns [`ToolError::InputValidation`] if the JSON input is invalid.
    /// Propagates any error returned by the tool's `call` implementation.
    #[instrument(skip(self, input, ctx), fields(tool.name = name))]
    pub async fn call(
        &self,
        name: &str,
        input: serde_json::Value,
        ctx: ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let tool = self.get(name).ok_or_else(|| {
            ToolError::ExecutionFailed(format!("tool '{name}' not found in registry"))
        })?;

        if ctx.cancellation_token.is_cancelled() {
            return Err(ToolError::Cancelled);
        }

        validate_input(&tool.input_schema(), &input)?;

        tool.call(input, ctx).await
    }
}

fn validate_input(schema: &RootSchema, input: &serde_json::Value) -> Result<(), ToolError> {
    let schema_value = serde_json::to_value(schema)
        .map_err(|e| ToolError::InputValidation(format!("failed to serialize schema: {e}")))?;
    let compiled = jsonschema::JSONSchema::options()
        .with_draft(jsonschema::Draft::Draft7)
        .compile(&schema_value)
        .map_err(|e| ToolError::InputValidation(format!("invalid schema: {e}")))?;
    if let Err(errors) = compiled.validate(input) {
        let messages: Vec<String> = errors.map(|e| e.to_string()).collect();
        return Err(ToolError::InputValidation(messages.join("; ")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use kainetic_schema::{RootSchema, RunId};
    use schemars::schema_for;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::{ToolContext, ToolFuture};

    // ── Minimal test tool ─────────────────────────────────────────────────────

    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    struct EchoInput {
        message: String,
    }

    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    struct EchoOutput {
        echo: String,
    }

    struct EchoTool;

    impl Tool for EchoTool {
        fn name(&self) -> &'static str {
            "echo"
        }

        fn description(&self) -> &'static str {
            "Echoes the input message."
        }

        fn input_schema(&self) -> RootSchema {
            schema_for!(EchoInput)
        }

        fn output_schema(&self) -> RootSchema {
            schema_for!(EchoOutput)
        }

        fn call(&self, input: serde_json::Value, _ctx: ToolContext) -> ToolFuture<'_> {
            Box::pin(async move {
                let typed: EchoInput = serde_json::from_value(input)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
                serde_json::to_value(EchoOutput {
                    echo: typed.message,
                })
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
            })
        }
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    fn ctx() -> ToolContext {
        ToolContext::new(RunId::new(), CancellationToken::new())
    }

    #[test]
    fn new_registry_is_empty() {
        let reg = ToolRegistry::new();
        assert!(reg.list().is_empty());
    }

    #[test]
    fn register_and_list() {
        let reg = ToolRegistry::new();
        reg.register(EchoTool);
        let descriptors = reg.list();
        assert_eq!(descriptors.len(), 1);
        assert_eq!(descriptors[0].name, "echo");
    }

    #[test]
    fn get_returns_tool_by_name() {
        let reg = ToolRegistry::new();
        reg.register(EchoTool);
        assert!(reg.get("echo").is_some());
    }

    #[test]
    fn get_returns_none_for_unknown() {
        let reg = ToolRegistry::new();
        assert!(reg.get("no_such_tool").is_none());
    }

    #[test]
    #[should_panic(expected = "already registered")]
    fn register_panics_on_duplicate() {
        let reg = ToolRegistry::new();
        reg.register(EchoTool);
        reg.register(EchoTool);
    }

    #[tokio::test]
    async fn call_succeeds_with_valid_input() {
        let reg = ToolRegistry::new();
        reg.register(EchoTool);
        let result = reg
            .call("echo", serde_json::json!({"message": "hello"}), ctx())
            .await
            .unwrap();
        assert_eq!(result["echo"], "hello");
    }

    #[tokio::test]
    async fn call_validates_input_missing_required_field() {
        let reg = ToolRegistry::new();
        reg.register(EchoTool);
        let err = reg
            .call("echo", serde_json::json!({}), ctx())
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InputValidation(_)));
    }

    #[tokio::test]
    async fn call_validates_input_wrong_type() {
        let reg = ToolRegistry::new();
        reg.register(EchoTool);
        let err = reg
            .call("echo", serde_json::json!({"message": 42}), ctx())
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InputValidation(_)));
    }

    #[tokio::test]
    async fn call_returns_execution_failed_for_unknown_tool() {
        let reg = ToolRegistry::new();
        let err = reg
            .call("no_such_tool", serde_json::json!({}), ctx())
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::ExecutionFailed(_)));
    }

    #[tokio::test]
    async fn call_returns_cancelled_when_token_is_cancelled() {
        let reg = ToolRegistry::new();
        reg.register(EchoTool);

        let token = CancellationToken::new();
        token.cancel();
        let ctx = ToolContext::new(RunId::new(), token);

        let err = reg
            .call("echo", serde_json::json!({"message": "x"}), ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Cancelled));
    }

    #[tokio::test]
    async fn concurrent_reads_are_safe() {
        use std::sync::Arc;

        let reg = Arc::new(ToolRegistry::new());
        reg.register(EchoTool);

        let handles: Vec<_> = (0..10)
            .map(|i| {
                let reg = Arc::clone(&reg);
                tokio::spawn(async move {
                    reg.call(
                        "echo",
                        serde_json::json!({"message": format!("msg-{i}")}),
                        ToolContext::new(RunId::new(), CancellationToken::new()),
                    )
                    .await
                    .unwrap()
                })
            })
            .collect();

        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result["echo"].as_str().unwrap().starts_with("msg-"));
        }
    }
}
