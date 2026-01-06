"""Unit tests for EventEmitter memory event factories.

Tests the CommBus integration for memory events defined in messages/events.py.
"""

import pytest
from unittest.mock import Mock, AsyncMock


@pytest.fixture
def mock_event_repository():
    """Create a mock event repository."""
    repo = Mock()
    repo.append = AsyncMock(return_value="event-123")
    return repo


@pytest.fixture
def event_emitter(mock_event_repository):
    """Create an EventEmitter with mocked dependencies."""
    from jeeves_memory_module.services.event_emitter import (
        EventEmitter,
        SessionDedupCache,
    )

    return EventEmitter(
        event_repository=mock_event_repository,
        feature_flags=None,  # Defaults to enabled
        dedup_cache=SessionDedupCache(),
        logger=None,
    )


class TestMemoryStoredEvent:
    """Tests for emit_memory_stored factory."""

    @pytest.mark.asyncio
    async def test_emits_event(self, event_emitter, mock_event_repository):
        """Test that memory_stored event is emitted."""
        result = await event_emitter.emit_memory_stored(
            item_id="item-123",
            layer="L2",
            user_id="user-456",
            item_type="fact",
            metadata={"key": "value"},
        )

        assert result == "event-123"
        mock_event_repository.append.assert_called_once()

        # Verify event structure
        call_args = mock_event_repository.append.call_args
        event = call_args[0][0]
        assert event.aggregate_type == "memory"
        assert event.aggregate_id == "item-123"
        assert event.event_type == "memory_stored"
        assert event.payload["layer"] == "L2"
        assert event.payload["item_type"] == "fact"

    @pytest.mark.asyncio
    async def test_includes_session_id_for_dedup(self, event_emitter, mock_event_repository):
        """Test that session_id is used for deduplication."""
        await event_emitter.emit_memory_stored(
            item_id="item-123",
            layer="L2",
            user_id="user-456",
            item_type="message",
            session_id="session-789",
        )

        call_args = mock_event_repository.append.call_args
        event = call_args[0][0]
        assert event.user_id == "user-456"


class TestMemoryRetrievedEvent:
    """Tests for emit_memory_retrieved factory."""

    @pytest.mark.asyncio
    async def test_emits_event(self, event_emitter, mock_event_repository):
        """Test that memory_retrieved event is emitted."""
        result = await event_emitter.emit_memory_retrieved(
            user_id="user-456",
            query="search query",
            layer="L4",
            results_count=5,
            latency_ms=123.45,
        )

        assert result == "event-123"
        mock_event_repository.append.assert_called_once()

        call_args = mock_event_repository.append.call_args
        event = call_args[0][0]
        assert event.event_type == "memory_retrieved"
        assert event.payload["results_count"] == 5
        assert event.payload["latency_ms"] == 123.45

    @pytest.mark.asyncio
    async def test_truncates_long_queries(self, event_emitter, mock_event_repository):
        """Test that long queries are truncated."""
        long_query = "a" * 500
        await event_emitter.emit_memory_retrieved(
            user_id="user-456",
            query=long_query,
            layer="L4",
            results_count=0,
            latency_ms=50.0,
        )

        call_args = mock_event_repository.append.call_args
        event = call_args[0][0]
        assert len(event.payload["query"]) <= 200


class TestMemoryDeletedEvent:
    """Tests for emit_memory_deleted factory."""

    @pytest.mark.asyncio
    async def test_emits_soft_delete(self, event_emitter, mock_event_repository):
        """Test that soft delete event is emitted."""
        result = await event_emitter.emit_memory_deleted(
            item_id="item-123",
            layer="L2",
            user_id="user-456",
            soft_delete=True,
        )

        assert result == "event-123"

        call_args = mock_event_repository.append.call_args
        event = call_args[0][0]
        assert event.event_type == "memory_deleted"
        assert event.payload["soft_delete"] is True

    @pytest.mark.asyncio
    async def test_emits_hard_delete(self, event_emitter, mock_event_repository):
        """Test that hard delete event is emitted."""
        await event_emitter.emit_memory_deleted(
            item_id="item-123",
            layer="L2",
            user_id="user-456",
            soft_delete=False,
        )

        call_args = mock_event_repository.append.call_args
        event = call_args[0][0]
        assert event.payload["soft_delete"] is False


class TestSessionStateChangedEvent:
    """Tests for emit_session_state_changed factory."""

    @pytest.mark.asyncio
    async def test_emits_event(self, event_emitter, mock_event_repository):
        """Test that session_state_changed event is emitted."""
        result = await event_emitter.emit_session_state_changed(
            session_id="session-123",
            user_id="user-456",
            change_type="focus",
        )

        assert result == "event-123"

        call_args = mock_event_repository.append.call_args
        event = call_args[0][0]
        assert event.aggregate_type == "session"
        assert event.event_type == "session_state_changed"
        assert event.payload["change_type"] == "focus"


class TestFocusChangedEvent:
    """Tests for emit_focus_changed factory."""

    @pytest.mark.asyncio
    async def test_emits_event(self, event_emitter, mock_event_repository):
        """Test that focus_changed event is emitted."""
        result = await event_emitter.emit_focus_changed(
            session_id="session-123",
            focus_type="task",
            user_id="user-456",
            focus_id="task-789",
            focus_label="Buy groceries",
        )

        assert result == "event-123"

        call_args = mock_event_repository.append.call_args
        event = call_args[0][0]
        assert event.event_type == "focus_changed"
        assert event.payload["focus_type"] == "task"
        assert event.payload["focus_id"] == "task-789"
        assert event.payload["focus_label"] == "Buy groceries"


class TestEntityReferencedEvent:
    """Tests for emit_entity_referenced factory."""

    @pytest.mark.asyncio
    async def test_emits_event(self, event_emitter, mock_event_repository):
        """Test that entity_referenced event is emitted."""
        result = await event_emitter.emit_entity_referenced(
            session_id="session-123",
            entity_type="project",
            entity_id="project-456",
            label="Project Alpha",
            user_id="user-789",
        )

        assert result == "event-123"

        call_args = mock_event_repository.append.call_args
        event = call_args[0][0]
        assert event.event_type == "entity_referenced"
        assert event.payload["entity_type"] == "project"
        assert event.payload["entity_id"] == "project-456"
        assert event.payload["label"] == "Project Alpha"


class TestClarificationEvents:
    """Tests for clarification event factories."""

    @pytest.mark.asyncio
    async def test_emits_clarification_requested(self, event_emitter, mock_event_repository):
        """Test that clarification_requested event is emitted."""
        result = await event_emitter.emit_clarification_requested(
            session_id="session-123",
            question="Which project?",
            original_request="Add task to project",
            user_id="user-456",
            options=["Project A", "Project B"],
        )

        assert result == "event-123"

        call_args = mock_event_repository.append.call_args
        event = call_args[0][0]
        assert event.event_type == "clarification_requested"
        assert event.payload["question"] == "Which project?"
        assert event.payload["options"] == ["Project A", "Project B"]

    @pytest.mark.asyncio
    async def test_emits_clarification_resolved(self, event_emitter, mock_event_repository):
        """Test that clarification_resolved event is emitted."""
        result = await event_emitter.emit_clarification_resolved(
            session_id="session-123",
            answer="Project A",
            user_id="user-456",
            was_expired=False,
        )

        assert result == "event-123"

        call_args = mock_event_repository.append.call_args
        event = call_args[0][0]
        assert event.event_type == "clarification_resolved"
        assert event.payload["answer"] == "Project A"
        assert event.payload["was_expired"] is False

    @pytest.mark.asyncio
    async def test_handles_expired_clarification(self, event_emitter, mock_event_repository):
        """Test that expired clarification is correctly recorded."""
        await event_emitter.emit_clarification_resolved(
            session_id="session-123",
            answer="",
            user_id="user-456",
            was_expired=True,
        )

        call_args = mock_event_repository.append.call_args
        event = call_args[0][0]
        assert event.payload["was_expired"] is True



