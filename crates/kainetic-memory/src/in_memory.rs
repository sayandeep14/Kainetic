//! `InMemoryBackend` — ephemeral, `DashMap`-based memory store.

use async_trait::async_trait;
use dashmap::DashMap;

use crate::{backend::MemoryBackend, MemoryEntry, MemoryError, MemoryKey, SemanticQuery};

/// An ephemeral, in-process memory backend backed by a [`DashMap`].
///
/// All data is lost when the backend is dropped. Suitable for tests and
/// single-process agents that don't need cross-run persistence.
///
/// [`search`] is not supported — use `UsearchBackend` (feature `usearch`) for semantic search.
///
/// [`search`]: InMemoryBackend::search
#[derive(Default)]
pub struct InMemoryBackend {
    store: DashMap<String, MemoryEntry>,
}

impl InMemoryBackend {
    /// Creates a new, empty in-memory backend.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of entries currently stored.
    #[must_use]
    pub fn len(&self) -> usize {
        self.store.len()
    }

    /// Returns `true` if the backend contains no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.store.is_empty()
    }
}

#[async_trait]
impl MemoryBackend for InMemoryBackend {
    async fn read(&self, key: &MemoryKey) -> Result<Option<MemoryEntry>, MemoryError> {
        Ok(self.store.get(&key.to_string()).map(|e| e.clone()))
    }

    async fn write(&self, key: MemoryKey, entry: MemoryEntry) -> Result<(), MemoryError> {
        self.store.insert(key.to_string(), entry);
        Ok(())
    }

    async fn search(&self, _query: &SemanticQuery) -> Result<Vec<MemoryEntry>, MemoryError> {
        Err(MemoryError::Unsupported(
            "InMemoryBackend does not support semantic search; use UsearchBackend".into(),
        ))
    }

    async fn delete(&self, key: &MemoryKey) -> Result<(), MemoryError> {
        self.store.remove(&key.to_string());
        Ok(())
    }

    async fn flush(&self) -> Result<(), MemoryError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemoryEntry;

    fn key(ns: &str, k: &str) -> MemoryKey {
        MemoryKey::new(ns, k)
    }

    #[tokio::test]
    async fn write_and_read_round_trips() {
        let backend = InMemoryBackend::new();
        let k = key("test", "foo");
        backend
            .write(k.clone(), MemoryEntry::new("hello"))
            .await
            .unwrap();
        let entry = backend.read(&k).await.unwrap();
        assert_eq!(entry.unwrap().content, "hello");
    }

    #[tokio::test]
    async fn read_missing_key_returns_none() {
        let backend = InMemoryBackend::new();
        assert!(backend
            .read(&key("test", "missing"))
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn write_overwrites_existing_entry() {
        let backend = InMemoryBackend::new();
        let k = key("ns", "k");
        backend
            .write(k.clone(), MemoryEntry::new("v1"))
            .await
            .unwrap();
        backend
            .write(k.clone(), MemoryEntry::new("v2"))
            .await
            .unwrap();
        assert_eq!(backend.read(&k).await.unwrap().unwrap().content, "v2");
    }

    #[tokio::test]
    async fn delete_removes_entry() {
        let backend = InMemoryBackend::new();
        let k = key("ns", "del");
        backend
            .write(k.clone(), MemoryEntry::new("bye"))
            .await
            .unwrap();
        backend.delete(&k).await.unwrap();
        assert!(backend.read(&k).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn delete_nonexistent_key_is_ok() {
        let backend = InMemoryBackend::new();
        backend.delete(&key("ns", "ghost")).await.unwrap();
    }

    #[tokio::test]
    async fn search_returns_unsupported() {
        let backend = InMemoryBackend::new();
        let q = SemanticQuery::new("test");
        let err = backend.search(&q).await.unwrap_err();
        assert!(matches!(err, MemoryError::Unsupported(_)));
    }

    #[tokio::test]
    async fn flush_is_noop() {
        InMemoryBackend::new().flush().await.unwrap();
    }

    #[test]
    fn len_and_is_empty() {
        let b = InMemoryBackend::new();
        assert!(b.is_empty());
        b.store.insert("k".into(), MemoryEntry::new("v"));
        assert_eq!(b.len(), 1);
        assert!(!b.is_empty());
    }
}
