"""Contract tests for Envelope roundtrip serialization.

These tests verify that Envelope can be serialized and deserialized
correctly between Python and Go (via proto) and Python and JSON.

Contract: Envelope state must be preserved across serialization boundaries.
"""

import json
from copy import deepcopy
from datetime import datetime, timezone

import pytest

from jeeves_protocols import RequestContext
from jeeves_protocols.envelope import Envelope
from jeeves_protocols.enums import TerminalReason


# =============================================================================
# JSON Roundtrip Tests
# =============================================================================


def _ctx(request_id: str, user_id: str, session_id: str) -> RequestContext:
    return RequestContext(
        request_id=request_id,
        capability="test_capability",
        user_id=user_id,
        session_id=session_id,
    )


class TestEnvelopeJsonRoundtrip:
    """Test Envelope JSON serialization roundtrip."""

    def test_empty_envelope_roundtrip(self):
        """Test roundtrip of minimal envelope."""
        original = Envelope(
            request_context=_ctx("req_test", "user_001", "session_001"),
            envelope_id="env_test",
            request_id="req_test",
            user_id="user_001",
            session_id="session_001",
            raw_input="test",
        )

        # Serialize to dict
        data = original.to_dict()

        # Deserialize back
        restored = Envelope.from_dict(data)

        # Core fields should match
        assert restored.envelope_id == original.envelope_id
        assert restored.request_id == original.request_id
        assert restored.user_id == original.user_id
        assert restored.session_id == original.session_id
        assert restored.raw_input == original.raw_input

    def test_full_envelope_roundtrip(self):
        """Test roundtrip of fully populated envelope."""
        now = datetime.now(timezone.utc)

        original = Envelope(
            request_context=_ctx("req_full", "user_002", "session_002"),
            envelope_id="env_full",
            request_id="req_full",
            user_id="user_002",
            session_id="session_002",
            raw_input="complex test input",
            received_at=now,
            created_at=now,
            current_stage="analysis",
            stage_order=["intake", "analysis", "output"],
            iteration=1,
            max_iterations=5,
            llm_call_count=3,
            max_llm_calls=15,
            agent_hop_count=2,
            max_agent_hops=30,
            terminated=False,
            termination_reason=None,
            terminal_reason=None,
            interrupt_pending=False,
            parallel_mode=True,
            current_stage_number=2,
            max_stages=3,
            all_goals=["goal1", "goal2"],
            remaining_goals=["goal2"],
            outputs={
                "agent1": {"result": "value1"},
                "agent2": {"result": "value2"},
            },
            metadata={"key": "value"},
        )

        # Serialize and deserialize
        data = original.to_dict()
        restored = Envelope.from_dict(data)

        # Verify fields that participate in to_dict/from_dict
        assert restored.envelope_id == original.envelope_id
        assert restored.current_stage == original.current_stage
        assert restored.stage_order == original.stage_order
        assert restored.iteration == original.iteration
        assert restored.max_iterations == original.max_iterations
        assert restored.llm_call_count == original.llm_call_count
        assert restored.agent_hop_count == original.agent_hop_count
        assert restored.parallel_mode == original.parallel_mode
        assert restored.all_goals == original.all_goals
        assert restored.remaining_goals == original.remaining_goals
        assert restored.outputs == original.outputs
        assert restored.metadata == original.metadata

    def test_envelope_with_terminal_reason_roundtrip(self):
        """Test roundtrip preserves terminal_reason enum."""
        original = Envelope(
            request_context=_ctx("req_term", "user", "session"),
            envelope_id="env_term",
            request_id="req_term",
            user_id="user",
            session_id="session",
            raw_input="test",
            terminated=True,
            terminal_reason=TerminalReason.MAX_LLM_CALLS_EXCEEDED,
        )

        data = original.to_dict()
        restored = Envelope.from_dict(data)

        assert restored.terminated is True
        assert restored.terminal_reason == TerminalReason.MAX_LLM_CALLS_EXCEEDED

    def test_envelope_to_json_and_back(self):
        """Test full JSON string serialization."""
        original = Envelope(
            request_context=_ctx("req_json", "user", "session"),
            envelope_id="env_json",
            request_id="req_json",
            user_id="user",
            session_id="session",
            raw_input="json test",
            outputs={"agent": {"data": [1, 2, 3]}},
        )

        # To JSON string
        json_str = json.dumps(original.to_dict(), default=str)

        # Parse back
        data = json.loads(json_str)
        restored = Envelope.from_dict(data)

        assert restored.envelope_id == original.envelope_id
        assert restored.outputs == original.outputs


# =============================================================================
# State Dict Roundtrip Tests
# =============================================================================


class TestEnvelopeStateDictRoundtrip:
    """Test Envelope state dict serialization."""

    def test_to_state_dict_minimal(self):
        """Test to_state_dict with minimal envelope."""
        envelope = Envelope(
            request_context=_ctx("req_state", "user", "session"),
            envelope_id="env_state",
            request_id="req_state",
            user_id="user",
            session_id="session",
            raw_input="state test",
        )

        state = envelope.to_state_dict()

        assert isinstance(state, dict)
        assert state["envelope_id"] == "env_state"
        assert state["current_stage"] == "start"  # default

    def test_to_state_dict_captures_progress(self):
        """Test to_state_dict captures execution progress."""
        envelope = Envelope(
            request_context=_ctx("req_progress", "user", "session"),
            envelope_id="env_progress",
            request_id="req_progress",
            user_id="user",
            session_id="session",
            raw_input="progress test",
            iteration=2,
            llm_call_count=5,
            agent_hop_count=3,
        )

        state = envelope.to_state_dict()

        assert state["iteration"] == 2
        assert state["llm_call_count"] == 5
        assert state["agent_hop_count"] == 3


# =============================================================================
# Deep Copy Contract Tests
# =============================================================================


class TestEnvelopeDeepCopy:
    """Test that envelope deep copy creates independent copies."""

    def test_deepcopy_creates_independent_copy(self):
        """Test that deepcopy creates truly independent copy."""
        original = Envelope(
            request_context=_ctx("req_copy", "user", "session"),
            envelope_id="env_copy",
            request_id="req_copy",
            user_id="user",
            session_id="session",
            raw_input="copy test",
            outputs={"agent": {"data": "value"}},
            metadata={"key": "value"},
        )

        copy = deepcopy(original)

        # Modify copy
        copy.llm_call_count = 10
        copy.outputs["new_agent"] = {"new": "data"}
        copy.metadata["new_key"] = "new_value"

        # Original should be unchanged
        assert original.llm_call_count == 0
        assert "new_agent" not in original.outputs
        assert "new_key" not in original.metadata

    def test_deepcopy_preserves_all_fields(self):
        """Test that deepcopy preserves all fields."""
        now = datetime.now(timezone.utc)

        original = Envelope(
            request_context=_ctx("req_full_copy", "user", "session"),
            envelope_id="env_full_copy",
            request_id="req_full_copy",
            user_id="user",
            session_id="session",
            raw_input="full copy test",
            received_at=now,
            created_at=now,
            current_stage="processing",
            iteration=2,
            terminal_reason=TerminalReason.COMPLETED,
        )

        copy = deepcopy(original)

        assert copy.envelope_id == original.envelope_id
        assert copy.current_stage == original.current_stage
        assert copy.iteration == original.iteration
        assert copy.terminal_reason == original.terminal_reason


# =============================================================================
# Contract Invariant Tests
# =============================================================================


class TestEnvelopeInvariants:
    """Test Envelope invariants that must hold."""

    def test_bounds_are_non_negative(self):
        """Test that bound values are non-negative."""
        envelope = Envelope(
            request_context=_ctx("req_bounds", "user", "session"),
            envelope_id="env_bounds",
            request_id="req_bounds",
            user_id="user",
            session_id="session",
            raw_input="bounds test",
        )

        assert envelope.llm_call_count >= 0
        assert envelope.agent_hop_count >= 0
        assert envelope.iteration >= 0
        assert envelope.max_llm_calls > 0
        assert envelope.max_agent_hops > 0
        assert envelope.max_iterations > 0

    def test_terminated_envelope_has_reason(self):
        """Test convention: terminated envelopes should have a reason."""
        envelope = Envelope(
            request_context=_ctx("req_terminated", "user", "session"),
            envelope_id="env_terminated",
            request_id="req_terminated",
            user_id="user",
            session_id="session",
            raw_input="terminated test",
            terminated=True,
            terminal_reason=TerminalReason.COMPLETED,
        )

        # When terminated, terminal_reason should be set
        # (This is a soft contract - not enforced at class level)
        assert envelope.terminated is True
        assert envelope.terminal_reason is not None

    def test_envelope_id_is_unique_format(self):
        """Test envelope ID follows expected format."""
        from jeeves_protocols.grpc_client import GrpcGoClient

        client = GrpcGoClient()
        envelope = client.create_envelope(
            raw_input="test",
            request_context=_ctx("req_test", "user", "session"),
        )

        # IDs should have expected prefix
        assert envelope.envelope_id.startswith("env_")
        assert envelope.request_id.startswith("req_")
