//! Qdrant vector database memory backend.
//!
//! Uses [`qdrant-client`] to store and retrieve [`MemoryEntry`] values in a
//! named Qdrant collection.  Semantic search delegates to Qdrant's native
//! approximate nearest-neighbour index.
//!
//! # Feature
//!
//! Enable with `features = ["qdrant"]` in `Cargo.toml`.
//!
//! # Collection schema
//!
//! Each Qdrant point stores:
//! - vector: the entry's embedding
//! - payload: `{ namespace, key, content, metadata_json, created_at_ms }`
//!
//! The point ID is a `u64` computed as `fnv1a(namespace + "/" + key)`.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use qdrant_client::{
    qdrant::{
        CreateCollectionBuilder, DeletePointsBuilder, Distance, GetPointsBuilder, PointStruct,
        SearchPointsBuilder, UpsertPointsBuilder, VectorParamsBuilder,
    },
    Qdrant,
};
use serde_json::Value;
use tracing::{debug, instrument};

use crate::{MemoryBackend, MemoryEntry, MemoryError, MemoryKey, SemanticQuery};

/// Qdrant vector-database memory backend.
///
/// Stores [`MemoryEntry`] values as Qdrant points in a named collection.
/// Requires the Qdrant service to be reachable at `url`.
pub struct QdrantBackend {
    client: Qdrant,
    collection: String,
    vector_dim: u64,
}

impl QdrantBackend {
    /// Connects to Qdrant at `url` and ensures `collection` exists with the
    /// given `vector_dim` using cosine distance.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError::Backend`] if the connection or collection
    /// creation fails.
    pub async fn new(
        url: impl Into<String>,
        collection: impl Into<String>,
        vector_dim: u64,
    ) -> Result<Self, MemoryError> {
        let client = Qdrant::from_url(&url.into())
            .build()
            .map_err(|e| MemoryError::Backend(e.to_string()))?;

        let collection = collection.into();

        // Create collection if it doesn't exist yet.
        let collections = client
            .list_collections()
            .await
            .map_err(|e| MemoryError::Backend(e.to_string()))?;

        let exists = collections.collections.iter().any(|c| c.name == collection);

        if !exists {
            client
                .create_collection(
                    CreateCollectionBuilder::new(&collection)
                        .vectors_config(VectorParamsBuilder::new(vector_dim, Distance::Cosine)),
                )
                .await
                .map_err(|e| MemoryError::Backend(e.to_string()))?;
        }

        Ok(Self {
            client,
            collection,
            vector_dim,
        })
    }

    /// Returns a `u64` ID for `(namespace, key)` using FNV-1a hashing.
    fn point_id(key: &MemoryKey) -> u64 {
        fnv1a(format!("{}/{}", key.namespace, key.key).as_bytes())
    }
}

/// Simple FNV-1a 64-bit hash.
fn fnv1a(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

// ─── Payload helpers ───────────────────────────────────────────────────────────

fn entry_to_payload(
    key: &MemoryKey,
    entry: &MemoryEntry,
) -> HashMap<String, qdrant_client::qdrant::Value> {
    use qdrant_client::qdrant::value::Kind;
    use qdrant_client::qdrant::Value as QVal;

    let metadata_json = serde_json::to_string(&entry.metadata).unwrap_or_else(|_| "{}".to_string());

    let mut map = HashMap::new();
    map.insert(
        "namespace".to_string(),
        QVal {
            kind: Some(Kind::StringValue(key.namespace.clone())),
        },
    );
    map.insert(
        "key".to_string(),
        QVal {
            kind: Some(Kind::StringValue(key.key.clone())),
        },
    );
    map.insert(
        "content".to_string(),
        QVal {
            kind: Some(Kind::StringValue(entry.content.clone())),
        },
    );
    map.insert(
        "metadata_json".to_string(),
        QVal {
            kind: Some(Kind::StringValue(metadata_json)),
        },
    );
    map.insert(
        "created_at_ms".to_string(),
        QVal {
            kind: Some(Kind::IntegerValue(entry.created_at.timestamp_millis())),
        },
    );
    map
}

fn payload_to_entry(
    payload: &HashMap<String, qdrant_client::qdrant::Value>,
    embedding: Option<Vec<f32>>,
) -> Result<(MemoryKey, MemoryEntry), MemoryError> {
    use qdrant_client::qdrant::value::Kind;

    let get_str = |field: &str| -> Result<String, MemoryError> {
        payload
            .get(field)
            .and_then(|v| {
                if let Some(Kind::StringValue(s)) = &v.kind {
                    Some(s.clone())
                } else {
                    None
                }
            })
            .ok_or_else(|| MemoryError::Serialization(format!("missing field '{field}'")))
    };

    let namespace = get_str("namespace")?;
    let key = get_str("key")?;
    let content = get_str("content")?;
    let metadata_json = get_str("metadata_json")?;
    let created_at_ms = payload
        .get("created_at_ms")
        .and_then(|v| {
            if let Some(Kind::IntegerValue(n)) = &v.kind {
                Some(*n)
            } else {
                None
            }
        })
        .unwrap_or(0);

    let metadata: HashMap<String, Value> = serde_json::from_str(&metadata_json).unwrap_or_default();

    let created_at = Utc
        .timestamp_millis_opt(created_at_ms)
        .single()
        .unwrap_or_else(Utc::now);

    Ok((
        MemoryKey::new(namespace, key),
        MemoryEntry {
            content,
            metadata,
            embedding,
            created_at,
        },
    ))
}

// ─── MemoryBackend impl ────────────────────────────────────────────────────────

#[async_trait]
impl MemoryBackend for QdrantBackend {
    #[instrument(skip(self))]
    async fn read(&self, key: &MemoryKey) -> Result<Option<MemoryEntry>, MemoryError> {
        debug!(key = %key, "qdrant: read");
        let id = Self::point_id(key);

        let response = self
            .client
            .get_points(
                GetPointsBuilder::new(&self.collection, vec![id.into()])
                    .with_payload(true)
                    .with_vectors(true),
            )
            .await
            .map_err(|e| MemoryError::Backend(e.to_string()))?;

        let Some(point) = response.result.into_iter().next() else {
            return Ok(None);
        };

        #[allow(deprecated)]
        let embedding = point.vectors.and_then(|v| {
            if let Some(qdrant_client::qdrant::vectors_output::VectorsOptions::Vector(vec)) =
                v.vectors_options
            {
                Some(vec.data)
            } else {
                None
            }
        });

        let (_, entry) = payload_to_entry(&point.payload, embedding)?;
        Ok(Some(entry))
    }

    #[instrument(skip(self, entry))]
    async fn write(&self, key: MemoryKey, entry: MemoryEntry) -> Result<(), MemoryError> {
        debug!(key = %key, "qdrant: write");

        let Some(ref emb) = entry.embedding else {
            return Err(MemoryError::Unsupported(
                "QdrantBackend: MemoryEntry.embedding must be set".into(),
            ));
        };

        if emb.len() as u64 != self.vector_dim {
            return Err(MemoryError::Backend(format!(
                "embedding length {} does not match collection dimension {}",
                emb.len(),
                self.vector_dim
            )));
        }

        let id = Self::point_id(&key);
        let payload = entry_to_payload(&key, &entry);
        let vector = emb.clone();

        let point = PointStruct::new(id, vector, payload);

        self.client
            .upsert_points(UpsertPointsBuilder::new(&self.collection, vec![point]))
            .await
            .map_err(|e| MemoryError::Backend(e.to_string()))?;

        Ok(())
    }

    #[instrument(skip(self, query))]
    async fn search(&self, query: &SemanticQuery) -> Result<Vec<MemoryEntry>, MemoryError> {
        let Some(ref query_emb) = query.embedding else {
            return Err(MemoryError::Unsupported(
                "QdrantBackend: SemanticQuery.embedding must be provided".into(),
            ));
        };

        debug!(text = %query.text, top_k = query.top_k, "qdrant: search");

        let response = self
            .client
            .search_points(
                SearchPointsBuilder::new(
                    &self.collection,
                    query_emb.clone(),
                    u64::from(query.top_k),
                )
                .with_payload(true)
                .with_vectors(true)
                .score_threshold(query.threshold),
            )
            .await
            .map_err(|e| MemoryError::Backend(e.to_string()))?;

        let mut entries = Vec::new();
        for scored in response.result {
            #[allow(deprecated)]
            let embedding = scored.vectors.and_then(|v| {
                if let Some(qdrant_client::qdrant::vectors_output::VectorsOptions::Vector(vec)) =
                    v.vectors_options
                {
                    Some(vec.data)
                } else {
                    None
                }
            });
            if let Ok((_, entry)) = payload_to_entry(&scored.payload, embedding) {
                entries.push(entry);
            }
        }

        Ok(entries)
    }

    #[instrument(skip(self))]
    async fn delete(&self, key: &MemoryKey) -> Result<(), MemoryError> {
        debug!(key = %key, "qdrant: delete");
        let id = Self::point_id(key);

        self.client
            .delete_points(
                DeletePointsBuilder::new(&self.collection)
                    .points(vec![qdrant_client::qdrant::PointId::from(id)]),
            )
            .await
            .map_err(|e| MemoryError::Backend(e.to_string()))?;

        Ok(())
    }

    async fn flush(&self) -> Result<(), MemoryError> {
        Ok(())
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_stable() {
        let id1 = fnv1a(b"episodic/entry-1");
        let id2 = fnv1a(b"episodic/entry-1");
        assert_eq!(id1, id2);
        assert_ne!(id1, fnv1a(b"episodic/entry-2"));
    }

    #[test]
    fn point_id_deterministic() {
        let key = MemoryKey::new("ns", "key");
        assert_eq!(QdrantBackend::point_id(&key), QdrantBackend::point_id(&key));
    }

    #[cfg(feature = "integration")]
    #[tokio::test]
    async fn integration_write_read_search_delete() {
        let url =
            std::env::var("QDRANT_URL").unwrap_or_else(|_| "http://localhost:6334".to_string());
        let backend = match QdrantBackend::new(&url, "kainetic_test", 3).await {
            Ok(b) => b,
            Err(e) => {
                eprintln!("qdrant: could not connect to {url} ({e}) — skipping");
                return;
            }
        };

        let key = MemoryKey::new("test", "q-entry-1");
        let entry = MemoryEntry::builder("hello qdrant")
            .embedding(vec![1.0f32, 0.0, 0.0])
            .build();

        backend.write(key.clone(), entry).await.unwrap();

        let read = backend.read(&key).await.unwrap().unwrap();
        assert_eq!(read.content, "hello qdrant");

        let results = backend
            .search(
                &SemanticQuery::new("test")
                    .embedding(vec![1.0f32, 0.0, 0.0])
                    .top_k(1),
            )
            .await
            .unwrap();
        assert_eq!(results.len(), 1);

        backend.delete(&key).await.unwrap();
        assert!(backend.read(&key).await.unwrap().is_none());
    }
}
