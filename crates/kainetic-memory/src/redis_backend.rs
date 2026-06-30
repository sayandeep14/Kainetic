//! `RedisBackend` — distributed episodic memory backed by Redis.

use async_trait::async_trait;
use redis::AsyncCommands;

use crate::{backend::MemoryBackend, MemoryEntry, MemoryError, MemoryKey, SemanticQuery};

/// Redis-backed distributed memory store.
///
/// Entries are serialised to JSON and stored as Redis string values.
/// The Redis key format is `kainetic:{namespace}:{key}`.
///
/// # Authentication
///
/// Pass a full Redis URL (e.g. `redis://user:pass@host:6379/0`) to
/// [`RedisBackend::new`].
pub struct RedisBackend {
    client: redis::Client,
    key_prefix: String,
}

impl RedisBackend {
    /// Creates a backend connected to the given Redis URL.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError::Backend`] if the URL is invalid or the client
    /// cannot be constructed.
    pub fn new(url: impl AsRef<str>) -> Result<Self, MemoryError> {
        let client =
            redis::Client::open(url.as_ref()).map_err(|e| MemoryError::Backend(e.to_string()))?;
        Ok(Self {
            client,
            key_prefix: "kainetic".to_owned(),
        })
    }

    /// Creates a backend with a custom key prefix (default: `"kainetic"`).
    #[must_use]
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.key_prefix = prefix.into();
        self
    }

    fn redis_key(&self, key: &MemoryKey) -> String {
        format!("{}:{}:{}", self.key_prefix, key.namespace, key.key)
    }
}

#[async_trait]
impl MemoryBackend for RedisBackend {
    async fn read(&self, key: &MemoryKey) -> Result<Option<MemoryEntry>, MemoryError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| MemoryError::Backend(e.to_string()))?;
        let rk = self.redis_key(key);
        let value: Option<String> = conn
            .get(&rk)
            .await
            .map_err(|e| MemoryError::Backend(e.to_string()))?;
        value
            .map(|s| {
                serde_json::from_str(&s).map_err(|e| MemoryError::Serialization(e.to_string()))
            })
            .transpose()
    }

    async fn write(&self, key: MemoryKey, entry: MemoryEntry) -> Result<(), MemoryError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| MemoryError::Backend(e.to_string()))?;
        let json =
            serde_json::to_string(&entry).map_err(|e| MemoryError::Serialization(e.to_string()))?;
        let rk = self.redis_key(&key);
        conn.set::<_, _, ()>(&rk, json)
            .await
            .map_err(|e| MemoryError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn search(&self, _query: &SemanticQuery) -> Result<Vec<MemoryEntry>, MemoryError> {
        Err(MemoryError::Unsupported(
            "RedisBackend does not support semantic search; use UsearchBackend".into(),
        ))
    }

    async fn delete(&self, key: &MemoryKey) -> Result<(), MemoryError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| MemoryError::Backend(e.to_string()))?;
        let rk = self.redis_key(key);
        conn.del::<_, ()>(&rk)
            .await
            .map_err(|e| MemoryError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn flush(&self) -> Result<(), MemoryError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns a `RedisBackend` pointing at a local Redis instance if
    /// `REDIS_URL` is set; otherwise skips the test.
    fn redis_backend() -> Option<RedisBackend> {
        let url = std::env::var("REDIS_URL").ok()?;
        RedisBackend::new(&url).ok()
    }

    #[tokio::test]
    async fn write_and_read_round_trips() {
        let Some(backend) = redis_backend() else {
            return;
        };
        let k = MemoryKey::new("test", format!("key-{}", uuid::Uuid::new_v4()));
        backend
            .write(k.clone(), MemoryEntry::new("hello redis"))
            .await
            .unwrap();
        let entry = backend.read(&k).await.unwrap().unwrap();
        assert_eq!(entry.content, "hello redis");
        backend.delete(&k).await.unwrap();
    }

    #[tokio::test]
    async fn read_missing_returns_none() {
        let Some(backend) = redis_backend() else {
            return;
        };
        let k = MemoryKey::new("test", format!("missing-{}", uuid::Uuid::new_v4()));
        assert!(backend.read(&k).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn search_returns_unsupported() {
        let Some(backend) = redis_backend() else {
            return;
        };
        let err = backend.search(&SemanticQuery::new("q")).await.unwrap_err();
        assert!(matches!(err, MemoryError::Unsupported(_)));
    }
}
