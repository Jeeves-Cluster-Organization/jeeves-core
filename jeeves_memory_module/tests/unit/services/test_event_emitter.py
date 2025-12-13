"""Unit tests for EventEmitter service."""

import pytest
from datetime import datetime, timezone
from unittest.mock import AsyncMock, MagicMock, patch

from jeeves_memory_module.services.event_emitter import EventEmitter
from jeeves_memory_module.repositories.event_repository import EventRepository, DomainEvent


class TestEmitter:
    """Tests for EventEmitter service functionality."""

    @pytest.fixture
    def mock_repository(self):
        """Create mock event repository."""
        repo = AsyncMock(spec=EventRepository)
        repo.append = AsyncMock(return_value="evt-123")
        return repo

    @pytest.fixture
    def emitter_enabled(self, mock_repository):
        """Create emitter with events enabled."""
        with patch('jeeves_memory_module.services.event_emitter.get_feature_flags') as mock_flags:
            flags = MagicMock()
            flags.memory_event_sourcing_mode = "log_only"
            mock_flags.return_value = flags
            emitter = EventEmitter(mock_repository)
            emitter._flags = flags
            return emitter

    @pytest.fixture
    def emitter_disabled(self, mock_repository):
        """Create emitter with events disabled."""
        with patch('jeeves_memory_module.services.event_emitter.get_feature_flags') as mock_flags:
            flags = MagicMock()
            flags.memory_event_sourcing_mode = "disabled"
            mock_flags.return_value = flags
            emitter = EventEmitter(mock_repository)
            emitter._flags = flags
            return emitter

    @pytest.mark.asyncio
    async def test_emit_when_enabled(self, emitter_enabled, mock_repository):
        """Test event emission when enabled."""
        result = await emitter_enabled.emit(
            aggregate_type="task",
            aggregate_id="task-123",
            event_type="task_created",
            payload={"title": "Test Task"},
            user_id="user-1",
            actor="user"
        )

        assert result == "evt-123"
        mock_repository.append.assert_called_once()

    @pytest.mark.asyncio
    async def test_emit_when_disabled(self, emitter_disabled, mock_repository):
        """Test that events are not emitted when disabled."""
        result = await emitter_disabled.emit(
            aggregate_type="task",
            aggregate_id="task-123",
            event_type="task_created",
            payload={"title": "Test"},
            user_id="user-1"
        )

        assert result is None
        mock_repository.append.assert_not_called()

    @pytest.mark.asyncio
    async def test_emit_task_created(self, emitter_enabled, mock_repository):
        """Test task_created event factory."""
        result = await emitter_enabled.emit_task_created(
            task_id="task-123",
            user_id="user-1",
            title="My Task",
            description="Task description",
            priority=8
        )

        assert result == "evt-123"
        mock_repository.append.assert_called_once()

        # Verify the event payload
        call_args = mock_repository.append.call_args[0][0]
        assert call_args.aggregate_type == "task"
        assert call_args.event_type == "task_created"
        assert call_args.payload["title"] == "My Task"
        assert call_args.payload["priority"] == 8

    @pytest.mark.asyncio
    async def test_emit_task_completed(self, emitter_enabled, mock_repository):
        """Test task_completed event factory."""
        result = await emitter_enabled.emit_task_completed(
            task_id="task-123",
            user_id="user-1",
            actor="user"
        )

        assert result == "evt-123"
        call_args = mock_repository.append.call_args[0][0]
        assert call_args.event_type == "task_completed"

    @pytest.mark.asyncio
    async def test_emit_journal_ingested(self, emitter_enabled, mock_repository):
        """Test journal_ingested event factory."""
        result = await emitter_enabled.emit_journal_ingested(
            entry_id="journal-1",
            user_id="user-1",
            content="Today was productive..."
        )

        assert result == "evt-123"
        call_args = mock_repository.append.call_args[0][0]
        assert call_args.aggregate_type == "journal"
        assert call_args.event_type == "journal_ingested"

    @pytest.mark.asyncio
    async def test_emit_tool_quarantined(self, emitter_enabled, mock_repository):
        """Test tool_quarantined event factory."""
        result = await emitter_enabled.emit_tool_quarantined(
            tool_name="create_task",
            user_id="system",
            reason="High error rate",
            error_rate=0.45,
            threshold=0.35
        )

        assert result == "evt-123"
        call_args = mock_repository.append.call_args[0][0]
        assert call_args.aggregate_type == "governance"
        assert call_args.event_type == "tool_quarantined"
        assert call_args.payload["error_rate"] == 0.45

    @pytest.mark.asyncio
    async def test_emit_with_correlation(self, emitter_enabled, mock_repository):
        """Test event emission with correlation ID."""
        result = await emitter_enabled.emit(
            aggregate_type="session",
            aggregate_id="session-1",
            event_type="session_started",
            payload={},
            user_id="user-1",
            correlation_id="corr-123"
        )

        call_args = mock_repository.append.call_args[0][0]
        assert call_args.correlation_id == "corr-123"

    @pytest.mark.asyncio
    async def test_emit_handles_repository_error(self, emitter_enabled, mock_repository):
        """Test that repository errors don't crash the emitter."""
        mock_repository.append.side_effect = Exception("DB error")

        # Should not raise, just return None
        result = await emitter_enabled.emit(
            aggregate_type="task",
            aggregate_id="task-1",
            event_type="task_created",
            payload={},
            user_id="user-1"
        )

        assert result is None

    def test_enabled_property(self, emitter_enabled, emitter_disabled):
        """Test the enabled property."""
        assert emitter_enabled.enabled is True
        assert emitter_disabled.enabled is False
