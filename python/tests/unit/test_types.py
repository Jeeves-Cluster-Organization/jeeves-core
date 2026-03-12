"""Tests for types.py — chain()/graph() replace() cleanup and new retrieval types."""

import pytest
from dataclasses import FrozenInstanceError

from jeeves_core.protocols.types import (
    AgentConfig,
    PipelineConfig,
    Edge,
    stage,
    RetrievedContext,
    ClassificationResult,
)
from jeeves_core.protocols.routing import eq


# =============================================================================
# Phase 0: chain()/graph() use dataclasses.replace — new fields auto-forwarded
# =============================================================================


class TestChainUsesReplace:
    """Verify chain() auto-forwards all AgentConfig fields via replace()."""

    def test_chain_preserves_all_existing_fields(self):
        """Ensure existing fields (temperature, allowed_tools, etc.) survive replace()."""
        agents = [
            stage("x", prompt_key="x_prompt", temperature=0.7, allowed_tools={"search"}),
            stage("y", prompt_key="y_prompt"),
        ]
        pipeline = PipelineConfig.chain("test", agents)
        wired = {a.name: a for a in pipeline.agents}

        assert wired["x"].temperature == 0.7
        assert wired["x"].allowed_tools == {"search"}
        assert wired["x"].has_llm is True

    def test_chain_wires_default_next(self):
        """chain() wires default_next sequentially."""
        agents = [stage("a"), stage("b"), stage("c")]
        pipeline = PipelineConfig.chain("test", agents)
        wired = {a.name: a for a in pipeline.agents}

        assert wired["a"].default_next == "b"
        assert wired["b"].default_next == "c"
        assert wired["c"].default_next is None

    def test_chain_respects_explicit_default_next(self):
        """If agent already has default_next set, chain() keeps it."""
        agents = [
            stage("a", default_next="c"),
            stage("b"),
            stage("c"),
        ]
        pipeline = PipelineConfig.chain("test", agents)
        wired = {a.name: a for a in pipeline.agents}

        assert wired["a"].default_next == "c"  # explicit, not overwritten

    def test_chain_wires_error_next(self):
        """chain() applies global error_next to non-terminal stages."""
        agents = [stage("a"), stage("b"), stage("c")]
        pipeline = PipelineConfig.chain("test", agents, error_next="error_handler")
        wired = {a.name: a for a in pipeline.agents}

        assert wired["a"].error_next == "error_handler"
        assert wired["b"].error_next == "error_handler"
        assert wired["c"].error_next is None  # terminal stage


class TestGraphUsesReplace:
    """Verify graph() auto-forwards all AgentConfig fields via replace()."""

    def test_graph_preserves_all_existing_fields(self):
        """Ensure existing fields survive replace()."""
        stages = {
            "x": stage("x", prompt_key="x_prompt", temperature=0.5, max_visits=3),
        }
        pipeline = PipelineConfig.graph("test", stages, [])
        wired = pipeline.agents[0]

        assert wired.temperature == 0.5
        assert wired.max_visits == 3

    def test_graph_wires_routing(self):
        """graph() wires routing rules from conditional edges."""
        stages = {
            "a": stage("a"),
            "b": stage("b"),
            "c": stage("c"),
        }
        edges = [
            Edge(source="a", target="b", when=eq("intent", "search")),
            Edge(source="a", target="c"),  # unconditional = default_next
        ]
        pipeline = PipelineConfig.graph("test", stages, edges)
        wired = {a.name: a for a in pipeline.agents}

        assert len(wired["a"].routing_rules) == 1
        assert wired["a"].routing_rules[0].target == "b"
        assert wired["a"].default_next == "c"
