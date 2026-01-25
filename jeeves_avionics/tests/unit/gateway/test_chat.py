"""Unit tests for chat router functions.

Tests focus on helper functions and event classification logic.
Integration tests for the full endpoint are in integration tests.
"""

import pytest
from jeeves_avionics.gateway.routers.chat import (
    _classify_event_category,
    EVENT_CATEGORY_MAP,
)
from jeeves_protocols.events import EventCategory


class TestEventCategoryClassification:
    """Test event type → category classification."""

    def test_exact_match_agent_lifecycle(self):
        """Test exact match for agent lifecycle events."""
        assert _classify_event_category("agent.started") == EventCategory.AGENT_LIFECYCLE
        assert _classify_event_category("agent.completed") == EventCategory.AGENT_LIFECYCLE
        assert _classify_event_category("agent.perception") == EventCategory.AGENT_LIFECYCLE
        assert _classify_event_category("agent.intent") == EventCategory.AGENT_LIFECYCLE
        assert _classify_event_category("agent.planner") == EventCategory.AGENT_LIFECYCLE
        assert _classify_event_category("agent.executor") == EventCategory.AGENT_LIFECYCLE
        assert _classify_event_category("agent.synthesizer") == EventCategory.AGENT_LIFECYCLE
        assert _classify_event_category("agent.integration") == EventCategory.AGENT_LIFECYCLE

    def test_exact_match_critic_decision(self):
        """Test exact match for critic decision events."""
        assert _classify_event_category("agent.critic") == EventCategory.CRITIC_DECISION
        assert _classify_event_category("critic.decision") == EventCategory.CRITIC_DECISION

    def test_exact_match_tool_execution(self):
        """Test exact match for tool execution events."""
        assert _classify_event_category("tool.started") == EventCategory.TOOL_EXECUTION
        assert _classify_event_category("tool.completed") == EventCategory.TOOL_EXECUTION
        assert _classify_event_category("tool.failed") == EventCategory.TOOL_EXECUTION

    def test_exact_match_pipeline_flow(self):
        """Test exact match for pipeline flow events."""
        assert _classify_event_category("orchestrator.started") == EventCategory.PIPELINE_FLOW
        assert _classify_event_category("orchestrator.completed") == EventCategory.PIPELINE_FLOW
        assert _classify_event_category("flow.started") == EventCategory.PIPELINE_FLOW
        assert _classify_event_category("flow.completed") == EventCategory.PIPELINE_FLOW

    def test_exact_match_stage_transition(self):
        """Test exact match for stage transition events."""
        assert _classify_event_category("stage.transition") == EventCategory.STAGE_TRANSITION
        assert _classify_event_category("stage.completed") == EventCategory.STAGE_TRANSITION

    def test_legacy_prefix_match_agent_lifecycle(self):
        """Test backward compatibility with legacy event type strings (prefix match)."""
        # Legacy event types without "agent." prefix
        assert _classify_event_category("perception.started") == EventCategory.AGENT_LIFECYCLE
        assert _classify_event_category("intent.complete") == EventCategory.AGENT_LIFECYCLE
        assert _classify_event_category("planner.thinking") == EventCategory.AGENT_LIFECYCLE
        assert _classify_event_category("executor.running") == EventCategory.AGENT_LIFECYCLE
        assert _classify_event_category("synthesizer.complete") == EventCategory.AGENT_LIFECYCLE
        assert _classify_event_category("integration.done") == EventCategory.AGENT_LIFECYCLE

    def test_legacy_prefix_match_critic(self):
        """Test backward compatibility for legacy critic events."""
        assert _classify_event_category("critic.analyzing") == EventCategory.CRITIC_DECISION

    def test_legacy_prefix_match_tool(self):
        """Test backward compatibility for legacy tool events."""
        assert _classify_event_category("tool.execute") == EventCategory.TOOL_EXECUTION

    def test_legacy_prefix_match_orchestrator(self):
        """Test backward compatibility for legacy orchestrator events."""
        assert _classify_event_category("orchestrator.running") == EventCategory.PIPELINE_FLOW

    def test_legacy_prefix_match_flow(self):
        """Test backward compatibility for legacy flow events."""
        assert _classify_event_category("flow.processing") == EventCategory.PIPELINE_FLOW

    def test_legacy_prefix_match_stage(self):
        """Test backward compatibility for legacy stage events."""
        assert _classify_event_category("stage.executing") == EventCategory.STAGE_TRANSITION

    def test_unknown_event_type_returns_domain_event(self):
        """Test that unknown event types default to DOMAIN_EVENT category."""
        assert _classify_event_category("unknown.event") == EventCategory.DOMAIN_EVENT
        assert _classify_event_category("random.type") == EventCategory.DOMAIN_EVENT
        assert _classify_event_category("custom.event") == EventCategory.DOMAIN_EVENT

    def test_empty_string_returns_domain_event(self):
        """Test that empty string defaults to DOMAIN_EVENT category."""
        assert _classify_event_category("") == EventCategory.DOMAIN_EVENT

    def test_event_category_map_completeness(self):
        """Test that EVENT_CATEGORY_MAP contains all expected entries."""
        # Verify map has entries for all documented event types
        expected_prefixes = [
            "agent.started", "agent.completed", "agent.perception", "agent.intent",
            "agent.planner", "agent.executor", "agent.synthesizer", "agent.integration",
            "agent.critic", "critic.decision",
            "tool.started", "tool.completed", "tool.failed",
            "orchestrator.started", "orchestrator.completed",
            "flow.started", "flow.completed",
            "stage.transition", "stage.completed",
        ]

        for event_type in expected_prefixes:
            assert event_type in EVENT_CATEGORY_MAP, f"{event_type} missing from EVENT_CATEGORY_MAP"

    def test_exact_match_takes_precedence_over_prefix(self):
        """Test that exact matches are preferred over prefix matches."""
        # If "tool.started" is in the map, it should use exact match
        # rather than falling back to prefix matching
        result = _classify_event_category("tool.started")
        assert result == EventCategory.TOOL_EXECUTION

        # Verify it's using the exact match path by checking it's in the map
        assert "tool.started" in EVENT_CATEGORY_MAP


class TestEventCategoryMapConfiguration:
    """Test the EVENT_CATEGORY_MAP configuration."""

    def test_map_structure(self):
        """Test that EVENT_CATEGORY_MAP has correct structure."""
        assert isinstance(EVENT_CATEGORY_MAP, dict)
        assert len(EVENT_CATEGORY_MAP) > 0

        # All keys should be strings
        for key in EVENT_CATEGORY_MAP.keys():
            assert isinstance(key, str), f"Key {key} is not a string"

        # All values should be valid EventCategory names (strings)
        valid_categories = {
            "AGENT_LIFECYCLE",
            "CRITIC_DECISION",
            "TOOL_EXECUTION",
            "PIPELINE_FLOW",
            "STAGE_TRANSITION",
            "DOMAIN_EVENT",
        }

        for value in EVENT_CATEGORY_MAP.values():
            assert isinstance(value, str), f"Value {value} is not a string"
            assert value in valid_categories, f"Value {value} is not a valid EventCategory name"

    def test_no_duplicate_mappings(self):
        """Test that there are no conflicting mappings."""
        # Each event type should map to exactly one category
        seen = {}
        for event_type, category in EVENT_CATEGORY_MAP.items():
            if event_type in seen:
                pytest.fail(f"Duplicate event type: {event_type}")
            seen[event_type] = category


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
