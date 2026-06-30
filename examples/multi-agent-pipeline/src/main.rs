//! A three-agent pipeline demonstrating [`kainetic_orchestra::Pipeline`].
//!
//! The pipeline processes a user prompt through three mock agents:
//!
//! ```text
//! [Researcher] ──► [Writer] ──► [Reviewer]
//! ```
//!
//! No real LLM calls are made; each agent transforms its input with a simple
//! string operation so the example runs without any API keys.
//!
//! # Running
//!
//! ```bash
//! cargo run --example multi-agent-pipeline -- "Rust async runtimes"
//! ```
#![deny(clippy::all, unsafe_code)]

use std::sync::Arc;

use async_trait::async_trait;
use kainetic_core::{Agent, AgentConfig, AgentContext, AgentError, AgentFuture};
use kainetic_orchestra::{parallel, Pipeline};
use kainetic_providers::{
    BoxStream, CompletionChunk, CompletionRequest, CompletionResponse, ModelProvider, ProviderError,
};
use kainetic_schema::TokenUsage;
use kainetic_tools::ToolRegistry;
use serde::{Deserialize, Serialize};

// ── Shared data types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Research {
    topic: String,
    findings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Draft {
    title: String,
    body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReviewedDraft {
    draft: Draft,
    approved: bool,
    feedback: String,
}

// ── Mock provider (no real HTTP calls) ────────────────────────────────────

struct NoopProvider;

#[async_trait]
impl ModelProvider for NoopProvider {
    async fn complete(&self, _: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        Err(ProviderError::AuthFailed)
    }
    async fn stream(
        &self,
        _: CompletionRequest,
    ) -> Result<BoxStream<Result<CompletionChunk, ProviderError>>, ProviderError> {
        Err(ProviderError::AuthFailed)
    }
    fn cost_usd(&self, _: &TokenUsage, _: &str) -> f64 {
        0.0
    }
    fn name(&self) -> &'static str {
        "noop"
    }
    fn default_model(&self) -> &'static str {
        "noop-model"
    }
}

// ── Agent implementations ──────────────────────────────────────────────────

/// Produces mock research findings for a topic.
struct ResearcherAgent {
    config: AgentConfig,
}

impl ResearcherAgent {
    fn new() -> Self {
        Self {
            config: AgentConfig::builder().build(),
        }
    }
}

impl Agent for ResearcherAgent {
    type Input = String;
    type Output = Research;
    type Error = AgentError;

    fn name(&self) -> &'static str {
        "researcher"
    }
    fn description(&self) -> &'static str {
        "Gathers findings about a topic."
    }
    fn config(&self) -> &AgentConfig {
        &self.config
    }

    fn run(&self, topic: String, _ctx: AgentContext) -> AgentFuture<'_, Research, AgentError> {
        Box::pin(async move {
            tracing::info!(topic, "researcher: gathering findings");
            Ok(Research {
                findings: vec![
                    format!("{topic} is widely used in production systems."),
                    format!("{topic} has excellent performance characteristics."),
                    format!("{topic} benefits from a strong community ecosystem."),
                ],
                topic,
            })
        })
    }
}

/// Turns research findings into a draft article.
struct WriterAgent {
    config: AgentConfig,
}

impl WriterAgent {
    fn new() -> Self {
        Self {
            config: AgentConfig::builder().build(),
        }
    }
}

impl Agent for WriterAgent {
    type Input = Research;
    type Output = Draft;
    type Error = AgentError;

    fn name(&self) -> &'static str {
        "writer"
    }
    fn description(&self) -> &'static str {
        "Drafts an article from research findings."
    }
    fn config(&self) -> &AgentConfig {
        &self.config
    }

    fn run(&self, research: Research, _ctx: AgentContext) -> AgentFuture<'_, Draft, AgentError> {
        Box::pin(async move {
            tracing::info!(topic = research.topic, "writer: drafting article");
            let body = research.findings.join("\n\n");
            Ok(Draft {
                title: format!("An Introduction to {}", research.topic),
                body,
            })
        })
    }
}

/// Reviews a draft and marks it approved.
struct ReviewerAgent {
    config: AgentConfig,
}

impl ReviewerAgent {
    fn new() -> Self {
        Self {
            config: AgentConfig::builder().build(),
        }
    }
}

impl Agent for ReviewerAgent {
    type Input = Draft;
    type Output = ReviewedDraft;
    type Error = AgentError;

    fn name(&self) -> &'static str {
        "reviewer"
    }
    fn description(&self) -> &'static str {
        "Reviews and approves or rejects a draft."
    }
    fn config(&self) -> &AgentConfig {
        &self.config
    }

    fn run(&self, draft: Draft, _ctx: AgentContext) -> AgentFuture<'_, ReviewedDraft, AgentError> {
        Box::pin(async move {
            tracing::info!(title = draft.title, "reviewer: reviewing draft");
            Ok(ReviewedDraft {
                approved: true,
                feedback: "Well-structured and concise. Approved.".to_owned(),
                draft,
            })
        })
    }
}

// ── Helper: build an AgentContext without a real runtime ───────────────────

fn standalone_ctx() -> AgentContext {
    AgentContext::for_testing(Arc::new(NoopProvider), Arc::new(ToolRegistry::new()))
}

// ── Main ───────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("multi_agent_pipeline=info".parse().unwrap())
                .add_directive("kainetic_orchestra=debug".parse().unwrap()),
        )
        .init();

    let topic = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "Rust async runtimes".to_owned());

    println!("=== Multi-agent pipeline demo ===");
    println!("Topic: {topic}");
    println!();

    // Build the pipeline: Researcher → Writer → Reviewer (terminal).
    let pipeline = Pipeline::builder()
        .agent("researcher", ResearcherAgent::new())
        .agent("writer", WriterAgent::new())
        .agent("reviewer", ReviewerAgent::new())
        .edge_passthrough("researcher", "writer")
        .edge_passthrough("writer", "reviewer")
        .build()
        .expect("pipeline graph is valid");

    println!("Pipeline nodes: {:?}", pipeline.node_names());
    println!("Entry: {}", pipeline.entry());
    println!();

    let ctx = standalone_ctx();
    let output = pipeline.run(topic, ctx).await.expect("pipeline run failed");

    let reviewed: ReviewedDraft =
        serde_json::from_value(output).expect("output is a ReviewedDraft");

    println!("=== Result ===");
    println!("Title   : {}", reviewed.draft.title);
    println!("Approved: {}", reviewed.approved);
    println!("Feedback: {}", reviewed.feedback);
    println!();
    println!("--- Body ---");
    println!("{}", reviewed.draft.body);

    // Demonstrate parallel! macro: run two independent agents concurrently.
    println!();
    println!("=== parallel! macro demo ===");

    let ctx_a = standalone_ctx();
    let ctx_b = standalone_ctx();
    let researcher_a = ResearcherAgent::new();
    let researcher_b = ResearcherAgent::new();

    let (ra, rb) = parallel!(
        researcher_a.run("Tokio".to_owned(), ctx_a),
        researcher_b.run("async-std".to_owned(), ctx_b),
    );

    println!("Parallel result A: {:?}", ra.unwrap().topic);
    println!("Parallel result B: {:?}", rb.unwrap().topic);
}
