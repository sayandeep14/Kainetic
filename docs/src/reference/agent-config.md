# AgentConfig Reference

`AgentConfig` controls the runtime behaviour of a single agent. Build one with `AgentConfig::builder()`.

## Fields

| Field | Type | Default | Description |
|---|---|---|---|
| `model` | `String` | `"claude-sonnet-4-6"` | Model identifier sent to the provider |
| `system_prompt` | `Option<SystemPrompt>` | `None` | System prompt prepended to every run |
| `max_iterations` | `u32` | `20` | Maximum ReAct loop iterations before `MaxIterationsExceeded` |
| `max_tokens` | `Option<u32>` | `None` | Maximum tokens per LLM call; provider default if `None` |
| `temperature` | `Option<f32>` | `None` | Sampling temperature; provider default if `None` |
| `parallel_tools` | `bool` | `true` | Dispatch independent tool calls concurrently |
| `timeout` | `Option<Duration>` | `None` (120 s) | Wall-clock budget for the entire run |

## Builder API

```rust
use kainetic_core::{AgentConfig, SystemPrompt};
use std::time::Duration;

let config = AgentConfig::builder()
    .model("claude-opus-4-8")
    .system_prompt("You are a concise, expert Rust programmer.")
    .max_iterations(10)
    .max_tokens(4096)
    .temperature(0.3)
    .sequential_tools()          // disables parallel dispatch
    .timeout(Duration::from_secs(30))
    .build();
```

## SystemPrompt

`SystemPrompt` supports `{{variable}}` interpolation:

```rust
let prompt = SystemPrompt::new("You are a {{role}} working on {{project}}.");
let rendered = prompt.render(&HashMap::from([
    ("role".into(), "senior engineer".into()),
    ("project".into(), "Kainetic".into()),
]));
// "You are a senior engineer working on Kainetic."
```

Unknown keys are left as-is: `{{unknown}}` remains in the output.

## Per-agent vs per-run config

`AgentConfig` is **per-agent-type** (set once on the struct). To vary configuration per-run (e.g., per-user temperature), create multiple agent instances with different configs.

```rust
let fast = ResearchAgent::with_config(
    AgentConfig::builder().temperature(0.0).build()
);
let creative = ResearchAgent::with_config(
    AgentConfig::builder().temperature(0.9).build()
);
```
