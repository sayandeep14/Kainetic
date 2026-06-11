//! `AgentNode` — type-erased agent runner for use inside a [`crate::Pipeline`].

use std::sync::Arc;

use futures::future::BoxFuture;
use kainetic_core::{Agent, AgentContext, AgentError};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

use crate::error::PipelineError;

/// Type alias for the boxed runner closure inside an [`AgentNode`].
type NodeRunner =
    Box<dyn Fn(Value, AgentContext) -> BoxFuture<'static, Result<Value, PipelineError>> + Send + Sync>;

/// A named, type-erased agent node that communicates via [`serde_json::Value`].
///
/// All agents registered in a [`crate::Pipeline`] are wrapped in an
/// `AgentNode`. The node serialises/deserialises inputs and outputs at the
/// JSON boundary, so the pipeline graph itself is type-agnostic.
///
/// Create via [`AgentNode::new`].
pub struct AgentNode {
    /// The node's identifier within the pipeline.
    pub name: String,
    runner: NodeRunner,
}

impl AgentNode {
    /// Wraps `agent` in a type-erased node with the given `name`.
    ///
    /// `I` must implement [`DeserializeOwned`] (it is decoded from the JSON
    /// value arriving at this node's input). `O` must implement [`Serialize`]
    /// (it is encoded to JSON and passed downstream).
    pub fn new<A, I, O>(name: impl Into<String>, agent: A) -> Self
    where
        A: Agent<Input = I, Output = O> + 'static,
        A::Error: Into<AgentError> + Send,
        I: DeserializeOwned + Send + 'static,
        O: Serialize + Send + 'static,
    {
        let name = name.into();
        let agent = Arc::new(agent);

        let runner = Box::new(move |input_json: Value, ctx: AgentContext| {
            let agent = Arc::clone(&agent);
            Box::pin(async move {
                let input: I = serde_json::from_value(input_json)
                    .map_err(|e| PipelineError::Serialization(e.to_string()))?;
                let output = agent
                    .run(input, ctx)
                    .await
                    .map_err(|e| {
                        let ae: AgentError = e.into();
                        PipelineError::Agent(ae.to_string())
                    })?;
                serde_json::to_value(output)
                    .map_err(|e| PipelineError::Serialization(e.to_string()))
            }) as BoxFuture<'static, Result<Value, PipelineError>>
        });

        Self { name, runner }
    }

    /// Executes the agent with JSON `input`, returning a JSON output.
    pub(crate) async fn run(
        &self,
        input: Value,
        ctx: AgentContext,
    ) -> Result<Value, PipelineError> {
        (self.runner)(input, ctx).await
    }
}

impl std::fmt::Debug for AgentNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentNode")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}
