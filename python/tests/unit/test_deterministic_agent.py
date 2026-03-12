"""Tests for DeterministicAgent base class and agent_class bridge (Phase 3)."""

import asyncio
import pytest
from jeeves_core.runtime.deterministic import DeterministicAgent
from jeeves_core.protocols.types import AgentConfig, AgentContext, PipelineConfig, stage
from jeeves_core.runtime.agents import Agent, PipelineRunner, _NullLogger
from jeeves_core.testing.helpers import make_agent_context


# --- Test DeterministicAgent subclasses ---

class CounterAgent(DeterministicAgent):
    async def execute(self, context: AgentContext) -> dict:
        return {"count": 42}


class AsyncDelayAgent(DeterministicAgent):
    async def execute(self, context: AgentContext) -> dict:
        await asyncio.sleep(0.01)
        return {"delayed": True}


class NotADeterministicAgent:
    async def execute(self, context):
        return {}


# --- Tests ---

class TestDeterministicAgentExecution:
    """DeterministicAgent subclass executes via mock_handler bridge."""

    @pytest.mark.asyncio
    async def test_sync_execute(self):
        """DeterministicAgent subclass produces output."""
        config = stage("counter", agent_class=CounterAgent)
        pipeline = PipelineConfig(name="test", agents=[config])
        runner = PipelineRunner(config=pipeline, logger=_NullLogger())
        agent = runner.get_agent("counter")
        ctx = make_agent_context("test")
        output, meta = await agent.process(ctx)
        assert output == {"count": 42}

    @pytest.mark.asyncio
    async def test_async_execute(self):
        """Async DeterministicAgent.execute() works (verifies async mock_handler fix)."""
        config = stage("delayed", agent_class=AsyncDelayAgent)
        pipeline = PipelineConfig(name="test", agents=[config])
        runner = PipelineRunner(config=pipeline, logger=_NullLogger())
        agent = runner.get_agent("delayed")
        ctx = make_agent_context("test")
        output, meta = await agent.process(ctx)
        assert output == {"delayed": True}


class TestAgentClassConfig:
    """AgentConfig.agent_class validation."""

    def test_agent_class_none_falls_through(self):
        """agent_class=None uses normal Agent path."""
        config = stage("normal", mock_handler=lambda ctx: {"ok": True})
        pipeline = PipelineConfig(name="test", agents=[config])
        runner = PipelineRunner(config=pipeline, logger=_NullLogger(), use_mock=True)
        agent = runner.get_agent("normal")
        assert agent.mock_handler is not None

    def test_agent_class_and_has_llm_raises(self):
        with pytest.raises(ValueError, match="agent_class and has_llm are mutually exclusive"):
            AgentConfig(name="bad", agent_class=CounterAgent, has_llm=True)

    def test_agent_class_and_mock_handler_raises(self):
        with pytest.raises(ValueError, match="agent_class and mock_handler are mutually exclusive"):
            AgentConfig(name="bad", agent_class=CounterAgent, mock_handler=lambda ctx: {})

    def test_non_deterministic_subclass_raises(self):
        """agent_class that isn't a DeterministicAgent subclass raises TypeError."""
        config = stage("bad")
        # Bypass stage() validation by setting agent_class directly
        config.agent_class = NotADeterministicAgent
        pipeline = PipelineConfig(name="test", agents=[config])
        with pytest.raises(TypeError, match="DeterministicAgent subclass"):
            PipelineRunner(config=pipeline, logger=_NullLogger())


class TestHooksWithDeterministicAgent:
    """Pre/post hooks still fire around DeterministicAgent.execute()."""

    @pytest.mark.asyncio
    async def test_pre_post_hooks_fire(self):
        hook_log = []

        def pre(ctx, agent):
            hook_log.append("pre")
            return ctx

        def post(ctx, output, agent):
            hook_log.append("post")
            return ctx

        config = stage("hooked", agent_class=CounterAgent,
                       pre_process=pre, post_process=post)
        pipeline = PipelineConfig(name="test", agents=[config])
        runner = PipelineRunner(config=pipeline, logger=_NullLogger())
        agent = runner.get_agent("hooked")
        ctx = make_agent_context("test")
        await agent.process(ctx)
        assert hook_log == ["pre", "post"]


class TestAsyncMockHandlerFix:
    """Regression test: async mock_handler returns awaited result, not coroutine."""

    @pytest.mark.asyncio
    async def test_async_mock_handler(self):
        async def my_handler(ctx):
            return {"async": True}

        config = stage("mock_async", mock_handler=my_handler)
        pipeline = PipelineConfig(name="test", agents=[config])
        runner = PipelineRunner(config=pipeline, logger=_NullLogger(), use_mock=True)
        agent = runner.get_agent("mock_async")
        ctx = make_agent_context("test")
        output, meta = await agent.process(ctx)
        assert output == {"async": True}
