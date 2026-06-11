//! `SqliteBackend` — persistent episodic memory backed by SQLite.

use async_trait::async_trait;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use std::path::Path;

use crate::{backend::MemoryBackend, MemoryEntry, MemoryError, MemoryKey, SemanticQuery};

/// SQLite-backed persistent memory store.
///
/// Uses a connection pool (`r2d2` + `r2d2_sqlite`) and runs all blocking
/// SQLite operations on the `tokio` blocking thread pool via
/// `tokio::task::spawn_blocking`.
///
/// # Schema
///
/// The table `kainetic_memory` is created automatically on first use:
///
/// ```sql
/// CREATE TABLE IF NOT EXISTS kainetic_memory (
///     namespace TEXT NOT NULL,
///     key TEXT NOT NULL,
///     content TEXT NOT NULL,
///     metadata TEXT NOT NULL,
///     created_at TEXT NOT NULL,
///     PRIMARY KEY (namespace, key)
/// );
/// ```
///
/// Embeddings are not stored in SQLite; use [`UsearchBackend`] for vector
/// search.
///
/// [`UsearchBackend`]: crate::UsearchBackend
#[derive(Clone)]
pub struct SqliteBackend {
    pool: Pool<SqliteConnectionManager>,
}

impl SqliteBackend {
    /// Opens (or creates) the database at the given file path.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError::Backend`] if the database cannot be opened or
    /// the schema cannot be initialised.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, MemoryError> {
        let manager = SqliteConnectionManager::file(path);
        let pool = Pool::new(manager).map_err(|e| MemoryError::Backend(e.to_string()))?;
        let conn = pool.get().map_err(|e| MemoryError::Backend(e.to_string()))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS kainetic_memory (
                namespace TEXT NOT NULL,
                key       TEXT NOT NULL,
                content   TEXT NOT NULL,
                metadata  TEXT NOT NULL,
                created_at TEXT NOT NULL,
                PRIMARY KEY (namespace, key)
            );",
        )
        .map_err(|e| MemoryError::Backend(e.to_string()))?;
        Ok(Self { pool })
    }

    /// Opens an in-memory SQLite database (useful for tests).
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError::Backend`] if the database cannot be initialised.
    pub fn in_memory() -> Result<Self, MemoryError> {
        let manager = SqliteConnectionManager::memory();
        let pool = Pool::builder()
            .max_size(1) // in-memory DBs can't share across connections
            .build(manager)
            .map_err(|e| MemoryError::Backend(e.to_string()))?;
        let conn = pool.get().map_err(|e| MemoryError::Backend(e.to_string()))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS kainetic_memory (
                namespace TEXT NOT NULL,
                key       TEXT NOT NULL,
                content   TEXT NOT NULL,
                metadata  TEXT NOT NULL,
                created_at TEXT NOT NULL,
                PRIMARY KEY (namespace, key)
            );",
        )
        .map_err(|e| MemoryError::Backend(e.to_string()))?;
        Ok(Self { pool })
    }
}

#[async_trait]
impl MemoryBackend for SqliteBackend {
    async fn read(&self, key: &MemoryKey) -> Result<Option<MemoryEntry>, MemoryError> {
        let pool = self.pool.clone();
        let ns = key.namespace.clone();
        let k = key.key.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| MemoryError::Backend(e.to_string()))?;
            let mut stmt = conn
                .prepare(
                    "SELECT content, metadata, created_at \
                     FROM kainetic_memory WHERE namespace = ?1 AND key = ?2",
                )
                .map_err(|e| MemoryError::Backend(e.to_string()))?;
            let mut rows = stmt
                .query(params![ns, k])
                .map_err(|e| MemoryError::Backend(e.to_string()))?;
            match rows.next().map_err(|e| MemoryError::Backend(e.to_string()))? {
                None => Ok(None),
                Some(row) => {
                    let content: String = row.get(0).map_err(|e| MemoryError::Backend(e.to_string()))?;
                    let metadata_str: String = row.get(1).map_err(|e| MemoryError::Backend(e.to_string()))?;
                    let created_at_str: String = row.get(2).map_err(|e| MemoryError::Backend(e.to_string()))?;
                    let metadata = serde_json::from_str(&metadata_str)
                        .map_err(|e| MemoryError::Serialization(e.to_string()))?;
                    let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .map_err(|e| MemoryError::Serialization(e.to_string()))?;
                    Ok(Some(MemoryEntry {
                        content,
                        metadata,
                        embedding: None, // SQLite backend does not store embeddings
                        created_at,
                    }))
                }
            }
        })
        .await
        .map_err(|e| MemoryError::Backend(e.to_string()))?
    }

    async fn write(&self, key: MemoryKey, entry: MemoryEntry) -> Result<(), MemoryError> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let metadata_str = serde_json::to_string(&entry.metadata)
                .map_err(|e| MemoryError::Serialization(e.to_string()))?;
            let created_at_str = entry.created_at.to_rfc3339();
            let conn = pool.get().map_err(|e| MemoryError::Backend(e.to_string()))?;
            conn.execute(
                "INSERT INTO kainetic_memory (namespace, key, content, metadata, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5) \
                 ON CONFLICT(namespace, key) DO UPDATE SET \
                     content    = excluded.content, \
                     metadata   = excluded.metadata, \
                     created_at = excluded.created_at",
                params![key.namespace, key.key, entry.content, metadata_str, created_at_str],
            )
            .map_err(|e| MemoryError::Backend(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| MemoryError::Backend(e.to_string()))?
    }

    async fn search(&self, _query: &SemanticQuery) -> Result<Vec<MemoryEntry>, MemoryError> {
        Err(MemoryError::Unsupported(
            "SqliteBackend does not support semantic search; use UsearchBackend".into(),
        ))
    }

    async fn delete(&self, key: &MemoryKey) -> Result<(), MemoryError> {
        let pool = self.pool.clone();
        let ns = key.namespace.clone();
        let k = key.key.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| MemoryError::Backend(e.to_string()))?;
            conn.execute(
                "DELETE FROM kainetic_memory WHERE namespace = ?1 AND key = ?2",
                params![ns, k],
            )
            .map_err(|e| MemoryError::Backend(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| MemoryError::Backend(e.to_string()))?
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
        let backend = SqliteBackend::in_memory().unwrap();
        let k = key("test", "greet");
        backend
            .write(k.clone(), MemoryEntry::new("hello sqlite"))
            .await
            .unwrap();
        let entry = backend.read(&k).await.unwrap().unwrap();
        assert_eq!(entry.content, "hello sqlite");
    }

    #[tokio::test]
    async fn read_missing_key_returns_none() {
        let backend = SqliteBackend::in_memory().unwrap();
        assert!(backend.read(&key("x", "missing")).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn write_overwrites_existing() {
        let backend = SqliteBackend::in_memory().unwrap();
        let k = key("ns", "k");
        backend.write(k.clone(), MemoryEntry::new("v1")).await.unwrap();
        backend.write(k.clone(), MemoryEntry::new("v2")).await.unwrap();
        assert_eq!(backend.read(&k).await.unwrap().unwrap().content, "v2");
    }

    #[tokio::test]
    async fn delete_removes_entry() {
        let backend = SqliteBackend::in_memory().unwrap();
        let k = key("ns", "d");
        backend.write(k.clone(), MemoryEntry::new("bye")).await.unwrap();
        backend.delete(&k).await.unwrap();
        assert!(backend.read(&k).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn search_returns_unsupported() {
        let backend = SqliteBackend::in_memory().unwrap();
        let err = backend.search(&SemanticQuery::new("q")).await.unwrap_err();
        assert!(matches!(err, MemoryError::Unsupported(_)));
    }

    #[tokio::test]
    async fn metadata_is_persisted() {
        let backend = SqliteBackend::in_memory().unwrap();
        let k = key("ns", "meta");
        let entry = MemoryEntry::builder("data")
            .metadata("tag", serde_json::json!("important"))
            .build();
        backend.write(k.clone(), entry).await.unwrap();
        let loaded = backend.read(&k).await.unwrap().unwrap();
        assert_eq!(loaded.metadata["tag"], serde_json::json!("important"));
    }
}
