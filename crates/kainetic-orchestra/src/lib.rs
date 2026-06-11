//! Multi-agent orchestration for Kainetic.
//!
//! Provides [`Pipeline`] (a validated DAG of [`AgentNode`]s with typed edges),
//! [`AgentOutcome`] (Complete / Handoff / Escalate), typed handoff routing,
//! the [`parallel!`] macro for concurrent agent execution, the [`Supervisor`]
//! worker pool with configurable routing strategies, and [`StateMachineAgent`]
//! for durable long-running workflows with checkpoint/resume semantics.
#![deny(clippy::all, clippy::pedantic, missing_docs, unsafe_code)]
#![allow(clippy::module_name_repetitions)]

pub mod error;
pub mod node;
pub mod outcome;
pub mod pipeline;
pub mod state_machine;
pub mod supervisor;

pub use error::{PipelineError, StateMachineError, SupervisorError};
pub use node::AgentNode;
pub use outcome::{AgentOutcome, EscalationReason, HandoffTarget, TransferContext};
pub use pipeline::{Pipeline, PipelineBuilder, DONE};
pub use state_machine::{StateMachineAgent, StateMachineBuilder, Transition};
pub use supervisor::{RoutingStrategy, Supervisor, SupervisorBuilder};

/// Runs multiple agents concurrently and returns all their results as a tuple.
///
/// This is a thin wrapper around [`tokio::join!`] with a more expressive name.
/// All futures are polled concurrently on the current task; none is cancelled
/// if another fails.
///
/// # Examples
///
/// ```rust,ignore
/// use kainetic_orchestra::parallel;
///
/// let (r1, r2) = parallel!(
///     agent_a.run(input_a, ctx.clone()),
///     agent_b.run(input_b, ctx.clone()),
/// );
/// ```
#[macro_export]
macro_rules! parallel {
    ($($future:expr),+ $(,)?) => {
        ::tokio::join!($($future),+)
    };
}
