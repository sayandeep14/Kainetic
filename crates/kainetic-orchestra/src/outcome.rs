//! `AgentOutcome` — the typed return value for pipeline-aware agents.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The outcome of an agent that participates in a [`crate::Pipeline`].
///
/// An agent returns `Complete(T)` to signal that it has finished its work and
/// its output should be passed to the next pipeline stage. `Handoff` transfers
/// control to a named agent at runtime. `Escalate` signals an error condition
/// that requires human or supervisor intervention.
#[derive(Debug, Clone)]
pub enum AgentOutcome<T> {
    /// Normal completion — pass `T` to the next pipeline stage.
    Complete(T),
    /// Transfer control to the named agent, carrying a typed input.
    Handoff(HandoffTarget),
    /// The agent cannot proceed and requires external intervention.
    Escalate(EscalationReason),
}

/// Specifies the target of a runtime agent handoff.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffTarget {
    /// Name of the agent to hand off to (must be registered in the pipeline).
    pub agent: String,
    /// JSON-encoded input to pass to the target agent.
    pub input: Value,
    /// Trace and session context to forward across the handoff boundary.
    pub context: TransferContext,
}

/// Reason an agent is escalating rather than completing normally.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationReason {
    /// Short machine-readable code (e.g. `"missing_permissions"`).
    pub code: String,
    /// Human-readable description of what went wrong.
    pub message: String,
    /// Arbitrary additional context.
    pub details: HashMap<String, Value>,
}

/// Context forwarded across a handoff boundary to preserve observability
/// and session continuity.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransferContext {
    /// Tracing context (serialised W3C `traceparent` header value, if present).
    pub trace_parent: Option<String>,
    /// Session metadata forwarded from the originating agent.
    pub session_metadata: HashMap<String, Value>,
}

impl EscalationReason {
    /// Creates a new escalation reason with the given `code` and `message`.
    #[must_use]
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: HashMap::new(),
        }
    }

    /// Attaches an additional detail value.
    #[must_use]
    pub fn with_detail(mut self, key: impl Into<String>, value: Value) -> Self {
        self.details.insert(key.into(), value);
        self
    }
}
