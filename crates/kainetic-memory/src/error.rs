//! `MemoryError` — the error type for all memory backend operations.

use kainetic_schema::KaineticError;
use thiserror::Error;

/// Errors that can occur in any memory backend.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MemoryError {
    /// The requested key was not found.
    #[error("key not found")]
    NotFound,

    /// Serialization or deserialization of a stored value failed.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// The backend itself returned an error (I/O, network, etc.).
    #[error("backend error: {0}")]
    Backend(String),

    /// The requested operation is not supported by this backend.
    ///
    /// For example, [`InMemoryBackend`] returns this for
    /// [`MemoryBackend::search`] because it has no vector index.
    ///
    /// [`InMemoryBackend`]: crate::InMemoryBackend
    /// [`MemoryBackend::search`]: crate::MemoryBackend::search
    #[error("operation not supported by this backend: {0}")]
    Unsupported(String),
}

impl From<MemoryError> for KaineticError {
    fn from(e: MemoryError) -> Self {
        KaineticError::Memory(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_found_display() {
        assert_eq!(MemoryError::NotFound.to_string(), "key not found");
    }

    #[test]
    fn backend_display() {
        let e = MemoryError::Backend("connection refused".into());
        assert!(e.to_string().contains("connection refused"));
    }

    #[test]
    fn unsupported_display() {
        let e = MemoryError::Unsupported("semantic search".into());
        assert!(e.to_string().contains("semantic search"));
    }

    #[test]
    fn converts_to_kainetic_error() {
        let k: KaineticError = MemoryError::NotFound.into();
        assert!(matches!(k, KaineticError::Memory(_)));
    }
}
