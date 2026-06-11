//! Error types for `kainetic-orchestra`.

use thiserror::Error;

/// Errors that can occur during pipeline execution.
#[derive(Debug, Error)]
pub enum PipelineError {
    /// The named node does not exist in the pipeline graph.
    #[error("pipeline node not found: {0}")]
    NodeNotFound(String),

    /// Input/output serialisation failed at an edge boundary.
    #[error("serialisation error at edge: {0}")]
    Serialization(String),

    /// An agent within the pipeline returned an error.
    #[error("agent error: {0}")]
    Agent(String),

    /// The pipeline exceeded its maximum iteration limit without terminating.
    #[error("pipeline exceeded maximum iterations")]
    MaxIterationsExceeded,

    /// The pipeline graph failed validation at build time.
    #[error("pipeline graph is invalid: {0}")]
    InvalidGraph(String),

    /// An escalation was raised and no handler was configured.
    #[error("agent escalated: {0}")]
    Escalated(String),
}

/// Errors that can occur in a [`crate::Supervisor`].
#[derive(Debug, Error)]
pub enum SupervisorError {
    /// All retry attempts for a task failed.
    #[error("all {attempts} retry attempts failed; last error: {last_error}")]
    AllAttemptsFailed {
        /// Number of attempts made.
        attempts: u32,
        /// Error from the final attempt.
        last_error: String,
    },

    /// The worker pool is empty — no workers were registered.
    #[error("supervisor has no workers")]
    NoWorkers,

    /// Input/output serialisation failed.
    #[error("serialisation error: {0}")]
    Serialization(String),
}

/// Errors that can occur in a [`crate::StateMachineAgent`].
#[derive(Debug, Error)]
pub enum StateMachineError {
    /// Serialising or deserialising the state failed.
    #[error("state serialisation error: {0}")]
    Serialization(String),

    /// A memory backend operation failed during checkpointing.
    #[error("checkpoint error: {0}")]
    Checkpoint(String),

    /// The transition function returned an error.
    #[error("transition error: {0}")]
    Transition(String),
}
