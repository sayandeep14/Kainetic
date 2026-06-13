"""
Kainetic — Python bindings for the Kainetic agentic AI runtime.

Usage::

    from kainetic import KaineticRuntime, AnthropicProvider, tool, agent

    @tool(name="greet", description="Returns a greeting.")
    async def greet(name: str) -> str:
        return f"Hello, {name}!"

    @agent(name="hello", description="A greeting agent.")
    async def hello_agent(input: str) -> str:
        return f"The agent says: {input}"

    provider = AnthropicProvider.from_env()
    runtime = KaineticRuntime(provider=provider, tools=[greet])
    result = runtime.run(hello_agent, "world")
    print(result)
"""

from ._kainetic import (  # noqa: F401
    KaineticRuntime,
    AgentContext,
    AnthropicProvider,
    OpenAiProvider,
    tool,
    agent,
    Agent,
    Tool,
)

__all__ = [
    "KaineticRuntime",
    "AgentContext",
    "AnthropicProvider",
    "OpenAiProvider",
    "tool",
    "agent",
    "Agent",
    "Tool",
]
