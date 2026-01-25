"""Unit tests for TraceRepository."""

import pytest
from datetime import datetime, timezone
from unittest.mock import AsyncMock, MagicMock
from uuid import uuid4

from memory_module.repositories.trace_repository import TraceRepository, AgentTrace


class TestTrace:
    """Tests for AgentTrace dataclass."""

    def test_create_trace(self):
        """Test creating an agent trace."""
        trace = AgentTrace(
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
        assert trace.user_id == "user-1"
        assert trace.input_data == {"message": "Create a task"}
        assert trace.trace_id is not None
        assert trace.span_id is not None
        assert trace.started_at is not None
        assert trace.status == "success"

    def test_complete_trace(self):
        """Test completing a trace."""
        trace = AgentTrace(
            agent_name="executor",
            stage="execution",
            correlation_id="corr-1",
            request_id="req-1",
            user_id="user-1"
        )

        trace.complete(
            output_data={"result": "success"},
            status="success",
            confidence=0.95
        )

        assert trace.output_data == {"result": "success"}
        assert trace.status == "success"
        assert trace.confidence == 0.95
        assert trace.completed_at is not None
        assert trace.latency_ms is not None

    def test_complete_trace_with_error(self):
        """Test completing a trace with error."""
        trace = AgentTrace(
            agent_name="validator",
            stage="validation",
            correlation_id="corr-1",
            request_id="req-1",
            user_id="user-1"
        )

        trace.complete(
            status="error",
            error_details={"type": "ValidationError", "message": "Failed"}
        )

        assert trace.status == "error"
        assert trace.error_details["type"] == "ValidationError"
        assert trace.completed_at is not None

    def test_to_dict(self):
        """Test conversion to dictionary."""
        trace = AgentTrace(
            trace_id="trace-1",
            span_id="span-1",
            agent_name="critic",
            stage="critique",
            correlation_id="corr-1",
            request_id="req-1",
            user_id="user-1",
            input_data={"response": "test"},
            llm_model="qwen2.5:3b"
        )

        result = trace.to_dict()

        assert result["trace_id"] == "trace-1"
        assert result["span_id"] == "span-1"
        assert result["agent_name"] == "critic"
        assert result["stage"] == "critique"
        assert result["llm_model"] == "qwen2.5:3b"

    def test_from_dict(self):
        """Test creating trace from dictionary."""
        data = {
            "trace_id": "trace-1",
            "span_id": "span-1",
            "agent_name": "planner",
            "stage": "planning",
            "correlation_id": "corr-1",
            "request_id": "req-1",
            "user_id": "user-1",
            "input_data": {"message": "test"},
            "output_data": {"plan": []},
            "status": "success",
            "latency_ms": 150,
            "started_at": "2025-01-15T10:00:00+00:00",
            "completed_at": "2025-01-15T10:00:00.150+00:00"
        }

        trace = AgentTrace.from_dict(data)

        assert trace.trace_id == "trace-1"
        assert trace.agent_name == "planner"
        assert trace.latency_ms == 150


class TestRepository:
    """Tests for TraceRepository persistence."""

    @pytest.fixture
    def mock_db(self):
        """Create mock database client."""
        db = AsyncMock()
        db.execute = AsyncMock(return_value=None)
        db.fetch_one = AsyncMock(return_value=None)
        db.fetch_all = AsyncMock(return_value=[])
        return db

    @pytest.fixture
    def repository(self, mock_db):
        """Create repository with mock DB."""
        return TraceRepository(mock_db)

    @pytest.mark.asyncio
    async def test_save_trace(self, repository, mock_db):
        """Test saving a trace."""
        trace = AgentTrace(
            agent_name="planner",
            stage="planning",
            correlation_id="corr-1",
            request_id="req-1",
            user_id="user-1",
            input_data={"message": "test"}
        )

        result = await repository.save(trace)

        assert result == trace.trace_id
        mock_db.execute.assert_called_once()

    @pytest.mark.asyncio
    async def test_update_trace(self, repository, mock_db):
        """Test updating a trace."""
        trace = AgentTrace(
            trace_id="trace-1",
            agent_name="executor",
            stage="execution",
            correlation_id="corr-1",
            request_id="req-1",
            user_id="user-1"
        )
        trace.complete(output_data={"result": "done"}, status="success")

        result = await repository.update(trace)

        assert result is True
        mock_db.execute.assert_called_once()

    @pytest.mark.asyncio
    async def test_get_by_id(self, repository, mock_db):
        """Test retrieving trace by ID."""
        mock_db.fetch_one.return_value = {
            "trace_id": "trace-1",
            "span_id": "span-1",
            "agent_name": "planner",
            "stage": "planning",
            "correlation_id": "corr-1",
            "request_id": "req-1",
            "user_id": "user-1",
            "status": "success",
            "started_at": datetime.now(timezone.utc)
        }

        result = await repository.get_by_id("trace-1")

        assert result is not None
        assert result.trace_id == "trace-1"
        assert result.agent_name == "planner"

    @pytest.mark.asyncio
    async def test_get_by_correlation(self, repository, mock_db):
        """Test retrieving traces by correlation ID."""
        mock_db.fetch_all.return_value = [
            {
                "trace_id": "trace-1",
                "span_id": "span-1",
                "agent_name": "planner",
                "stage": "planning",
                "correlation_id": "corr-1",
                "request_id": "req-1",
                "user_id": "user-1",
                "status": "success",
                "started_at": datetime.now(timezone.utc)
            },
            {
                "trace_id": "trace-2",
                "span_id": "span-2",
                "agent_name": "executor",
                "stage": "execution",
                "correlation_id": "corr-1",
                "request_id": "req-1",
                "user_id": "user-1",
                "status": "success",
                "started_at": datetime.now(timezone.utc)
            }
        ]

        result = await repository.get_by_correlation("corr-1")

        assert len(result) == 2
        assert result[0].agent_name == "planner"
        assert result[1].agent_name == "executor"

    @pytest.mark.asyncio
    async def test_get_agent_metrics(self, repository, mock_db):
        """Test retrieving agent metrics."""
        mock_db.fetch_one.return_value = {
            "total": 100,
            "success_count": 95,
            "error_count": 5,
            "avg_latency_ms": 150.5,
            "avg_confidence": 0.92,
            "total_input_tokens": 50000,
            "total_output_tokens": 30000
        }

        result = await repository.get_agent_metrics("planner", "user-1")

        assert result["total_traces"] == 100
        assert result["success_count"] == 95
        assert result["error_count"] == 5
        assert result["success_rate"] == 0.95
        assert result["avg_latency_ms"] == 150.5

    @pytest.mark.asyncio
    async def test_get_error_traces(self, repository, mock_db):
        """Test retrieving error traces."""
        mock_db.fetch_all.return_value = [
            {
                "trace_id": "trace-err-1",
                "span_id": "span-1",
                "agent_name": "validator",
                "stage": "validation",
                "correlation_id": "corr-1",
                "request_id": "req-1",
                "user_id": "user-1",
                "status": "error",
                "error_details": '{"type": "ValidationError"}',
                "started_at": datetime.now(timezone.utc)
            }
        ]

        result = await repository.get_error_traces("user-1", limit=10)

        assert len(result) == 1
        assert result[0].status == "error"
