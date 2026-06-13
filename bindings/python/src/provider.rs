//! Python-facing provider classes.

use std::sync::Arc;

use kainetic_providers::{AnthropicProvider, ModelProvider, OpenAiProvider};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

/// Wraps any [`ModelProvider`] for passing into [`PyKaineticRuntime`].
#[derive(Clone)]
pub struct AnyProvider(pub Arc<dyn ModelProvider>);

// ── AnthropicProvider ──────────────────────────────────────────────────────

/// Anthropic Claude provider.
///
/// Reads `ANTHROPIC_API_KEY` from the environment.
///
/// Example::
///
///     from kainetic import AnthropicProvider
///     provider = AnthropicProvider.from_env()
#[pyclass(name = "AnthropicProvider")]
#[derive(Clone)]
pub struct PyAnthropicProvider {
    pub inner: Arc<AnthropicProvider>,
}

#[pymethods]
impl PyAnthropicProvider {
    /// Create an `AnthropicProvider` from the `ANTHROPIC_API_KEY` environment
    /// variable.
    #[staticmethod]
    pub fn from_env() -> PyResult<Self> {
        let p = AnthropicProvider::from_env()
            .map_err(|e| PyValueError::new_err(format!("ANTHROPIC_API_KEY: {e}")))?;
        Ok(Self { inner: Arc::new(p) })
    }

    /// Create an `AnthropicProvider` with an explicit API key.
    #[staticmethod]
    pub fn with_key(api_key: String) -> Self {
        Self {
            inner: Arc::new(AnthropicProvider::new(api_key)),
        }
    }

    fn __repr__(&self) -> &'static str {
        "AnthropicProvider()"
    }
}

impl From<PyAnthropicProvider> for AnyProvider {
    fn from(p: PyAnthropicProvider) -> Self {
        AnyProvider(p.inner)
    }
}

// ── OpenAiProvider ─────────────────────────────────────────────────────────

/// OpenAI provider.
///
/// Reads `OPENAI_API_KEY` from the environment.
///
/// Example::
///
///     from kainetic import OpenAiProvider
///     provider = OpenAiProvider.from_env()
#[pyclass(name = "OpenAiProvider")]
#[derive(Clone)]
pub struct PyOpenAiProvider {
    pub inner: Arc<OpenAiProvider>,
}

#[pymethods]
impl PyOpenAiProvider {
    /// Create an `OpenAiProvider` from the `OPENAI_API_KEY` environment variable.
    #[staticmethod]
    pub fn from_env() -> PyResult<Self> {
        let p = OpenAiProvider::from_env()
            .map_err(|e| PyValueError::new_err(format!("OPENAI_API_KEY: {e}")))?;
        Ok(Self { inner: Arc::new(p) })
    }

    /// Create an `OpenAiProvider` with an explicit API key.
    #[staticmethod]
    pub fn with_key(api_key: String) -> Self {
        Self {
            inner: Arc::new(OpenAiProvider::new(api_key)),
        }
    }

    fn __repr__(&self) -> &'static str {
        "OpenAiProvider()"
    }
}

impl From<PyOpenAiProvider> for AnyProvider {
    fn from(p: PyOpenAiProvider) -> Self {
        AnyProvider(p.inner)
    }
}

/// Extract a [`AnyProvider`] from a Python object that is either a
/// [`PyAnthropicProvider`] or [`PyOpenAiProvider`].
pub fn extract_provider(obj: &Bound<'_, PyAny>) -> PyResult<AnyProvider> {
    if let Ok(p) = obj.extract::<PyAnthropicProvider>() {
        return Ok(AnyProvider(p.inner));
    }
    if let Ok(p) = obj.extract::<PyOpenAiProvider>() {
        return Ok(AnyProvider(p.inner));
    }
    Err(PyValueError::new_err(
        "provider must be AnthropicProvider or OpenAiProvider",
    ))
}
