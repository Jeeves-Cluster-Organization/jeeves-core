"""Tests for to_kernel_dict() serialization and enum parity (Phase 4b)."""

import pytest
from jeeves_core.protocols.types import (
    AgentConfig, PipelineConfig, JoinStrategy, RoutingRule, EdgeLimit,
    TerminalReason, stage,
)
from jeeves_core.protocols.routing import eq, always


class TestNodeKindSerialization:
    def test_node_kind_agent_omitted(self):
        """Default node_kind='Agent' should NOT appear in kernel dict."""
        d = stage("a", prompt_key="p.a").to_kernel_dict()
        assert "node_kind" not in d

    def test_node_kind_gate_present(self):
        d = AgentConfig(name="router", node_kind="Gate").to_kernel_dict()
        assert d["node_kind"] == "Gate"

    def test_node_kind_fork_present(self):
        d = AgentConfig(name="splitter", node_kind="Fork").to_kernel_dict()
        assert d["node_kind"] == "Fork"
        assert "join_strategy" in d


class TestJoinStrategySerialization:
    def test_join_strategy_all(self):
        d = AgentConfig(name="f", node_kind="Fork", join_strategy=JoinStrategy.ALL).to_kernel_dict()
        assert d["join_strategy"] == "WaitAll"

    def test_join_strategy_any(self):
        d = AgentConfig(name="f", node_kind="Fork", join_strategy=JoinStrategy.ANY).to_kernel_dict()
        assert d["join_strategy"] == "WaitFirst"


class TestOutputKeySerialization:
    def test_output_key_same_as_name_omitted(self):
        d = stage("foo").to_kernel_dict()
        assert "output_key" not in d

    def test_output_key_differs_present(self):
        d = stage("foo", output_key="custom").to_kernel_dict()
        assert d["output_key"] == "custom"


class TestOptionalFieldsSerialization:
    def test_optional_fields_omitted(self):
        d = stage("a").to_kernel_dict()
        assert "default_next" not in d
        assert "error_next" not in d
        assert "max_visits" not in d

    def test_optional_fields_present_when_set(self):
        d = stage("a", default_next="b", error_next="c", max_visits=5).to_kernel_dict()
        assert d["default_next"] == "b"
        assert d["error_next"] == "c"
        assert d["max_visits"] == 5


class TestPipelineRoundTrip:
    def test_full_pipeline(self):
        p = PipelineConfig.chain("test", [
            stage("a", prompt_key="p.a"),
            stage("b", prompt_key="p.b"),
        ], edge_limits=[EdgeLimit(from_stage="a", to_stage="b", max_count=3)])
        d = p.to_kernel_dict()
        assert d["name"] == "test"
        assert len(d["stages"]) == 2
        assert d["stages"][0]["name"] == "a"
        assert d["stages"][1]["name"] == "b"
        assert len(d["edge_limits"]) == 1
        assert d["edge_limits"][0]["max_count"] == 3

    def test_edge_limits_always_serialized(self):
        p = PipelineConfig(name="test", agents=[stage("a")])
        d = p.to_kernel_dict()
        assert "edge_limits" in d
        assert d["edge_limits"] == []


class TestTerminalReasonVariants:
    def test_all_variants_valid(self):
        expected = {
            "COMPLETED", "BREAK_REQUESTED",
            "MAX_ITERATIONS_EXCEEDED", "MAX_LLM_CALLS_EXCEEDED",
            "MAX_AGENT_HOPS_EXCEEDED", "USER_CANCELLED",
            "TOOL_FAILED_FATALLY", "LLM_FAILED_FATALLY",
            "POLICY_VIOLATION", "MAX_STAGE_VISITS_EXCEEDED",
        }
        actual = {r.value for r in TerminalReason}
        assert expected.issubset(actual), f"Missing: {expected - actual}"
