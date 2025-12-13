"""Unit tests for EventRepository."""

import pytest
from datetime import datetime, timezone
from unittest.mock import AsyncMock, MagicMock, patch
from uuid import uuid4

from jeeves_memory_module.repositories.event_repository import EventRepository, DomainEvent


class TestEvent:
    """Tests for DomainEvent dataclass."""

    def test_create_event(self):
        """Test creating a domain event."""
        event = DomainEvent(
            aggregate_type="task",
            aggregate_id="task-123",
            event_type="task_created",
            payload={"title": "Test Task"},
            user_id="user-1",
            actor="user"
        )

        assert event.aggregate_type == "task"
        assert event.aggregate_id == "task-123"
        assert event.event_type == "task_created"
        assert event.payload == {"title": "Test Task"}
        assert event.user_id == "user-1"
        assert event.actor == "user"
        assert event.event_id is not None
        assert event.timestamp is not None
        assert event.schema_version == 1

    def test_idempotency_key(self):
        """Test idempotency key generation."""
        event = DomainEvent(
            aggregate_type="task",
            aggregate_id="task-123",
            event_type="task_created",
            payload={},
            user_id="user-1"
        )

        expected = "task:task-123:task_created:1"
        assert event.idempotency_key == expected

    def test_to_dict(self):
        """Test conversion to dictionary."""
        event = DomainEvent(
            event_id="evt-1",
            aggregate_type="task",
            aggregate_id="task-123",
            event_type="task_created",
            payload={"title": "Test"},
            user_id="user-1",
            actor="user",
            correlation_id="corr-1",
            causation_id="cause-1"
        )

        result = event.to_dict()

        assert result["event_id"] == "evt-1"
        assert result["aggregate_type"] == "task"
        assert result["aggregate_id"] == "task-123"
        assert result["event_type"] == "task_created"
        assert result["payload"] == {"title": "Test"}
        assert result["user_id"] == "user-1"
        assert result["actor"] == "user"
        assert result["correlation_id"] == "corr-1"
        assert result["causation_id"] == "cause-1"

    def test_from_dict(self):
        """Test creating event from dictionary."""
        data = {
            "event_id": "evt-1",
            "aggregate_type": "journal",
            "aggregate_id": "journal-1",
            "event_type": "journal_ingested",
            "payload": {"word_count": 100},
            "user_id": "user-1",
            "actor": "system",
            "timestamp": "2025-01-15T10:00:00+00:00"
        }

        event = DomainEvent.from_dict(data)

        assert event.event_id == "evt-1"
        assert event.aggregate_type == "journal"
        assert event.event_type == "journal_ingested"
        assert event.payload == {"word_count": 100}


class TestRepository:
    """Tests for EventRepository persistence."""

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
        return EventRepository(mock_db)

    @pytest.mark.asyncio
    async def test_append_event(self, repository, mock_db):
        """Test appending an event."""
        event = DomainEvent(
            aggregate_type="task",
            aggregate_id="task-123",
            event_type="task_created",
            payload={"title": "Test"},
            user_id="user-1"
        )

        result = await repository.append(event)

        assert result == event.event_id
        mock_db.execute.assert_called_once()

    @pytest.mark.asyncio
    async def test_append_duplicate_event(self, repository, mock_db):
        """Test that duplicate events are handled idempotently."""
        event = DomainEvent(
            aggregate_type="task",
            aggregate_id="task-123",
            event_type="task_created",
            payload={},
            user_id="user-1"
        )

        # Simulate duplicate key error
        mock_db.execute.side_effect = Exception("UNIQUE constraint failed")

        result = await repository.append(event)

        # Should return event ID (idempotent behavior)
        assert result == event.event_id

    @pytest.mark.asyncio
    async def test_get_by_id(self, repository, mock_db):
        """Test retrieving event by ID."""
        mock_db.fetch_one.return_value = {
            "event_id": "evt-1",
            "occurred_at": datetime.now(timezone.utc),
            "aggregate_type": "task",
            "aggregate_id": "task-123",
            "event_type": "task_created",
            "payload": {"title": "Test"},
            "user_id": "user-1",
            "actor": "user"
        }

        result = await repository.get_by_id("evt-1")

        assert result is not None
        assert result.event_id == "evt-1"
        assert result.event_type == "task_created"

    @pytest.mark.asyncio
    async def test_get_by_correlation(self, repository, mock_db):
        """Test retrieving events by correlation ID."""
        mock_db.fetch_all.return_value = [
            {
                "event_id": "evt-1",
                "occurred_at": datetime.now(timezone.utc),
                "aggregate_type": "task",
                "aggregate_id": "task-123",
                "event_type": "task_created",
                "payload": {},
                "user_id": "user-1",
                "actor": "user",
                "correlation_id": "corr-1"
            }
        ]

        result = await repository.get_by_correlation("corr-1")

        assert len(result) == 1
        assert result[0].event_id == "evt-1"

    @pytest.mark.asyncio
    async def test_get_by_aggregate(self, repository, mock_db):
        """Test retrieving events by aggregate."""
        mock_db.fetch_all.return_value = [
            {
                "event_id": "evt-1",
                "occurred_at": datetime.now(timezone.utc),
                "aggregate_type": "task",
                "aggregate_id": "task-123",
                "event_type": "task_created",
                "payload": {},
                "user_id": "user-1",
                "actor": "user"
            },
            {
                "event_id": "evt-2",
                "occurred_at": datetime.now(timezone.utc),
                "aggregate_type": "task",
                "aggregate_id": "task-123",
                "event_type": "task_completed",
                "payload": {},
                "user_id": "user-1",
                "actor": "user"
            }
        ]

        result = await repository.get_by_aggregate("task", "task-123")

        assert len(result) == 2
        assert result[0].event_type == "task_created"
        assert result[1].event_type == "task_completed"

    @pytest.mark.asyncio
    async def test_count_by_type(self, repository, mock_db):
        """Test counting events by type."""
        mock_db.fetch_all.return_value = [
            {"event_type": "task_created", "count": 10},
            {"event_type": "task_completed", "count": 5}
        ]

        result = await repository.count_by_type("user-1")

        assert result["task_created"] == 10
        assert result["task_completed"] == 5
