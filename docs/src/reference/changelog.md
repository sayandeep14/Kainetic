# Changelog

## 0.1.0 (unreleased)

Initial release of the Kainetic runtime.

### Core (`kainetic-core`)

- `Agent` trait with `BoxFuture` pattern for object safety
- `AgentContext` — cheap-clone execution context carrying provider, tools, memory, and cancellation token
- `AgentConfig` builder with model selection, system prompt, iteration/timeout limits, parallel/sequential tool dispatch
- `ReActLoop` — Reason → Act → Observe execution engine with `FuturesUnordered` parallel tool dispatch
- `KaineticRuntime` builder API
- `AgentEvent` broadcast channel for telemetry and monitoring
- `#[agent]` proc macro generating `Agent` impl from an async function
- Criterion benchmarks: cold start (< 5 ms target), parallel vs serial tool dispatch
- Proptest property tests: loop termination, `max_iterations` exact cap, cancellation
- Recovery tests: provider 500, context-length exceeded, unknown tool, malformed schema input, timeout

### Schema (`kainetic-schema`)

- `KaineticError` with variants for all subsystems, `#[non_exhaustive]`
- Typed ID newtypes: `RunId`, `SessionId`, `AgentId`, `ToolId`
- `Message`, `MessageRole`, `MessageContent` — provider-agnostic conversation representation
- `TokenUsage`, `CostEstimate`, `ToolDescriptor`

### Tools (`kainetic-tools`)

- `Tool` trait with `BoxFuture` pattern
- `ToolRegistry` — `DashMap`-backed, JSON Schema validation before every call
- Built-in: `CurrentDatetimeTool`, `HttpRequestTool`, `WebSearchTool`, `WebFetchTool`, `FileReadTool`, `FileWriteTool`, `SqlQueryTool`
- Feature-gated: `ShellTool` (`shell`), `CodeExecutorTool` (`code-executor`), `VectorSearchTool` (`vector-search`)
- `#[tool]` proc macro with `timeout = "Xs"` support
- Criterion benchmarks: single call, concurrent calls (N=1–32), `list()` snapshot
- `cargo-fuzz` targets: `fuzz_input_validation`, `fuzz_tool_call`

### Providers (`kainetic-providers`)

- `ModelProvider` trait
- `AnthropicProvider`, `OpenAiProvider`, `GeminiProvider`, `MistralProvider`, `OllamaProvider`, `AzureOpenAiProvider`
- `ProviderRouter` — fallback and cost-cap routing
- Retry on 429/529 with exponential backoff + jitter
- Cost estimation (hard-coded per-token prices)

### Memory (`kainetic-memory`)

- `MemoryBackend` trait
- `InMemoryBackend`, `WorkingMemory`, `EpisodicMemory`
- `SqliteBackend` (rusqlite + r2d2), `RedisBackend`, `UsearchBackend` (HNSW)
- `PgVectorBackend` (`pgvector` feature), `QdrantBackend` (`qdrant` feature)

### Telemetry (`kainetic-telemetry`)

- `TelemetryConfig` builder with OTLP and Prometheus output
- `TelemetryEventHandler` — subscribes to `AgentEvent`, updates 10 Prometheus metric families
- `CostAccumulator` with per-run and hourly alert thresholds
- Feature-gated OTLP export via `opentelemetry-otlp`

### Orchestration (`kainetic-orchestra`)

- `Pipeline` — validated DAG of agents with typed edges and conditional routing
- `Supervisor` — worker pool with `RoundRobin`, `LeastLoaded`, `Random`, `ContentBased` routing
- `StateMachineAgent` — durable state machine with checkpoint/resume
- `parallel!` macro wrapping `tokio::join!`

### CLI (`kainetic-cli`)

- `kainetic init <name>` — project scaffolding
- `kainetic new agent/tool <name>` — file generation
- `kainetic run <agent>` — invoke via `cargo run`
- `kainetic validate` — configuration validation
- `kainetic deploy` — deploys to Kainetic Cloud

### Cloud Backend (`kainetic-cloud`)

- Full Axum HTTP server with PostgreSQL backing
- Auth: argon2 API key hashing + HS256 JWT
- RBAC: viewer / developer / admin
- APIs: `/v1/ingest/spans`, `/v1/agents`, `/v1/runs`, `/v1/metrics`, team/API-key management, audit log

### Dashboard (`dashboard/`)

- React + TypeScript + Tailwind SPA
- Pages: Login, Overview, Runs, RunDetail, Cost, Latency, Agents, Evaluations, Prompts, Team, Billing

### Language Bindings

- Python: `kainetic` extension module via PyO3 0.24
- TypeScript/Node: `@kainetic/runtime` via napi-rs v2

### Hardening (Part 13)

- All `.unwrap()` / `.expect()` in production code replaced with proper error propagation
- Mutex poison errors in `UsearchBackend` convert to `MemoryError::Backend`
- PyO3 upgraded to 0.24 (fixes RUSTSEC-2025-0020, RUSTSEC-2026-0177)
- `deny.toml` updated with `cargo-deny` v2 advisory format
- Macro error messages point to the exact token (`new_spanned`) instead of `call_site()`
- Improved `#[tool]` / `#[agent]` errors: `async fn` check, unknown attribute detection
- 5 compile-fail tests via `trybuild` for macro misuse scenarios
- Documentation site at `docs/` (mdBook) covering Getting Started, Concepts, How-To, Reference
