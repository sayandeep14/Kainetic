//! `ToolContext` — per-call context propagated to every tool invocation.

use std::collections::HashMap;

use kainetic_schema::RunId;
use tokio_util::sync::CancellationToken;

/// Context passed to every tool invocation.
///
/// Provides the calling run's identity, a cancellation signal, the active
/// tracing span, and an escape-hatch `extras` map for framework-specific
/// metadata without breaking the `Tool` trait signature.
pub struct ToolContext {
    /// Identifier of the agent run that triggered this tool call.
    pub run_id: RunId,
    /// Token used to cancel in-flight operations cooperatively.
    ///
    /// Tools should check `cancellation_token.is_cancelled()` at natural
    /// yield points and return [`crate::ToolError::Cancelled`] if set.
    pub cancellation_token: CancellationToken,
    /// The active tracing span at the point the tool was dispatched.
    pub span: tracing::Span,
    /// Arbitrary key-value metadata that framework layers can attach without
    /// modifying the `ToolContext` struct.
    pub extras: HashMap<String, serde_json::Value>,
}

impl ToolContext {
    /// Creates a minimal context for testing and one-off calls.
    ///
    /// `span` is set to the ambient tracing span at the call site via
    /// [`tracing::Span::current`].
    #[must_use]
    pub fn new(run_id: RunId, cancellation_token: CancellationToken) -> Self {
        Self {
            run_id,
            cancellation_token,
            span: tracing::Span::current(),
            extras: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_context_has_empty_extras() {
        let ctx = ToolContext::new(RunId::new(), CancellationToken::new());
        assert!(ctx.extras.is_empty());
    }

    #[test]
    fn context_cancellation_token_propagates() {
        let token = CancellationToken::new();
        let child = token.child_token();
        let ctx = ToolContext::new(RunId::new(), child);
        assert!(!ctx.cancellation_token.is_cancelled());
        token.cancel();
        assert!(ctx.cancellation_token.is_cancelled());
    }
}
