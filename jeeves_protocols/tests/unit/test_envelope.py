"""Unit tests for GenericEnvelope and ProcessingRecord.

Tests envelope creation, serialization, deserialization, and state management.
"""

import pytest
from datetime import datetime, timezone


class TestGenericEnvelope:
    """Tests for GenericEnvelope dataclass."""

    def test_create_envelope_with_defaults(self):
        """Test creating envelope with default values."""
        from jeeves_protocols import GenericEnvelope

        env = GenericEnvelope()

        assert env.envelope_id == ""
        assert env.request_id == ""
        assert env.current_stage == "start"
        assert env.iteration == 0
        assert env.max_iterations == 3
        assert env.llm_call_count == 0
        assert env.max_llm_calls == 10
        assert env.terminated is False
        assert env.outputs == {}

    def test_create_envelope_with_values(self, sample_envelope):
        """Test creating envelope with specific values."""
        assert sample_envelope.envelope_id == "env-123"
        assert sample_envelope.request_id == "req-456"
        assert sample_envelope.user_id == "user-789"
        assert sample_envelope.session_id == "sess-abc"
        assert sample_envelope.raw_input == "Test input message"

    def test_envelope_from_dict(self, sample_envelope_dict):
        """Test creating envelope from dictionary."""
        from jeeves_protocols import GenericEnvelope

        env = GenericEnvelope.from_dict(sample_envelope_dict)

        assert env.envelope_id == "env-123"
        assert env.request_id == "req-456"
        assert env.llm_call_count == 2
        assert env.outputs["perception"]["entities"] == ["file.py"]

    def test_envelope_from_dict_with_empty_dict(self):
        """Test creating envelope from empty dictionary."""
        from jeeves_protocols import GenericEnvelope

        env = GenericEnvelope.from_dict({})

        assert env.envelope_id == ""
        assert env.current_stage == "start"

    def test_envelope_outputs_dict(self, sample_envelope):
        """Test outputs dictionary behavior."""
        sample_envelope.outputs["perception"] = {
            "entities": ["file.py"],
            "context": "Test context",
        }

        assert "perception" in sample_envelope.outputs
        assert sample_envelope.outputs["perception"]["entities"] == ["file.py"]

    def test_envelope_stage_order(self, sample_envelope):
        """Test stage order list."""
        assert len(sample_envelope.stage_order) == 6
        assert sample_envelope.stage_order[0] == "perception"
        assert sample_envelope.stage_order[-1] == "critic"

    def test_envelope_bounds_tracking(self, sample_envelope):
        """Test bounds tracking fields."""
        sample_envelope.llm_call_count = 5
        sample_envelope.agent_hop_count = 10

        assert sample_envelope.llm_call_count == 5
        assert sample_envelope.agent_hop_count == 10
        assert sample_envelope.llm_call_count < sample_envelope.max_llm_calls
        assert sample_envelope.agent_hop_count < sample_envelope.max_agent_hops


class TestProcessingRecord:
    """Tests for ProcessingRecord dataclass."""

    def test_create_processing_record_defaults(self):
        """Test creating processing record with defaults."""
        from jeeves_protocols import ProcessingRecord

        now = datetime.now(timezone.utc)
        record = ProcessingRecord(
            agent="perception",
            stage_order=1,
            started_at=now,
        )

        assert record.agent == "perception"
        assert record.stage_order == 1
        assert record.status == "running"
        assert record.completed_at is None
        assert record.duration_ms == 0
        assert record.llm_calls == 0

    def test_processing_record_completed(self, sample_processing_record):
        """Test completed processing record."""
        assert sample_processing_record.status == "success"
        assert sample_processing_record.completed_at is not None
        assert sample_processing_record.duration_ms == 150
        assert sample_processing_record.llm_calls == 1

    def test_processing_record_with_error(self):
        """Test processing record with error."""
        from jeeves_protocols import ProcessingRecord

        now = datetime.now(timezone.utc)
        record = ProcessingRecord(
            agent="plan",
            stage_order=3,
            started_at=now,
            completed_at=now,
            status="error",
            error="LLM timeout",
        )

        assert record.status == "error"
        assert record.error == "LLM timeout"


class TestEnvelopeControlFlow:
    """Tests for envelope control flow states using unified interrupt system."""

    def test_clarification_interrupt(self, sample_envelope):
        """Test clarification via unified interrupt system."""
        sample_envelope.interrupt_pending = True
        sample_envelope.interrupt = {
            "kind": "clarification",
            "question": "What file do you mean?",
            "response": None,
        }

        assert sample_envelope.interrupt_pending is True
        assert sample_envelope.interrupt["kind"] == "clarification"
        assert sample_envelope.interrupt["question"] == "What file do you mean?"
        assert sample_envelope.interrupt.get("response") is None

    def test_clarification_resolved(self, sample_envelope):
        """Test clarification response via unified interrupt system."""
        sample_envelope.interrupt_pending = False
        sample_envelope.interrupt = {
            "kind": "clarification",
            "question": "What file do you mean?",
            "response": "The main.py file",
        }

        assert sample_envelope.interrupt_pending is False
        assert sample_envelope.interrupt["response"] == "The main.py file"

    def test_confirmation_interrupt(self, sample_envelope):
        """Test confirmation via unified interrupt system."""
        sample_envelope.interrupt_pending = True
        sample_envelope.interrupt = {
            "kind": "confirmation",
            "id": "confirm-123",
            "message": "Delete all files?",
            "response": None,
        }

        assert sample_envelope.interrupt_pending is True
        assert sample_envelope.interrupt["kind"] == "confirmation"
        assert sample_envelope.interrupt["id"] == "confirm-123"
        assert sample_envelope.interrupt.get("response") is None

    def test_confirmation_resolved(self, sample_envelope):
        """Test confirmation response via unified interrupt system."""
        sample_envelope.interrupt_pending = False
        sample_envelope.interrupt = {
            "kind": "confirmation",
            "id": "confirm-123",
            "response": True,
        }

        assert sample_envelope.interrupt_pending is False
        assert sample_envelope.interrupt["response"] is True

    def test_termination_state(self, sample_envelope):
        """Test termination state."""
        from jeeves_protocols import TerminalReason

        sample_envelope.terminated = True
        sample_envelope.terminal_reason = TerminalReason.COMPLETED
        sample_envelope.termination_reason = "All goals achieved"

        assert sample_envelope.terminated is True
        assert sample_envelope.terminal_reason == TerminalReason.COMPLETED


class TestEnvelopeMultiStage:
    """Tests for multi-stage envelope fields."""

    def test_completed_stages(self, sample_envelope):
        """Test completed stages list."""
        sample_envelope.completed_stages.append({
            "stage": 1,
            "goal": "Parse user input",
            "status": "success",
        })

        assert len(sample_envelope.completed_stages) == 1
        assert sample_envelope.completed_stages[0]["goal"] == "Parse user input"

    def test_goal_tracking(self, sample_envelope):
        """Test goal tracking fields."""
        sample_envelope.all_goals = ["Goal 1", "Goal 2", "Goal 3"]
        sample_envelope.remaining_goals = ["Goal 2", "Goal 3"]
        sample_envelope.goal_completion_status = {
            "Goal 1": "completed",
            "Goal 2": "in_progress",
            "Goal 3": "pending",
        }

        assert len(sample_envelope.all_goals) == 3
        assert len(sample_envelope.remaining_goals) == 2
        assert sample_envelope.goal_completion_status["Goal 1"] == "completed"

    def test_prior_plans_and_feedback(self, sample_envelope):
        """Test prior plans and critic feedback."""
        sample_envelope.prior_plans.append({
            "plan_id": 1,
            "steps": ["Step 1", "Step 2"],
        })
        sample_envelope.critic_feedback.append("Plan needs more detail")

        assert len(sample_envelope.prior_plans) == 1
        assert len(sample_envelope.critic_feedback) == 1
        assert sample_envelope.critic_feedback[0] == "Plan needs more detail"
