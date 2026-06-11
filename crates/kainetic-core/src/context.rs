//! `AgentContext` — per-run execution context threaded through every layer.

use std::sync::Arc;

use kainetic_memory::{InMemoryBackend, MemoryBackend, MemoryEntry, MemoryError, MemoryKey};
use kainetic_providers::ModelProvider;
use kainetic_schema::{RunId, SessionId};
use kainetic_tools::ToolRegistry;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::event::AgentEvent;

/// Per-run execution context passed to every [`Agent::run`][crate::Agent::run] call.
///
/// `AgentContext` is the single "bag" that carries all dependencies an agent
/// needs during a run: identity, tools, the model provider, cancellation
/// signalling, the event bus, and the memory backend. It is created by
/// [`KaineticRuntime::run`][crate::KaineticRuntime::run] and threaded through
/// the [`ReActLoop`][crate::ReActLoop].
///
/// `AgentContext` is cheaply cloneable — all heavy resources are behind `Arc`s.
#[derive(Clone)]
pub struct AgentContext {
    /// Unique identifier for this run.
    pub run_id: RunId,
    /// Session identifier, shared across runs in the same conversation.
    pub session_id: SessionId,
    /// Registry of tools available to the agent.
    pub tools: Arc<ToolRegistry>,
    /// The language model provider that will receive completion requests.
    pub provider: Arc<dyn ModelProvider>,
    /// Memory backend for storing and retrieving agent state.
    pub memory: Arc<dyn MemoryBackend>,
    /// Token that is cancelled when the caller requests cooperative shutdown.
    pub cancellation_token: CancellationToken,
    /// `tracing` span associated with the root of this run.
    pub span: tracing::Span,
    pub(crate) event_tx: broadcast::Sender<AgentEvent>,
}

impl AgentContext {
    /// Creates a standalone `AgentContext` for tests and examples that do not
    /// go through [`KaineticRuntime`][crate::KaineticRuntime].
    ///
    /// Uses an in-memory event bus (capacity 64) and [`InMemoryBackend`] for
    /// memory. Pass a no-op provider if real LLM calls are not needed.
    #[must_use]
    pub fn for_testing(provider: Arc<dyn ModelProvider>, tools: Arc<ToolRegistry>) -> Self {
        let (event_tx, _) = broadcast::channel(64);
        Self {
            run_id: RunId::new(),
            session_id: SessionId::new(),
            tools,
            provider,
            memory: Self::default_memory(),
            cancellation_token: CancellationToken::new(),
            span: tracing::Span::current(),
            event_tx,
        }
    }

    /// Reads a value from memory, emitting a [`AgentEvent::MemoryRead`] event.
    ///
    /// # Errors
    ///
    /// Propagates [`MemoryError`] from the underlying backend.
    pub async fn memory_read(&self, key: &MemoryKey) -> Result<Option<MemoryEntry>, MemoryError> {
        let result = self.memory.read(key).await?;
        self.emit(AgentEvent::MemoryRead {
            run_id: self.run_id,
            key: key.to_string(),
            hit: result.is_some(),
        });
        Ok(result)
    }

    /// Writes a value to memory, emitting a [`AgentEvent::MemoryWrite`] event.
    ///
    /// # Errors
    ///
    /// Propagates [`MemoryError`] from the underlying backend.
    pub async fn memory_write(
        &self,
        key: MemoryKey,
        entry: MemoryEntry,
    ) -> Result<(), MemoryError> {
        let key_str = key.to_string();
        self.memory.write(key, entry).await?;
        self.emit(AgentEvent::MemoryWrite {
            run_id: self.run_id,
            key: key_str,
        });
        Ok(())
    }

    /// Returns an [`InMemoryBackend`] wrapped in an `Arc` — the default memory
    /// used when no backend is configured.
    #[must_use]
    pub fn default_memory() -> Arc<dyn MemoryBackend> {
        Arc::new(InMemoryBackend::new())
    }
}

impl AgentContext {
    /// Emits an event to all current subscribers on the event bus.
    ///
    /// Sending to a channel with no active receivers is silently ignored;
    /// this is the correct behaviour — events are advisory.
    pub fn emit(&self, event: AgentEvent) {
        let _ = self.event_tx.send(event);
    }
}
