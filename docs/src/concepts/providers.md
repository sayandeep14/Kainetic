# Model Providers

A provider wraps a language model API. Every provider implements the `ModelProvider` trait, which exposes two methods: `complete` (blocking full response) and `stream` (incremental SSE chunks).

## Available providers

| Provider | Type | Struct |
|---|---|---|
| Anthropic Claude | Cloud | `AnthropicProvider` |
| OpenAI GPT | Cloud | `OpenAiProvider` |
| Google Gemini | Cloud | `GeminiProvider` |
| Mistral | Cloud | `MistralProvider` |
| Azure OpenAI | Cloud | `AzureOpenAiProvider` |
| Ollama | Local | `OllamaProvider` |

## Quick setup

```rust
// From environment variable
let provider = AnthropicProvider::from_env()?;  // reads ANTHROPIC_API_KEY

// Explicit key
let provider = AnthropicProvider::new("sk-ant-...");

// OpenAI-compatible (Ollama, Azure, etc.)
let provider = OllamaProvider::new("http://localhost:11434");
```

## ProviderRouter

Route between providers based on fallback order or cost caps:

```rust
let router = ProviderRouter::builder()
    .provider(AnthropicProvider::from_env()?)  // try first
    .provider(OpenAiProvider::from_env()?)     // fallback if Anthropic fails
    .cost_cap_usd(0.10)                        // max $0.10 per run
    .build();
```

`ProviderRouter` retries on `RateLimited` and `NetworkError`, then falls back to the next provider in the list. `AuthFailed` is not retried — fix your key.

## Implementing a custom provider

```rust
use async_trait::async_trait;
use kainetic_providers::{
    BoxStream, CompletionChunk, CompletionRequest, CompletionResponse,
    ModelProvider, ProviderError,
};
use kainetic_schema::TokenUsage;

pub struct MyProvider;

#[async_trait]
impl ModelProvider for MyProvider {
    fn name(&self) -> &'static str { "my-provider" }
    fn default_model(&self) -> &'static str { "my-model-v1" }
    fn cost_usd(&self, usage: &TokenUsage, _model: &str) -> f64 {
        f64::from(usage.total_tokens) * 0.000_001
    }

    async fn complete(&self, req: CompletionRequest)
        -> Result<CompletionResponse, ProviderError>
    {
        // Serialize req, call your API, deserialize response
        todo!()
    }

    async fn stream(&self, req: CompletionRequest)
        -> Result<BoxStream<Result<CompletionChunk, ProviderError>>, ProviderError>
    {
        todo!()
    }
}
```

## Retry behaviour

All built-in providers retry on `ProviderError::RateLimited` with exponential backoff (base 1 s, max 60 s, jitter ± 10 %). They do not retry on `AuthFailed`, `ModelNotFound`, or `ContextLengthExceeded`.
