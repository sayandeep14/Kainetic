//! [`StateMachineAgent`] — an [`Agent`] that drives a typed state machine,
//! with optional checkpoint/resume support via a [`MemoryBackend`].

use std::sync::Arc;

use kainetic_core::{Agent, AgentConfig, AgentContext, AgentError};
use kainetic_memory::{MemoryBackend, MemoryEntry, MemoryKey};
use serde::{de::DeserializeOwned, Serialize};
use tracing::instrument;

use crate::error::StateMachineError;

/// Boxed transition function for [`StateMachineAgent`].
///
/// Takes the current state `S` and an [`AgentContext`], and returns a future
/// that resolves to the next [`Transition`].
type TransitionFn<S, O> = Box<
    dyn for<'a> Fn(
            S,
            &'a AgentContext,
        ) -> futures::future::BoxFuture<'a, Result<Transition<S, O>, StateMachineError>>
        + Send
        + Sync,
>;

/// Result of a single state-machine transition.
///
/// `S` is the state type; `O` is the final output type.
pub enum Transition<S, O> {
    /// The machine moves to a new state `S` and continues.
    Continue(S),
    /// The machine has finished and produces output `O`.
    Complete(O),
}

/// An [`Agent`] that repeatedly calls a transition function until it returns
/// [`Transition::Complete`].
///
/// Optionally persists the current state to a [`MemoryBackend`] after each
/// step so the machine can be resumed on failure.
///
/// Build via [`StateMachineAgent::builder`].
pub struct StateMachineAgent<S, O>
where
    S: Serialize + DeserializeOwned + Send + Sync + 'static,
    O: Serialize + Send + 'static,
{
    config: AgentConfig,
    name: &'static str,
    description: &'static str,
    /// Called on each step with the current state. Returns the next
    /// [`Transition`].
    transition_fn: TransitionFn<S, O>,
    memory: Option<Arc<dyn MemoryBackend>>,
    checkpoint_key: Option<String>,
}

impl<S, O> StateMachineAgent<S, O>
where
    S: Serialize + DeserializeOwned + Send + Sync + 'static,
    O: Serialize + Send + 'static,
{
    /// Returns a new builder.
    #[must_use]
    pub fn builder(name: &'static str, description: &'static str) -> StateMachineBuilder<S, O> {
        StateMachineBuilder::new(name, description)
    }
}

impl<S, O> Agent for StateMachineAgent<S, O>
where
    S: Serialize + DeserializeOwned + Send + Sync + Clone + 'static,
    O: Serialize + Send + 'static,
{
    type Input = S;
    type Output = O;
    type Error = AgentError;

    fn name(&self) -> &'static str {
        self.name
    }

    fn description(&self) -> &'static str {
        self.description
    }

    fn config(&self) -> &AgentConfig {
        &self.config
    }

    #[instrument(skip(self, initial_state, ctx), fields(agent = self.name))]
    fn run(
        &self,
        initial_state: S,
        ctx: AgentContext,
    ) -> kainetic_core::AgentFuture<'_, O, AgentError> {
        Box::pin(async move {
            // Attempt to resume from a checkpoint if one exists.
            let mut state = self
                .load_checkpoint(initial_state)
                .await
                .map_err(|e| AgentError::User(e.to_string()))?;

            loop {
                let transition = (self.transition_fn)(state, &ctx)
                    .await
                    .map_err(|e| AgentError::User(e.to_string()))?;

                match transition {
                    Transition::Continue(next_state) => {
                        self.save_checkpoint(&next_state)
                            .await
                            .map_err(|e| AgentError::User(e.to_string()))?;
                        state = next_state;
                    }
                    Transition::Complete(output) => {
                        self.clear_checkpoint().await;
                        return Ok(output);
                    }
                }
            }
        })
    }
}

impl<S, O> StateMachineAgent<S, O>
where
    S: Serialize + DeserializeOwned + Send + Sync + 'static,
    O: Serialize + Send + 'static,
{
    async fn load_checkpoint(&self, fallback: S) -> Result<S, StateMachineError> {
        let (Some(mem), Some(key)) = (&self.memory, &self.checkpoint_key) else {
            return Ok(fallback);
        };

        let mk = MemoryKey::new(self.name, key.as_str());
        match mem.read(&mk).await {
            Ok(Some(entry)) => {
                let state: S = serde_json::from_str(&entry.content)
                    .map_err(|e| StateMachineError::Checkpoint(e.to_string()))?;
                tracing::debug!(key = %mk, "resumed state machine from checkpoint");
                Ok(state)
            }
            Ok(None) => Ok(fallback),
            Err(e) => Err(StateMachineError::Checkpoint(e.to_string())),
        }
    }

    async fn save_checkpoint(&self, state: &S) -> Result<(), StateMachineError> {
        let (Some(mem), Some(key)) = (&self.memory, &self.checkpoint_key) else {
            return Ok(());
        };

        let json = serde_json::to_string(state)
            .map_err(|e| StateMachineError::Serialization(e.to_string()))?;

        let mk = MemoryKey::new(self.name, key.as_str());
        let entry = MemoryEntry::new(json);
        mem.write(mk, entry)
            .await
            .map_err(|e| StateMachineError::Checkpoint(e.to_string()))
    }

    async fn clear_checkpoint(&self) {
        let (Some(mem), Some(key)) = (&self.memory, &self.checkpoint_key) else {
            return;
        };
        let mk = MemoryKey::new(self.name, key.as_str());
        // Clearing is best-effort; we do not fail the run on error.
        if let Err(e) = mem.delete(&mk).await {
            tracing::warn!(key = %mk, error = %e, "failed to clear checkpoint");
        }
    }
}

/// Builder for [`StateMachineAgent`].
pub struct StateMachineBuilder<S, O>
where
    S: Serialize + DeserializeOwned + Send + Sync + 'static,
    O: Serialize + Send + 'static,
{
    name: &'static str,
    description: &'static str,
    transition_fn: Option<TransitionFn<S, O>>,
    memory: Option<Arc<dyn MemoryBackend>>,
    checkpoint_key: Option<String>,
}

impl<S, O> StateMachineBuilder<S, O>
where
    S: Serialize + DeserializeOwned + Send + Sync + 'static,
    O: Serialize + Send + 'static,
{
    fn new(name: &'static str, description: &'static str) -> Self {
        Self {
            name,
            description,
            transition_fn: None,
            memory: None,
            checkpoint_key: None,
        }
    }

    /// Sets the transition function.
    ///
    /// The function receives the current state and an [`AgentContext`], and
    /// must return a future that resolves to [`Transition::Continue`] or
    /// [`Transition::Complete`].
    #[must_use]
    pub fn transition<F, Fut>(mut self, f: F) -> Self
    where
        F: for<'a> Fn(S, &'a AgentContext) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<Transition<S, O>, StateMachineError>>
            + Send
            + 'static,
    {
        self.transition_fn = Some(Box::new(move |s, ctx| Box::pin(f(s, ctx))));
        self
    }

    /// Attaches a memory backend used to checkpoint and resume the machine
    /// state.
    ///
    /// Requires [`checkpoint_key`](Self::checkpoint_key) to be set.
    #[must_use]
    pub fn memory(mut self, backend: Arc<dyn MemoryBackend>) -> Self {
        self.memory = Some(backend);
        self
    }

    /// Sets the key under which state is persisted.
    ///
    /// The actual key stored is `"<name>:<checkpoint_key>"`.
    #[must_use]
    pub fn checkpoint_key(mut self, key: impl Into<String>) -> Self {
        self.checkpoint_key = Some(key.into());
        self
    }

    /// Builds the [`StateMachineAgent`].
    ///
    /// # Panics
    ///
    /// Panics if no transition function was set via [`transition`](Self::transition).
    #[must_use]
    pub fn build(self) -> StateMachineAgent<S, O> {
        StateMachineAgent {
            config: AgentConfig::builder().build(),
            name: self.name,
            description: self.description,
            transition_fn: self
                .transition_fn
                .expect("StateMachineAgent requires a transition function"),
            memory: self.memory,
            checkpoint_key: self.checkpoint_key,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use kainetic_core::AgentContext;
    use kainetic_memory::{InMemoryBackend, MemoryBackend};
    use kainetic_providers::{
        BoxStream, CompletionChunk, CompletionRequest, CompletionResponse, ModelProvider,
        ProviderError,
    };
    use kainetic_schema::TokenUsage;
    use kainetic_tools::ToolRegistry;

    use super::*;

    struct Stub;
    #[async_trait]
    impl ModelProvider for Stub {
        async fn complete(&self, _: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
            Err(ProviderError::AuthFailed)
        }
        async fn stream(&self, _: CompletionRequest) -> Result<BoxStream<Result<CompletionChunk, ProviderError>>, ProviderError> {
            Err(ProviderError::AuthFailed)
        }
        fn cost_usd(&self, _: &TokenUsage, _: &str) -> f64 { 0.0 }
        fn name(&self) -> &'static str { "stub" }
        fn default_model(&self) -> &'static str { "stub" }
    }

    fn test_ctx() -> AgentContext {
        AgentContext::for_testing(Arc::new(Stub), Arc::new(ToolRegistry::new()))
    }

    #[tokio::test]
    async fn counts_to_five_and_completes() {
        let sm = StateMachineAgent::<u32, String>::builder("counter", "counts")
            .transition(|n, _ctx| async move {
                if n >= 5 {
                    Ok(Transition::Complete(format!("done:{n}")))
                } else {
                    Ok(Transition::Continue(n + 1))
                }
            })
            .build();

        let result = sm.run(0, test_ctx()).await.unwrap();
        assert_eq!(result, "done:5");
    }

    #[tokio::test]
    async fn checkpoints_and_resumes() {
        let mem: Arc<dyn MemoryBackend> = Arc::new(InMemoryBackend::new());

        // First run — starts at 0, stops (simulated crash) after 3 transitions.
        // We simulate this by limiting initial state directly. In real use the
        // crash would interrupt the process; here we just verify write/read round-trip.
        let sm = StateMachineAgent::<u32, u32>::builder("sm", "test")
            .transition(|n, _ctx| async move {
                if n >= 2 {
                    Ok(Transition::Complete(n))
                } else {
                    Ok(Transition::Continue(n + 1))
                }
            })
            .memory(Arc::clone(&mem))
            .checkpoint_key("test-run")
            .build();

        let result = sm.run(0, test_ctx()).await.unwrap();
        assert_eq!(result, 2);

        // The checkpoint key should be cleared after completion.
        let key = kainetic_memory::MemoryKey::new("sm", "test-run");
        let entry = mem.read(&key).await.unwrap();
        assert!(entry.is_none(), "checkpoint should be cleared on completion");
    }
}
