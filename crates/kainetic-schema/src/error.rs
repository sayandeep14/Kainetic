//! Top-level error type for the Kainetic runtime.

use thiserror::Error;

/// Top-level error type returned by the Kainetic runtime and all sub-crates.
///
/// Each variant corresponds to one runtime subsystem. Subsystem crates
/// implement `From<SubsystemError> for KaineticError` so callers can use
/// `?` without extra mapping.
///
/// The enum is `#[non_exhaustive]` — new variants may be added in minor
/// releases without breaking downstream code.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum KaineticError {
    /// An error returned by a model provider (Anthropic, OpenAI, etc.).
    #[error("provider error: {0}")]
    Provider(String),

    /// An error raised during tool execution or tool-registry operations.
    #[error("tool error: {0}")]
    Tool(String),

    /// An error raised by a memory backend.
    #[error("memory error: {0}")]
    Memory(String),

    /// An error raised during multi-agent orchestration.
    #[error("orchestration error: {0}")]
    Orchestration(String),

    /// Input failed JSON Schema validation before reaching the handler.
    #[error("validation error: {0}")]
    Validation(String),

    /// The operation exceeded its configured time budget.
    ///
    /// `seconds` is the budget that was exceeded, not the actual elapsed time.
    #[error("operation timed out after {0}s")]
    Timeout(u64),

    /// The operation was cancelled via a `CancellationToken`.
    #[error("operation was cancelled")]
    Cancelled,
}

impl KaineticError {
    /// Constructs a [`KaineticError::Provider`] variant.
    #[must_use]
    pub fn provider(message: impl Into<String>) -> Self {
        Self::Provider(message.into())
    }

    /// Constructs a [`KaineticError::Tool`] variant.
    #[must_use]
    pub fn tool(message: impl Into<String>) -> Self {
        Self::Tool(message.into())
    }

    /// Constructs a [`KaineticError::Memory`] variant.
    #[must_use]
    pub fn memory(message: impl Into<String>) -> Self {
        Self::Memory(message.into())
    }

    /// Constructs a [`KaineticError::Orchestration`] variant.
    #[must_use]
    pub fn orchestration(message: impl Into<String>) -> Self {
        Self::Orchestration(message.into())
    }

    /// Constructs a [`KaineticError::Validation`] variant.
    #[must_use]
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }

    /// Constructs a [`KaineticError::Timeout`] variant.
    ///
    /// `seconds` is the time budget (in whole seconds) that was exceeded.
    #[must_use]
    pub fn timeout(seconds: u64) -> Self {
        Self::Timeout(seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_display() {
        let e = KaineticError::provider("rate limited");
        assert_eq!(e.to_string(), "provider error: rate limited");
    }

    #[test]
    fn tool_display() {
        let e = KaineticError::tool("execution failed");
        assert_eq!(e.to_string(), "tool error: execution failed");
    }

    #[test]
    fn memory_display() {
        let e = KaineticError::memory("connection refused");
        assert_eq!(e.to_string(), "memory error: connection refused");
    }

    #[test]
    fn orchestration_display() {
        let e = KaineticError::orchestration("unreachable node");
        assert_eq!(e.to_string(), "orchestration error: unreachable node");
    }

    #[test]
    fn validation_display() {
        let e = KaineticError::validation("missing required field 'query'");
        assert_eq!(
            e.to_string(),
            "validation error: missing required field 'query'"
        );
    }

    #[test]
    fn timeout_display() {
        let e = KaineticError::timeout(30);
        assert_eq!(e.to_string(), "operation timed out after 30s");
    }

    #[test]
    fn cancelled_display() {
        let e = KaineticError::Cancelled;
        assert_eq!(e.to_string(), "operation was cancelled");
    }
}
