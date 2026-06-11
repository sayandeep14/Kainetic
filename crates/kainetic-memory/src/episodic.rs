//! `EpisodicMemory` — session-keyed conversation history.

use std::sync::Arc;

use kainetic_schema::{Message, SessionId};

use crate::{backend::MemoryBackend, MemoryEntry, MemoryError, MemoryKey};

/// Default maximum number of messages kept in a session's history before
/// the oldest entries are trimmed.
const DEFAULT_MAX_ENTRIES: usize = 200;

/// Session-keyed conversation history backed by any [`MemoryBackend`].
///
/// History is stored as a single JSON-encoded entry under the key
/// `episodic/{session_id}/history`. When the number of stored messages
/// exceeds [`max_entries`], the oldest messages are silently dropped on the
/// next [`append`] call to keep the context window manageable.
///
/// # Context window management
///
/// For LLM-based summarisation of older turns, callers should load the
/// history via [`load`], summarise older entries using the model, and then
/// [`clear`] and re-populate with the summarised + recent messages.
///
/// [`max_entries`]: EpisodicMemory::max_entries
/// [`append`]: EpisodicMemory::append
/// [`load`]: EpisodicMemory::load
/// [`clear`]: EpisodicMemory::clear
pub struct EpisodicMemory {
    backend: Arc<dyn MemoryBackend>,
    session_id: SessionId,
    max_entries: usize,
}

impl EpisodicMemory {
    /// Creates a new episodic memory with the default maximum history length.
    #[must_use]
    pub fn new(backend: Arc<dyn MemoryBackend>, session_id: SessionId) -> Self {
        Self {
            backend,
            session_id,
            max_entries: DEFAULT_MAX_ENTRIES,
        }
    }

    /// Overrides the maximum number of messages retained per session.
    #[must_use]
    pub fn with_max_entries(mut self, max: usize) -> Self {
        self.max_entries = max;
        self
    }

    /// Returns the configured maximum history length.
    #[must_use]
    pub fn max_entries(&self) -> usize {
        self.max_entries
    }

    fn history_key(&self) -> MemoryKey {
        MemoryKey::new("episodic", format!("{}/history", self.session_id))
    }

    /// Appends `message` to the session's conversation history.
    ///
    /// If the history exceeds [`max_entries`] after appending, the oldest
    /// messages are trimmed so that exactly `max_entries` are retained.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError`] on backend read/write failure or if the stored
    /// history cannot be deserialised.
    ///
    /// [`max_entries`]: Self::max_entries
    pub async fn append(&self, message: Message) -> Result<(), MemoryError> {
        let mut history = self.load().await?;
        history.push(message);
        if history.len() > self.max_entries {
            let drop_count = history.len() - self.max_entries;
            history.drain(..drop_count);
        }
        self.save(&history).await
    }

    /// Loads the full conversation history for this session.
    ///
    /// Returns an empty `Vec` if no history has been stored yet.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError`] on backend read failure or deserialisation
    /// failure.
    pub async fn load(&self) -> Result<Vec<Message>, MemoryError> {
        let key = self.history_key();
        match self.backend.read(&key).await? {
            None => Ok(Vec::new()),
            Some(entry) => serde_json::from_str(&entry.content)
                .map_err(|e| MemoryError::Serialization(e.to_string())),
        }
    }

    /// Replaces the entire history with `messages`.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError`] on backend write failure.
    pub async fn save(&self, messages: &[Message]) -> Result<(), MemoryError> {
        let json = serde_json::to_string(messages)
            .map_err(|e| MemoryError::Serialization(e.to_string()))?;
        let entry = MemoryEntry::new(json);
        self.backend.write(self.history_key(), entry).await
    }

    /// Deletes all stored history for this session.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError`] on backend delete failure.
    pub async fn clear(&self) -> Result<(), MemoryError> {
        self.backend.delete(&self.history_key()).await
    }

    /// Returns the number of messages currently in history.
    ///
    /// # Errors
    ///
    /// Propagates errors from [`load`].
    ///
    /// [`load`]: Self::load
    pub async fn len(&self) -> Result<usize, MemoryError> {
        Ok(self.load().await?.len())
    }

    /// Returns `true` if no messages have been stored for this session.
    ///
    /// # Errors
    ///
    /// Propagates errors from [`load`].
    ///
    /// [`load`]: Self::load
    pub async fn is_empty(&self) -> Result<bool, MemoryError> {
        Ok(self.load().await?.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use kainetic_schema::{Message, SessionId};

    use super::*;
    use crate::InMemoryBackend;

    fn new_memory() -> EpisodicMemory {
        EpisodicMemory::new(Arc::new(InMemoryBackend::new()), SessionId::new())
    }

    #[tokio::test]
    async fn append_and_load_round_trips() {
        let mem = new_memory();
        mem.append(Message::user("hello")).await.unwrap();
        mem.append(Message::user("world")).await.unwrap();
        let history = mem.load().await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].content[0].as_text().unwrap(), "hello");
    }

    #[tokio::test]
    async fn load_empty_returns_empty_vec() {
        let mem = new_memory();
        assert!(mem.load().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn clear_removes_all_history() {
        let mem = new_memory();
        mem.append(Message::user("msg")).await.unwrap();
        mem.clear().await.unwrap();
        assert!(mem.load().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn max_entries_trims_oldest() {
        let mem = new_memory().with_max_entries(3);
        for i in 0u32..5 {
            mem.append(Message::user(format!("msg{i}"))).await.unwrap();
        }
        let history = mem.load().await.unwrap();
        assert_eq!(history.len(), 3);
        // The 3 most recent messages (msg2, msg3, msg4) are retained.
        assert_eq!(history[0].content[0].as_text().unwrap(), "msg2");
        assert_eq!(history[2].content[0].as_text().unwrap(), "msg4");
    }

    #[tokio::test]
    async fn two_sessions_have_isolated_history() {
        let backend: Arc<dyn MemoryBackend> = Arc::new(InMemoryBackend::new());
        let m1 = EpisodicMemory::new(Arc::clone(&backend), SessionId::new());
        let m2 = EpisodicMemory::new(Arc::clone(&backend), SessionId::new());
        m1.append(Message::user("s1")).await.unwrap();
        assert!(m2.load().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn save_and_load_preserves_all_fields() {
        let mem = new_memory();
        let msgs = vec![Message::user("u"), Message::user("v")];
        mem.save(&msgs).await.unwrap();
        let loaded = mem.load().await.unwrap();
        assert_eq!(loaded.len(), 2);
    }
}
