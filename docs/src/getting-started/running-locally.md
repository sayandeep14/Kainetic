# Running Locally

## Using the CLI

After `kainetic init my-agent`:

```bash
cd my-agent

# Run with inline JSON input
ANTHROPIC_API_KEY=sk-... kainetic run assistant --input '"What is 2+2?"'

# Run with a session ID to persist conversation history
kainetic run assistant \
  --input '"Remember my name is Alice."' \
  --session my-session-id

# Validate your agent configuration without running
kainetic validate
```

## Using `cargo run` directly

For projects not using the CLI:

```bash
ANTHROPIC_API_KEY=sk-... cargo run --example simple-agent
```

## Observability locally

Spin up a local Grafana + Tempo + Prometheus stack:

```bash
docker compose -f examples/observability/docker-compose.yml up -d
```

Then enable OTLP in your agent:

```rust
use kainetic_telemetry::TelemetryConfig;

TelemetryConfig::otlp("http://localhost:4317")
    .service_name("my-agent")
    .attach(runtime.subscribe_events())
    .await?;
```

Open [http://localhost:3000](http://localhost:3000) (Grafana) to see traces and metrics.

## Development tips

- Set `RUST_LOG=kainetic=debug` for detailed per-request logs.
- Use `AgentConfig::builder().sequential_tools()` during debugging to see tool calls in order.
- Use `AgentConfig::builder().timeout(Duration::from_secs(10))` to fail fast in CI.
- `cargo test --features integration` runs real-API tests (needs keys in env).
