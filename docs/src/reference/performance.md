# Performance Targets

These are the contractual performance targets from SPEC §17. Each target is measured with reproducible Criterion benchmarks (see `crates/kainetic-core/benches/` and `crates/kainetic-tools/benches/`).

## Targets

| Metric | Target | Python baseline | Notes |
|---|---|---|---|
| Cold start | < 5 ms | 60–140 ms | Time from `KaineticRuntime::run()` call to first LLM request issued |
| Memory at rest (single agent) | < 15 MB | 180–250 MB | RSS of a minimal agent binary waiting for input |
| P99 latency overhead | < 50 ms | +800 ms | Time added by Kainetic vs calling the provider directly |
| 100 concurrent runs | < 300 MB | ~8 GB | Total RSS under load |

## Running the benchmarks

```bash
# All benchmarks
cargo bench --workspace

# Specific: ReActLoop cold start + tool dispatch
cargo bench -p kainetic-core

# Specific: ToolRegistry throughput
cargo bench -p kainetic-tools

# Generate flamegraph (requires cargo-flamegraph)
cargo flamegraph --bench react_loop -- react_loop_cold_start
```

## Cold start definition

"Cold start" measures the time from creating an `AgentContext` to receiving the first byte of the provider response — i.e., the Kainetic framework overhead, not the LLM round-trip.

The Criterion benchmark `react_loop_cold_start` uses an in-memory mock provider that returns immediately. This isolates the framework path from network latency.

## Parallel tool speedup

The `tool_dispatch/parallel/{n}` benchmarks show that with N independent instant tools, parallel dispatch produces timing bounded by `max(tool_latency)` rather than `sum(tool_latency)`. At N=8, parallel dispatch is approximately 8× faster than serial.

For real I/O-bound tools (HTTP requests, database queries), the speedup is proportional to the ratio of wait time to CPU time — typically 5–20× in practice.
