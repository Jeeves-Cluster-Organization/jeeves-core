"""Unit tests for configuration types.

Tests AgentConfig, PipelineConfig, ContextBounds, and related types.
"""

import pytest


class TestAgentConfig:
    """Tests for AgentConfig dataclass."""

    def test_agent_config_defaults(self):
        """Test AgentConfig with default values."""
        from protocols import AgentConfig

        config = AgentConfig(name="test_agent")

        assert config.name == "test_agent"
        assert config.stage_order == 0
        assert config.has_llm is False
        assert config.has_tools is False

    def test_agent_config_with_values(self):
        """Test AgentConfig with specific values."""
        from protocols import AgentConfig, ToolAccess

        config = AgentConfig(
            name="perception",
            stage_order=1,
            has_llm=True,
            has_tools=True,
            tool_access=ToolAccess.READ,
            model_role="analyzer",
            temperature=0.1,
            max_tokens=2000,
        )

        assert config.name == "perception"
        assert config.stage_order == 1
        assert config.has_llm is True
        assert config.tool_access == ToolAccess.READ


class TestContextBounds:
    """Tests for ContextBounds dataclass."""

    def test_context_bounds_defaults(self):
        """Test ContextBounds with default values."""
        from protocols import ContextBounds

        bounds = ContextBounds()

        assert bounds.max_input_tokens == 4096
        assert bounds.max_output_tokens == 2048
        assert bounds.max_context_tokens == 16384
        assert bounds.reserved_tokens == 512

    def test_context_bounds_with_values(self):
        """Test ContextBounds with specific values."""
        from protocols import ContextBounds

        bounds = ContextBounds(
            max_input_tokens=8192,
            max_output_tokens=4096,
            max_context_tokens=32768,
        )

        assert bounds.max_input_tokens == 8192
        assert bounds.max_output_tokens == 4096


class TestPipelineConfig:
    """Tests for PipelineConfig dataclass."""

    def test_pipeline_config_creation(self):
        """Test creating PipelineConfig."""
        from protocols import PipelineConfig, AgentConfig

        config = PipelineConfig(
            name="seven_agent_pipeline",
            agents=[
                AgentConfig(name="perception", stage_order=1),
                AgentConfig(name="intent", stage_order=2),
                AgentConfig(name="plan", stage_order=3),
            ],
        )

        assert config.name == "seven_agent_pipeline"
        assert len(config.agents) == 3

    def test_pipeline_get_stage_order(self):
        """Test getting stage order from pipeline."""
        from protocols import PipelineConfig, AgentConfig

        config = PipelineConfig(
            name="test",
            agents=[
                AgentConfig(name="third", stage_order=3),
                AgentConfig(name="first", stage_order=1),
                AgentConfig(name="second", stage_order=2),
            ],
        )

        order = config.get_stage_order()
        assert order == ["first", "second", "third"]


class TestExecutionConfig:
    """Tests for ExecutionConfig dataclass."""

    def test_core_config_creation(self):
        """Test creating ExecutionConfig."""
        from protocols import ExecutionConfig

        config = ExecutionConfig(
            max_iterations=5,
            max_llm_calls=20,
        )

        assert config.max_iterations == 5
        assert config.max_llm_calls == 20


class TestRoutingRule:
    """Tests for RoutingRule dataclass."""

    def test_routing_rule_creation(self):
        """Test creating RoutingRule."""
        from protocols import RoutingRule

        rule = RoutingRule(
            condition="status",
            value="success",
            target="next_agent",
        )

        assert rule.condition == "status"
        assert rule.value == "success"
        assert rule.target == "next_agent"


class TestJoinStrategy:
    """Tests for JoinStrategy enum."""

    def test_join_strategy_values(self):
        """Test JoinStrategy enum values."""
        from protocols import JoinStrategy

        assert JoinStrategy.ALL.value == "all"
        assert JoinStrategy.ANY.value == "any"


class TestEdgeLimit:
    """Tests for EdgeLimit dataclass."""

    def test_edge_limit_creation(self):
        """Test creating EdgeLimit."""
        from protocols import EdgeLimit

        limit = EdgeLimit(
            from_stage="critic",
            to_stage="planner",
            max_count=3,
        )

        assert limit.from_stage == "critic"
        assert limit.to_stage == "planner"
        assert limit.max_count == 3


class TestAgentConfigParallelExecution:
    """Tests for AgentConfig parallel execution fields."""

    def test_agent_config_dependencies(self):
        """Test AgentConfig with dependency fields."""
        from protocols import AgentConfig, JoinStrategy

        config = AgentConfig(
            name="executor",
            stage_order=3,
            requires=["planner", "validator"],
            after=["perception"],
            join_strategy=JoinStrategy.ALL,
        )

        assert config.requires == ["planner", "validator"]
        assert config.after == ["perception"]
        assert config.join_strategy == JoinStrategy.ALL

    def test_agent_config_join_any(self):
        """Test AgentConfig with JoinStrategy.ANY."""
        from protocols import AgentConfig, JoinStrategy

        config = AgentConfig(
            name="fallback",
            requires=["primary", "secondary"],
            join_strategy=JoinStrategy.ANY,
        )

        assert config.join_strategy == JoinStrategy.ANY


class TestPipelineConfigEdgeLimits:
    """Tests for PipelineConfig edge limit functionality."""

    def test_pipeline_with_edge_limits(self):
        """Test PipelineConfig with edge limits for cyclic routing."""
        from protocols import PipelineConfig, AgentConfig, EdgeLimit

        config = PipelineConfig(
            name="cyclic_pipeline",
            agents=[
                AgentConfig(name="planner", stage_order=1),
                AgentConfig(name="executor", stage_order=2),
                AgentConfig(name="critic", stage_order=3),
            ],
            edge_limits=[
                EdgeLimit(from_stage="critic", to_stage="planner", max_count=3),
            ],
        )

        assert len(config.edge_limits) == 1
        assert config.get_edge_limit("critic", "planner") == 3
        assert config.get_edge_limit("executor", "critic") == 0

    def test_pipeline_get_ready_stages(self):
        """Test getting stages ready for execution."""
        from protocols import PipelineConfig, AgentConfig, JoinStrategy

        config = PipelineConfig(
            name="parallel_pipeline",
            agents=[
                AgentConfig(name="perception", stage_order=1),
                AgentConfig(name="intent", stage_order=2, requires=["perception"]),
                AgentConfig(name="plan", stage_order=3, requires=["perception", "intent"]),
            ],
        )

        # Initially only perception is ready
        ready = config.get_ready_stages({})
        assert ready == ["perception"]

        # After perception, intent is ready
        ready = config.get_ready_stages({"perception": True})
        assert ready == ["intent"]

        # After both, plan is ready
        ready = config.get_ready_stages({"perception": True, "intent": True})
        assert ready == ["plan"]

    def test_pipeline_get_ready_stages_join_any(self):
        """Test ready stages with JoinStrategy.ANY."""
        from protocols import PipelineConfig, AgentConfig, JoinStrategy

        config = PipelineConfig(
            name="any_join_pipeline",
            agents=[
                AgentConfig(name="source_a", stage_order=1),
                AgentConfig(name="source_b", stage_order=2),
                AgentConfig(
                    name="merger",
                    stage_order=3,
                    requires=["source_a", "source_b"],
                    join_strategy=JoinStrategy.ANY,
                ),
            ],
        )

        # Merger is ready when ANY of its requires is done
        ready = config.get_ready_stages({"source_a": True})
        assert "merger" in ready

    def test_pipeline_agent_review_resume_stage(self):
        """Test agent_review_resume_stage field."""
        from protocols import PipelineConfig

        config = PipelineConfig(
            name="review_pipeline",
            agent_review_resume_stage="planner",
        )

        assert config.agent_review_resume_stage == "planner"
