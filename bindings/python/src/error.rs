//! Error conversion from Kainetic errors to Python exceptions.

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::PyErr;

/// Converts a [`kainetic_core::KaineticError`] into a Python `RuntimeError`.
pub fn kainetic_err(e: impl std::fmt::Display) -> PyErr {
    PyRuntimeError::new_err(e.to_string())
}

/// Converts a serialisation error into a Python `ValueError`.
#[allow(dead_code)]
pub fn serde_err(e: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(format!("serialisation error: {e}"))
}
