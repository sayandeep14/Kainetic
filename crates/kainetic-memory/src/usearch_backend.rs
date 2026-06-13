//! `UsearchBackend` — in-process vector search backed by `usearch`.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use dashmap::DashMap;
use usearch::{Index, IndexOptions, MetricKind, ScalarKind};

use crate::{backend::MemoryBackend, MemoryEntry, MemoryError, MemoryKey, SemanticQuery};

/// In-process approximate nearest-neighbour memory backend.
///
/// Embeddings are indexed using [`usearch`] (hierarchical navigable small
/// world graphs, HNSW). The text content and metadata are stored in a
/// companion `DashMap` keyed by the same `u64` label as the vector index.
///
/// # Requirements
///
/// - Every [`MemoryEntry`] written to this backend **must** have its
///   `embedding` field populated; entries without an embedding are stored
///   but not indexed and will never appear in search results.
/// - The embedding dimension must be consistent with the value passed to
///   [`UsearchBackend::new`].
///
/// [`usearch`]: https://crates.io/crates/usearch
pub struct UsearchBackend {
    index: Arc<Mutex<Index>>,
    /// label → entry content
    store: Arc<DashMap<u64, MemoryEntry>>,
    /// full key string → usearch label
    label_map: Arc<DashMap<String, u64>>,
    next_label: Arc<Mutex<u64>>,
    dimensions: usize,
}

impl UsearchBackend {
    /// Creates a new backend with the given embedding `dimensions`.
    ///
    /// Uses cosine similarity and 32-bit float quantisation.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError::Backend`] if the underlying index cannot be
    /// created.
    pub fn new(dimensions: usize) -> Result<Self, MemoryError> {
        let options = IndexOptions {
            dimensions,
            metric: MetricKind::Cos,
            quantization: ScalarKind::F32,
            connectivity: 0,
            expansion_add: 0,
            expansion_search: 0,
            multi: false,
        };
        let index = Index::new(&options).map_err(|e| MemoryError::Backend(e.to_string()))?;
        index.reserve(1024).map_err(|e| MemoryError::Backend(e.to_string()))?;
        Ok(Self {
            index: Arc::new(Mutex::new(index)),
            store: Arc::new(DashMap::new()),
            label_map: Arc::new(DashMap::new()),
            next_label: Arc::new(Mutex::new(0)),
            dimensions,
        })
    }

    /// Returns the embedding dimensions this backend was created with.
    #[must_use]
    pub fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn alloc_label(&self) -> Result<u64, MemoryError> {
        let mut guard = self
            .next_label
            .lock()
            .map_err(|_| MemoryError::Backend("label counter mutex was poisoned".into()))?;
        let label = *guard;
        *guard += 1;
        Ok(label)
    }
}

#[async_trait]
impl MemoryBackend for UsearchBackend {
    async fn read(&self, key: &MemoryKey) -> Result<Option<MemoryEntry>, MemoryError> {
        let key_str = key.to_string();
        let label = match self.label_map.get(&key_str) {
            Some(l) => *l,
            None => return Ok(None),
        };
        Ok(self.store.get(&label).map(|e| e.clone()))
    }

    async fn write(&self, key: MemoryKey, entry: MemoryEntry) -> Result<(), MemoryError> {
        let key_str = key.to_string();

        let label = if let Some(existing) = self.label_map.get(&key_str) {
            *existing
        } else {
            let new_label = self.alloc_label()?;
            self.label_map.insert(key_str.clone(), new_label);
            new_label
        };

        // Index the embedding if provided.
        if let Some(ref emb) = entry.embedding {
            if emb.len() != self.dimensions {
                return Err(MemoryError::Backend(format!(
                    "embedding dimension mismatch: expected {}, got {}",
                    self.dimensions,
                    emb.len()
                )));
            }
            let index = self
                .index
                .lock()
                .map_err(|_| MemoryError::Backend("usearch index mutex was poisoned".into()))?;
            // Remove existing vector at this label if present, then re-add.
            let _ = index.remove(label); // ignore "not found" error
            index
                .add(label, emb)
                .map_err(|e| MemoryError::Backend(e.to_string()))?;
        }

        self.store.insert(label, entry);
        Ok(())
    }

    async fn search(&self, query: &SemanticQuery) -> Result<Vec<MemoryEntry>, MemoryError> {
        let emb = query.embedding.as_ref().ok_or_else(|| {
            MemoryError::Unsupported(
                "UsearchBackend requires a pre-computed embedding in SemanticQuery::embedding"
                    .into(),
            )
        })?;

        if emb.len() != self.dimensions {
            return Err(MemoryError::Backend(format!(
                "query embedding dimension mismatch: expected {}, got {}",
                self.dimensions,
                emb.len()
            )));
        }

        let (keys, distances) = {
            let index = self
                .index
                .lock()
                .map_err(|_| MemoryError::Backend("usearch index mutex was poisoned".into()))?;
            let results = index
                .search(emb, query.top_k as usize)
                .map_err(|e| MemoryError::Backend(e.to_string()))?;
            (results.keys, results.distances)
        };

        let threshold = query.threshold;
        let entries = keys
            .into_iter()
            .zip(distances)
            .filter_map(|(label, dist)| {
                // usearch returns distances; for cosine similarity distance = 1 - similarity
                let similarity = 1.0 - dist;
                if similarity < threshold {
                    return None;
                }
                self.store.get(&label).map(|e| e.clone())
            })
            .collect();

        Ok(entries)
    }

    async fn delete(&self, key: &MemoryKey) -> Result<(), MemoryError> {
        let key_str = key.to_string();
        if let Some((_, label)) = self.label_map.remove(&key_str) {
            let index = self
                .index
                .lock()
                .map_err(|_| MemoryError::Backend("usearch index mutex was poisoned".into()))?;
            let _ = index.remove(label);
            self.store.remove(&label);
        }
        Ok(())
    }

    async fn flush(&self) -> Result<(), MemoryError> {
        Ok(())
    }
}

// Manually implement Clone by forwarding through the Arcs.
impl Clone for UsearchBackend {
    fn clone(&self) -> Self {
        Self {
            index: Arc::clone(&self.index),
            store: Arc::clone(&self.store),
            label_map: Arc::clone(&self.label_map),
            next_label: Arc::clone(&self.next_label),
            dimensions: self.dimensions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemoryEntry;

    fn backend() -> UsearchBackend {
        UsearchBackend::new(4).unwrap()
    }

    fn entry_with_emb(content: &str, emb: Vec<f32>) -> MemoryEntry {
        MemoryEntry::builder(content).embedding(emb).build()
    }

    #[tokio::test]
    async fn write_and_read_round_trips() {
        let b = backend();
        let k = MemoryKey::new("test", "a");
        b.write(k.clone(), entry_with_emb("hello", vec![1.0, 0.0, 0.0, 0.0]))
            .await
            .unwrap();
        assert_eq!(b.read(&k).await.unwrap().unwrap().content, "hello");
    }

    #[tokio::test]
    async fn read_missing_returns_none() {
        let b = backend();
        assert!(b.read(&MemoryKey::new("ns", "missing")).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn delete_removes_entry() {
        let b = backend();
        let k = MemoryKey::new("ns", "del");
        b.write(k.clone(), entry_with_emb("bye", vec![0.0, 1.0, 0.0, 0.0]))
            .await
            .unwrap();
        b.delete(&k).await.unwrap();
        assert!(b.read(&k).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn search_returns_nearest_neighbours() {
        let b = backend();

        // Two entries: one very close to query, one orthogonal.
        b.write(
            MemoryKey::new("ns", "close"),
            entry_with_emb("close", vec![1.0, 0.0, 0.0, 0.0]),
        )
        .await
        .unwrap();
        b.write(
            MemoryKey::new("ns", "far"),
            entry_with_emb("far", vec![0.0, 1.0, 0.0, 0.0]),
        )
        .await
        .unwrap();

        let query = SemanticQuery::new("test")
            .embedding(vec![1.0, 0.0, 0.0, 0.0])
            .top_k(1);

        let results = b.search(&query).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "close");
    }

    #[tokio::test]
    async fn search_without_embedding_returns_unsupported() {
        let b = backend();
        let err = b.search(&SemanticQuery::new("no emb")).await.unwrap_err();
        assert!(matches!(err, MemoryError::Unsupported(_)));
    }

    #[tokio::test]
    async fn write_wrong_dimension_returns_error() {
        let b = backend(); // 4-dimensional
        let k = MemoryKey::new("ns", "dim_err");
        let err = b
            .write(k, entry_with_emb("oops", vec![1.0, 0.0])) // 2-dimensional
            .await
            .unwrap_err();
        assert!(matches!(err, MemoryError::Backend(_)));
    }

}
