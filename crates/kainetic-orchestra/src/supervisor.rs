//! [`Supervisor`] — a worker pool that routes tasks, handles retries, and
//! aggregates results.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use kainetic_core::AgentContext;
use rand::Rng;
use serde::Serialize;
use serde_json::Value;
use tracing::{instrument, warn};

use crate::error::SupervisorError;
use crate::node::AgentNode;

/// Strategy used by [`Supervisor`] to select a worker for each task.
pub enum RoutingStrategy {
    /// Sends tasks in round-robin order across all workers.
    RoundRobin,
    /// Sends each task to the worker with the fewest in-flight requests.
    LeastLoaded,
    /// Picks a worker at random for each task.
    Random,
    /// Calls a user-supplied function that receives the input JSON and returns
    /// the index of the worker to use.
    ContentBased(Box<dyn Fn(&Value) -> usize + Send + Sync>),
}

impl std::fmt::Debug for RoutingStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RoundRobin => write!(f, "RoundRobin"),
            Self::LeastLoaded => write!(f, "LeastLoaded"),
            Self::Random => write!(f, "Random"),
            Self::ContentBased(_) => write!(f, "ContentBased(fn)"),
        }
    }
}

/// A worker pool that routes single tasks to one of its workers, retrying on
/// failure up to `max_retries` times.
///
/// Build via [`Supervisor::builder`].
pub struct Supervisor {
    workers: Vec<AgentNode>,
    strategy: RoutingStrategy,
    max_retries: u32,
    /// Per-worker in-flight counters for [`RoutingStrategy::LeastLoaded`].
    in_flight: Vec<Arc<AtomicUsize>>,
    /// Round-robin cursor for [`RoutingStrategy::RoundRobin`].
    rr_cursor: Arc<AtomicUsize>,
}

impl Supervisor {
    /// Returns a new builder with no workers.
    #[must_use]
    pub fn builder() -> SupervisorBuilder {
        SupervisorBuilder::default()
    }

    /// Routes `input` to a worker, retrying up to `max_retries` times on
    /// failure. Returns the first successful JSON output, or
    /// [`SupervisorError::AllAttemptsFailed`] if every attempt fails.
    ///
    /// # Errors
    ///
    /// - [`SupervisorError::NoWorkers`] if no workers were registered.
    /// - [`SupervisorError::AllAttemptsFailed`] after exhausting retries.
    /// - [`SupervisorError::Serialization`] if `input` cannot be serialised.
    #[instrument(skip(self, input, ctx), fields(workers = self.workers.len(), max_retries = self.max_retries))]
    pub async fn run(
        &self,
        input: impl Serialize,
        ctx: AgentContext,
    ) -> Result<Value, SupervisorError> {
        if self.workers.is_empty() {
            return Err(SupervisorError::NoWorkers);
        }

        let input_value = serde_json::to_value(input)
            .map_err(|e| SupervisorError::Serialization(e.to_string()))?;

        let mut last_error = String::new();

        for attempt in 0..=self.max_retries {
            let idx = self.select_worker(&input_value);
            let counter = Arc::clone(&self.in_flight[idx]);

            counter.fetch_add(1, Ordering::Relaxed);
            let result = self.workers[idx].run(input_value.clone(), ctx.clone()).await;
            counter.fetch_sub(1, Ordering::Relaxed);

            match result {
                Ok(output) => return Ok(output),
                Err(e) => {
                    last_error = e.to_string();
                    if attempt < self.max_retries {
                        warn!(attempt, worker = idx, error = %last_error, "worker failed, retrying");
                    }
                }
            }
        }

        Err(SupervisorError::AllAttemptsFailed {
            attempts: self.max_retries + 1,
            last_error,
        })
    }

    fn select_worker(&self, input: &Value) -> usize {
        let n = self.workers.len();
        match &self.strategy {
            RoutingStrategy::RoundRobin => {
                self.rr_cursor.fetch_add(1, Ordering::Relaxed) % n
            }
            RoutingStrategy::LeastLoaded => {
                self.in_flight
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, c)| c.load(Ordering::Relaxed))
                    .map_or(0, |(i, _)| i)
            }
            RoutingStrategy::Random => rand::thread_rng().gen_range(0..n),
            RoutingStrategy::ContentBased(f) => f(input) % n,
        }
    }
}

/// Builder for [`Supervisor`].
#[derive(Default)]
pub struct SupervisorBuilder {
    workers: Vec<AgentNode>,
    strategy: Option<RoutingStrategy>,
    max_retries: Option<u32>,
}

impl SupervisorBuilder {
    /// Adds a worker node to the pool.
    ///
    /// Workers are selected according to the configured [`RoutingStrategy`].
    #[must_use]
    pub fn worker(mut self, node: AgentNode) -> Self {
        self.workers.push(node);
        self
    }

    /// Sets the routing strategy (default: [`RoutingStrategy::RoundRobin`]).
    #[must_use]
    pub fn routing_strategy(mut self, strategy: RoutingStrategy) -> Self {
        self.strategy = Some(strategy);
        self
    }

    /// Sets the maximum number of retries on worker failure (default: `0`).
    ///
    /// A value of `0` means no retries — the task fails immediately on the
    /// first worker error.
    #[must_use]
    pub fn max_retries(mut self, n: u32) -> Self {
        self.max_retries = Some(n);
        self
    }

    /// Builds the [`Supervisor`].
    #[must_use]
    pub fn build(self) -> Supervisor {
        let in_flight = self
            .workers
            .iter()
            .map(|_| Arc::new(AtomicUsize::new(0)))
            .collect();
        Supervisor {
            workers: self.workers,
            strategy: self.strategy.unwrap_or(RoutingStrategy::RoundRobin),
            max_retries: self.max_retries.unwrap_or(0),
            in_flight,
            rr_cursor: Arc::new(AtomicUsize::new(0)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use kainetic_core::{Agent, AgentConfig, AgentContext, AgentError, AgentFuture};
    use kainetic_providers::{
        BoxStream, CompletionChunk, CompletionRequest, CompletionResponse, ModelProvider,
        ProviderError,
    };
    use kainetic_schema::TokenUsage;
    use kainetic_tools::ToolRegistry;

    use super::*;

    struct MockProvider;

    #[async_trait]
    impl ModelProvider for MockProvider {
        async fn complete(&self, _: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
            Err(ProviderError::AuthFailed)
        }
        async fn stream(&self, _: CompletionRequest) -> Result<BoxStream<Result<CompletionChunk, ProviderError>>, ProviderError> {
            Err(ProviderError::AuthFailed)
        }
        fn cost_usd(&self, _: &TokenUsage, _: &str) -> f64 { 0.0 }
        fn name(&self) -> &'static str { "mock" }
        fn default_model(&self) -> &'static str { "mock-model" }
    }

    fn test_ctx() -> AgentContext {
        AgentContext::for_testing(Arc::new(MockProvider), Arc::new(ToolRegistry::new()))
    }

    struct ConstAgent {
        config: AgentConfig,
        value: String,
        fail_count: Mutex<u32>,
        fail_times: u32,
    }

    impl ConstAgent {
        fn new(value: &str) -> Self {
            Self {
                config: AgentConfig::builder().build(),
                value: value.to_owned(),
                fail_count: Mutex::new(0),
                fail_times: 0,
            }
        }

        fn fail_first(value: &str, times: u32) -> Self {
            Self {
                config: AgentConfig::builder().build(),
                value: value.to_owned(),
                fail_count: Mutex::new(0),
                fail_times: times,
            }
        }
    }

    impl Agent for ConstAgent {
        type Input = String;
        type Output = String;
        type Error = AgentError;

        fn name(&self) -> &'static str { "const" }
        fn description(&self) -> &'static str { "Returns a constant." }
        fn config(&self) -> &AgentConfig { &self.config }

        fn run(&self, _: String, _: AgentContext) -> AgentFuture<'_, String, AgentError> {
            let mut count = self.fail_count.lock().unwrap();
            if *count < self.fail_times {
                *count += 1;
                return Box::pin(async { Err(AgentError::User("forced failure".to_owned())) });
            }
            let v = self.value.clone();
            Box::pin(async move { Ok(v) })
        }
    }

    #[tokio::test]
    async fn round_robin_routes_in_order() {
        let sup = Supervisor::builder()
            .worker(AgentNode::new("w0", ConstAgent::new("w0")))
            .worker(AgentNode::new("w1", ConstAgent::new("w1")))
            .routing_strategy(RoutingStrategy::RoundRobin)
            .build();

        let r0 = sup.run("x".to_owned(), test_ctx()).await.unwrap();
        let r1 = sup.run("x".to_owned(), test_ctx()).await.unwrap();
        // First call hits worker 0, second hits worker 1.
        assert_eq!(r0.as_str().unwrap(), "w0");
        assert_eq!(r1.as_str().unwrap(), "w1");
    }

    #[tokio::test]
    async fn retries_on_worker_failure() {
        // Worker fails 2 times, then succeeds.
        let sup = Supervisor::builder()
            .worker(AgentNode::new("w", ConstAgent::fail_first("ok", 2)))
            .max_retries(3)
            .build();

        let result = sup.run("x".to_owned(), test_ctx()).await.unwrap();
        assert_eq!(result.as_str().unwrap(), "ok");
    }

    #[tokio::test]
    async fn fails_after_exhausting_retries() {
        let sup = Supervisor::builder()
            .worker(AgentNode::new("w", ConstAgent::fail_first("never", 10)))
            .max_retries(2)
            .build();

        let err = sup.run("x".to_owned(), test_ctx()).await.unwrap_err();
        assert!(matches!(err, SupervisorError::AllAttemptsFailed { attempts: 3, .. }));
    }

    #[tokio::test]
    async fn no_workers_returns_error() {
        let sup = Supervisor::builder().build();
        let err = sup.run("x".to_owned(), test_ctx()).await.unwrap_err();
        assert!(matches!(err, SupervisorError::NoWorkers));
    }

    #[tokio::test]
    async fn content_based_routing() {
        let sup = Supervisor::builder()
            .worker(AgentNode::new("even", ConstAgent::new("even")))
            .worker(AgentNode::new("odd", ConstAgent::new("odd")))
            .routing_strategy(RoutingStrategy::ContentBased(Box::new(|v| {
                // Route based on the first character's ASCII value.
                v.as_str().map_or(0, |s| s.len() % 2)
            })))
            .build();

        let r = sup.run("ab".to_owned(), test_ctx()).await.unwrap(); // len=2 → even (idx 0)
        assert_eq!(r.as_str().unwrap(), "even");
    }
}
