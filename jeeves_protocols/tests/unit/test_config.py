"""Unit tests for configuration types.

Tests AgentConfig, PipelineConfig, ContextBounds, and related types.
"""

import pytest


class TestAgentConfig:
    """Tests for AgentConfig dataclass."""

    def test_agent_config_defaults(self):
        """Test AgentConfig with default values."""
        from jeeves_protocols import AgentConfig

        config = AgentConfig(name="test_agent")

        assert config.name == "test_agent"
        assert config.stage_order == 0
        assert config.has_llm is False
        assert config.has_tools is False

    def test_agent_config_with_values(self):
        """Test AgentConfig with specific values."""
        from jeeves_protocols import AgentConfig, ToolAccess

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
        from jeeves_protocols import ContextBounds

        bounds = ContextBounds()

        assert bounds.max_input_tokens == 4096
        assert bounds.max_output_tokens == 2048
        assert bounds.max_context_tokens == 16384
        assert bounds.reserved_tokens == 512

    def test_context_bounds_with_values(self):
        """Test ContextBounds with specific values."""
        from jeeves_protocols import ContextBounds

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
        from jeeves_protocols import PipelineConfig, AgentConfig

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
        from jeeves_protocols import PipelineConfig, AgentConfig

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


class TestCoreConfig:
    """Tests for CoreConfig dataclass."""

    def test_core_config_creation(self):
        """Test creating CoreConfig."""
        from jeeves_protocols import CoreConfig

        config = CoreConfig(
            max_iterations=5,
            max_llm_calls=20,
        )

        assert config.max_iterations == 5
        assert config.max_llm_calls == 20


class TestRoutingRule:
    """Tests for RoutingRule dataclass."""

    def test_routing_rule_creation(self):
        """Test creating RoutingRule."""
        from jeeves_protocols import RoutingRule

        rule = RoutingRule(
            condition="status",
            value="success",
            target="next_agent",
        )

        assert rule.condition == "status"
        assert rule.value == "success"
        assert rule.target == "next_agent"
