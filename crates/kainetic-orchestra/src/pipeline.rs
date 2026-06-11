//! [`Pipeline`] — a validated, directed graph of [`AgentNode`]s.

use std::collections::HashMap;

use indexmap::IndexMap;
use kainetic_core::{Agent, AgentContext, AgentError};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use tracing::instrument;

use crate::error::PipelineError;
use crate::node::AgentNode;

/// Sentinel value returned from a conditional edge to signal pipeline
/// termination.
pub const DONE: &str = "__done__";

/// Default maximum number of pipeline iterations before bailing out.
const DEFAULT_MAX_ITERATIONS: u32 = 100;

/// Type alias for the transform closure on a [`DirectEdge`].
type TransformFn = Box<dyn Fn(Value) -> Result<Value, PipelineError> + Send + Sync>;

/// Type alias for the routing closure on a [`ConditionalEdge`].
type RouteFn = Box<dyn Fn(&Value) -> Result<String, PipelineError> + Send + Sync>;

/// An edge leaving a node that always routes to the same target.
pub(crate) struct DirectEdge {
    /// Name of the next node.
    pub to: String,
    /// Transforms the source node's output into the target node's input.
    pub transform: TransformFn,
}

/// An edge that decides the next node at runtime by inspecting the output.
pub(crate) struct ConditionalEdge {
    /// Returns the name of the next node, or [`DONE`] to terminate.
    pub route: RouteFn,
}

pub(crate) enum PipelineEdge {
    Direct(DirectEdge),
    Conditional(ConditionalEdge),
}

/// A directed graph of [`AgentNode`]s connected by typed edges.
///
/// Build with [`PipelineBuilder`] via [`Pipeline::builder`].
///
/// # Execution model
///
/// [`Pipeline::run`] starts at the *entry* node and follows edges until it
/// reaches a node with no outgoing edge (a terminal) or a conditional edge
/// that returns [`DONE`]. Each node's output is serialised to
/// [`serde_json::Value`] and passed through any transform on the edge before
/// being deserialised into the next node's input type.
pub struct Pipeline {
    /// Ordered map preserving insertion order (entry node first).
    nodes: IndexMap<String, AgentNode>,
    edges: HashMap<String, PipelineEdge>,
    entry: String,
    max_iterations: u32,
}

impl Pipeline {
    /// Returns a new builder.
    #[must_use]
    pub fn builder() -> PipelineBuilder {
        PipelineBuilder::default()
    }

    /// Runs the pipeline starting from the entry node with JSON `input`.
    ///
    /// Returns the JSON output of the last node that executed.
    ///
    /// # Errors
    ///
    /// - [`PipelineError::NodeNotFound`] if an edge references a missing node.
    /// - [`PipelineError::Serialization`] on input/output codec failures.
    /// - [`PipelineError::Agent`] if any agent returns an error.
    /// - [`PipelineError::MaxIterationsExceeded`] if the loop runs too long.
    #[instrument(skip(self, input, ctx), fields(pipeline.entry = %self.entry))]
    pub async fn run(
        &self,
        input: impl Serialize,
        ctx: AgentContext,
    ) -> Result<Value, PipelineError> {
        let mut current_node = self.entry.clone();
        let mut current_input = serde_json::to_value(input)
            .map_err(|e| PipelineError::Serialization(e.to_string()))?;

        for iteration in 0..self.max_iterations {
            tracing::debug!(iteration, node = %current_node, "pipeline step");

            let node = self
                .nodes
                .get(&current_node)
                .ok_or_else(|| PipelineError::NodeNotFound(current_node.clone()))?;

            let output = node.run(current_input, ctx.clone()).await?;

            match self.edges.get(&current_node) {
                // Terminal node — no outgoing edge.
                None => return Ok(output),

                Some(PipelineEdge::Direct(DirectEdge { to, transform })) => {
                    current_input = transform(output)?;
                    current_node.clone_from(to);
                }

                Some(PipelineEdge::Conditional(ConditionalEdge { route })) => {
                    let next = route(&output)?;
                    if next == DONE {
                        return Ok(output);
                    }
                    // Verify the target exists before advancing.
                    if !self.nodes.contains_key(&next) {
                        return Err(PipelineError::NodeNotFound(next));
                    }
                    current_input = output;
                    current_node = next;
                }
            }
        }

        Err(PipelineError::MaxIterationsExceeded)
    }

    /// Returns the names of all nodes in the pipeline, in insertion order.
    #[must_use]
    pub fn node_names(&self) -> Vec<&str> {
        self.nodes.keys().map(String::as_str).collect()
    }

    /// Returns the name of the entry node.
    #[must_use]
    pub fn entry(&self) -> &str {
        &self.entry
    }
}

/// Builder for [`Pipeline`].
///
/// Obtained via [`Pipeline::builder`].
#[derive(Default)]
pub struct PipelineBuilder {
    nodes: IndexMap<String, AgentNode>,
    edges: HashMap<String, PipelineEdge>,
    entry: Option<String>,
    max_iterations: Option<u32>,
}

impl PipelineBuilder {
    /// Registers an agent as a named node.
    ///
    /// The first agent registered becomes the pipeline's *entry* node.
    ///
    /// `I` and `O` are the agent's input and output types respectively; they
    /// must implement [`DeserializeOwned`] and [`Serialize`] so the pipeline
    /// can pass values across edge boundaries.
    #[must_use]
    pub fn agent<A, I, O>(mut self, name: impl Into<String>, agent: A) -> Self
    where
        A: Agent<Input = I, Output = O> + 'static,
        A::Error: Into<AgentError> + Send,
        I: DeserializeOwned + Send + 'static,
        O: Serialize + Send + 'static,
    {
        let name = name.into();
        if self.entry.is_none() {
            self.entry = Some(name.clone());
        }
        self.nodes.insert(name.clone(), AgentNode::new(name, agent));
        self
    }

    /// Adds a fixed edge from `from` → `to`, applying `transform` to convert
    /// the source node's output into the target node's input.
    ///
    /// `transform` may return [`PipelineError::Serialization`] if the
    /// conversion fails.
    #[must_use]
    pub fn edge(
        mut self,
        from: impl Into<String>,
        to: impl Into<String>,
        transform: impl Fn(Value) -> Result<Value, PipelineError> + Send + Sync + 'static,
    ) -> Self {
        self.edges.insert(
            from.into(),
            PipelineEdge::Direct(DirectEdge {
                to: to.into(),
                transform: Box::new(transform),
            }),
        );
        self
    }

    /// Adds an identity edge from `from` → `to` that passes the output
    /// unchanged (use when adjacent nodes share the same JSON shape).
    #[must_use]
    pub fn edge_passthrough(mut self, from: impl Into<String>, to: impl Into<String>) -> Self {
        self.edges.insert(
            from.into(),
            PipelineEdge::Direct(DirectEdge {
                to: to.into(),
                transform: Box::new(Ok),
            }),
        );
        self
    }

    /// Adds a conditional edge from `from` that calls `routing_fn` with the
    /// node's output to decide the next node at runtime.
    ///
    /// `routing_fn` must return either the name of a registered node or
    /// [`DONE`] to terminate the pipeline.
    #[must_use]
    pub fn conditional_edge(
        mut self,
        from: impl Into<String>,
        routing_fn: impl Fn(&Value) -> Result<String, PipelineError> + Send + Sync + 'static,
    ) -> Self {
        self.edges.insert(
            from.into(),
            PipelineEdge::Conditional(ConditionalEdge {
                route: Box::new(routing_fn),
            }),
        );
        self
    }

    /// Sets the maximum number of node executions before the pipeline aborts.
    ///
    /// Defaults to 100. Increase for pipelines with long feedback loops.
    #[must_use]
    pub fn max_iterations(mut self, n: u32) -> Self {
        self.max_iterations = Some(n);
        self
    }

    /// Validates the graph and builds the [`Pipeline`].
    ///
    /// # Errors
    ///
    /// - [`PipelineError::InvalidGraph`] if no agents were registered.
    /// - [`PipelineError::InvalidGraph`] if any edge references a node name
    ///   that was not registered.
    pub fn build(self) -> Result<Pipeline, PipelineError> {
        let entry = self
            .entry
            .ok_or_else(|| PipelineError::InvalidGraph("no agents registered".into()))?;

        // Validate that all edge targets exist.
        for (from, edge) in &self.edges {
            match edge {
                PipelineEdge::Direct(DirectEdge { to, .. }) => {
                    if !self.nodes.contains_key(to) {
                        return Err(PipelineError::InvalidGraph(format!(
                            "edge from `{from}` points to unknown node `{to}`"
                        )));
                    }
                }
                // Conditional edges are validated at runtime.
                PipelineEdge::Conditional(_) => {}
            }
        }

        // Verify all non-entry nodes are reachable from the entry.
        let reachable = reachable_nodes(&entry, &self.nodes, &self.edges);
        for name in self.nodes.keys() {
            if !reachable.contains(name.as_str()) {
                return Err(PipelineError::InvalidGraph(format!(
                    "node `{name}` is not reachable from the entry node `{entry}`"
                )));
            }
        }

        Ok(Pipeline {
            nodes: self.nodes,
            edges: self.edges,
            entry,
            max_iterations: self.max_iterations.unwrap_or(DEFAULT_MAX_ITERATIONS),
        })
    }
}

/// Returns the set of node names reachable from `entry` by following
/// `Direct` edges. Conditional edges are treated as reaching all registered
/// nodes (since we cannot know their targets statically).
fn reachable_nodes<'a>(
    entry: &'a str,
    nodes: &'a IndexMap<String, AgentNode>,
    edges: &'a HashMap<String, PipelineEdge>,
) -> std::collections::HashSet<&'a str> {
    let mut visited = std::collections::HashSet::new();
    let mut stack = vec![entry];

    while let Some(node) = stack.pop() {
        if visited.insert(node) {
            match edges.get(node) {
                Some(PipelineEdge::Direct(DirectEdge { to, .. })) => {
                    if nodes.contains_key(to.as_str()) {
                        stack.push(to.as_str());
                    }
                }
                // Conditional edges: treat as potentially visiting every node.
                Some(PipelineEdge::Conditional(_)) => {
                    for n in nodes.keys() {
                        stack.push(n.as_str());
                    }
                }
                None => {}
            }
        }
    }

    visited
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
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

    // ── Minimal mock infrastructure ────────────────────────────────────────

    struct MockProvider;

    #[async_trait]
    impl ModelProvider for MockProvider {
        async fn complete(&self, _: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
            Err(ProviderError::AuthFailed)
        }
        async fn stream(
            &self,
            _: CompletionRequest,
        ) -> Result<BoxStream<Result<CompletionChunk, ProviderError>>, ProviderError> {
            Err(ProviderError::AuthFailed)
        }
        fn cost_usd(&self, _: &TokenUsage, _: &str) -> f64 { 0.0 }
        fn name(&self) -> &'static str { "mock" }
        fn default_model(&self) -> &'static str { "mock-model" }
    }

    fn test_ctx() -> AgentContext {
        AgentContext::for_testing(
            std::sync::Arc::new(MockProvider),
            std::sync::Arc::new(ToolRegistry::new()),
        )
    }

    // ── A trivial agent that returns a preset JSON value ───────────────────

    struct EchoAgent {
        config: AgentConfig,
        responses: Mutex<VecDeque<String>>,
    }

    impl EchoAgent {
        fn new(responses: impl IntoIterator<Item = &'static str>) -> Self {
            Self {
                config: AgentConfig::builder().build(),
                responses: Mutex::new(responses.into_iter().map(str::to_owned).collect()),
            }
        }
    }

    impl Agent for EchoAgent {
        type Input = String;
        type Output = String;
        type Error = AgentError;

        fn name(&self) -> &'static str { "echo" }
        fn description(&self) -> &'static str { "Returns preset strings." }
        fn config(&self) -> &AgentConfig { &self.config }

        fn run(&self, _input: String, _ctx: AgentContext) -> AgentFuture<'_, String, AgentError> {
            let response = self
                .responses
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_default();
            Box::pin(async move { Ok(response) })
        }
    }

    // ── Tests ──────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn linear_two_node_pipeline() {
        let pipeline = Pipeline::builder()
            .agent("a", EchoAgent::new(["hello"]))
            .agent("b", EchoAgent::new(["world"]))
            .edge_passthrough("a", "b")
            .build()
            .unwrap();

        let output = pipeline.run("start".to_owned(), test_ctx()).await.unwrap();
        assert_eq!(output.as_str().unwrap(), "world");
    }

    #[tokio::test]
    async fn terminal_node_returns_output() {
        let pipeline = Pipeline::builder()
            .agent("only", EchoAgent::new(["final"]))
            .build()
            .unwrap();

        let output = pipeline.run("in".to_owned(), test_ctx()).await.unwrap();
        assert_eq!(output.as_str().unwrap(), "final");
    }

    #[tokio::test]
    async fn conditional_edge_routes_to_done() {
        let pipeline = Pipeline::builder()
            .agent("gate", EchoAgent::new(["bye"]))
            .conditional_edge("gate", |_| Ok(DONE.to_owned()))
            .build()
            .unwrap();

        let output = pipeline.run("in".to_owned(), test_ctx()).await.unwrap();
        assert_eq!(output.as_str().unwrap(), "bye");
    }

    #[tokio::test]
    async fn conditional_feedback_loop() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        let counter = Arc::new(AtomicU32::new(0));
        let c = Arc::clone(&counter);

        struct CountAgent {
            config: AgentConfig,
            counter: Arc<AtomicU32>,
        }

        impl Agent for CountAgent {
            type Input = String;
            type Output = String;
            type Error = AgentError;

            fn name(&self) -> &'static str { "count" }
            fn description(&self) -> &'static str { "Counts." }
            fn config(&self) -> &AgentConfig { &self.config }

            fn run(&self, _: String, _: AgentContext) -> AgentFuture<'_, String, AgentError> {
                let n = self.counter.fetch_add(1, Ordering::Relaxed) + 1;
                Box::pin(async move { Ok(n.to_string()) })
            }
        }

        let pipeline = Pipeline::builder()
            .agent("loop_node", CountAgent { config: AgentConfig::builder().build(), counter: c })
            .conditional_edge("loop_node", move |out| {
                let n: u32 = out.as_str().unwrap().parse().unwrap();
                if n >= 3 { Ok(DONE.to_owned()) } else { Ok("loop_node".to_owned()) }
            })
            .build()
            .unwrap();

        let output = pipeline.run("go".to_owned(), test_ctx()).await.unwrap();
        assert_eq!(output.as_str().unwrap(), "3");
        assert_eq!(counter.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn build_fails_for_unknown_edge_target() {
        let result = Pipeline::builder()
            .agent("a", EchoAgent::new([]))
            .edge_passthrough("a", "nonexistent")
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn build_fails_for_unreachable_node() {
        let result = Pipeline::builder()
            .agent("a", EchoAgent::new([]))
            .agent("orphan", EchoAgent::new([]))
            // No edge from "a" to "orphan"
            .build();
        assert!(result.is_err());
    }
}
