//! Python bindings for Kainetic via PyO3.
//!
//! Exposes the Kainetic runtime to Python as a native extension module.
//! Build with `maturin develop` or `maturin build`.
//!
//! # Python usage
//!
//! ```python
//! import asyncio
//! from kainetic import KaineticRuntime, AnthropicProvider, tool, agent
//!
//! @tool(name="greet", description="Returns a greeting.")
//! async def greet(name: str) -> str:
//!     return f"Hello, {name}!"
//!
//! @agent(name="hello", description="A greeting agent.")
//! async def hello_agent(input: str) -> str:
//!     return f"The agent says: {input}"
//!
//! async def main():
//!     provider = AnthropicProvider.from_env()
//!     runtime = KaineticRuntime(provider=provider, tools=[greet])
//!     result = await runtime.run(hello_agent, "world")
//!     print(result)
//!
//! asyncio.run(main())
//! ```

#![deny(unsafe_code)]
// PyO3-generated code triggers several pedantic lints we cannot suppress at
// the call site, so we allow them crate-wide.
#![allow(
    clippy::used_underscore_binding,
    clippy::needless_pass_by_value,
    clippy::missing_errors_doc,
    // pyo3 #[pymethods] macro expansion triggers useless-conversion false positives
    clippy::useless_conversion
)]

mod agent;
mod context;
mod error;
mod provider;
mod runtime;
mod tool;

use pyo3::prelude::*;

/// Kainetic Python extension module.
#[pymodule]
fn _kainetic(m: &Bound<'_, PyModule>) -> PyResult<()> {
    pyo3::prepare_freethreaded_python();

    m.add_class::<runtime::PyKaineticRuntime>()?;
    m.add_class::<context::PyAgentContext>()?;
    m.add_class::<provider::PyAnthropicProvider>()?;
    m.add_class::<provider::PyOpenAiProvider>()?;
    m.add_function(wrap_pyfunction!(tool::py_tool, m)?)?;
    m.add_function(wrap_pyfunction!(agent::py_agent, m)?)?;
    Ok(())
}
