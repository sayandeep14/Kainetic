//! `py_tool` factory — wraps a Python callable as a Kainetic [`Tool`].

use std::sync::Arc;

use kainetic_schema::RootSchema;
use kainetic_tools::{Tool, ToolContext, ToolError, ToolFuture};
use pyo3::prelude::*;
use schemars::schema_for;
use serde_json::Value;

/// A Kainetic [`Tool`] backed by a Python callable.
///
/// The callable receives a JSON string as its first argument and must return a
/// JSON string. Async callables should use `asyncio.run(...)` internally.
pub struct PyTool {
    name: String,
    description: String,
    callable: Py<PyAny>,
}

impl Clone for PyTool {
    fn clone(&self) -> Self {
        Python::with_gil(|py| Self {
            name: self.name.clone(),
            description: self.description.clone(),
            callable: self.callable.clone_ref(py),
        })
    }
}

impl Tool for PyTool {
    fn name(&self) -> &'static str {
        Box::leak(self.name.clone().into_boxed_str())
    }

    fn description(&self) -> &'static str {
        Box::leak(self.description.clone().into_boxed_str())
    }

    fn input_schema(&self) -> RootSchema {
        schema_for!(Value)
    }

    fn output_schema(&self) -> RootSchema {
        schema_for!(Value)
    }

    fn call(&self, input: Value, _ctx: ToolContext) -> ToolFuture<'_> {
        let callable = Python::with_gil(|py| self.callable.clone_ref(py));
        Box::pin(async move {
            Python::with_gil(|py| -> PyResult<Value> {
                let input_str = serde_json::to_string(&input)
                    .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
                let result: String = callable.call1(py, (input_str,))?.extract(py)?;
                serde_json::from_str(&result)
                    .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
            })
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
        })
    }
}

// ── @tool decorator ────────────────────────────────────────────────────────

/// Register a Python callable as a Kainetic tool.
///
/// The callable receives a JSON string as input and must return a JSON string.
///
/// Example::
///
///     tool_handle = tool(
///         name="add",
///         description="Adds two numbers.",
///         callable=lambda s: json.dumps({"sum": json.loads(s)["a"] + json.loads(s)["b"]})
///     )
#[pyfunction]
#[pyo3(signature = (name, description, callable))]
pub fn py_tool(name: String, description: String, callable: PyObject) -> PyToolHandle {
    PyToolHandle {
        inner: Arc::new(PyTool {
            name,
            description,
            callable,
        }),
    }
}

/// Handle to a Python-backed tool, storable in Python and passable to the runtime.
#[pyclass(name = "Tool")]
#[derive(Clone)]
pub struct PyToolHandle {
    pub(crate) inner: Arc<PyTool>,
}

#[pymethods]
impl PyToolHandle {
    fn __repr__(&self) -> String {
        format!("Tool(name='{}')", self.inner.name)
    }
}
