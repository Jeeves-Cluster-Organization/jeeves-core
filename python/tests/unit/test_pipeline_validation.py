"""Tests for PipelineConfig.validate() cross-reference checks (Phase 2)."""

import pytest
from jeeves_core.protocols.types import (
    PipelineConfig, AgentConfig, RoutingRule, stage, Edge,
)
from jeeves_core.protocols.routing import eq, always, agent, not_exists, and_


class TestPipelineValidate:
    """PipelineConfig.validate() cross-reference checks."""

    def _simple_pipeline(self, **overrides):
        """Build a minimal valid pipeline for testing."""
        agents = overrides.pop("agents", [
            stage("understand", prompt_key="p.understand", default_next="respond"),
            stage("respond", prompt_key="p.respond"),
        ])
        return PipelineConfig(name="test", agents=agents, **overrides)

    def test_valid_pipeline_empty_errors(self):
        p = self._simple_pipeline()
        assert p.validate() == []

    def test_routing_target_typo(self):
        p = self._simple_pipeline(agents=[
            stage("a", prompt_key="p.a",
                  routing_rules=[RoutingRule(expr=always(), target="nonexistent")]),
            stage("b", prompt_key="p.b"),
        ])
        errors = p.validate()
        assert any("nonexistent" in e for e in errors)

    def test_default_next_typo(self):
        p = self._simple_pipeline(agents=[
            stage("a", prompt_key="p.a", default_next="typo"),
        ])
        errors = p.validate()
        assert any("typo" in e for e in errors)

    def test_error_next_typo(self):
        p = self._simple_pipeline(agents=[
            stage("a", prompt_key="p.a", error_next="missing"),
            stage("b", prompt_key="p.b"),
        ])
        errors = p.validate()
        assert any("missing" in e for e in errors)

    def test_gate_with_has_llm(self):
        p = self._simple_pipeline(agents=[
            AgentConfig(name="router", node_kind="Gate", has_llm=True, default_next="a"),
            stage("a", prompt_key="p.a"),
        ])
        errors = p.validate()
        assert any("Gate node must have has_llm=False" in e for e in errors)

    def test_gate_with_current_scope_top_level(self):
        p = self._simple_pipeline(agents=[
            AgentConfig(
                name="router", node_kind="Gate",
                routing_rules=[RoutingRule(expr=eq("field", "val"), target="a")],
                default_next="a",
            ),
            stage("a", prompt_key="p.a"),
        ])
        errors = p.validate()
        assert any("Current scope" in e for e in errors)

    def test_gate_with_current_scope_nested(self):
        p = self._simple_pipeline(agents=[
            AgentConfig(
                name="router", node_kind="Gate",
                routing_rules=[
                    RoutingRule(
                        expr=and_(eq("x", 1), eq(agent("a", "y"), 2)),
                        target="a",
                    )
                ],
                default_next="a",
            ),
            stage("a", prompt_key="p.a"),
        ])
        errors = p.validate()
        # Should detect the Current scope in the first sub-expr of And
        assert any("Current scope" in e for e in errors)

    def test_gate_with_agent_scope_ok(self):
        """Gate using agent() scope should be valid."""
        p = self._simple_pipeline(agents=[
            AgentConfig(
                name="router", node_kind="Gate",
                routing_rules=[
                    RoutingRule(expr=eq(agent("a", "done"), True), target="a")
                ],
                default_next="a",
            ),
            stage("a", prompt_key="p.a"),
        ])
        errors = p.validate()
        assert not any("Current scope" in e for e in errors)

    def test_chain_passes(self):
        """chain() auto-wired pipeline should pass validation."""
        p = PipelineConfig.chain("test", [
            stage("a", prompt_key="p.a"),
            stage("b", prompt_key="p.b"),
            stage("c", prompt_key="p.c"),
        ])
        assert p.validate() == []

    def test_graph_passes(self):
        """graph() with valid edges should pass validation."""
        p = PipelineConfig.graph(
            "test",
            stages={
                "a": stage("a", prompt_key="p.a"),
                "b": stage("b", prompt_key="p.b"),
            },
            edges=[Edge(source="a", target="b")],
        )
        assert p.validate() == []

    def test_terminal_stage_valid(self):
        """Stage with no default_next and no routing_rules is valid (Temporal pattern)."""
        p = self._simple_pipeline(agents=[
            stage("a", prompt_key="p.a", default_next="b"),
            stage("b", prompt_key="p.b"),
        ])
        assert p.validate() == []
