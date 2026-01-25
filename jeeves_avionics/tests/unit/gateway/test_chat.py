"""Unit tests for chat router functions.

Tests focus on helper functions and event classification logic.
Integration tests for the full endpoint are in integration tests.
"""

import pytest
from jeeves_avionics.gateway.routers.chat import (
    _classify_event_category,
    _build_grpc_request,
    _is_internal_event,
    EVENT_CATEGORY_MAP,
    MessageSend,
    ResponseReadyHandler,
    ClarificationHandler,
    ConfirmationHandler,
    ErrorHandler,
    EVENT_HANDLERS,
)
from jeeves_protocols.events import EventCategory
from unittest.mock import MagicMock

# Import proto types (generated)
try:
    from proto import jeeves_pb2
except ImportError:
    jeeves_pb2 = None


@pytest.mark.skipif(jeeves_pb2 is None, reason="gRPC stubs not generated")
class TestBuildGrpcRequest:
    """Test gRPC request building helper."""

    def test_build_grpc_request_minimal(self):
        """Test building request with only required fields."""
        body = MessageSend(message="Hello")
        result = _build_grpc_request("user123", body)

        assert result.user_id == "user123"
        assert result.message == "Hello"
        assert result.session_id == ""
        assert result.context == {}

    def test_build_grpc_request_with_mode(self):
        """Test building request with mode."""
        body = MessageSend(message="Analyze", mode="code-analysis")
        result = _build_grpc_request("user123", body)

        assert result.user_id == "user123"
        assert result.message == "Analyze"
        assert result.context["mode"] == "code-analysis"
        assert "repo_path" not in result.context

    def test_build_grpc_request_full(self):
        """Test building request with all fields."""
        body = MessageSend(
            message="Analyze",
            mode="code-analysis",
            session_id="sess123",
            repo_path="/path/to/repo"
        )
        result = _build_grpc_request("user123", body)

        assert result.user_id == "user123"
        assert result.message == "Analyze"
        assert result.session_id == "sess123"
        assert result.context["mode"] == "code-analysis"
        assert result.context["repo_path"] == "/path/to/repo"


@pytest.mark.skipif(jeeves_pb2 is None, reason="gRPC stubs not generated")
class TestIsInternalEvent:
    """Test internal event type classification."""

    def test_is_internal_event_lifecycle(self):
        """Test internal lifecycle events return True."""
        assert _is_internal_event(jeeves_pb2.FlowEvent.FLOW_STARTED) == True
        assert _is_internal_event(jeeves_pb2.FlowEvent.AGENT_STARTED) == True
        assert _is_internal_event(jeeves_pb2.FlowEvent.AGENT_COMPLETED) == True
        assert _is_internal_event(jeeves_pb2.FlowEvent.TOOL_STARTED) == True
        assert _is_internal_event(jeeves_pb2.FlowEvent.TOOL_COMPLETED) == True
        assert _is_internal_event(jeeves_pb2.FlowEvent.PLAN_CREATED) == True
        assert _is_internal_event(jeeves_pb2.FlowEvent.CRITIC_DECISION) == True
        assert _is_internal_event(jeeves_pb2.FlowEvent.SYNTHESIZER_COMPLETE) == True
        assert _is_internal_event(jeeves_pb2.FlowEvent.STAGE_TRANSITION) == True

    def test_is_internal_event_terminal(self):
        """Test terminal events return False."""
        assert _is_internal_event(jeeves_pb2.FlowEvent.RESPONSE_READY) == False
        assert _is_internal_event(jeeves_pb2.FlowEvent.CLARIFICATION) == False
        assert _is_internal_event(jeeves_pb2.FlowEvent.CONFIRMATION) == False
        assert _is_internal_event(jeeves_pb2.FlowEvent.ERROR) == False


class TestResponseReadyHandler:
    """Test ResponseReadyHandler event processing."""

    def test_handle_without_mode_config(self):
        """Test handling RESPONSE_READY without mode configuration."""
        handler = ResponseReadyHandler()
        payload = {"response_text": "Task completed successfully"}

        result = handler.handle(payload, mode_config=None)

        assert result["status"] == "completed"
        assert result["response"] == "Task completed successfully"
        assert len(result) == 2  # Only status and response fields

    def test_handle_with_mode_config(self):
        """Test handling RESPONSE_READY with mode configuration fields."""
        handler = ResponseReadyHandler()
        payload = {
            "response": "Analysis complete",
            "files_examined": ["/path/to/file.py"],
            "citations": ["ref1", "ref2"],
        }

        # Mock mode_config with response_fields
        mock_mode_config = MagicMock()
        mock_mode_config.response_fields = ["files_examined", "citations"]

        result = handler.handle(payload, mode_config=mock_mode_config)

        assert result["status"] == "completed"
        assert result["response"] == "Analysis complete"
        assert result["files_examined"] == ["/path/to/file.py"]
        assert result["citations"] == ["ref1", "ref2"]


class TestClarificationHandler:
    """Test ClarificationHandler event processing."""

    def test_handle_without_mode_config(self):
        """Test handling CLARIFICATION without mode configuration."""
        handler = ClarificationHandler()
        payload = {"question": "Please provide more details"}

        result = handler.handle(payload, mode_config=None)

        assert result["status"] == "clarification"
        assert result["clarification_needed"] == True
        assert result["clarification_question"] == "Please provide more details"
        assert len(result) == 3  # Only status, flag, and question

    def test_handle_with_mode_config(self):
        """Test handling CLARIFICATION with mode configuration fields."""
        handler = ClarificationHandler()
        payload = {
            "question": "Which file to analyze?",
            "thread_id": "thread123",
        }

        # Mock mode_config with response_fields
        mock_mode_config = MagicMock()
        mock_mode_config.response_fields = ["thread_id"]

        result = handler.handle(payload, mode_config=mock_mode_config)

        assert result["status"] == "clarification"
        assert result["clarification_needed"] == True
        assert result["clarification_question"] == "Which file to analyze?"
        assert result["thread_id"] == "thread123"


class TestConfirmationHandler:
    """Test ConfirmationHandler event processing."""

    def test_handle_confirmation(self):
        """Test handling CONFIRMATION event."""
        handler = ConfirmationHandler()
        payload = {
            "message": "Delete 5 files?",
            "confirmation_id": "confirm123",
        }

        result = handler.handle(payload, mode_config=None)

        assert result["status"] == "confirmation"
        assert result["confirmation_needed"] == True
        assert result["confirmation_message"] == "Delete 5 files?"
        assert result["confirmation_id"] == "confirm123"


class TestErrorHandler:
    """Test ErrorHandler event processing."""

    def test_handle_error_with_message(self):
        """Test handling ERROR with error message."""
        handler = ErrorHandler()
        payload = {"error": "Failed to connect to database"}

        result = handler.handle(payload, mode_config=None)

        assert result["status"] == "error"
        assert result["response"] == "Failed to connect to database"

    def test_handle_error_without_message(self):
        """Test handling ERROR without error message (default)."""
        handler = ErrorHandler()
        payload = {}

        result = handler.handle(payload, mode_config=None)

        assert result["status"] == "error"
        assert result["response"] == "Unknown error"


@pytest.mark.skipif(jeeves_pb2 is None, reason="gRPC stubs not generated")
class TestEventHandlersRegistry:
    """Test EVENT_HANDLERS registry configuration."""

    def test_registry_contains_all_handlers(self):
        """Test that EVENT_HANDLERS contains all terminal event types."""
        assert jeeves_pb2.FlowEvent.RESPONSE_READY in EVENT_HANDLERS
        assert jeeves_pb2.FlowEvent.CLARIFICATION in EVENT_HANDLERS
        assert jeeves_pb2.FlowEvent.CONFIRMATION in EVENT_HANDLERS
        assert jeeves_pb2.FlowEvent.ERROR in EVENT_HANDLERS

    def test_registry_handler_types(self):
        """Test that registry contains correct handler instances."""
        assert isinstance(EVENT_HANDLERS[jeeves_pb2.FlowEvent.RESPONSE_READY], ResponseReadyHandler)
        assert isinstance(EVENT_HANDLERS[jeeves_pb2.FlowEvent.CLARIFICATION], ClarificationHandler)
        assert isinstance(EVENT_HANDLERS[jeeves_pb2.FlowEvent.CONFIRMATION], ConfirmationHandler)
        assert isinstance(EVENT_HANDLERS[jeeves_pb2.FlowEvent.ERROR], ErrorHandler)


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
