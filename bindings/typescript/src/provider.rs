//! TypeScript-facing provider classes.

use std::sync::Arc;

use kainetic_providers::{
    AnthropicProvider as CoreAnthropicProvider, ModelProvider, OpenAiProvider as CoreOpenAiProvider,
};
use napi_derive::napi;

/// Wraps any provider for passing into `KaineticRuntime`.
#[derive(Clone)]
pub struct AnyProvider(pub Arc<dyn ModelProvider>);

// ── AnthropicProvider ──────────────────────────────────────────────────────

/// Anthropic Claude provider.
///
/// @example
/// ```typescript
/// const provider = AnthropicProvider.fromEnv();
/// ```
#[napi]
pub struct AnthropicProvider {
    pub(crate) arc: Arc<CoreAnthropicProvider>,
}

#[napi]
impl AnthropicProvider {
    /// Create an `AnthropicProvider` from the `ANTHROPIC_API_KEY` environment variable.
    #[napi(factory)]
    pub fn from_env() -> napi::Result<Self> {
        let p = CoreAnthropicProvider::from_env()
            .map_err(|e| napi::Error::from_reason(format!("ANTHROPIC_API_KEY: {e}")))?;
        Ok(Self { arc: Arc::new(p) })
    }

    /// Create an `AnthropicProvider` with an explicit API key.
    #[napi(factory)]
    pub fn with_key(api_key: String) -> Self {
        Self {
            arc: Arc::new(CoreAnthropicProvider::new(api_key)),
        }
    }
}

impl From<&AnthropicProvider> for AnyProvider {
    fn from(p: &AnthropicProvider) -> Self {
        AnyProvider(Arc::clone(&p.arc) as Arc<dyn ModelProvider>)
    }
}

// ── OpenAiProvider ─────────────────────────────────────────────────────────

/// OpenAI provider.
///
/// @example
/// ```typescript
/// const provider = OpenAiProvider.fromEnv();
/// ```
#[napi]
pub struct OpenAiProvider {
    pub(crate) arc: Arc<CoreOpenAiProvider>,
}

#[napi]
impl OpenAiProvider {
    /// Create an `OpenAiProvider` from the `OPENAI_API_KEY` environment variable.
    #[napi(factory)]
    pub fn from_env() -> napi::Result<Self> {
        let p = CoreOpenAiProvider::from_env()
            .map_err(|e| napi::Error::from_reason(format!("OPENAI_API_KEY: {e}")))?;
        Ok(Self { arc: Arc::new(p) })
    }

    /// Create an `OpenAiProvider` with an explicit API key.
    #[napi(factory)]
    pub fn with_key(api_key: String) -> Self {
        Self {
            arc: Arc::new(CoreOpenAiProvider::new(api_key)),
        }
    }
}

impl From<&OpenAiProvider> for AnyProvider {
    fn from(p: &OpenAiProvider) -> Self {
        AnyProvider(Arc::clone(&p.arc) as Arc<dyn ModelProvider>)
    }
}
