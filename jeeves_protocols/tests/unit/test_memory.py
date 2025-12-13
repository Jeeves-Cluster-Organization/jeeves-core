"""Unit tests for memory types and operations.

Tests WorkingMemory, FocusState, EntityRef, and related operations.
"""

import pytest
from datetime import datetime, timezone


class TestWorkingMemory:
    """Tests for WorkingMemory dataclass."""

    def test_create_working_memory(self):
        """Test creating working memory."""
        from jeeves_protocols import WorkingMemory

        memory = WorkingMemory(
            session_id="sess-abc",
            user_id="user-789",
        )

        assert memory.session_id == "sess-abc"
        assert memory.user_id == "user-789"
        assert memory.current_focus is None
        assert len(memory.entities) == 0
        assert len(memory.findings) == 0

    def test_working_memory_add_entity(self):
        """Test adding entity to working memory."""
        from jeeves_protocols import WorkingMemory, EntityRef

        memory = WorkingMemory(session_id="sess", user_id="user")
        entity = EntityRef(
            entity_type="file",
            entity_id="file-123",
            name="test.py",
        )

        memory.add_entity(entity)

        assert len(memory.entities) == 1
        assert memory.entities[0].entity_id == "file-123"


class TestFocusState:
    """Tests for FocusState dataclass."""

    def test_create_focus_state(self):
        """Test creating focus state."""
        from jeeves_protocols import FocusState, FocusType

        focus = FocusState(
            focus_type=FocusType.FILE,
            focus_id="file-123",
            focus_name="main.py",
            context={"path": "/src/main.py"},
        )

        assert focus.focus_type == FocusType.FILE
        assert focus.focus_id == "file-123"
        assert focus.focus_name == "main.py"

    def test_set_focus(self):
        """Test setting focus on working memory."""
        from jeeves_protocols import WorkingMemory, FocusState, FocusType

        memory = WorkingMemory(session_id="sess", user_id="user")
        focus = FocusState(
            focus_type=FocusType.FILE,
            focus_id="file-123",
            focus_name="test.py",
        )

        memory.set_focus(focus)

        assert memory.current_focus is not None
        assert memory.current_focus.focus_type == FocusType.FILE

    def test_clear_focus(self):
        """Test clearing focus from working memory."""
        from jeeves_protocols import WorkingMemory, FocusState, FocusType

        memory = WorkingMemory(session_id="sess", user_id="user")
        focus = FocusState(
            focus_type=FocusType.FILE,
            focus_id="file-123",
            focus_name="test.py",
        )

        memory.set_focus(focus)
        memory.clear_focus()

        assert memory.current_focus is None
        assert len(memory.focus_history) == 1


class TestFocusType:
    """Tests for FocusType enum."""

    def test_focus_types(self):
        """Test FocusType enum values."""
        from jeeves_protocols import FocusType

        assert FocusType.FILE is not None
        assert FocusType.FUNCTION is not None
        assert FocusType.CLASS is not None
        assert FocusType.MODULE is not None
        assert FocusType.CONCEPT is not None


class TestEntityRef:
    """Tests for EntityRef dataclass."""

    def test_create_entity_ref(self):
        """Test creating entity reference."""
        from jeeves_protocols import EntityRef

        entity = EntityRef(
            entity_type="file",
            entity_id="file-123",
            name="test.py",
            context="source file",
        )

        assert entity.entity_type == "file"
        assert entity.entity_id == "file-123"
        assert entity.name == "test.py"
        assert entity.context == "source file"


class TestFinding:
    """Tests for Finding dataclass."""

    def test_create_finding(self):
        """Test creating a finding."""
        from jeeves_protocols import Finding

        finding = Finding(
            finding_id="find-123",
            finding_type="bug",
            title="Memory leak detected",
            description="Unclosed file handle in process_data()",
            location="src/utils.py:42",
            severity="high",
        )

        assert finding.finding_id == "find-123"
        assert finding.finding_type == "bug"
        assert finding.severity == "high"

    def test_add_finding_to_memory(self):
        """Test adding finding to working memory."""
        from jeeves_protocols import WorkingMemory, Finding

        memory = WorkingMemory(session_id="sess", user_id="user")
        finding = Finding(
            finding_id="find-123",
            finding_type="issue",
            title="Test Finding",
            description="A test finding",
        )

        memory.add_finding(finding)

        assert len(memory.findings) == 1
        assert memory.findings[0].finding_id == "find-123"
