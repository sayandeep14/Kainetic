# Observability

Kainetic emits structured telemetry via OpenTelemetry traces and Prometheus metrics. One call to `TelemetryConfig::attach` wires everything together.

## Quick setup

```rust
use kainetic_telemetry::TelemetryConfig;

// OTLP traces → Grafana Tempo / Jaeger
TelemetryConfig::otlp("http://localhost:4317")
    .service_name("my-agent")
    .attach(runtime.subscribe_events())
    .await?;

// Prometheus metrics endpoint on :9090
TelemetryConfig::prometheus(9090)
    .attach(runtime.subscribe_events())
    .await?;
```

## Traces

Every agent run produces a root span `agent.run` with child spans:
- `llm.complete` — latency and token usage for each LLM call
- `tool.call` — latency and input/output for each tool invocation
- `memory.read` / `memory.write` — memory backend access

Standard span attributes follow OpenTelemetry semantic conventions:
- `gen_ai.system` — provider name (e.g. `"anthropic"`)
- `gen_ai.request.model` — model identifier
- `gen_ai.usage.prompt_tokens` / `gen_ai.usage.completion_tokens`
- `kainetic.run_id` — unique run identifier
- `kainetic.agent_name` — agent name from `Agent::name()`

## Prometheus metrics

| Metric | Type | Description |
|---|---|---|
| `kainetic_runs_total` | Counter | Total runs by agent and status |
| `kainetic_run_duration_seconds` | Histogram | P50/P95/P99 latency per agent |
| `kainetic_llm_tokens_total` | Counter | Token usage by model and direction |
| `kainetic_tool_calls_total` | Counter | Tool call count by name and status |
| `kainetic_tool_duration_seconds` | Histogram | Per-tool latency |
| `kainetic_cost_usd_total` | Counter | Accumulated cost by agent and model |
| `kainetic_memory_operations_total` | Counter | Memory read/write counts |
| `kainetic_active_runs` | Gauge | Currently executing runs |

## Cost tracking

`CostAccumulator` tracks spend per run and per hour:

```rust
TelemetryConfig::prometheus(9090)
    .cost_alert_per_run_usd(0.50)       // alert if a single run costs > $0.50
    .cost_alert_hourly_usd(10.0)        // alert if hourly spend > $10
    .on_alert(|alert| eprintln!("Cost alert: {alert:?}"))
    .attach(runtime.subscribe_events())
    .await?;
```

## Local development

```bash
docker compose -f examples/observability/docker-compose.yml up -d
```

Opens:
- Grafana at http://localhost:3000 (user: admin, password: admin)
- Prometheus at http://localhost:9090
- Tempo at http://localhost:3200
