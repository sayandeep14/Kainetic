//! Python-facing `KaineticRuntime` wrapper.
//!
//! Uses a dedicated Tokio multi-thread runtime stored inside the Python
//! class.  Python calls `runtime.run(agent, input)` synchronously (blocking
//! the calling thread); the Rust side drives the future on its own runtime.
//! This keeps the binding simple and avoids requiring a Python event loop.

use std::sync::Arc;

use kainetic_core::KaineticRuntime;
use pyo3::prelude::*;

use crate::agent::PyAgentHandle;
use crate::error::kainetic_err;
use crate::provider::extract_provider;
use crate::tool::PyToolHandle;

/// The Kainetic agent runtime.
///
/// Example::
///
///     from kainetic import KaineticRuntime, AnthropicProvider
///
///     provider = AnthropicProvider.from_env()
///     runtime = KaineticRuntime(provider=provider, tools=[my_tool])
///     result = runtime.run(my_agent, "hello")
///     print(result)
#[pyclass(name = "KaineticRuntime")]
pub struct PyKaineticRuntime {
    /// The Rust runtime that drives async operations.
    tokio_rt: tokio::runtime::Runtime,
    /// The underlying Kainetic runtime, shared across Python references.
    inner: Arc<KaineticRuntime>,
}

#[pymethods]
impl PyKaineticRuntime {
    /// Create a new runtime.
    ///
    /// Args:
    ///     provider: An ``AnthropicProvider`` or ``OpenAiProvider`` instance.
    ///     tools:    List of ``Tool`` handles registered with the runtime.
    #[new]
    #[pyo3(signature = (provider, tools = None))]
    pub fn new(provider: &Bound<'_, PyAny>, tools: Option<Vec<PyToolHandle>>) -> PyResult<Self> {
        let any_provider = extract_provider(provider)?;

        let mut builder = KaineticRuntime::builder().provider_arc(any_provider.0);
        if let Some(tool_list) = tools {
            for t in tool_list {
                // Unwrap the Arc — Tool is cheaply cloneable.
                let tool_impl = Arc::try_unwrap(t.inner)
                    .unwrap_or_else(|arc| (*arc).clone());
                builder = builder.tool(tool_impl);
            }
        }
        let inner = Arc::new(builder.build());

        let tokio_rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("kainetic-python")
            .build()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        Ok(Self { tokio_rt, inner })
    }

    /// Run an agent with the given string input.
    ///
    /// Blocks the calling thread until the agent completes.
    ///
    /// Args:
    ///     agent: An ``Agent`` handle created with ``@agent``.
    ///     input: String input passed to the agent.
    ///
    /// Returns:
    ///     str: The agent's string output.
    pub fn run(&self, agent_handle: PyAgentHandle, input: String) -> PyResult<String> {
        let inner = Arc::clone(&self.inner);
        let agent = agent_handle.inner.clone();

        self.tokio_rt
            .block_on(async move { inner.run(agent.as_ref(), input).await })
            .map_err(kainetic_err)
    }

    fn __repr__(&self) -> &'static str {
        "KaineticRuntime()"
    }
}
