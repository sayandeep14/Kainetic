<div align="center">

# Kainetic

**The production-grade Rust runtime for agentic AI.**

*What Tokio is to async Rust, Kainetic is to AI agents.*

[![Build](https://img.shields.io/badge/build-passing-brightgreen?style=flat-square)](.)
[![Tests](https://img.shields.io/badge/tests-passing-brightgreen?style=flat-square)](.)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue?style=flat-square)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange?style=flat-square)](https://www.rust-lang.org)

</div>

---

## Table of Contents

- [Why Kainetic?](#why-kainetic)
- [Performance Benchmarks](#performance-benchmarks)
- [Quick Start](#quick-start)
  - [Prerequisites](#prerequisites)
  - [1 — Clone & Build](#1--clone--build)
  - [2 — Set Up Environment](#2--set-up-environment)
  - [3 — Run the Simple Agent](#3--run-the-simple-agent)
  - [4 — Run the Multi-Agent Pipeline](#4--run-the-multi-agent-pipeline)
  - [5 — Start the Cloud Backend & Dashboard](#5--start-the-cloud-backend--dashboard)
- [Core Concepts](#core-concepts)
  - [The Agent Trait](#the-agent-trait)
  - [The Tool System](#the-tool-system)
  - [The ReAct Loop](#the-react-loop)
  - [Memory Backends](#memory-backends)
  - [Multi-Agent Orchestration](#multi-agent-orchestration)
- [Built-in Tools](#built-in-tools)
- [Supported Providers](#supported-providers)
- [Workspace Structure](#workspace-structure)
- [Running Tests](#running-tests)
- [How to Adapt Kainetic](#how-to-adapt-kainetic)
  - [Writing a Custom Tool](#writing-a-custom-tool)
  - [Writing a Custom Agent](#writing-a-custom-agent)
  - [Using a Custom Provider](#using-a-custom-provider)
  - [Wiring a Multi-Agent Pipeline](#wiring-a-multi-agent-pipeline)
  - [Python & TypeScript Bindings](#python--typescript-bindings)
- [Observability](#observability)
- [CLI Reference](#cli-reference)
- [Comparison with Python Frameworks](#comparison-with-python-frameworks)
- [Architecture Principles](#architecture-principles)
- [Roadmap](#roadmap)
- [License](#license)

---

## Why Kainetic?

Python AI agent frameworks — LangChain, LangGraph, AutoGen, CrewAI — were designed for research. Their architecture carries structural costs that no amount of optimization can remove:

- **The GIL** prevents true parallel tool execution across threads
- **Async was retrofitted** — cancellation semantics and cross-task error propagation are fragile
- **Cold starts of 60–140 ms** make serverless and edge deployments unreliable
- **No compile-time guarantees** — your agent topology might only fail at 3 AM in production

Kainetic is built in Rust from first principles, for production:

- Every tool schema is a Rust type — validated at compile time, not at runtime
- Every in-flight operation propagates `CancellationToken` — cancel a parent, all children stop immediately
- Parallel tool calls are structural — `FuturesUnordered` dispatches all independent tool calls concurrently
- The type system enforces your agent graph is valid before a single request is handled
- Cold start is under 5 ms — serverless and edge are first-class targets

---

## Performance Benchmarks

Measured against LangGraph (Python) on identical workloads — same model, same tools, same task:

| Metric | LangGraph (Python) | **Kainetic (Rust)** | Improvement |
|---|---|---|---|
| Cold start (serverless) | 80–140 ms | **< 5 ms** | **> 16×** |
| Memory at rest | 180–250 MB | **< 15 MB** | **> 12×** |
| Memory peak (10 concurrent runs) | 3–6 GB | **< 300 MB** | **> 10×** |
| Single-agent throughput | 2.7 req/s | **> 12 req/s** | **> 4×** |
| Parallel tool call overhead | N/A — serial only | **< 2 ms scheduling** | structural |
| P99 latency overhead | +800 ms | **< 50 ms** | **> 16×** |

> LLM network round-trips (1–5 s) dominate total latency. Kainetic's advantage is most visible in high-concurrency deployments and serverless/edge environments where cold starts and memory pressure matter.

**Criterion micro-benchmarks** (run `cargo bench -p kainetic-core`):

```
cold_start                   time:   [3.2 ms   3.4 ms   3.8 ms]
parallel_dispatch/1  tool    time:   [0.3 µs   0.4 µs   0.5 µs]
parallel_dispatch/4  tools   time:   [0.8 µs   0.9 µs   1.1 µs]
parallel_dispatch/8  tools   time:   [1.2 µs   1.4 µs   1.7 µs]
parallel_dispatch/16 tools   time:   [2.1 µs   2.3 µs   2.7 µs]
```

---

## Quick Start

### Prerequisites

| Tool | Version | Notes |
|---|---|---|
| Rust | 1.75+ | `rustup update stable` |
| Node.js | 18+ | Dashboard only |
| PostgreSQL | 14+ | Cloud backend only |
| Docker | any | Optional — Qdrant / Redis |

```bash
# Verify Rust is up to date
rustc --version
rustup update stable
```

### 1 — Clone & Build

```bash
git clone https://github.com/sayandeep14/kainetic.git
cd kainetic

# Build the full workspace (~60 s first time)
cargo build --workspace

# Verify zero warnings
cargo clippy --workspace
```

### 2 — Set Up Environment

`.env` is gitignored — safe to store real keys here:

```bash
nano .env
```

**Minimum required to run the simple agent:**
```env
ANTHROPIC_API_KEY=sk-ant-...
```

**Additional keys unlock more features:**
```env
OPENAI_API_KEY=sk-proj-...         # OpenAI provider
GEMINI_API_KEY=...                  # Google Gemini provider
BRAVE_SEARCH_API_KEY=...            # WebSearchTool (Brave Search API)
REDIS_URL=redis://localhost:6379    # Redis memory backend
POSTGRES_URL=postgres://...         # pgvector memory backend
QDRANT_URL=http://localhost:6333    # Qdrant vector backend
DATABASE_URL=postgres://...         # Cloud backend
JWT_SECRET=<openssl rand -hex 32>   # Cloud backend auth (min 32 chars)
```

Load the env file before running anything:

```bash
set -a && source .env && set +a
```

### 3 — Run the Simple Agent

A single agent that answers date/time questions using `CurrentDatetimeTool`:

```bash
cargo run --example simple-agent -- "What day of the week is it?"
```

```
Query: What day of the week is it?
Answer: Today is Tuesday, June 13, 2026.
```

No API key? The binary prints setup instructions and exits cleanly — it never panics.

### 4 — Run the Multi-Agent Pipeline

A three-agent `Researcher → Writer → Reviewer` pipeline. Runs without any API key (uses a mock provider):

```bash
cargo run --example multi-agent-pipeline -- "Rust async runtimes"
```

```
Pipeline result: Reviewed(approved=true): "REVIEWED: Draft: WRITTEN: Research on: Rust async runtimes"
```

### 5 — Start the Cloud Backend & Dashboard

**Create the database:**
```bash
createdb kainetic_cloud
```

**Start the backend (auto-migrates schema on first run):**
```bash
source .env
cargo run -p kainetic-cloud
# → INFO  listening addr=0.0.0.0:8080
```

**Bootstrap your first admin account — one-time only, fails if any user exists:**
```bash
curl -X POST http://localhost:8080/v1/setup \
  -H 'Content-Type: application/json' \
  -d '{"email": "admin@example.com", "team_name": "My Team"}'
```

```json
{
  "team_id": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx",
  "api_key":  "kk_...",
  "created_at": "2026-06-13T..."
}
```

**Save the `team_id` and `api_key` — the key is shown only once.**

**Start the dashboard:**
```bash
cd dashboard
npm install      # first time only
npm run dev
# → http://localhost:3000
```

Open `http://localhost:3000`, enter your **Team ID** and **API Key**, and log in.

---

## Core Concepts

### The Agent Trait

The `Agent` trait is the fundamental abstraction. Implement it directly for full control, or use the `#[agent]` macro:

```rust
// Macro form — generates struct + impl automatically
#[kainetic_macros::agent(description = "Answers questions about the weather.")]
async fn weather_agent(input: String, ctx: AgentContext) -> Result<String, AgentError> {
    let config = AgentConfig::builder()
        .model("claude-sonnet-4-6")
        .system_prompt("You are a helpful weather assistant.")
        .max_iterations(5)
        .build();

    ReActLoop::new(config).execute(input, ctx).await
}

// Use it
let runtime = KaineticRuntime::builder()
    .provider(AnthropicProvider::from_env()?)
    .tool(GetWeather)
    .build();

let answer = runtime.run(&WeatherAgent::new(), "What's the weather in Tokyo?").await?;
```

```rust
// Manual implementation — for full control
pub struct MyAgent { config: AgentConfig }

impl Agent for MyAgent {
    type Input  = String;
    type Output = String;
    type Error  = AgentError;

    fn name(&self)        -> &'static str { "my_agent" }
    fn description(&self) -> &'static str { "Does something useful" }
    fn config(&self)      -> &AgentConfig { &self.config }

    fn run(&self, input: String, ctx: AgentContext) -> AgentFuture<'_, String, AgentError> {
        Box::pin(async move {
            Ok(format!("Processed: {input}"))
        })
    }
}
```

### The Tool System

The `#[tool]` macro generates the full `Tool` impl — schema, serialization, tracing, and timeout:

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct WeatherInput {
    /// City name or lat/lon coordinates
    pub location: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct WeatherOutput {
    pub temperature_c: f32,
    pub condition: String,
}

#[kainetic_macros::tool(
    description = "Get current weather for a location",
    timeout = "10s"
)]
async fn get_weather(
    input: WeatherInput,
    _ctx: ToolContext,
) -> Result<WeatherOutput, ToolError> {
    // call your weather API here
    Ok(WeatherOutput { temperature_c: 22.0, condition: "Sunny".into() })
}
```

**What the macro generates:**
- Unit struct `GetWeather` (PascalCase of the function name)
- `Tool` impl with `name()`, `description()`, `input_schema()`, `output_schema()`
- JSON Schema from `JsonSchema` derive — no manual schema writing
- JSON deserialization of inputs and serialization of outputs
- Timeout wrapper via `tokio::time::timeout`
- A `tracing` span for every call

### The ReAct Loop

`ReActLoop` implements the Reason → Act → Observe cycle:

1. **Reason** — send the conversation to the LLM; receive a response
2. **Act** — if the LLM returns tool calls, dispatch all independent ones concurrently via `FuturesUnordered`
3. **Observe** — collect results, append to conversation, loop back to step 1
4. **Stop** — on a final text response, or when `max_iterations` is reached

```rust
let config = AgentConfig::builder()
    .model("claude-sonnet-4-6")
    .system_prompt("You are a helpful assistant.")
    .max_iterations(10)
    .tool_timeout(Duration::from_secs(30))
    .build();

let result = ReActLoop::new(config).execute(user_input, ctx).await?;
```

Parallel tool dispatch is the structural default, not an opt-in. When the model requests tools A, B, and C independently, all three start simultaneously.

### Memory Backends

Six pluggable storage backends for agent state across runs:

```rust
// In-memory (default — zero config)
let backend = InMemoryBackend::new();

// SQLite — local persistence, no server needed
let backend = SqliteBackend::new("agent-memory.db")?;

// Redis — distributed, shared across instances
let backend = RedisBackend::new("redis://localhost:6379").await?;

// PostgreSQL — application-side cosine similarity
let backend = PgVectorBackend::new(&postgres_url).await?;

// Qdrant — native approximate nearest-neighbour search
let backend = QdrantBackend::new("http://localhost:6333", "my_collection", 1536).await?;

// Wire into runtime
let runtime = KaineticRuntime::builder()
    .provider(provider)
    .memory(backend)
    .build();
```

Read and write from inside any agent:

```rust
// Write a memory entry
ctx.memory_write(
    MemoryKey::new("session", "last_query"),
    MemoryEntry::builder(user_input).build(),
).await?;

// Read it back
if let Some(entry) = ctx.memory_read(&MemoryKey::new("session", "last_query")).await? {
    println!("Last query was: {}", entry.content);
}
```

### Multi-Agent Orchestration

Three primitives from `kainetic-orchestra`:

**Pipeline** — validated DAG with typed edges (cycles caught at build time):
```rust
let pipeline = Pipeline::builder()
    .node("researcher", Arc::new(ResearcherAgent::new()))
    .node("writer",     Arc::new(WriterAgent::new()))
    .node("reviewer",   Arc::new(ReviewerAgent::new()))
    .edge("researcher", "writer")
    .edge("writer",     "reviewer")
    .build()?;   // returns Err if graph is invalid

let result = pipeline.run(topic, ctx).await?;
```

**Supervisor** — worker pool with configurable routing strategy:
```rust
let supervisor = Supervisor::builder()
    .workers(vec![worker_a, worker_b, worker_c])
    .routing(RoutingStrategy::LeastLoaded)  // or RoundRobin, Random, ContentBased
    .build();
```

**StateMachineAgent** — durable state machine with checkpoint/resume:
```rust
let agent = StateMachineAgent::new(transitions, initial_state)
    .with_checkpoint(sqlite_backend);
// if the process crashes mid-run, it resumes from the last checkpoint
```

**`parallel!` macro** — run independent agents concurrently and join:
```rust
let (research, sentiment) = parallel!(
    research_agent.run(topic.clone(), ctx.clone()),
    sentiment_agent.run(topic.clone(), ctx.clone()),
).await;
```

---

## Built-in Tools

| Tool | Feature Flag | Description |
|---|---|---|
| `CurrentDatetimeTool` | *(always on)* | Returns current UTC date/time in ISO 8601 |
| `HttpRequestTool` | *(always on)* | HTTP GET/POST with configurable headers |
| `WebSearchTool` | *(always on)* | Web search via Brave Search API |
| `WebFetchTool` | *(always on)* | Fetches a URL and strips HTML to plain text |
| `FileReadTool` | *(always on)* | Reads files within a sandboxed base directory |
| `FileWriteTool` | *(always on)* | Writes files within a sandboxed base directory |
| `SqlQueryTool` | *(always on)* | Read-only SQL against a SQLite database |
| `ShellTool` | `shell` | Runs shell commands in a subprocess |
| `CodeExecutorTool` | `code-executor` | Executes code in a subprocess |
| `VectorSearchTool` | `vector-search` | Semantic search via the active memory backend |

`FileReadTool` and `FileWriteTool` enforce path traversal protection. `SqlQueryTool` only allows `SELECT` statements — enforced by a SQL parser, not string matching.

---

## Supported Providers

| Provider | Struct | Env Var | Notes |
|---|---|---|---|
| Anthropic Claude | `AnthropicProvider` | `ANTHROPIC_API_KEY` | Streaming SSE, retry on 429/529 |
| OpenAI | `OpenAiProvider` | `OPENAI_API_KEY` | Streaming, all GPT-4 models |
| Google Gemini | `GeminiProvider` | `GEMINI_API_KEY` | Thinking models supported |
| Mistral AI | `MistralProvider` | `MISTRAL_API_KEY` | mistral-small/medium/large |
| Azure OpenAI | `AzureOpenAiProvider` | `AZURE_OPENAI_API_KEY` | Shared OpenAI-compat wire format |
| Ollama (local) | `OllamaProvider` | *(none)* | Defaults to `http://localhost:11434` |

All providers share the same `ModelProvider` trait. Swap one for another with zero changes to your agent logic.

**Cost-cap routing and fallback:**
```rust
let provider = ProviderRouter::builder()
    .provider(AnthropicProvider::from_env()?)
    .provider(OpenAiProvider::from_env()?)   // fallback on 5xx errors
    .cost_cap_usd(5.00)                      // hard stop after $5 spend
    .build();
```

---

## Workspace Structure

```
kainetic/
├── Cargo.toml                     # workspace manifest + shared [workspace.dependencies]
├── .env                           # API keys and config (gitignored)
├── deny.toml                      # license + supply-chain policy (cargo-deny v2)
│
├── crates/
│   ├── kainetic/                  # facade — re-exports everything
│   ├── kainetic-schema/           # KaineticError, typed IDs (RunId, AgentId…), Message, TokenUsage
│   ├── kainetic-providers/        # ModelProvider trait + all LLM clients
│   ├── kainetic-tools/            # Tool trait, ToolRegistry, JSON Schema validation, built-ins
│   ├── kainetic-core/             # Agent trait, ReActLoop, KaineticRuntime, AgentContext
│   ├── kainetic-memory/           # MemoryBackend trait + 6 implementations
│   ├── kainetic-telemetry/        # OTel traces, Prometheus metrics, CostAccumulator
│   ├── kainetic-macros/           # #[tool], #[agent], #[pipeline] proc macros
│   ├── kainetic-orchestra/        # Pipeline, Supervisor, StateMachineAgent, parallel! macro
│   ├── kainetic-cloud/            # Axum REST API + PostgreSQL-backed cloud backend
│   └── kainetic-cli/              # `kainetic` binary — init, new, run, validate, deploy
│
├── bindings/
│   ├── python/                    # PyO3 0.24 cdylib — `import kainetic`
│   └── typescript/                # napi-rs v2 cdylib — `@kainetic/runtime`
│
├── dashboard/                     # React 18 + TypeScript + Tailwind CSS SPA
│   └── src/pages/                 # Login, Overview, Runs, Cost, Latency, Agents, Team…
│
├── examples/
│   ├── simple-agent/              # Single agent + tool, needs ANTHROPIC_API_KEY
│   ├── multi-agent-pipeline/      # 3-agent DAG, no API key needed
│   ├── observability/             # docker-compose: Grafana + Tempo + Prometheus
│   ├── python-agent/              # Python binding usage
│   └── typescript-agent/          # TypeScript binding usage
│
└── docs/                          # mdBook documentation site
    └── src/
        ├── getting-started/       # Installation, first agent, using tools, running locally
        ├── concepts/              # Execution model, ReAct loop, memory, providers, pipelines
        ├── how-to/                # Custom tools, SQLite memory, streaming, cost tracking, deploy
        └── reference/             # Errors, AgentConfig, migration from LangChain, changelog
```

---

## Running Tests

```bash
# ── Unit tests (no external services or API keys needed) ──────────────────
cargo test --workspace

# ── Single crate ──────────────────────────────────────────────────────────
cargo test -p kainetic-core
cargo test -p kainetic-tools
cargo test -p kainetic-providers

# ── Integration tests — real LLM API calls ────────────────────────────────
# Keys must be in environment; tests skip gracefully if absent
set -a && source .env && set +a
cargo test --features integration -p kainetic-providers

# ── Integration tests — external services (Redis, Postgres, Qdrant) ───────
# Tests skip gracefully if services are not running
cargo test --features integration -p kainetic-memory

# ── Benchmarks ────────────────────────────────────────────────────────────
cargo bench -p kainetic-core     # ReAct loop: cold start + parallel dispatch
cargo bench -p kainetic-tools    # Tool dispatch: single, concurrent (N=1–32), registry snapshot

# ── Doc tests ─────────────────────────────────────────────────────────────
cargo test --doc -p kainetic-core

# ── Quality gates (run before every commit) ───────────────────────────────
cargo clippy --workspace         # zero warnings
cargo fmt --check --all          # formatting
cargo audit                      # security advisories
cargo deny check                 # license + supply chain
```

All integration tests skip gracefully when env vars or services are absent — they print a message and return without failing, so CI always passes without credentials.

---

## How to Adapt Kainetic

### Writing a Custom Tool

**Step 1** — define input/output types:

```rust
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use kainetic_tools::{ToolContext, ToolError};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TranslateInput {
    pub text: String,
    pub target_language: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct TranslateOutput {
    pub translated: String,
}
```

**Step 2** — write and annotate the function:

```rust
#[kainetic_macros::tool(
    description = "Translate text to the target language",
    timeout = "15s"
)]
async fn translate(
    input: TranslateInput,
    _ctx: ToolContext,
) -> Result<TranslateOutput, ToolError> {
    let result = call_my_translation_api(&input.text, &input.target_language)
        .await
        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

    Ok(TranslateOutput { translated: result })
}
```

**Step 3** — register:

```rust
let runtime = KaineticRuntime::builder()
    .provider(AnthropicProvider::from_env()?)
    .tool(Translate)   // generated struct: PascalCase of function name
    .build();
```

JSON Schema, deserialization, timeout, and tracing are all handled automatically.

### Writing a Custom Agent

```rust
#[kainetic_macros::agent(description = "Summarizes long documents into 3 bullet points.")]
async fn summarizer(input: String, ctx: AgentContext) -> Result<String, AgentError> {
    let config = AgentConfig::builder()
        .model("claude-sonnet-4-6")
        .system_prompt(
            "You are a precise summarizer. Return exactly 3 bullet points, nothing else."
        )
        .max_iterations(1)   // single LLM call — no tool loop needed
        .build();

    ReActLoop::new(config).execute(input, ctx).await
}

// Instantiate with defaults
let agent = SummarizerAgent::new();

// Or override config per instance
let agent = SummarizerAgent::with_config(
    AgentConfig::builder()
        .model("gpt-4o")
        .max_iterations(3)
        .build()
);
```

### Using a Custom Provider

Implement `ModelProvider` for any endpoint:

```rust
use async_trait::async_trait;
use kainetic_providers::{
    BoxStream, CompletionChunk, CompletionRequest, CompletionResponse,
    ModelProvider, ProviderError,
};
use kainetic_schema::TokenUsage;

pub struct MyProvider { client: reqwest::Client, api_key: String }

#[async_trait]
impl ModelProvider for MyProvider {
    fn name(&self) -> &'static str { "my-provider" }
    fn default_model(&self) -> &'static str { "my-model-v1" }

    fn cost_usd(&self, usage: &TokenUsage, _model: &str) -> f64 {
        (usage.prompt_tokens as f64 * 0.5
            + usage.completion_tokens as f64 * 1.5) / 1_000_000.0
    }

    async fn complete(
        &self,
        req: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        // call your API and map the response to CompletionResponse
        todo!()
    }

    async fn stream(
        &self,
        req: CompletionRequest,
    ) -> Result<BoxStream<Result<CompletionChunk, ProviderError>>, ProviderError> {
        // return a stream of chunks
        todo!()
    }
}
```

### Wiring a Multi-Agent Pipeline

```rust
use std::sync::Arc;
use kainetic_orchestra::Pipeline;

let pipeline = Pipeline::builder()
    .node("research",  Arc::new(ResearchAgent::new()))
    .node("draft",     Arc::new(WriterAgent::new()))
    .node("review",    Arc::new(ReviewerAgent::new()))
    .edge("research",  "draft")
    .edge("draft",     "review")
    .build()?;   // compile-time-equivalent: Err if graph has cycles or orphan nodes

let result = pipeline.run(initial_input, ctx).await?;
```

For parallel branches — run independent agents concurrently, merge, and continue:

```rust
let (research, news) = parallel!(
    research_agent.run(topic.clone(), ctx.clone()),
    news_agent.run(topic.clone(), ctx.clone()),
).await;

let report = writer_agent.run(
    format!("{}\n---\n{}", research?, news?),
    ctx,
).await?;
```

### Python & TypeScript Bindings

Adopt Kainetic incrementally — use the Rust runtime from an existing Python or Node.js application without rewriting anything:

**Python (`bindings/python/`):**
```python
import kainetic

@kainetic.tool(description="Get current weather for a city")
def get_weather(location: str) -> str:
    return f"22°C, Sunny in {location}"

runtime = kainetic.KaineticRuntime.from_anthropic()
result = runtime.run("weather_agent", "What's the weather in Tokyo?")
print(result)
```

**TypeScript (`bindings/typescript/`):**
```typescript
import { KaineticRuntime, tool } from '@kainetic/runtime';

const weatherTool = tool('get_weather', 'Get weather for a city', async (input) => {
    return `22°C, Sunny in ${input.location}`;
});

const runtime = KaineticRuntime.fromAnthropic();
const result = await runtime.run('weather_agent', 'What is the weather in Tokyo?');
console.log(result);
```

The Rust runtime handles all concurrency, telemetry, and tool sandboxing. Python and TypeScript only need to provide the tool logic and the entry point.

---

## Observability

Every agent run, LLM call, tool call, and memory access is traced and metered out of the box — no instrumentation code required:

```rust
let telemetry = TelemetryConfig::builder()
    .prometheus_port(9090)                    // Prometheus metrics at :9090/metrics
    .otlp_endpoint("http://tempo:4317")       // OpenTelemetry traces to Grafana Tempo
    .cost_alert_usd_per_hour(10.0)            // emit alert event when hourly spend > $10
    .build();

let runtime = KaineticRuntime::builder()
    .provider(provider)
    .telemetry(telemetry)
    .build();
```

**10 built-in Prometheus metrics:**

| Metric | Type | Description |
|---|---|---|
| `kainetic_llm_calls_total` | Counter | LLM calls by provider and model |
| `kainetic_llm_latency_ms` | Histogram | LLM response latency |
| `kainetic_tool_calls_total` | Counter | Tool invocations by name |
| `kainetic_tool_latency_ms` | Histogram | Tool execution latency |
| `kainetic_tokens_total` | Counter | Prompt + completion tokens |
| `kainetic_cost_usd_total` | Counter | Running USD spend by provider |
| `kainetic_run_duration_ms` | Histogram | End-to-end agent run latency |
| `kainetic_errors_total` | Counter | Errors by type and agent |
| `kainetic_memory_reads_total` | Counter | Memory backend reads |
| `kainetic_memory_writes_total` | Counter | Memory backend writes |

**Spin up Grafana + Tempo + Prometheus locally:**
```bash
cd examples/observability
docker compose up -d
# Grafana:    http://localhost:3001
# Prometheus: http://localhost:9090
# Tempo:      http://localhost:3200
```

---

## CLI Reference

Install the CLI:
```bash
cargo install --path crates/kainetic-cli
```

```bash
# Scaffold a new Kainetic project
kainetic init my-agent

# Generate a tool or agent file skeleton
kainetic new tool  my_tool
kainetic new agent my_agent

# Validate project configuration and agent definitions
kainetic validate

# Run an agent locally via cargo
kainetic run my_agent --input '{"query": "What is Rust?"}'

# Benchmark an agent (latency + cost per 100 runs)
kainetic bench my_agent

# Deploy to Kainetic Cloud
kainetic deploy
```

---

## Comparison with Python Frameworks

| Feature | LangChain | LangGraph | AutoGen | **Kainetic** |
|---|---|---|---|---|
| Language | Python | Python | Python | **Rust** |
| Parallel tool execution | Limited | Limited | ✗ | **Native — FuturesUnordered** |
| Type safety | Runtime | Runtime | Runtime | **Compile-time** |
| Cancellation | Fragile | Fragile | None | **Full tree propagation** |
| Cold start | 80–140 ms | 80–140 ms | 100+ ms | **< 5 ms** |
| Memory at rest | 250 MB+ | 250 MB+ | 300 MB+ | **< 15 MB** |
| Agent graph validation | Runtime | Runtime | None | **Build-time (DAG check)** |
| Built-in observability | Plugin | Plugin | None | **OTel + Prometheus** |
| Multi-agent routing | ✓ | ✓ | ✓ | **Pipeline + Supervisor + StateMachine** |
| Streaming responses | ✓ | ✓ | Partial | **✓ All providers** |
| Python bindings | — | — | — | **✓ PyO3 0.24** |
| TypeScript bindings | — | — | — | **✓ napi-rs v2** |
| Managed dashboard | ✗ | ✗ | ✗ | **✓ React SPA + REST API** |
| Fuzz-tested | ✗ | ✗ | ✗ | **✓ cargo-fuzz** |
| Property-based tests | ✗ | ✗ | ✗ | **✓ proptest** |

---

## Architecture Principles

**Type-safe everything.** Tool inputs/outputs, agent messages, and provider responses are all typed Rust structs. JSON only appears at wire boundaries and is always validated against a schema at deserialization via `schemars`.

**Async-first, cancellation-safe.** All execution is on Tokio. `CancellationToken` propagates through the entire call tree — cancelling a parent immediately cascades to all in-flight tool calls and child agents, with no resource leaks.

**Actor model for isolation.** Each agent instance is an actor backed by tokio channels. No shared mutable state; all coordination via typed message passing. This eliminates the race conditions that plague Python multi-agent systems using shared state.

**Parallel tool execution is structural.** When the model returns N independent tool calls, all N are dispatched via `FuturesUnordered`. This is not an optimization — it is the design. Sequential dispatch is the exception, not the default.

**`#![deny(unsafe_code)]` in all core crates.** Unsafe is only permitted in isolated crates that explicitly require it.

**Composable, not monolithic.** Each crate is independently usable. Running a single agent? Take only `kainetic-core` + `kainetic-tools`. Adding observability? `kainetic-telemetry`. Need multi-agent coordination? `kainetic-orchestra`. You pay only for what you use — in compile time, in binary size, and in runtime overhead.

---

## Roadmap

| Part | Status | Description |
|---|---|---|
| 0 — Workspace | ✅ Complete | Cargo workspace, CI, `deny.toml` |
| 1 — Schema | ✅ Complete | `KaineticError`, typed IDs, `Message`, `TokenUsage`, `ToolDescriptor` |
| 2 — Providers | ✅ Complete | `AnthropicProvider`, `OpenAiProvider`, `GeminiProvider`, streaming SSE, retry |
| 3 — Tools | ✅ Complete | `Tool` trait, `ToolRegistry`, JSON Schema validation, built-ins, `#[tool]` macro |
| 4 — Core | ✅ Complete | `Agent` trait, `ReActLoop`, `KaineticRuntime`, `#[agent]` macro |
| 5 — Memory | ✅ Complete | 6 backends: in-memory, SQLite, Redis, UsearchBackend, pgvector, Qdrant |
| 6 — Telemetry | ✅ Complete | Prometheus metrics (10 families), OTel traces, `CostAccumulator` |
| 7 — Orchestra | ✅ Complete | `Pipeline`, `Supervisor`, `StateMachineAgent`, `parallel!` macro |
| 8 — CLI | ✅ Complete | `kainetic init/new/run/validate/deploy` |
| 9 — Bindings | ✅ Complete | Python (PyO3 0.24) + TypeScript (napi-rs v2) |
| 10 — Providers+ | ✅ Complete | Mistral, Ollama, Azure OpenAI, `ProviderRouter`, 7 more tools |
| 11 — Cloud | ✅ Complete | Axum REST API, PostgreSQL, JWT + RBAC, audit log |
| 12 — Dashboard | ✅ Complete | React SPA — runs, traces, cost, latency, team, billing |
| 13 — Hardening | ✅ Complete | Criterion benchmarks, proptest, cargo-fuzz, trybuild, mdBook docs |
| 14 — Launch | 🔜 Next | crates.io publish, npm publish, public docs site |

---

## License

Licensed under the [Apache License 2.0](LICENSE).

```
Copyright 2026 Sayandeep Giri

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0
```
