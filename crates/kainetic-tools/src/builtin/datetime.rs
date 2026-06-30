//! `CurrentDatetimeTool` — returns the current UTC date and time.

use chrono::Utc;
use kainetic_schema::RootSchema;
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};

use crate::{Tool, ToolContext, ToolError, ToolFuture};

/// Input for [`CurrentDatetimeTool`].
///
/// Empty: no parameters are required to get the current time.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct DatetimeInput {}

/// Output of [`CurrentDatetimeTool`].
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct DatetimeOutput {
    /// Current UTC timestamp in RFC 3339 / ISO 8601 format (e.g. `"2025-01-15T12:30:00+00:00"`).
    pub iso8601: String,
    /// Current UTC timestamp as Unix seconds since the epoch.
    pub unix_timestamp: i64,
}

/// Tool that returns the current UTC date and time.
///
/// Trivial in isolation, but useful as a first example of the `Tool` pattern
/// and as a dependency for agents that need temporal awareness.
pub struct CurrentDatetimeTool;

impl Tool for CurrentDatetimeTool {
    fn name(&self) -> &'static str {
        "current_datetime"
    }

    fn description(&self) -> &'static str {
        "Returns the current UTC date and time in ISO 8601 format and as a Unix timestamp."
    }

    fn input_schema(&self) -> RootSchema {
        schema_for!(DatetimeInput)
    }

    fn output_schema(&self) -> RootSchema {
        schema_for!(DatetimeOutput)
    }

    fn call(&self, _input: serde_json::Value, _ctx: ToolContext) -> ToolFuture<'_> {
        Box::pin(async move {
            let now = Utc::now();
            let output = DatetimeOutput {
                iso8601: now.to_rfc3339(),
                unix_timestamp: now.timestamp(),
            };
            serde_json::to_value(output).map_err(|e| ToolError::ExecutionFailed(e.to_string()))
        })
    }
}

#[cfg(test)]
mod tests {
    use kainetic_schema::RunId;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::{Tool, ToolContext};

    #[tokio::test]
    async fn returns_iso8601_string() {
        let tool = CurrentDatetimeTool;
        let ctx = ToolContext::new(RunId::new(), CancellationToken::new());
        let result = tool.call(serde_json::json!({}), ctx).await.unwrap();

        let iso = result["iso8601"]
            .as_str()
            .expect("iso8601 field is a string");
        // RFC 3339 contains a 'T' separator and a '+' or 'Z' timezone
        assert!(iso.contains('T'), "expected ISO 8601 format, got: {iso}");
    }

    #[tokio::test]
    async fn returns_positive_unix_timestamp() {
        let tool = CurrentDatetimeTool;
        let ctx = ToolContext::new(RunId::new(), CancellationToken::new());
        let result = tool.call(serde_json::json!({}), ctx).await.unwrap();

        let ts = result["unix_timestamp"]
            .as_i64()
            .expect("unix_timestamp is i64");
        assert!(
            ts > 1_000_000_000,
            "expected unix timestamp > 2001, got {ts}"
        );
    }

    #[test]
    fn name_and_description() {
        let tool = CurrentDatetimeTool;
        assert_eq!(tool.name(), "current_datetime");
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn input_schema_is_object() {
        let schema = CurrentDatetimeTool.input_schema();
        let value = serde_json::to_value(&schema).unwrap();
        assert_eq!(value["type"], "object");
    }
}
