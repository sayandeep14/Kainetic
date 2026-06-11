//! `AgentEvent` — lifecycle events emitted during a run.

use kainetic_schema::RunId;

/// Lifecycle events emitted by the [`crate::KaineticRuntime`] and [`crate::ReActLoop`].
///
/// All variants are `Clone` so they can be distributed to multiple subscribers
/// via a `tokio::sync::broadcast` channel. Subscribe with
/// [`KaineticRuntime::subscribe_events`][crate::KaineticRuntime::subscribe_events].
///
/// The enum is `#[non_exhaustive]` — new variants may be added in minor
/// releases without breaking downstream `match` arms.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum AgentEvent {
    /// A new agent run has started.
    RunStarted {
        /// Identifier for this run.
        run_id: RunId,
        /// Name of the agent (from [`crate::Agent::name`]).
        agent: String,
    },
    /// A completion request has been dispatched to the language model.
    LlmCallStarted {
        /// Identifier for the parent run.
        run_id: RunId,
        /// Provider name (e.g. `"anthropic"`, `"openai"`, `"gemini"`).
        provider: String,
        /// The model identifier that received the request.
        model: String,
        /// Number of messages in the conversation history at call time.
        messages: u32,
    },
    /// The language model returned a completion response.
    LlmCallCompleted {
        /// Identifier for the parent run.
        run_id: RunId,
        /// Prompt tokens consumed in this call.
        prompt_tokens: u32,
        /// Completion tokens generated in this call.
        completion_tokens: u32,
        /// Wall-clock round-trip time in milliseconds.
        latency_ms: u64,
    },
    /// A tool call has been dispatched.
    ToolCallStarted {
        /// Identifier for the parent run.
        run_id: RunId,
        /// Name of the tool being invoked.
        tool: String,
        /// Raw JSON input passed to the tool.
        input: serde_json::Value,
    },
    /// A tool call completed successfully.
    ToolCallCompleted {
        /// Identifier for the parent run.
        run_id: RunId,
        /// Name of the tool that was invoked.
        tool: String,
        /// Serialised JSON output from the tool.
        output: serde_json::Value,
        /// Wall-clock duration of the tool call in milliseconds.
        latency_ms: u64,
    },
    /// A tool call failed.
    ToolCallFailed {
        /// Identifier for the parent run.
        run_id: RunId,
        /// Name of the tool that failed.
        tool: String,
        /// Human-readable description of the failure.
        error: String,
    },
    /// The agent run completed successfully.
    RunCompleted {
        /// Identifier for this run.
        run_id: RunId,
        /// Total tokens consumed across all LLM calls in the run.
        total_tokens: u32,
        /// Estimated cost in US dollars for the run.
        cost_usd: f64,
        /// Total wall-clock duration in milliseconds.
        latency_ms: u64,
    },
    /// A memory entry was read from the backend.
    MemoryRead {
        /// Identifier for the parent run.
        run_id: RunId,
        /// The namespace/key that was read.
        key: String,
        /// Whether the key was found.
        hit: bool,
    },
    /// A memory entry was written to the backend.
    MemoryWrite {
        /// Identifier for the parent run.
        run_id: RunId,
        /// The namespace/key that was written.
        key: String,
    },
    /// The agent run failed.
    RunFailed {
        /// Identifier for this run.
        run_id: RunId,
        /// Human-readable description of the failure.
        error: String,
    },
}
