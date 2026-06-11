//! The `MemoryBackend` trait — the common interface for all memory stores.

use async_trait::async_trait;

use crate::{MemoryEntry, MemoryError, MemoryKey, SemanticQuery};

/// The common interface implemented by all Kainetic memory backends.
///
/// Backends are keyed by [`MemoryKey`] (namespace + key string) and store
/// [`MemoryEntry`] values. [`search`] provides semantic similarity search
/// and is only meaningful for vector-indexed backends such as
/// [`UsearchBackend`]; non-vector backends return
/// [`MemoryError::Unsupported`].
///
/// All methods take `&self`, meaning implementations must use interior
/// mutability (e.g. `DashMap`, `Mutex`) for any mutable state.
///
/// [`search`]: MemoryBackend::search
/// [`UsearchBackend`]: crate::UsearchBackend
#[async_trait]
pub trait MemoryBackend: Send + Sync + 'static {
    /// Retrieves the entry stored under `key`, or `None` if absent.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError::Backend`] on I/O or network failure.
    async fn read(&self, key: &MemoryKey) -> Result<Option<MemoryEntry>, MemoryError>;

    /// Inserts or replaces the entry at `key`.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError::Backend`] on I/O or network failure, or
    /// [`MemoryError::Serialization`] if the entry cannot be serialized.
    async fn write(&self, key: MemoryKey, entry: MemoryEntry) -> Result<(), MemoryError>;

    /// Finds entries whose stored embeddings are semantically similar to `query`.
    ///
    /// Results are sorted by descending similarity and capped at
    /// [`SemanticQuery::top_k`]. Entries with similarity below
    /// [`SemanticQuery::threshold`] are excluded.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError::Unsupported`] for backends without a vector
    /// index. Returns [`MemoryError::Backend`] on I/O failure.
    async fn search(&self, query: &SemanticQuery) -> Result<Vec<MemoryEntry>, MemoryError>;

    /// Deletes the entry at `key`.
    ///
    /// Is a no-op (returns `Ok(())`) when the key does not exist.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError::Backend`] on I/O or network failure.
    async fn delete(&self, key: &MemoryKey) -> Result<(), MemoryError>;

    /// Persists any buffered writes and releases transient resources.
    ///
    /// For in-memory backends this is a no-op.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError::Backend`] on I/O failure.
    async fn flush(&self) -> Result<(), MemoryError>;
}
