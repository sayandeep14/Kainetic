"""
Integration tests for the Python bindings.

Run with::

    maturin develop
    pytest bindings/python/tests/

These tests require a built extension module (`maturin develop`).
They do NOT make real LLM API calls; they validate that the FFI layer
correctly bridges Python ↔ Rust.
"""

import pytest


def test_import():
    """Extension module must be importable after `maturin develop`."""
    try:
        import kainetic  # noqa: F401
    except ImportError as e:
        pytest.skip(f"kainetic extension not built: {e}")


def test_anthropic_provider_requires_key(monkeypatch):
    """AnthropicProvider.from_env() raises if ANTHROPIC_API_KEY is unset."""
    try:
        from kainetic import AnthropicProvider
    except ImportError:
        pytest.skip("kainetic extension not built")

    monkeypatch.delenv("ANTHROPIC_API_KEY", raising=False)
    with pytest.raises(Exception):
        AnthropicProvider.from_env()


def test_anthropic_provider_with_key():
    """AnthropicProvider.with_key() succeeds without env vars."""
    try:
        from kainetic import AnthropicProvider
    except ImportError:
        pytest.skip("kainetic extension not built")

    p = AnthropicProvider.with_key("test-key")
    assert repr(p) == "AnthropicProvider()"


def test_tool_decorator():
    """@tool decorator returns a Tool handle with the correct repr."""
    try:
        from kainetic import tool
    except ImportError:
        pytest.skip("kainetic extension not built")

    @tool(name="echo", description="Echoes input.")
    async def echo(text: str) -> str:
        return text

    assert "echo" in repr(echo)


def test_agent_decorator():
    """@agent decorator returns an Agent handle with the correct repr."""
    try:
        from kainetic import agent
    except ImportError:
        pytest.skip("kainetic extension not built")

    @agent(name="mirror", description="Mirrors input.")
    async def mirror(input: str) -> str:
        return input

    assert "mirror" in repr(mirror)


def test_runtime_construction():
    """KaineticRuntime can be constructed from a provider."""
    try:
        from kainetic import AnthropicProvider, KaineticRuntime
    except ImportError:
        pytest.skip("kainetic extension not built")

    provider = AnthropicProvider.with_key("test-key")
    rt = KaineticRuntime(provider=provider)
    assert repr(rt) == "KaineticRuntime()"


def test_agent_context_cancel():
    """AgentContext.cancel() and is_cancelled() work correctly."""
    try:
        from kainetic import AgentContext
    except ImportError:
        pytest.skip("kainetic extension not built")

    ctx = AgentContext()
    assert not ctx.is_cancelled()
    ctx.cancel()
    assert ctx.is_cancelled()
