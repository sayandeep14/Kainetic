# Deploy to Kainetic Cloud

## Prerequisites

1. A Kainetic Cloud account (sign up at https://kainetic.dev)
2. An API key from the Team Settings page
3. A `kainetic.toml` in your project root (created by `kainetic init`)

## Deployment

```bash
export KAINETIC_API_KEY=kk_your_api_key_here
kainetic deploy
```

This command:
1. Reads `kainetic.toml` for agent metadata.
2. Exchanges your API key for a short-lived JWT.
3. Registers (or updates) the agent in the Kainetic Cloud registry.
4. Prints the agent's deployment URL.

## `kainetic.toml` format

```toml
[agent]
name        = "my-assistant"
version     = "1.0.0"
description = "A helpful assistant with web search."
```

## After deployment

Your agent is immediately accessible at the URL printed by `kainetic deploy`. You can view runs, traces, and cost in the [Kainetic Cloud dashboard](https://app.kainetic.dev).

## CI/CD integration

Add to your GitHub Actions workflow:

```yaml
- name: Deploy to Kainetic Cloud
  env:
    KAINETIC_API_KEY: ${{ secrets.KAINETIC_API_KEY }}
  run: kainetic deploy
```
