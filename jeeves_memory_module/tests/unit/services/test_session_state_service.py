"""Unit tests for SessionStateService.

Tests for L4 Working Memory session state management including:
- Session lifecycle (get_or_create, delete)
- Focus tracking (update_focus, clear_focus)
- Entity reference management
- Turn counting and summarization triggers
- Context formatting for prompts
"""

import pytest
from datetime import datetime, timezone
from unittest.mock import AsyncMock, MagicMock, patch

from jeeves_memory_module.services.session_state_service import SessionStateService
from jeeves_memory_module.repositories.session_state_repository import (
    SessionStateRepository,
    SessionState
)


class TestService:
    """Tests for SessionStateService - main service functionality."""

    @pytest.fixture
    def mock_db(self):
        """Create mock database client."""
        db = AsyncMock()
        db.execute = AsyncMock()
        db.fetch_one = AsyncMock()
        db.fetch_all = AsyncMock()
        return db

    @pytest.fixture
    def mock_repository(self):
        """Create mock session state repository."""
        repo = AsyncMock(spec=SessionStateRepository)
        repo.ensure_table = AsyncMock()
        repo.get = AsyncMock()
        repo.upsert = AsyncMock()
        repo.update_focus = AsyncMock()
        repo.add_referenced_entity = AsyncMock()
        repo.increment_turn = AsyncMock()
        repo.update_short_term_memory = AsyncMock()
        repo.delete = AsyncMock()
        return repo

    @pytest.fixture
    def service(self, mock_db, mock_repository):
        """Create service with mocked dependencies."""
        return SessionStateService(db=mock_db, repository=mock_repository)

    @pytest.fixture
    def sample_session_state(self):
        """Create sample session state for testing."""
        return SessionState(
            session_id="sess-123",
            user_id="user-456",
            focus_type="task",
            focus_id="task-789",
            focus_context={"last_action": "created"},
            referenced_entities=[
                {"type": "task", "id": "task-789", "title": "Test Task"}
            ],
            short_term_memory="User created a task about testing",
            turn_count=5
        )

    # ========== Initialization Tests ==========

    @pytest.mark.asyncio
    async def test_ensure_initialized_calls_repository(self, service, mock_repository):
        """Test that ensure_initialized creates the table."""
        await service.ensure_initialized()
        mock_repository.ensure_table.assert_called_once()

    # ========== Get or Create Tests ==========

    @pytest.mark.asyncio
    async def test_get_or_create_returns_existing_state(
        self, service, mock_repository, sample_session_state
    ):
        """Test get_or_create returns existing session state."""
        mock_repository.get.return_value = sample_session_state

        result = await service.get_or_create("sess-123", "user-456")

        mock_repository.get.assert_called_once_with("sess-123")
        mock_repository.upsert.assert_not_called()
        assert result.session_id == "sess-123"
        assert result.turn_count == 5

    @pytest.mark.asyncio
    async def test_get_or_create_creates_new_state_when_not_exists(
        self, service, mock_repository
    ):
        """Test get_or_create creates new state if none exists."""
        mock_repository.get.return_value = None
        mock_repository.upsert.return_value = SessionState(
            session_id="sess-new",
            user_id="user-456",
            focus_type="general",
            turn_count=0
        )

        result = await service.get_or_create("sess-new", "user-456")

        mock_repository.get.assert_called_once_with("sess-new")
        mock_repository.upsert.assert_called_once()

        # Verify the created state has correct defaults
        created_state = mock_repository.upsert.call_args[0][0]
        assert created_state.session_id == "sess-new"
        assert created_state.user_id == "user-456"
        assert created_state.focus_type == "general"
        assert created_state.turn_count == 0

    @pytest.mark.asyncio
    async def test_get_or_create_sets_correct_user_id(self, service, mock_repository):
        """Test that get_or_create associates state with correct user."""
        mock_repository.get.return_value = None
        mock_repository.upsert.return_value = SessionState(
            session_id="sess-1",
            user_id="specific-user"
        )

        await service.get_or_create("sess-1", "specific-user")

        created_state = mock_repository.upsert.call_args[0][0]
        assert created_state.user_id == "specific-user"

    # ========== Get Tests ==========

    @pytest.mark.asyncio
    async def test_get_returns_session_state(
        self, service, mock_repository, sample_session_state
    ):
        """Test get returns session state by ID."""
        mock_repository.get.return_value = sample_session_state

        result = await service.get("sess-123")

        mock_repository.get.assert_called_once_with("sess-123")
        assert result == sample_session_state

    @pytest.mark.asyncio
    async def test_get_returns_none_when_not_found(self, service, mock_repository):
        """Test get returns None when session not found."""
        mock_repository.get.return_value = None

        result = await service.get("nonexistent")

        assert result is None

    # ========== Focus Tracking Tests ==========

    @pytest.mark.asyncio
    async def test_update_focus_changes_focus_type(
        self, service, mock_repository, sample_session_state
    ):
        """Test update_focus changes the focus type."""
        updated_state = SessionState(
            session_id="sess-123",
            user_id="user-456",
            focus_type="journal",
            focus_id="journal-1"
        )
        mock_repository.update_focus.return_value = updated_state

        result = await service.update_focus(
            session_id="sess-123",
            focus_type="journal",
            focus_id="journal-1",
            focus_context={"entry_type": "note"}
        )

        mock_repository.update_focus.assert_called_once_with(
            session_id="sess-123",
            focus_type="journal",
            focus_id="journal-1",
            focus_context={"entry_type": "note"}
        )
        assert result.focus_type == "journal"

    @pytest.mark.asyncio
    async def test_update_focus_returns_none_when_session_not_found(
        self, service, mock_repository
    ):
        """Test update_focus returns None if session doesn't exist."""
        mock_repository.update_focus.return_value = None

        result = await service.update_focus(
            session_id="nonexistent",
            focus_type="task"
        )

        assert result is None

    @pytest.mark.asyncio
    async def test_clear_focus_resets_to_general(self, service, mock_repository):
        """Test clear_focus resets focus to general mode."""
        cleared_state = SessionState(
            session_id="sess-123",
            user_id="user-456",
            focus_type="general",
            focus_id=None,
            focus_context={}
        )
        mock_repository.update_focus.return_value = cleared_state

        result = await service.clear_focus("sess-123")

        mock_repository.update_focus.assert_called_once_with(
            session_id="sess-123",
            focus_type="general",
            focus_id=None,
            focus_context={}
        )
        assert result.focus_type == "general"
        assert result.focus_id is None

    # ========== Entity Reference Tests ==========

    @pytest.mark.asyncio
    async def test_record_entity_reference_adds_entity(
        self, service, mock_repository, sample_session_state
    ):
        """Test record_entity_reference adds entity to session."""
        updated_state = SessionState(
            session_id="sess-123",
            user_id="user-456",
            referenced_entities=[
                {"type": "task", "id": "task-789", "title": "Test Task"},
                {"type": "task", "id": "task-new", "title": "New Task"}
            ]
        )
        mock_repository.add_referenced_entity.return_value = updated_state

        result = await service.record_entity_reference(
            session_id="sess-123",
            entity_type="task",
            entity_id="task-new",
            entity_title="New Task"
        )

        mock_repository.add_referenced_entity.assert_called_once_with(
            session_id="sess-123",
            entity_type="task",
            entity_id="task-new",
            entity_title="New Task"
        )
        assert len(result.referenced_entities) == 2

    @pytest.mark.asyncio
    async def test_record_entity_reference_without_title(
        self, service, mock_repository
    ):
        """Test record_entity_reference works without title."""
        mock_repository.add_referenced_entity.return_value = SessionState(
            session_id="sess-123",
            user_id="user-456",
            referenced_entities=[{"type": "journal", "id": "j-1"}]
        )

        result = await service.record_entity_reference(
            session_id="sess-123",
            entity_type="journal",
            entity_id="j-1"
        )

        mock_repository.add_referenced_entity.assert_called_once()
        assert result is not None

    # ========== Turn Counting Tests ==========

    @pytest.mark.asyncio
    async def test_on_user_turn_increments_turn_count(
        self, service, mock_repository
    ):
        """Test on_user_turn increments the turn counter."""
        updated_state = SessionState(
            session_id="sess-123",
            user_id="user-456",
            turn_count=6
        )
        mock_repository.increment_turn.return_value = updated_state

        result = await service.on_user_turn("sess-123", "Hello")

        mock_repository.increment_turn.assert_called_once_with("sess-123")
        assert result.turn_count == 6

    @pytest.mark.asyncio
    async def test_on_user_turn_returns_none_when_session_not_found(
        self, service, mock_repository
    ):
        """Test on_user_turn returns None if session doesn't exist."""
        mock_repository.increment_turn.return_value = None

        result = await service.on_user_turn("nonexistent", "Hello")

        assert result is None

    # ========== Memory Update Tests ==========

    @pytest.mark.asyncio
    async def test_update_memory_sets_summary(self, service, mock_repository):
        """Test update_memory updates short-term memory."""
        updated_state = SessionState(
            session_id="sess-123",
            user_id="user-456",
            short_term_memory="User discussed task management"
        )
        mock_repository.update_short_term_memory.return_value = updated_state

        result = await service.update_memory(
            "sess-123",
            "User discussed task management"
        )

        mock_repository.update_short_term_memory.assert_called_once_with(
            session_id="sess-123",
            memory="User discussed task management"
        )
        assert result.short_term_memory == "User discussed task management"

    # ========== Summarization Trigger Tests ==========

    @pytest.mark.asyncio
    async def test_should_summarize_returns_true_at_threshold(
        self, service, mock_repository
    ):
        """Test should_summarize returns True at summarization threshold."""
        mock_repository.get.return_value = SessionState(
            session_id="sess-123",
            user_id="user-456",
            turn_count=5  # Default threshold
        )

        result = await service.should_summarize("sess-123")

        assert result is True

    @pytest.mark.asyncio
    async def test_should_summarize_returns_false_below_threshold(
        self, service, mock_repository
    ):
        """Test should_summarize returns False below threshold."""
        mock_repository.get.return_value = SessionState(
            session_id="sess-123",
            user_id="user-456",
            turn_count=3
        )

        result = await service.should_summarize("sess-123")

        assert result is False

    @pytest.mark.asyncio
    async def test_should_summarize_returns_false_at_zero_turns(
        self, service, mock_repository
    ):
        """Test should_summarize returns False with zero turns."""
        mock_repository.get.return_value = SessionState(
            session_id="sess-123",
            user_id="user-456",
            turn_count=0
        )

        result = await service.should_summarize("sess-123")

        assert result is False

    @pytest.mark.asyncio
    async def test_should_summarize_custom_threshold(
        self, service, mock_repository
    ):
        """Test should_summarize with custom threshold."""
        mock_repository.get.return_value = SessionState(
            session_id="sess-123",
            user_id="user-456",
            turn_count=10
        )

        # Every 10 turns
        result = await service.should_summarize("sess-123", summarize_every_n_turns=10)
        assert result is True

        # Not at 10 if threshold is 3
        mock_repository.get.return_value.turn_count = 7
        result = await service.should_summarize("sess-123", summarize_every_n_turns=3)
        assert result is False  # 7 % 3 = 1, not 0

    @pytest.mark.asyncio
    async def test_should_summarize_returns_false_when_session_not_found(
        self, service, mock_repository
    ):
        """Test should_summarize returns False if session doesn't exist."""
        mock_repository.get.return_value = None

        result = await service.should_summarize("nonexistent")

        assert result is False

    # ========== Context for Prompt Tests ==========

    @pytest.mark.asyncio
    async def test_get_context_for_prompt_includes_turn_count(
        self, service, mock_repository, sample_session_state
    ):
        """Test get_context_for_prompt includes turn count."""
        mock_repository.get.return_value = sample_session_state

        context = await service.get_context_for_prompt("sess-123")

        assert "turn_count" in context
        assert context["turn_count"] == 5

    @pytest.mark.asyncio
    async def test_get_context_for_prompt_includes_focus(
        self, service, mock_repository, sample_session_state
    ):
        """Test get_context_for_prompt includes focus info."""
        mock_repository.get.return_value = sample_session_state

        context = await service.get_context_for_prompt("sess-123")

        assert "focus" in context
        assert context["focus"]["type"] == "task"
        assert context["focus"]["id"] == "task-789"
        assert context["focus"]["context"] == {"last_action": "created"}

    @pytest.mark.asyncio
    async def test_get_context_for_prompt_includes_recent_entities(
        self, service, mock_repository, sample_session_state
    ):
        """Test get_context_for_prompt includes recent entity references."""
        mock_repository.get.return_value = sample_session_state

        context = await service.get_context_for_prompt("sess-123")

        assert "recent_entities" in context
        assert len(context["recent_entities"]) == 1
        assert context["recent_entities"][0]["type"] == "task"

    @pytest.mark.asyncio
    async def test_get_context_for_prompt_includes_conversation_summary(
        self, service, mock_repository, sample_session_state
    ):
        """Test get_context_for_prompt includes short-term memory summary."""
        mock_repository.get.return_value = sample_session_state

        context = await service.get_context_for_prompt("sess-123")

        assert "conversation_summary" in context
        assert context["conversation_summary"] == "User created a task about testing"

    @pytest.mark.asyncio
    async def test_get_context_for_prompt_limits_recent_entities(
        self, service, mock_repository
    ):
        """Test get_context_for_prompt limits recent entities to 5."""
        state = SessionState(
            session_id="sess-123",
            user_id="user-456",
            referenced_entities=[
                {"type": "task", "id": f"task-{i}"} for i in range(10)
            ]
        )
        mock_repository.get.return_value = state

        context = await service.get_context_for_prompt("sess-123")

        # Should only include last 5
        assert len(context["recent_entities"]) == 5
        # Should be the last 5 (task-5 through task-9)
        assert context["recent_entities"][0]["id"] == "task-5"

    @pytest.mark.asyncio
    async def test_get_context_for_prompt_returns_empty_dict_when_not_found(
        self, service, mock_repository
    ):
        """Test get_context_for_prompt returns empty dict if session not found."""
        mock_repository.get.return_value = None

        context = await service.get_context_for_prompt("nonexistent")

        assert context == {}

    @pytest.mark.asyncio
    async def test_get_context_for_prompt_handles_no_focus(
        self, service, mock_repository
    ):
        """Test get_context_for_prompt handles sessions with no focus."""
        state = SessionState(
            session_id="sess-123",
            user_id="user-456",
            focus_type=None,
            turn_count=2
        )
        mock_repository.get.return_value = state

        context = await service.get_context_for_prompt("sess-123")

        assert context["focus"] is None

    @pytest.mark.asyncio
    async def test_get_context_for_prompt_handles_no_memory(
        self, service, mock_repository
    ):
        """Test get_context_for_prompt handles sessions with no summary."""
        state = SessionState(
            session_id="sess-123",
            user_id="user-456",
            short_term_memory=None,
            turn_count=2
        )
        mock_repository.get.return_value = state

        context = await service.get_context_for_prompt("sess-123")

        assert "conversation_summary" not in context

    # ========== Delete Tests ==========

    @pytest.mark.asyncio
    async def test_delete_removes_session_state(self, service, mock_repository):
        """Test delete removes session state."""
        mock_repository.delete.return_value = True

        result = await service.delete("sess-123")

        mock_repository.delete.assert_called_once_with("sess-123")
        assert result is True

    @pytest.mark.asyncio
    async def test_delete_returns_false_when_not_found(self, service, mock_repository):
        """Test delete returns False if session not found."""
        mock_repository.delete.return_value = False

        result = await service.delete("nonexistent")

        assert result is False


class TestDataclass:
    """Tests for SessionState dataclass serialization."""

    def test_session_state_to_dict(self):
        """Test SessionState serialization."""
        now = datetime.now(timezone.utc)
        state = SessionState(
            session_id="sess-1",
            user_id="user-1",
            focus_type="task",
            focus_id="task-1",
            focus_context={"key": "value"},
            referenced_entities=[{"type": "task", "id": "task-1"}],
            short_term_memory="summary",
            turn_count=5,
            created_at=now,
            updated_at=now
        )

        result = state.to_dict()

        assert result["session_id"] == "sess-1"
        assert result["user_id"] == "user-1"
        assert result["focus_type"] == "task"
        assert result["focus_id"] == "task-1"
        assert result["focus_context"] == {"key": "value"}
        assert result["referenced_entities"] == [{"type": "task", "id": "task-1"}]
        assert result["short_term_memory"] == "summary"
        assert result["turn_count"] == 5
        assert result["created_at"] == now.isoformat()

    def test_session_state_from_dict(self):
        """Test SessionState deserialization."""
        now = datetime.now(timezone.utc)
        data = {
            "session_id": "sess-1",
            "user_id": "user-1",
            "focus_type": "journal",
            "focus_id": None,
            "focus_context": {},
            "referenced_entities": [],
            "short_term_memory": None,
            "turn_count": 3,
            "created_at": now.isoformat(),
            "updated_at": now.isoformat()
        }

        state = SessionState.from_dict(data)

        assert state.session_id == "sess-1"
        assert state.focus_type == "journal"
        assert state.turn_count == 3
        assert state.created_at == now

    def test_session_state_defaults(self):
        """Test SessionState default values."""
        state = SessionState(
            session_id="sess-1",
            user_id="user-1"
        )

        assert state.focus_type is None
        assert state.focus_id is None
        assert state.focus_context == {}
        assert state.referenced_entities == []
        assert state.short_term_memory is None
        assert state.turn_count == 0
        assert state.created_at is not None
        assert state.updated_at is not None
