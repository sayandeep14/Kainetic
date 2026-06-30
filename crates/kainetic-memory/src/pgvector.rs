//! PostgreSQL-backed memory with embedding similarity search.
//!
//! Uses `sqlx` against a PostgreSQL database.  Embeddings are stored as JSON
//! arrays and cosine similarity is computed in the application layer, making
//! this backend compatible with any PostgreSQL instance (no `pgvector`
//! extension required).
//!
//! # Table
//!
//! The backend creates a `kainetic_memory` table automatically on first
//! [`PgVectorBackend::new`] call.
//!
//! # Feature
//!
//! Enable with `features = ["pgvector"]` in `Cargo.toml`.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{postgres::PgPool, Row as _};
use tracing::{debug, instrument};

use crate::{MemoryBackend, MemoryEntry, MemoryError, MemoryKey, SemanticQuery};

/// PostgreSQL memory backend.
///
/// Stores [`MemoryEntry`] values in a `kainetic_memory` table.  Semantic
/// search iterates the full namespace and ranks entries by cosine similarity
/// in Rust.
///
/// # Limitations
///
/// - Cosine similarity is computed application-side; large namespaces will
///   be slow.  For production workloads, install the `pgvector` extension and
///   migrate the `embedding` column to `vector(N)`.
pub struct PgVectorBackend {
    pool: PgPool,
}

impl PgVectorBackend {
    /// Connects to the PostgreSQL database at `database_url` and ensures the
    /// `kainetic_memory` table exists.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError::Backend`] if the connection or table creation
    /// fails.
    pub async fn new(database_url: &str) -> Result<Self, MemoryError> {
        let pool = PgPool::connect(database_url)
            .await
            .map_err(|e| MemoryError::Backend(e.to_string()))?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS kainetic_memory (
                namespace   TEXT        NOT NULL,
                key         TEXT        NOT NULL,
                content     TEXT        NOT NULL,
                metadata    JSONB       NOT NULL DEFAULT '{}',
                embedding   JSONB,
                created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                PRIMARY KEY (namespace, key)
            )
            "#,
        )
        .execute(&pool)
        .await
        .map_err(|e| MemoryError::Backend(e.to_string()))?;

        Ok(Self { pool })
    }
}

// ─── Row deserialisation ───────────────────────────────────────────────────────

struct Row {
    namespace: String,
    key: String,
    content: String,
    metadata: Value,
    embedding: Option<Value>,
    created_at: DateTime<Utc>,
}

fn row_to_entry(row: Row) -> Result<(MemoryKey, MemoryEntry), MemoryError> {
    let key = MemoryKey::new(row.namespace, row.key);

    let metadata: HashMap<String, Value> = match row.metadata {
        Value::Object(m) => m.into_iter().collect(),
        _ => HashMap::new(),
    };

    let embedding = row
        .embedding
        .and_then(|v| serde_json::from_value::<Vec<f32>>(v).ok());

    let entry = MemoryEntry {
        content: row.content,
        metadata,
        embedding,
        created_at: row.created_at,
    };

    Ok((key, entry))
}

// ─── MemoryBackend impl ────────────────────────────────────────────────────────

#[async_trait]
impl MemoryBackend for PgVectorBackend {
    #[instrument(skip(self))]
    async fn read(&self, key: &MemoryKey) -> Result<Option<MemoryEntry>, MemoryError> {
        debug!(key = %key, "pgvector: read");

        let row = sqlx::query(
            "SELECT namespace, key, content, metadata, embedding, created_at \
             FROM kainetic_memory WHERE namespace = $1 AND key = $2",
        )
        .bind(&key.namespace)
        .bind(&key.key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| MemoryError::Backend(e.to_string()))?;

        let Some(r) = row else { return Ok(None) };

        let de = |e: sqlx::Error| MemoryError::Backend(e.to_string());
        let (_, entry) = row_to_entry(Row {
            namespace: r.try_get("namespace").map_err(de)?,
            key: r.try_get("key").map_err(de)?,
            content: r.try_get("content").map_err(de)?,
            metadata: r.try_get("metadata").map_err(de)?,
            embedding: r.try_get("embedding").map_err(de)?,
            created_at: r.try_get("created_at").map_err(de)?,
        })?;

        Ok(Some(entry))
    }

    #[instrument(skip(self, entry))]
    async fn write(&self, key: MemoryKey, entry: MemoryEntry) -> Result<(), MemoryError> {
        debug!(key = %key, "pgvector: write");

        let metadata = serde_json::to_value(&entry.metadata)
            .map_err(|e| MemoryError::Serialization(e.to_string()))?;
        let embedding = entry
            .embedding
            .map(|e| serde_json::to_value(&e))
            .transpose()
            .map_err(|e| MemoryError::Serialization(e.to_string()))?;

        sqlx::query(
            "INSERT INTO kainetic_memory \
                (namespace, key, content, metadata, embedding, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (namespace, key) DO UPDATE SET \
                content    = EXCLUDED.content, \
                metadata   = EXCLUDED.metadata, \
                embedding  = EXCLUDED.embedding, \
                created_at = EXCLUDED.created_at",
        )
        .bind(&key.namespace)
        .bind(&key.key)
        .bind(&entry.content)
        .bind(&metadata)
        .bind(&embedding)
        .bind(entry.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| MemoryError::Backend(e.to_string()))?;

        Ok(())
    }

    #[instrument(skip(self, query))]
    async fn search(&self, query: &SemanticQuery) -> Result<Vec<MemoryEntry>, MemoryError> {
        let Some(ref query_emb) = query.embedding else {
            return Err(MemoryError::Unsupported(
                "PgVectorBackend: SemanticQuery.embedding must be provided".into(),
            ));
        };

        debug!(text = %query.text, top_k = query.top_k, "pgvector: search");

        let rows = sqlx::query(
            "SELECT namespace, key, content, metadata, embedding, created_at \
             FROM kainetic_memory WHERE embedding IS NOT NULL",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| MemoryError::Backend(e.to_string()))?;

        let mut scored: Vec<(f32, MemoryEntry)> = rows
            .into_iter()
            .filter_map(|r| {
                let (_, entry) = row_to_entry(Row {
                    namespace: r.try_get("namespace").ok()?,
                    key: r.try_get("key").ok()?,
                    content: r.try_get("content").ok()?,
                    metadata: r.try_get("metadata").ok()?,
                    embedding: r.try_get("embedding").ok()?,
                    created_at: r.try_get("created_at").ok()?,
                })
                .ok()?;
                let emb = entry.embedding.as_ref()?;
                let score = cosine_similarity(query_emb, emb);
                Some((score, entry))
            })
            .filter(|(score, _)| *score >= query.threshold)
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(query.top_k as usize);

        Ok(scored.into_iter().map(|(_, e)| e).collect())
    }

    #[instrument(skip(self))]
    async fn delete(&self, key: &MemoryKey) -> Result<(), MemoryError> {
        debug!(key = %key, "pgvector: delete");

        sqlx::query("DELETE FROM kainetic_memory WHERE namespace = $1 AND key = $2")
            .bind(&key.namespace)
            .bind(&key.key)
            .execute(&self.pool)
            .await
            .map_err(|e| MemoryError::Backend(e.to_string()))?;

        Ok(())
    }

    async fn flush(&self) -> Result<(), MemoryError> {
        Ok(())
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_similarity_identical() {
        let v = vec![1.0f32, 0.0, 0.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_orthogonal() {
        let a = vec![1.0f32, 0.0];
        let b = vec![0.0f32, 1.0];
        assert!((cosine_similarity(&a, &b)).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_empty() {
        assert_eq!(cosine_similarity(&[], &[]), 0.0);
    }

    #[cfg(feature = "integration")]
    #[tokio::test]
    async fn integration_write_read_search() {
        let url = match std::env::var("POSTGRES_URL") {
            Ok(u) => u,
            Err(_) => {
                eprintln!("POSTGRES_URL not set — skipping pgvector integration test");
                return;
            }
        };
        let backend = match PgVectorBackend::new(&url).await {
            Ok(b) => b,
            Err(e) => {
                eprintln!("pgvector: could not connect ({e}) — skipping");
                return;
            }
        };

        let key = MemoryKey::new("test", "entry-1");
        let entry = MemoryEntry::builder("hello postgres")
            .embedding(vec![1.0, 0.0, 0.0])
            .build();

        backend.write(key.clone(), entry).await.unwrap();

        let read = backend.read(&key).await.unwrap().unwrap();
        assert_eq!(read.content, "hello postgres");

        let results = backend
            .search(
                &SemanticQuery::new("test")
                    .embedding(vec![1.0, 0.0, 0.0])
                    .top_k(1),
            )
            .await
            .unwrap();
        assert_eq!(results.len(), 1);

        backend.delete(&key).await.unwrap();
        assert!(backend.read(&key).await.unwrap().is_none());
    }
}
