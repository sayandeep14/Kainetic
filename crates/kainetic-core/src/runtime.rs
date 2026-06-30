//! `KaineticRuntime` — the top-level entry point for running agents.

use std::sync::Arc;
use std::time::Instant;

use kainetic_memory::MemoryBackend;
use kainetic_providers::ModelProvider;
use kainetic_schema::{KaineticError, RunId, SessionId};
use kainetic_tools::{Tool, ToolRegistry};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::instrument;

use crate::event::AgentEvent;
use crate::{Agent, AgentContext, AgentError};

const EVENT_CHANNEL_CAPACITY: usize = 1024;

/// The top-level Kainetic runtime.
///
/// `KaineticRuntime` owns the shared tool registry and model provider. Each
/// call to [`run`] creates a fresh [`AgentContext`] (new `RunId`, `SessionId`,
/// `CancellationToken`) and invokes the agent, wrapping lifecycle events around
/// it.
///
/// Build with [`KaineticRuntime::builder`].
///
/// # Example
///
/// ```rust,no_run
/// use kainetic_core::KaineticRuntime;
/// use kainetic_providers::AnthropicProvider;
/// use kainetic_tools::builtin::CurrentDatetimeTool;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let runtime = KaineticRuntime::builder()
///     .provider(AnthropicProvider::from_env()?)
///     .tool(CurrentDatetimeTool)
///     .build();
/// # Ok(())
/// # }
/// ```
///
/// [`run`]: KaineticRuntime::run
pub struct KaineticRuntime {
    provider: Arc<dyn ModelProvider>,
    tools: Arc<ToolRegistry>,
    memory: Arc<dyn MemoryBackend>,
    event_tx: broadcast::Sender<AgentEvent>,
}

impl KaineticRuntime {
    /// Returns a builder pre-configured with an empty tool registry.
    #[must_use]
    pub fn builder() -> KaineticRuntimeBuilder {
        KaineticRuntimeBuilder::default()
    }

    /// Returns a new receiver for the runtime's event bus.
    ///
    /// The channel buffers up to 1 024 events. Lagging receivers that fall
    /// more than 1 024 events behind are automatically dropped by tokio.
    #[must_use]
    pub fn subscribe_events(&self) -> broadcast::Receiver<AgentEvent> {
        self.event_tx.subscribe()
    }

    /// Runs the given agent with the supplied input.
    ///
    /// Creates a fresh [`AgentContext`] for the run, dispatches the agent, and
    /// converts any error into [`KaineticError`].
    ///
    /// # Errors
    ///
    /// Returns [`KaineticError`] on any agent failure.
    #[instrument(skip(self, agent, input), fields(agent.name = agent.name()))]
    pub async fn run<A>(&self, agent: &A, input: A::Input) -> Result<A::Output, KaineticError>
    where
        A: Agent,
        A::Error: Into<AgentError>,
    {
        let run_id = RunId::new();
        let ctx = AgentContext {
            run_id,
            session_id: SessionId::new(),
            tools: Arc::clone(&self.tools),
            provider: Arc::clone(&self.provider),
            memory: Arc::clone(&self.memory),
            cancellation_token: CancellationToken::new(),
            span: tracing::Span::current(),
            event_tx: self.event_tx.clone(),
        };

        ctx.emit(AgentEvent::RunStarted {
            run_id,
            agent: agent.name().to_owned(),
        });

        let run_start = Instant::now();
        match agent.run(input, ctx.clone()).await {
            Ok(output) => {
                ctx.emit(AgentEvent::RunCompleted {
                    run_id,
                    total_tokens: 0,
                    cost_usd: 0.0,
                    latency_ms: u64::try_from(run_start.elapsed().as_millis()).unwrap_or(u64::MAX),
                });
                Ok(output)
            }
            Err(e) => {
                let agent_err: AgentError = e.into();
                ctx.emit(AgentEvent::RunFailed {
                    run_id,
                    error: agent_err.to_string(),
                });
                Err(agent_err.into())
            }
        }
    }
}

/// Builder for [`KaineticRuntime`].
///
/// Obtained via [`KaineticRuntime::builder`].
#[derive(Default)]
pub struct KaineticRuntimeBuilder {
    provider: Option<Arc<dyn ModelProvider>>,
    tools: ToolRegistry,
    memory: Option<Arc<dyn MemoryBackend>>,
}

impl KaineticRuntimeBuilder {
    /// Sets the language model provider.
    #[must_use]
    pub fn provider(mut self, provider: impl ModelProvider) -> Self {
        self.provider = Some(Arc::new(provider));
        self
    }

    /// Sets the language model provider from a pre-existing `Arc`.
    ///
    /// Useful when the provider is already behind an `Arc` (e.g. from FFI
    /// bindings that share the provider across multiple runtimes).
    #[must_use]
    pub fn provider_arc(mut self, provider: Arc<dyn ModelProvider>) -> Self {
        self.provider = Some(provider);
        self
    }

    /// Registers a tool with the runtime.
    ///
    /// See [`ToolRegistry::register`][kainetic_tools::ToolRegistry::register]
    /// for the panicking behaviour on duplicate names.
    #[must_use]
    pub fn tool(self, tool: impl Tool) -> Self {
        self.tools.register(tool);
        self
    }

    /// Sets a custom memory backend.
    ///
    /// Defaults to a fresh [`kainetic_memory::InMemoryBackend`] if not called.
    #[must_use]
    pub fn memory(mut self, backend: impl MemoryBackend) -> Self {
        self.memory = Some(Arc::new(backend));
        self
    }

    /// Builds the [`KaineticRuntime`].
    ///
    /// # Panics
    ///
    /// Panics if no provider was set via [`provider`][Self::provider].
    #[must_use]
    pub fn build(self) -> KaineticRuntime {
        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        KaineticRuntime {
            provider: self
                .provider
                .expect("KaineticRuntime::builder().provider(...) must be called before build()"),
            tools: Arc::new(self.tools),
            memory: self.memory.unwrap_or_else(AgentContext::default_memory),
            event_tx,
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use kainetic_providers::{
        BoxStream, CompletionChunk, CompletionRequest, CompletionResponse, ModelProvider,
        ProviderError, StopReason,
    };
    use kainetic_schema::{MessageContent, TokenUsage};

    use super::*;
    use crate::{Agent, AgentConfig, AgentContext, AgentError, AgentFuture};

    struct MockProvider {
        responses: Mutex<VecDeque<CompletionResponse>>,
    }

    impl MockProvider {
        fn with(responses: impl IntoIterator<Item = CompletionResponse>) -> Self {
            Self {
                responses: Mutex::new(responses.into_iter().collect()),
            }
        }
    }

    #[async_trait]
    impl ModelProvider for MockProvider {
        async fn complete(
            &self,
            _req: CompletionRequest,
        ) -> Result<CompletionResponse, ProviderError> {
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .ok_or(ProviderError::AuthFailed)
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<BoxStream<Result<CompletionChunk, ProviderError>>, ProviderError> {
            Err(ProviderError::AuthFailed)
        }

        fn cost_usd(&self, _: &kainetic_schema::TokenUsage, _: &str) -> f64 {
            0.0
        }

        fn name(&self) -> &'static str {
            "mock"
        }

        fn default_model(&self) -> &'static str {
            "mock-model"
        }
    }

    struct EchoAgent {
        config: AgentConfig,
    }

    impl EchoAgent {
        fn new() -> Self {
            Self {
                config: AgentConfig::builder().build(),
            }
        }
    }

    impl Agent for EchoAgent {
        type Input = String;
        type Output = String;
        type Error = AgentError;

        fn name(&self) -> &'static str {
            "echo"
        }

        fn description(&self) -> &'static str {
            "Echoes the input."
        }

        fn config(&self) -> &AgentConfig {
            &self.config
        }

        fn run(&self, input: String, _ctx: AgentContext) -> AgentFuture<'_, String, AgentError> {
            Box::pin(async move { Ok(format!("echo: {input}")) })
        }
    }

    fn text_response(text: &str) -> CompletionResponse {
        CompletionResponse {
            content: vec![MessageContent::Text {
                text: text.to_owned(),
            }],
            stop_reason: StopReason::EndTurn,
            usage: TokenUsage::new(5, 3),
            model: "mock".to_owned(),
        }
    }

    #[tokio::test]
    async fn run_returns_agent_output() {
        let runtime = KaineticRuntime::builder()
            .provider(MockProvider::with([text_response("irrelevant")]))
            .build();

        let result = runtime.run(&EchoAgent::new(), "hello".to_owned()).await;
        assert_eq!(result.unwrap(), "echo: hello");
    }

    #[tokio::test]
    async fn run_emits_started_and_completed_events() {
        let runtime = KaineticRuntime::builder()
            .provider(MockProvider::with([text_response("ok")]))
            .build();

        let mut rx = runtime.subscribe_events();
        runtime
            .run(&EchoAgent::new(), "x".to_owned())
            .await
            .unwrap();

        let first = rx.try_recv().unwrap();
        assert!(matches!(first, AgentEvent::RunStarted { .. }));
    }

    #[tokio::test]
    async fn failed_agent_emits_run_failed_event() {
        struct FailAgent {
            config: AgentConfig,
        }

        impl Agent for FailAgent {
            type Input = ();
            type Output = ();
            type Error = AgentError;

            fn name(&self) -> &'static str {
                "fail"
            }

            fn description(&self) -> &'static str {
                "Always fails."
            }

            fn config(&self) -> &AgentConfig {
                &self.config
            }

            fn run(&self, _input: (), _ctx: AgentContext) -> AgentFuture<'_, (), AgentError> {
                Box::pin(async { Err(AgentError::User("intentional".to_owned())) })
            }
        }

        let runtime = KaineticRuntime::builder()
            .provider(MockProvider::with([text_response("ok")]))
            .build();

        let mut rx = runtime.subscribe_events();
        let _ = runtime
            .run(
                &FailAgent {
                    config: AgentConfig::builder().build(),
                },
                (),
            )
            .await;

        let mut events = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            events.push(ev);
        }

        let has_failed = events
            .iter()
            .any(|e| matches!(e, AgentEvent::RunFailed { .. }));
        assert!(has_failed);
    }

    #[test]
    #[should_panic(expected = "provider")]
    fn builder_panics_without_provider() {
        let _ = KaineticRuntime::builder().build();
    }
}
