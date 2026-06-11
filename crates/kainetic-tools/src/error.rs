//! `ToolError` — the error type for all tool operations.

use kainetic_schema::KaineticError;
use thiserror::Error;

/// Errors that can occur during tool registration, input validation, or execution.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ToolError {
    /// The supplied JSON input did not conform to the tool's input schema.
    #[error("input validation failed: {0}")]
    InputValidation(String),

    /// The tool's body returned an error during execution.
    #[error("tool execution failed: {0}")]
    ExecutionFailed(String),

    /// The tool call exceeded its configured timeout.
    #[error("tool call timed out")]
    Timeout,

    /// The run's `CancellationToken` was cancelled before or during execution.
    #[error("tool call was cancelled")]
    Cancelled,

    /// The caller does not have permission to invoke this tool.
    #[error("unauthorized: insufficient permissions to call this tool")]
    Unauthorized,
}

impl From<ToolError> for KaineticError {
    fn from(e: ToolError) -> Self {
        match e {
            ToolError::InputValidation(msg) => KaineticError::Validation(msg),
            ToolError::Timeout => KaineticError::Timeout(0),
            ToolError::Cancelled => KaineticError::Cancelled,
            ToolError::ExecutionFailed(msg) => KaineticError::Tool(msg),
            ToolError::Unauthorized => KaineticError::Tool(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_validation_display() {
        let e = ToolError::InputValidation("field 'query' is required".to_owned());
        assert!(e.to_string().contains("input validation failed"));
    }

    #[test]
    fn timeout_display() {
        assert_eq!(ToolError::Timeout.to_string(), "tool call timed out");
    }

    #[test]
    fn cancelled_display() {
        assert_eq!(ToolError::Cancelled.to_string(), "tool call was cancelled");
    }

    #[test]
    fn converts_to_kainetic_validation_error() {
        let k: KaineticError = ToolError::InputValidation("bad".to_owned()).into();
        assert!(matches!(k, KaineticError::Validation(_)));
    }

    #[test]
    fn converts_to_kainetic_timeout() {
        let k: KaineticError = ToolError::Timeout.into();
        assert!(matches!(k, KaineticError::Timeout(_)));
    }

    #[test]
    fn converts_to_kainetic_cancelled() {
        let k: KaineticError = ToolError::Cancelled.into();
        assert!(matches!(k, KaineticError::Cancelled));
    }

    #[test]
    fn converts_execution_failed_to_kainetic_tool() {
        let k: KaineticError = ToolError::ExecutionFailed("oops".to_owned()).into();
        assert!(matches!(k, KaineticError::Tool(_)));
    }
}
