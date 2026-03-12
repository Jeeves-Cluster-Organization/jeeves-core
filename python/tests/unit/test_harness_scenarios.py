"""TestPipeline scenario tests — routing, interrupts, parallel, schema, bounds.

Exercises MockKernelClient + TestPipeline through real pipeline configs
using the routing expression builders.
"""

import pytest
from jeeves_core.protocols.types import PipelineConfig, RoutingRule, Edge, stage
from jeeves_core.protocols.routing import eq, not_, agent, always
from jeeves_core.testing import TestPipeline


# =============================================================================
# 2a: Routing scenarios
# =============================================================================


class TestConditionalRouting:
    """Conditional routing: eq('intent', 'search') → search_agent."""

    @pytest.mark.asyncio
    async def test_routes_to_search_on_intent(self):
        config = PipelineConfig.graph(
            "conditional",
            {
                "understand": stage("understand", mock_handler=lambda ctx: {"intent": "search"}),
                "search_agent": stage("search_agent", mock_handler=lambda ctx: {"results": ["a"]}),
                "chat_agent": stage("chat_agent", mock_handler=lambda ctx: {"reply": "hi"}),
            },
            [
                Edge(source="understand", target="search_agent", when=eq("intent", "search")),
                Edge(source="understand", target="chat_agent", when=eq("intent", "chat")),
            ],
            max_iterations=10,
        )
        result = await TestPipeline(config).run("find something")
        assert result.terminated
        assert result.terminal_reason == "COMPLETED"
        assert "search_agent" in result.outputs
        assert "chat_agent" not in result.outputs

    @pytest.mark.asyncio
    async def test_routes_to_chat_on_intent(self):
        config = PipelineConfig.graph(
            "conditional",
            {
                "understand": stage("understand", mock_handler=lambda ctx: {"intent": "chat"}),
                "search_agent": stage("search_agent", mock_handler=lambda ctx: {"results": ["a"]}),
                "chat_agent": stage("chat_agent", mock_handler=lambda ctx: {"reply": "hi"}),
            },
            [
                Edge(source="understand", target="search_agent", when=eq("intent", "search")),
                Edge(source="understand", target="chat_agent", when=eq("intent", "chat")),
            ],
            max_iterations=10,
        )
        result = await TestPipeline(config).run("hello")
        assert result.terminated
        assert "chat_agent" in result.outputs
        assert "search_agent" not in result.outputs


class TestCrossAgentRouting:
    """Cross-agent field reference: agent('understand', 'topic') → route."""

    @pytest.mark.asyncio
    async def test_cross_agent_ref_routes(self):
        config = PipelineConfig.graph(
            "cross_agent",
            {
                "understand": stage("understand", mock_handler=lambda ctx: {"topic": "time"}),
                "think_tools": stage("think_tools", mock_handler=lambda ctx: {"time": "3pm"}),
                "think_general": stage("think_general", mock_handler=lambda ctx: {"answer": "42"}),
            },
            [
                Edge(source="understand", target="think_tools", when=eq(agent("understand", "topic"), "time")),
                Edge(source="understand", target="think_general"),  # unconditional = default_next
            ],
            max_iterations=10,
        )
        result = await TestPipeline(config).run()
        assert "think_tools" in result.outputs
        assert "think_general" not in result.outputs


class TestErrorRouting:
    """Error routing: error_next on agent failure."""

    @pytest.mark.asyncio
    async def test_error_next_on_failure(self):
        def _failing_handler(ctx):
            raise RuntimeError("agent crashed")

        config = PipelineConfig.graph(
            "error_route",
            {
                "risky_agent": stage("risky_agent", mock_handler=_failing_handler, error_next="fallback"),
                "fallback": stage("fallback", mock_handler=lambda ctx: {"recovery": True}),
            },
            [],
            max_iterations=10,
        )
        result = await TestPipeline(config).run()
        assert result.terminated
        assert "fallback" in result.outputs
        assert result.outputs["fallback"]["recovery"] is True


class TestTemporalLoop:
    """Temporal pattern: route to CONTINUE until condition met."""

    @pytest.mark.asyncio
    async def test_loop_until_completed(self):
        call_count = 0

        def _looping_handler(ctx):
            nonlocal call_count
            call_count += 1
            return {"completed": call_count >= 3, "iteration": call_count}

        config = PipelineConfig.graph(
            "temporal_loop",
            {
                "worker": stage("worker", mock_handler=_looping_handler, max_visits=5),
            },
            [
                Edge(source="worker", target="worker", when=not_(eq("completed", True))),
            ],
            max_iterations=10,
        )
        result = await TestPipeline(config).run()
        assert result.terminated
        assert result.terminal_reason == "COMPLETED"
        assert call_count == 3


# =============================================================================
# 2b: Interrupt injection
# =============================================================================


class TestInterruptInjection:
    """WAIT_INTERRUPT after a stage completes."""

    @pytest.mark.asyncio
    async def test_interrupt_after_stage(self):
        from jeeves_core.testing.mock_kernel import MockKernelClient
        from jeeves_core.pipeline_worker import PipelineWorker
        from jeeves_core.runtime.agents import Agent, _NullLogger
        from jeeves_core.testing.test_pipeline import _MockPromptRegistry
        from uuid import uuid4

        config = PipelineConfig.chain(
            "interrupt_test",
            [
                stage("understand", mock_handler=lambda ctx: {"intent": "ask"}),
                stage("respond", mock_handler=lambda ctx: {"reply": "done"}),
            ],
            max_iterations=10,
        )

        agents = {}
        logger = _NullLogger()
        for ac in config.agents:
            agents[ac.name] = Agent(
                config=ac, logger=logger, llm=None, tools=None,
                prompt_registry=_MockPromptRegistry(),
                mock_handler=ac.mock_handler,
            )

        kernel = MockKernelClient()
        kernel.inject_interrupt("understand", {"kind": "HUMAN_REVIEW", "message": "please confirm"})

        worker = PipelineWorker(kernel_client=kernel, agents=agents, logger=logger)
        process_id = f"test-{uuid4().hex[:8]}"
        pipeline_dict = config.to_kernel_dict()
        result = await worker.execute(
            process_id=process_id,
            pipeline_config=pipeline_dict,
            user_id="test-user",
            session_id="test-session",
            raw_input="test",
        )

        assert result.interrupted
        assert result.interrupt_kind == "HUMAN_REVIEW"
        # respond should NOT have run
        assert "respond" not in result.outputs


# =============================================================================
# 2c: Parallel fan-out
# =============================================================================


class TestForkFanOut:
    """Fork fan-out: Fork node evaluates ALL routing rules, dispatches parallel branches."""

    @pytest.mark.asyncio
    async def test_fork_dispatches_parallel_branches(self):
        config = PipelineConfig(
            name="fork_test",
            agents=[
                stage("understand", mock_handler=lambda ctx: {"intent": "multi"}, default_next="router"),
                stage("router", node_kind="Fork",
                      routing_rules=[
                          RoutingRule(expr=always(), target="think_a"),
                          RoutingRule(expr=always(), target="think_b"),
                      ],
                      default_next="respond"),
                stage("think_a", mock_handler=lambda ctx: {"a": 1}),
                stage("think_b", mock_handler=lambda ctx: {"b": 2}),
                stage("respond", mock_handler=lambda ctx: {"done": True}),
            ],
            max_iterations=10,
            max_agent_hops=10,
        )
        result = await TestPipeline(config).run()
        assert result.terminated
        assert "think_a" in result.outputs
        assert "think_b" in result.outputs
        assert result.outputs["think_a"]["a"] == 1
        assert result.outputs["think_b"]["b"] == 2
        assert "respond" in result.outputs
        assert result.outputs["respond"]["done"] is True


# =============================================================================
# 2d: Schema validation
# =============================================================================


class TestSchemaValidation:
    """Output schema validation: missing required fields triggers error_next."""

    @pytest.mark.asyncio
    async def test_missing_required_fields_routes_to_error_next(self):
        config = PipelineConfig.graph(
            "schema_test",
            {
                "strict_agent": stage(
                    "strict_agent",
                    mock_handler=lambda ctx: {"partial": True},
                    output_schema={"required": ["intent", "confidence"]},
                    error_next="error_handler",
                ),
                "error_handler": stage("error_handler", mock_handler=lambda ctx: {"handled": True}),
            },
            [],
            max_iterations=10,
        )
        result = await TestPipeline(config).run()
        assert result.terminated
        assert "error_handler" in result.outputs

    @pytest.mark.asyncio
    async def test_valid_schema_passes(self):
        config = PipelineConfig.chain(
            "schema_pass",
            [
                stage(
                    "good_agent",
                    mock_handler=lambda ctx: {"intent": "chat", "confidence": 0.9},
                    output_schema={"required": ["intent", "confidence"]},
                ),
            ],
            max_iterations=10,
        )
        result = await TestPipeline(config).run()
        assert result.terminated
        assert result.terminal_reason == "COMPLETED"
        assert "good_agent" in result.outputs


# =============================================================================
# Bounds enforcement
# =============================================================================


class TestBoundsEnforcement:
    """Bounds: max_visits, max_iterations terminate correctly."""

    @pytest.mark.asyncio
    async def test_max_visits_terminates(self):
        config = PipelineConfig.graph(
            "max_visits_test",
            {
                "looper": stage("looper", mock_handler=lambda ctx: {"x": 1}, max_visits=2),
            },
            [
                Edge(source="looper", target="looper", when=always()),
            ],
            max_iterations=10,
        )
        result = await TestPipeline(config).run()
        assert result.terminated
        assert result.terminal_reason == "MAX_STAGE_VISITS_EXCEEDED"

    @pytest.mark.asyncio
    async def test_max_iterations_terminates(self):
        config = PipelineConfig.graph(
            "max_iter_test",
            {
                "looper": stage("looper", mock_handler=lambda ctx: {"x": 1}),
            },
            [
                Edge(source="looper", target="looper", when=always()),
            ],
            max_iterations=3,
        )
        result = await TestPipeline(config).run()
        assert result.terminated
        assert result.terminal_reason == "MAX_ITERATIONS_EXCEEDED"
