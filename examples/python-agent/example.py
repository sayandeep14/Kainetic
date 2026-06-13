"""
Python binding example — simple echo agent.

Build the extension first:
    cd bindings/python
    maturin develop

Then run:
    python examples/python-agent/example.py
"""
import os
import sys

try:
    from kainetic import AnthropicProvider, KaineticRuntime, agent, tool
except ImportError:
    print("kainetic extension not built. Run `maturin develop` in bindings/python/.")
    sys.exit(1)


# ── Define a tool ──────────────────────────────────────────────────────────────

@tool(name="word_count", description="Counts words in a string.")
async def word_count(text: str) -> dict:
    return {"count": len(text.split())}


# ── Define an agent ────────────────────────────────────────────────────────────

@agent(name="echo", description="Echoes the input string back to the caller.")
async def echo_agent(input: str) -> str:
    return f"[echo] {input}"


# ── Wire up the runtime ────────────────────────────────────────────────────────

def main() -> None:
    api_key = os.environ.get("ANTHROPIC_API_KEY", "demo-key")
    provider = AnthropicProvider.with_key(api_key)

    runtime = KaineticRuntime(provider=provider, tools=[word_count])
    result = runtime.run(echo_agent, "Hello from Python!")
    print(result)


if __name__ == "__main__":
    main()
