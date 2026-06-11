//! Integration tests for the `#[agent]` proc macro.

use std::collections::VecDeque;
use std::sync::Mutex;

use async_trait::async_trait;
use kainetic_core::{Agent, AgentConfig, AgentContext, AgentError, KaineticRuntime};
use kainetic_providers::{
    BoxStream, CompletionChunk, CompletionRequest, CompletionResponse, ModelProvider, ProviderError,
    StopReason,
};
use kainetic_schema::{MessageContent, TokenUsage};

// ─── Mock provider ────────────────────────────────────────────────────────────

struct MockProvider {
    responses: Mutex<VecDeque<CompletionResponse>>,
}

impl MockProvider {
    fn text(text: &str) -> Self {
        let response = CompletionResponse {
            content: vec![MessageContent::Text {
                text: text.to_owned(),
            }],
            stop_reason: StopReason::EndTurn,
            usage: TokenUsage::new(10, 5),
            model: "mock".to_owned(),
        };
        Self {
            responses: Mutex::new(std::iter::once(response).collect()),
        }
    }
}

#[async_trait]
impl ModelProvider for MockProvider {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
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

    fn cost_usd(&self, _: &TokenUsage, _: &str) -> f64 {
        0.0
    }

    fn name(&self) -> &'static str {
        "mock"
    }

    fn default_model(&self) -> &'static str {
        "mock-model"
    }
}

// ─── Agent defined via #[agent] macro ────────────────────────────────────────

#[kainetic_macros::agent(description = "Echoes the input string back.")]
async fn echo_agent(input: String, ctx: AgentContext) -> Result<String, AgentError> {
    let _ = ctx;
    Ok(format!("echo: {input}"))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[test]
fn generated_struct_has_correct_name() {
    let _ = EchoAgent::new();
    let _ = EchoAgent::default();
}

#[test]
fn name_matches_function_name() {
    assert_eq!(EchoAgent::new().name(), "echo_agent");
}

#[test]
fn description_matches_attribute() {
    assert_eq!(
        EchoAgent::new().description(),
        "Echoes the input string back."
    );
}

#[test]
fn with_config_stores_config() {
    let cfg = AgentConfig::builder().max_iterations(5).build();
    let agent = EchoAgent::with_config(cfg.clone());
    assert_eq!(agent.config().max_iterations, 5);
}

#[tokio::test]
async fn agent_run_returns_correct_output() {
    let runtime = KaineticRuntime::builder()
        .provider(MockProvider::text("irrelevant"))
        .build();
    let result = runtime
        .run(&EchoAgent::new(), "hello".to_owned())
        .await
        .unwrap();
    assert_eq!(result, "echo: hello");
}

#[tokio::test]
async fn agent_run_produces_correct_output() {
    let runtime = KaineticRuntime::builder()
        .provider(MockProvider::text("irrelevant"))
        .build();
    let result = runtime
        .run(&EchoAgent::new(), "world".to_owned())
        .await
        .unwrap();
    assert_eq!(result, "echo: world");
}
