//! `py_agent` factory — wraps a Python callable as a Kainetic [`Agent`].

use std::sync::Arc;

use kainetic_core::{Agent, AgentConfig, AgentContext, AgentError, AgentFuture};
use pyo3::prelude::*;

/// A Kainetic [`Agent`] backed by a Python callable.
///
/// The callable receives a string input and must return a string output.
/// Async callables should use `asyncio.run(...)` internally.
pub struct PyAgentImpl {
    agent_name: String,
    agent_description: String,
    config: AgentConfig,
    callable: Py<PyAny>,
}

impl Agent for PyAgentImpl {
    type Input = String;
    type Output = String;
    type Error = AgentError;

    fn name(&self) -> &'static str {
        Box::leak(self.agent_name.clone().into_boxed_str())
    }

    fn description(&self) -> &'static str {
        Box::leak(self.agent_description.clone().into_boxed_str())
    }

    fn config(&self) -> &AgentConfig {
        &self.config
    }

    fn run(&self, input: String, _ctx: AgentContext) -> AgentFuture<'_, String, AgentError> {
        let callable = Python::with_gil(|py| self.callable.clone_ref(py));
        Box::pin(async move {
            Python::with_gil(|py| -> PyResult<String> {
                callable.call1(py, (input,))?.extract(py)
            })
            .map_err(|e| AgentError::User(e.to_string()))
        })
    }
}

// ── @agent decorator ───────────────────────────────────────────────────────

/// Register a Python callable as a Kainetic agent.
///
/// The callable receives a string input and must return a string output.
///
/// Example::
///
///     agent_handle = agent(
///         name="echo",
///         description="Echoes its input.",
///         callable=lambda s: s
///     )
#[pyfunction]
#[pyo3(signature = (name, description, callable))]
pub fn py_agent(name: String, description: String, callable: PyObject) -> PyAgentHandle {
    PyAgentHandle {
        inner: Arc::new(PyAgentImpl {
            agent_name: name,
            agent_description: description,
            config: AgentConfig::builder().build(),
            callable,
        }),
    }
}

/// Handle to a Python-backed agent, storable in Python and passable to the runtime.
#[pyclass(name = "Agent")]
#[derive(Clone)]
pub struct PyAgentHandle {
    pub(crate) inner: Arc<PyAgentImpl>,
}

#[pymethods]
impl PyAgentHandle {
    fn __repr__(&self) -> String {
        format!("Agent(name='{}')", self.inner.agent_name)
    }
}
