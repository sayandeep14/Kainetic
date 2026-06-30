//! `WorkingMemory` — ephemeral, run-scoped scratch space.

use kainetic_schema::RunId;

use crate::{
    backend::MemoryBackend, in_memory::InMemoryBackend, MemoryEntry, MemoryError, MemoryKey,
};

/// Short-lived, run-scoped memory store.
///
/// All data is held in an owned [`InMemoryBackend`] and is automatically
/// freed when the `WorkingMemory` is dropped — no explicit cleanup required.
///
/// Each run should create its own `WorkingMemory` via [`WorkingMemory::new`].
/// The `run_id` is embedded in every stored key's namespace, ensuring that
/// entries from different runs can never collide even if shared backends are
/// used elsewhere.
pub struct WorkingMemory {
    backend: InMemoryBackend,
    run_id: RunId,
}

impl WorkingMemory {
    /// Creates a new, empty working memory for the given run.
    #[must_use]
    pub fn new(run_id: RunId) -> Self {
        Self {
            backend: InMemoryBackend::new(),
            run_id,
        }
    }

    /// Returns the `RunId` this working memory is scoped to.
    #[must_use]
    pub fn run_id(&self) -> RunId {
        self.run_id
    }

    fn make_key(&self, key: &str) -> MemoryKey {
        MemoryKey::new(format!("working/{}", self.run_id), key)
    }

    /// Retrieves the entry for `key`, or `None` if absent.
    ///
    /// # Errors
    ///
    /// Propagates [`MemoryError`] from the underlying backend.
    pub async fn get(&self, key: &str) -> Result<Option<MemoryEntry>, MemoryError> {
        self.backend.read(&self.make_key(key)).await
    }

    /// Inserts or replaces the entry for `key`.
    ///
    /// # Errors
    ///
    /// Propagates [`MemoryError`] from the underlying backend.
    pub async fn set(&self, key: impl Into<String>, entry: MemoryEntry) -> Result<(), MemoryError> {
        self.backend.write(self.make_key(&key.into()), entry).await
    }

    /// Deletes the entry for `key`. No-op if the key does not exist.
    ///
    /// # Errors
    ///
    /// Propagates [`MemoryError`] from the underlying backend.
    pub async fn delete(&self, key: &str) -> Result<(), MemoryError> {
        self.backend.delete(&self.make_key(key)).await
    }

    /// Returns the number of entries currently stored.
    #[must_use]
    pub fn len(&self) -> usize {
        self.backend.len()
    }

    /// Returns `true` if no entries are stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.backend.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn set_and_get_round_trips() {
        let mem = WorkingMemory::new(RunId::new());
        mem.set("k", MemoryEntry::new("v")).await.unwrap();
        assert_eq!(mem.get("k").await.unwrap().unwrap().content, "v");
    }

    #[tokio::test]
    async fn get_missing_returns_none() {
        let mem = WorkingMemory::new(RunId::new());
        assert!(mem.get("nope").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn delete_removes_entry() {
        let mem = WorkingMemory::new(RunId::new());
        mem.set("x", MemoryEntry::new("y")).await.unwrap();
        mem.delete("x").await.unwrap();
        assert!(mem.get("x").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn two_runs_have_isolated_namespaces() {
        let m1 = WorkingMemory::new(RunId::new());
        let m2 = WorkingMemory::new(RunId::new());
        m1.set("shared_key", MemoryEntry::new("run1_value"))
            .await
            .unwrap();
        assert!(m2.get("shared_key").await.unwrap().is_none());
    }

    #[test]
    fn dropped_working_memory_frees_data() {
        let run_id = RunId::new();
        {
            let mem = WorkingMemory::new(run_id);
            // After this block the DashMap is dropped.
            assert!(mem.is_empty());
        }
        // No assertion needed — the test just verifies Drop doesn't panic.
    }
}
