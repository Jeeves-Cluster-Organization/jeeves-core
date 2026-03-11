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
    """Test that allowed_tools blocks unauthorized tool calls before execution."""

    @pytest.mark.asyncio
    async def test_denied_tool_returns_error(self):
        """Tool not in allowed_tools should be blocked with error, not executed."""
        agent = _make_agent(allowed_tools={"search", "summarize"})
        context = _make_context()

        # Simulate LLM output with tool_calls including a disallowed tool
        output_with_tools = {
            "tool_calls": [
                {"name": "delete_all", "params": {}},
            ],
        }

        result = await agent._execute_tools(context, output_with_tools)

        # Should have error result for blocked tool
        assert len(result["tool_results"]) == 1
        assert "error" in result["tool_results"][0]
        assert "not allowed" in result["tool_results"][0]["error"]

        # Tool executor should NOT have been called
        agent.tools.execute.assert_not_called()

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
    async def test_dispatch_tool_denied(self):
        """_dispatch_tool should block unauthorized tools."""
        agent = _make_agent(
            allowed_tools={"search"},
            tool_dispatch="auto",
        )
        context = _make_context(
            outputs={"source": {"tool": "delete_all", "params": {}}},
        )

        result = await agent._dispatch_tool(context)

        assert result["status"] == "error"
        assert "not allowed" in result["error"]
        agent.tools.execute.assert_not_called()

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
