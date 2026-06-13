# Installation

## Requirements

- Rust 1.80+ (`rustup update stable`)
- An Anthropic or OpenAI API key (for the built-in providers)

## Add to `Cargo.toml`

```toml
[dependencies]
kainetic       = "0.1"             # umbrella re-export crate
tokio          = { version = "1", features = ["full"] }

# Provider of your choice
kainetic-providers = "0.1"

# Optional: telemetry
kainetic-telemetry = "0.1"
```

Or add individual crates for a leaner dependency footprint:

```toml
kainetic-core      = "0.1"   # Agent trait, ReActLoop, KaineticRuntime
kainetic-tools     = "0.1"   # Tool trait, ToolRegistry, built-ins
kainetic-providers = "0.1"   # AnthropicProvider, OpenAiProvider, …
kainetic-macros    = "0.1"   # #[tool], #[agent], #[pipeline]
```

## Environment variables

| Variable | Required | Description |
|---|---|---|
| `ANTHROPIC_API_KEY` | For Anthropic provider | Claude model API key |
| `OPENAI_API_KEY` | For OpenAI provider | GPT model API key |
| `KAINETIC_API_KEY` | For `kainetic deploy` | Kainetic Cloud API key |
| `BRAVE_API_KEY` | For `WebSearchTool` | Brave Search API key |

## Install the CLI

```bash
cargo install kainetic-cli
```

Verify:

```bash
kainetic --version
# kainetic 0.1.0
```

## Quick-start a new project

```bash
kainetic init my-agent
cd my-agent
ANTHROPIC_API_KEY=sk-... kainetic run assistant --input '"Hello!"'
```
