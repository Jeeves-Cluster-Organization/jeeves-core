"""Tests for Agent — allowed_tools pre-execution enforcement (W12)."""

import pytest
from unittest.mock import AsyncMock, MagicMock
from dataclasses import dataclass
from typing import Any, Dict, List, Optional, Set

from jeeves_core.runtime.agents import Agent, _NullLogger
from jeeves_core.protocols.types import AgentConfig, AgentContext


def _make_agent(
    name: str = "test_agent",
    has_tools: bool = True,
    allowed_tools: Optional[Set[str]] = None,
    tool_dispatch: Optional[str] = None,
) -> Agent:
    """Create an Agent with a mock tool executor."""
    config = AgentConfig(
        name=name,
        has_tools=has_tools,
        allowed_tools=allowed_tools,
        tool_dispatch=tool_dispatch,
        tool_source_agent="source",
        tool_name_field="tool",
        tool_params_field="params",
    )

    mock_executor = AsyncMock()
    mock_executor.execute = AsyncMock(return_value={"status": "success", "data": "result"})

    agent = Agent(
        config=config,
        logger=_NullLogger(),
        tools=mock_executor,
    )
    return agent


def _make_context(**kwargs) -> AgentContext:
    """Create a minimal AgentContext."""
    defaults = {
        "envelope_id": "test-env",
        "request_id": "req-1",
        "user_id": "user-1",
        "session_id": "sess-1",
        "raw_input": "test input",
        "outputs": {},
        "metadata": {},
    }
    defaults.update(kwargs)
    return AgentContext(**defaults)


class TestAllowedToolsEnforcement:
    """Test tool execution — allowed_tools enforcement is in Rust kernel."""

    @pytest.mark.asyncio
    async def test_allowed_tools_none_unrestricted(self):
        """When allowed_tools is None, all tools should be permitted."""
        agent = _make_agent(allowed_tools=None)
        context = _make_context()

        output_with_tools = {
            "tool_calls": [
                {"name": "any_tool", "params": {}},
            ],
        }

        result = await agent._execute_tools(context, output_with_tools)

        # Tool should have been executed
        agent.tools.execute.assert_called_once_with("any_tool", {})

    @pytest.mark.asyncio
    async def test_allowed_tool_is_executed(self):
        """Tool in allowed_tools should be executed normally."""
        agent = _make_agent(allowed_tools={"search", "summarize"})
        context = _make_context()

        output_with_tools = {
            "tool_calls": [
                {"name": "search", "params": {"query": "test"}},
            ],
        }

        result = await agent._execute_tools(context, output_with_tools)

        # Tool should have been executed
        agent.tools.execute.assert_called_once()
        assert "result" in result["tool_results"][0]

    @pytest.mark.asyncio
    async def test_dispatch_tool_allowed(self):
        """_dispatch_tool should allow authorized tools."""
        agent = _make_agent(
            allowed_tools={"search"},
            tool_dispatch="auto",
        )
        context = _make_context(
            outputs={"source": {"tool": "search", "params": {"q": "test"}}},
        )

        result = await agent._dispatch_tool(context)

        assert result["status"] == "success"
        agent.tools.execute.assert_called_once()


# =============================================================================
# Lifecycle Hook Tests
# =============================================================================

class TestPreProcessHooks:
    """Test pre_process hook invocation in Agent.process()."""

    @pytest.mark.asyncio
    async def test_pre_process_hook_is_called(self):
        """Pre-process hook receives (context, agent) and its return replaces context."""
        hook_calls = []

        def my_hook(context, agent):
            hook_calls.append(("pre", context.raw_input, agent.name))
            # Modify context metadata
            context.metadata["enriched"] = True
            return context

        agent = _make_agent_with_mock(pre_process=[my_hook])
        context = _make_context(raw_input="hello")

        await agent.process(context)

        assert len(hook_calls) == 1
        assert hook_calls[0] == ("pre", "hello", "test_agent")

    @pytest.mark.asyncio
    async def test_pre_process_hooks_chain(self):
        """Multiple pre-process hooks run in order, each receiving prior result."""
        order = []

        def hook_a(context, agent):
            order.append("a")
            context.metadata["a"] = True
            return context

        def hook_b(context, agent):
            order.append("b")
            assert context.metadata.get("a") is True  # sees hook_a's mutation
            context.metadata["b"] = True
            return context

        agent = _make_agent_with_mock(pre_process=[hook_a, hook_b])
        context = _make_context()

        await agent.process(context)

        assert order == ["a", "b"]

    @pytest.mark.asyncio
    async def test_async_pre_process_hook(self):
        """Async pre-process hooks are awaited."""
        called = []

        async def async_hook(context, agent):
            called.append(True)
            context.metadata["async_ran"] = True
            return context

        agent = _make_agent_with_mock(pre_process=[async_hook])
        context = _make_context()

        await agent.process(context)

        assert len(called) == 1


class TestPostProcessHooks:
    """Test post_process hook invocation in Agent.process()."""

    @pytest.mark.asyncio
    async def test_post_process_hook_receives_output(self):
        """Post-process hook receives (context, output, agent)."""
        hook_calls = []

        def my_hook(context, output, agent):
            hook_calls.append(list(output.keys()))
            return context

        mock_output = {"intent": "greeting", "confidence": 0.95}
        agent = _make_agent_with_mock(
            post_process=[my_hook],
            mock_output=mock_output,
        )
        context = _make_context()

        await agent.process(context)

        assert len(hook_calls) == 1
        assert "intent" in hook_calls[0]
        assert "confidence" in hook_calls[0]

    @pytest.mark.asyncio
    async def test_post_process_can_modify_metadata(self):
        """Post-process hook can enrich metadata returned from process()."""

        def enrich_meta(context, output, agent):
            context.metadata["post_processed"] = True
            context.metadata["output_keys"] = list(output.keys())
            return context

        agent = _make_agent_with_mock(
            post_process=[enrich_meta],
            mock_output={"response": "hi"},
        )
        context = _make_context()

        output, metadata = await agent.process(context)

        assert metadata["post_processed"] is True
        assert metadata["output_keys"] == ["response"]

    @pytest.mark.asyncio
    async def test_hook_exception_propagates(self):
        """Hook exceptions propagate — no silent swallowing."""

        def bad_hook(context, output, agent):
            raise RuntimeError("hook failed")

        agent = _make_agent_with_mock(post_process=[bad_hook])
        context = _make_context()

        with pytest.raises(RuntimeError, match="hook failed"):
            await agent.process(context)


# =============================================================================
# Hook test helpers
# =============================================================================

def _make_agent_with_mock(
    name: str = "test_agent",
    pre_process=None,
    post_process=None,
    mock_output=None,
) -> Agent:
    """Create an Agent with a mock_handler (bypasses LLM)."""
    config = AgentConfig(
        name=name,
        has_tools=False,
    )

    output = mock_output or {"result": "ok"}

    agent = Agent(
        config=config,
        logger=_NullLogger(),
        use_mock=True,
        mock_handler=lambda ctx: output,
        pre_process=pre_process or [],
        post_process=post_process or [],
    )
    return agent
