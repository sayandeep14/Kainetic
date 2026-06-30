//! Core memory types: [`MemoryKey`], [`MemoryEntry`], and [`SemanticQuery`].

use std::collections::HashMap;
use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A namespaced key that uniquely identifies a memory entry.
///
/// The `namespace` groups related entries (e.g. `"working/run-123"`,
/// `"episodic/session-abc"`). The `key` is the entry name within that
/// namespace.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MemoryKey {
    /// Logical grouping (e.g. `"episodic"`, `"working/run-123"`).
    pub namespace: String,
    /// Entry name within the namespace.
    pub key: String,
}

impl MemoryKey {
    /// Creates a new key from a namespace and a key string.
    #[must_use]
    pub fn new(namespace: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            key: key.into(),
        }
    }
}

impl fmt::Display for MemoryKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.namespace, self.key)
    }
}

/// A stored memory entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// The primary content of this entry (plain text or JSON string).
    pub content: String,
    /// Arbitrary structured metadata attached to the entry.
    pub metadata: HashMap<String, serde_json::Value>,
    /// Pre-computed embedding vector, required for semantic search backends.
    pub embedding: Option<Vec<f32>>,
    /// Wall-clock time at which this entry was created.
    pub created_at: DateTime<Utc>,
}

impl MemoryEntry {
    /// Creates a minimal entry with only a content string.
    ///
    /// `metadata` is empty and `embedding` is `None`.
    #[must_use]
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            metadata: HashMap::new(),
            embedding: None,
            created_at: Utc::now(),
        }
    }

    /// Returns a builder that allows setting optional fields.
    #[must_use]
    pub fn builder(content: impl Into<String>) -> MemoryEntryBuilder {
        MemoryEntryBuilder::new(content)
    }
}

/// Builder for [`MemoryEntry`].
pub struct MemoryEntryBuilder {
    content: String,
    metadata: HashMap<String, serde_json::Value>,
    embedding: Option<Vec<f32>>,
    created_at: DateTime<Utc>,
}

impl MemoryEntryBuilder {
    fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            metadata: HashMap::new(),
            embedding: None,
            created_at: Utc::now(),
        }
    }

    /// Adds a metadata key-value pair.
    #[must_use]
    pub fn metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    /// Attaches a pre-computed embedding vector.
    #[must_use]
    pub fn embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }

    /// Overrides the creation timestamp.
    #[must_use]
    pub fn created_at(mut self, ts: DateTime<Utc>) -> Self {
        self.created_at = ts;
        self
    }

    /// Builds the [`MemoryEntry`].
    #[must_use]
    pub fn build(self) -> MemoryEntry {
        MemoryEntry {
            content: self.content,
            metadata: self.metadata,
            embedding: self.embedding,
            created_at: self.created_at,
        }
    }
}

/// A semantic search query against a vector-indexed backend.
///
/// Either `embedding` (preferred) or `text` (for backends that generate
/// embeddings internally) should be populated before calling
/// [`MemoryBackend::search`].
///
/// [`MemoryBackend::search`]: crate::MemoryBackend::search
#[derive(Debug, Clone)]
pub struct SemanticQuery {
    /// Raw query text (used as a fallback label for logging).
    pub text: String,
    /// Maximum number of results to return.
    pub top_k: u32,
    /// Minimum similarity score (0.0–1.0) for a result to be included.
    pub threshold: f32,
    /// Pre-computed query embedding.
    ///
    /// Must be supplied when calling backends that do not generate embeddings
    /// themselves (e.g. `UsearchBackend`).
    pub embedding: Option<Vec<f32>>,
}

impl SemanticQuery {
    /// Creates a query with the given text, `top_k = 10`, and `threshold = 0.0`.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            top_k: 10,
            threshold: 0.0,
            embedding: None,
        }
    }

    /// Sets the maximum number of results.
    #[must_use]
    pub fn top_k(mut self, k: u32) -> Self {
        self.top_k = k;
        self
    }

    /// Sets the minimum similarity threshold.
    #[must_use]
    pub fn threshold(mut self, t: f32) -> Self {
        self.threshold = t;
        self
    }

    /// Attaches a pre-computed query embedding.
    #[must_use]
    pub fn embedding(mut self, emb: Vec<f32>) -> Self {
        self.embedding = Some(emb);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_key_display() {
        let k = MemoryKey::new("episodic", "sess-1/history");
        assert_eq!(k.to_string(), "episodic/sess-1/history");
    }

    #[test]
    fn memory_entry_new() {
        let e = MemoryEntry::new("hello");
        assert_eq!(e.content, "hello");
        assert!(e.embedding.is_none());
        assert!(e.metadata.is_empty());
    }

    #[test]
    fn memory_entry_builder() {
        let e = MemoryEntry::builder("content")
            .metadata("source", serde_json::json!("test"))
            .embedding(vec![0.1, 0.2])
            .build();
        assert_eq!(e.content, "content");
        assert!(e.embedding.is_some());
        assert_eq!(e.metadata["source"], serde_json::json!("test"));
    }

    #[test]
    fn semantic_query_builder() {
        let q = SemanticQuery::new("search").top_k(5).threshold(0.7);
        assert_eq!(q.top_k, 5);
        assert!((q.threshold - 0.7).abs() < f32::EPSILON);
    }
}
