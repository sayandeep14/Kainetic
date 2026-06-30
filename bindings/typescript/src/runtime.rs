//! TypeScript-facing `KaineticRuntime` class.

use std::sync::Arc;

use kainetic_core::KaineticRuntime as CoreRuntime;
use napi_derive::napi;

use crate::agent::AgentHandle;
use crate::provider::{AnthropicProvider, AnyProvider, OpenAiProvider};
use crate::tool::{JsTool, ToolHandle};

/// The Kainetic async runtime for Node.js.
///
/// Construct via the factory methods:
/// ```typescript
/// const runtime = KaineticRuntime.fromAnthropic(AnthropicProvider.fromEnv());
/// const result = await runtime.run(myAgent, 'hello');
/// ```
#[napi]
pub struct KaineticRuntime {
    inner: Arc<CoreRuntime>,
}

#[napi]
impl KaineticRuntime {
    /// Build a runtime backed by Anthropic Claude.
    ///
    /// @param provider - An `AnthropicProvider` instance.
    /// @param tools    - Optional default tools for every agent.
    #[napi(factory)]
    pub fn from_anthropic(
        provider: &AnthropicProvider,
        tools: Option<Vec<&ToolHandle>>,
    ) -> napi::Result<Self> {
        build_runtime(AnyProvider::from(provider), tools)
    }

    /// Build a runtime backed by OpenAI GPT.
    ///
    /// @param provider - An `OpenAiProvider` instance.
    /// @param tools    - Optional default tools for every agent.
    #[napi(factory)]
    pub fn from_openai(
        provider: &OpenAiProvider,
        tools: Option<Vec<&ToolHandle>>,
    ) -> napi::Result<Self> {
        build_runtime(AnyProvider::from(provider), tools)
    }

    /// Run an agent and return its string output.
    ///
    /// @param agentHandle - Agent created by `agent()`.
    /// @param input       - String input for the agent.
    /// @returns Promise that resolves to the agent's string output.
    #[napi]
    pub async fn run(&self, agent_handle: &AgentHandle, input: String) -> napi::Result<String> {
        let inner = Arc::clone(&self.inner);
        let agent_arc = Arc::clone(&agent_handle.inner);
        inner
            .run(&*agent_arc, input)
            .await
            .map_err(|e| napi::Error::from_reason(e.to_string()))
    }
}

fn build_runtime(
    provider: AnyProvider,
    tools: Option<Vec<&ToolHandle>>,
) -> napi::Result<KaineticRuntime> {
    let mut builder = CoreRuntime::builder().provider_arc(provider.0);

    if let Some(tool_list) = tools {
        for th in tool_list {
            let js_tool: JsTool = (*th.inner).clone();
            builder = builder.tool(js_tool);
        }
    }

    Ok(KaineticRuntime {
        inner: Arc::new(builder.build()),
    })
}
