//! `AgentError` — the error type for all agent operations.

use kainetic_schema::KaineticError;
use thiserror::Error;

/// Errors that can occur during agent execution.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AgentError {
    /// The agent's configured maximum iterations were reached without producing
    /// a final text response.
    #[error("maximum iterations ({0}) exceeded without a final response")]
    MaxIterationsExceeded(u32),

    /// The run's `CancellationToken` was cancelled before the agent completed.
    #[error("agent run was cancelled")]
    Cancelled,

    /// The agent run exceeded its configured wall-clock timeout.
    #[error("agent run timed out")]
    Timeout,

    /// An error was returned by the underlying language model provider.
    #[error("provider error: {0}")]
    ProviderError(String),

    /// An error propagated from a tool call during the `ReAct` loop.
    #[error("tool error: {0}")]
    ToolError(String),

    /// A user-defined error raised inside an `#[agent]` function body.
    #[error("{0}")]
    User(String),
}

impl From<AgentError> for KaineticError {
    fn from(e: AgentError) -> Self {
        let display = e.to_string();
        match e {
            AgentError::Cancelled => KaineticError::Cancelled,
            AgentError::Timeout => KaineticError::Timeout(0),
            AgentError::ProviderError(msg) => KaineticError::Provider(msg),
            AgentError::ToolError(msg) => KaineticError::Tool(msg),
            AgentError::MaxIterationsExceeded(_) | AgentError::User(_) => {
                KaineticError::Provider(display)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelled_display() {
        assert_eq!(AgentError::Cancelled.to_string(), "agent run was cancelled");
    }

    #[test]
    fn timeout_display() {
        assert_eq!(AgentError::Timeout.to_string(), "agent run timed out");
    }

    #[test]
    fn max_iterations_display() {
        assert!(AgentError::MaxIterationsExceeded(5)
            .to_string()
            .contains('5'));
    }

    #[test]
    fn provider_error_display() {
        let e = AgentError::ProviderError("rate limited".to_owned());
        assert!(e.to_string().contains("rate limited"));
    }

    #[test]
    fn converts_cancelled_to_kainetic() {
        let k: KaineticError = AgentError::Cancelled.into();
        assert!(matches!(k, KaineticError::Cancelled));
    }

    #[test]
    fn converts_timeout_to_kainetic() {
        let k: KaineticError = AgentError::Timeout.into();
        assert!(matches!(k, KaineticError::Timeout(_)));
    }

    #[test]
    fn converts_provider_error_to_kainetic() {
        let k: KaineticError = AgentError::ProviderError("err".to_owned()).into();
        assert!(matches!(k, KaineticError::Provider(_)));
    }

    #[test]
    fn converts_tool_error_to_kainetic() {
        let k: KaineticError = AgentError::ToolError("tool failed".to_owned()).into();
        assert!(matches!(k, KaineticError::Tool(_)));
    }

    #[test]
    fn converts_max_iterations_to_kainetic_provider() {
        let k: KaineticError = AgentError::MaxIterationsExceeded(10).into();
        assert!(matches!(k, KaineticError::Provider(_)));
    }
}
