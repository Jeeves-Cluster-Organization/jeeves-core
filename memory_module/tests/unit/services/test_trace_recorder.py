"""Unit tests for TraceRecorder service."""

import pytest
from datetime import datetime, timezone
from unittest.mock import AsyncMock, MagicMock

from memory_module.services.trace_recorder import TraceRecorder
from memory_module.repositories.trace_repository import TraceRepository, AgentTrace


class TestRecorder:
    """Tests for TraceRecorder service functionality."""

    @pytest.fixture
    def mock_repository(self):
        """Create mock trace repository."""
        repo = AsyncMock(spec=TraceRepository)
        repo.save = AsyncMock(return_value="trace-123")
        repo.update = AsyncMock(return_value=True)
        repo.get_by_request = AsyncMock(return_value=[])
        repo.get_by_correlation = AsyncMock(return_value=[])
        repo.get_agent_metrics = AsyncMock(return_value={})
        repo.get_error_traces = AsyncMock(return_value=[])
        return repo

    @pytest.fixture
    def mock_flags_enabled(self):
        """Create mock feature flags with tracing enabled."""
        flags = MagicMock()
        flags.is_enabled = MagicMock(return_value=True)
        return flags

    @pytest.fixture
    def mock_flags_disabled(self):
        """Create mock feature flags with tracing disabled."""
        flags = MagicMock()
        flags.is_enabled = MagicMock(return_value=False)
        return flags

    @pytest.fixture
    def recorder_enabled(self, mock_repository, mock_flags_enabled):
        """Create recorder with tracing enabled via DI."""
        return TraceRecorder(mock_repository, feature_flags=mock_flags_enabled)

    @pytest.fixture
    def recorder_disabled(self, mock_repository, mock_flags_disabled):
        """Create recorder with tracing disabled via DI."""
        return TraceRecorder(mock_repository, feature_flags=mock_flags_disabled)

    def test_start_trace(self, recorder_enabled):
        """Test starting a trace."""
        trace = recorder_enabled.start_trace(
            agent_name="planner",
            stage="planning",
            correlation_id="corr-1",
            request_id="req-1",
            user_id="user-1",
            input_data={"message": "Create a task"}
        )

        assert trace.agent_name == "planner"
        assert trace.stage == "planning"
        assert trace.correlation_id == "corr-1"
        assert trace.request_id == "req-1"
        assert trace.input_data == {"message": "Create a task"}
        assert trace.trace_id in recorder_enabled._active_traces

    @pytest.mark.asyncio
    async def test_complete_trace(self, recorder_enabled, mock_repository):
        """Test completing a trace."""
        trace = recorder_enabled.start_trace(
            agent_name="executor",
            stage="execution",
            correlation_id="corr-1",
            request_id="req-1",
            user_id="user-1"
        )

        result = await recorder_enabled.complete_trace(
            trace=trace,
            output_data={"result": "success"},
            status="success",
            confidence=0.95
        )

        assert result == trace.trace_id
        mock_repository.save.assert_called_once()
        assert trace.trace_id not in recorder_enabled._active_traces

    @pytest.mark.asyncio
    async def test_complete_trace_when_disabled(self, recorder_disabled):
        """Test that trace is not saved when disabled."""
        trace = AgentTrace(
            agent_name="validator",
            stage="validation",
            correlation_id="corr-1",
            request_id="req-1",
            user_id="user-1"
        )

        result = await recorder_disabled.complete_trace(
            trace=trace,
            output_data={"valid": True}
        )

        assert result is None

    @pytest.mark.asyncio
    async def test_record_complete_trace(self, recorder_enabled, mock_repository):
        """Test recording a complete trace in one call."""
        result = await recorder_enabled.record(
            agent_name="planner",
            stage="planning",
            correlation_id="corr-1",
            request_id="req-1",
            user_id="user-1",
            input_data={"message": "test"},
            output_data={"plan": []},
            latency_ms=150,
            confidence=0.9,
            status="success"
        )

        assert result is not None
        mock_repository.save.assert_called_once()

    @pytest.mark.asyncio
    async def test_record_when_disabled(self, recorder_disabled, mock_repository):
        """Test that record does nothing when disabled."""
        result = await recorder_disabled.record(
            agent_name="planner",
            stage="planning",
            correlation_id="corr-1",
            request_id="req-1",
            user_id="user-1"
        )

        assert result is None
        mock_repository.save.assert_not_called()

    @pytest.mark.asyncio
    async def test_trace_context_success(self, recorder_enabled, mock_repository):
        """Test trace context manager with success."""
        async with recorder_enabled.trace_context(
            agent_name="critic",
            stage="critique",
            correlation_id="corr-1",
            request_id="req-1",
            user_id="user-1",
            input_data={"response": "test"}
        ) as trace:
            trace.output_data = {"decision": "accept"}
            trace.confidence = 0.95

        mock_repository.save.assert_called_once()
        # Verify saved trace has output data
        saved_trace = mock_repository.save.call_args[0][0]
        assert saved_trace.output_data == {"decision": "accept"}
        assert saved_trace.status == "success"

    @pytest.mark.asyncio
    async def test_trace_context_error(self, recorder_enabled, mock_repository):
        """Test trace context manager with error."""
        with pytest.raises(ValueError):
            async with recorder_enabled.trace_context(
                agent_name="executor",
                stage="execution",
                correlation_id="corr-1",
                request_id="req-1",
                user_id="user-1"
            ) as trace:
                raise ValueError("Tool execution failed")

        # Trace should still be saved with error status
        mock_repository.save.assert_called_once()
        saved_trace = mock_repository.save.call_args[0][0]
        assert saved_trace.status == "error"
        assert "ValueError" in saved_trace.error_details["type"]

    @pytest.mark.asyncio
    async def test_get_traces_for_request(self, recorder_enabled, mock_repository):
        """Test retrieving traces for a request."""
        mock_repository.get_by_request.return_value = [
            AgentTrace(
                agent_name="planner",
                stage="planning",
                correlation_id="corr-1",
                request_id="req-1",
                user_id="user-1"
            )
        ]

        result = await recorder_enabled.get_traces_for_request("req-1")

        assert len(result) == 1
        mock_repository.get_by_request.assert_called_once_with("req-1")

    @pytest.mark.asyncio
    async def test_get_agent_metrics(self, recorder_enabled, mock_repository):
        """Test retrieving agent metrics."""
        mock_repository.get_agent_metrics.return_value = {
            "total_traces": 100,
            "success_rate": 0.95
        }

        result = await recorder_enabled.get_agent_metrics("planner", "user-1")

        assert result["total_traces"] == 100
        assert result["success_rate"] == 0.95

    def test_enabled_property(self, recorder_enabled, recorder_disabled):
        """Test the enabled property."""
        assert recorder_enabled.enabled is True
        assert recorder_disabled.enabled is False
