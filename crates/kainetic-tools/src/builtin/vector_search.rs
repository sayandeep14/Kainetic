//! `VectorSearchTool` — semantic similarity search over a `MemoryBackend`.
//!
//! Requires the `vector-search` crate feature.

#[cfg(feature = "vector-search")]
mod inner {
    use std::sync::Arc;

    use kainetic_memory::{MemoryBackend, SemanticQuery};
    use kainetic_schema::RootSchema;
    use schemars::{schema_for, JsonSchema};
    use serde::{Deserialize, Serialize};
    use tracing::debug;

    use crate::{Tool, ToolContext, ToolError, ToolFuture};

    /// Performs semantic similarity search over a [`MemoryBackend`].
    ///
    /// The backend must support [`MemoryBackend::search`] (e.g.
    /// [`kainetic_memory::UsearchBackend`]).  Non-vector backends return a
    /// [`ToolError::ExecutionFailed`] with `MemoryError::Unsupported`.
    pub struct VectorSearchTool {
        backend: Arc<dyn MemoryBackend>,
    }

    impl VectorSearchTool {
        /// Creates a tool backed by `backend`.
        #[must_use]
        pub fn new(backend: Arc<dyn MemoryBackend>) -> Self {
            Self { backend }
        }
    }

    #[derive(Deserialize, JsonSchema)]
    struct Input {
        /// The natural-language query string.
        query: String,
        /// Maximum number of results to return (default: 5).
        #[serde(default = "default_top_k")]
        top_k: usize,
        /// Minimum similarity threshold 0.0–1.0 (default: 0.0 = no filter).
        #[serde(default)]
        threshold: f32,
    }

    fn default_top_k() -> usize {
        5
    }

    #[derive(Serialize, JsonSchema)]
    struct ResultItem {
        key: String,
        content: String,
        score: f32,
    }

    #[derive(Serialize, JsonSchema)]
    struct Output {
        results: Vec<ResultItem>,
        count: usize,
    }

    impl Tool for VectorSearchTool {
        fn name(&self) -> &'static str {
            "vector_search"
        }

        fn description(&self) -> &'static str {
            "Perform semantic similarity search over the agent's memory backend."
        }

        fn input_schema(&self) -> RootSchema {
            schema_for!(Input)
        }

        fn output_schema(&self) -> RootSchema {
            schema_for!(Output)
        }

        fn call(&self, input: serde_json::Value, ctx: ToolContext) -> ToolFuture<'_> {
            let backend = Arc::clone(&self.backend);
            Box::pin(async move {
                let params: Input = serde_json::from_value(input)
                    .map_err(|e| ToolError::InputValidation(e.to_string()))?;

                debug!(query = %params.query, top_k = params.top_k, "vector_search");

                let query = SemanticQuery::new(&params.query)
                    .top_k(params.top_k as u32)
                    .threshold(params.threshold);

                let search = backend.search(&query);
                let entries = tokio::select! {
                    res = search => res.map_err(|e| ToolError::ExecutionFailed(e.to_string()))?,
                    _ = ctx.cancellation_token.cancelled() => return Err(ToolError::Cancelled),
                };

                let results: Vec<ResultItem> = entries
                    .into_iter()
                    .map(|e| ResultItem {
                        key: e.key.to_string(),
                        content: e.content.clone(),
                        score: e.embedding.as_ref().map_or(0.0, |_| 1.0),
                    })
                    .collect();

                let count = results.len();
                serde_json::to_value(Output { results, count })
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
            })
        }
    }
}

#[cfg(feature = "vector-search")]
pub use inner::VectorSearchTool;
