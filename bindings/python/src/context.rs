//! Python-facing `AgentContext` wrapper.

use pyo3::prelude::*;
use tokio_util::sync::CancellationToken;

/// Execution context passed to Python agent/tool callables.
///
/// Provides access to memory and cancellation.
///
/// Example::
///
///     @agent(name="demo", description="Demo agent.")
///     async def demo(input: str, ctx: AgentContext) -> str:
///         await ctx.memory_write("last_input", input)
///         return input.upper()
#[pyclass(name = "AgentContext")]
#[derive(Clone)]
pub struct PyAgentContext {
    pub(crate) token: CancellationToken,
}

#[pymethods]
impl PyAgentContext {
    /// Request cooperative cancellation of the current run.
    ///
    /// All in-flight tool calls and child agents will observe the cancellation
    /// on their next yield point.
    pub fn cancel(&self) {
        self.token.cancel();
    }

    /// Returns `True` if cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    fn __repr__(&self) -> String {
        format!("AgentContext(cancelled={})", self.token.is_cancelled())
    }
}
