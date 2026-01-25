"""Unit tests for PostgresCheckpointAdapter.

Tests checkpoint storage, retrieval, and time-travel debugging functionality.
Coverage target: 80%+

Test Strategy:
- Test all public methods (save, load, list, delete, fork)
- Test edge cases (not found, empty results, boundaries)
- Test sequential checkpoint ordering and parent linking
- Test fork/branch functionality for time-travel debugging
- Mock PostgreSQL client to avoid infrastructure dependencies
"""

import pytest
from datetime import datetime, timezone
from unittest.mock import AsyncMock, MagicMock
from typing import Any, Dict

from avionics.checkpoint.postgres_adapter import PostgresCheckpointAdapter
from protocols import CheckpointRecord


# =============================================================================
# Test Fixtures
# =============================================================================

@pytest.fixture
def mock_db():
    """Mock PostgreSQL client."""
    db = AsyncMock()
    db.execute_script = AsyncMock()
    db.insert = AsyncMock()
    db.fetch_one = AsyncMock()
    db.fetch_all = AsyncMock()
    db.execute = AsyncMock()
    return db


@pytest.fixture
def mock_logger():
    """Mock logger."""
    logger = MagicMock()
    return logger


@pytest.fixture
def adapter(mock_db, mock_logger):
    """Create adapter with mocked dependencies."""
    return PostgresCheckpointAdapter(mock_db, mock_logger)


# =============================================================================
# Initialization Tests
# =============================================================================

class TestInitialization:
    """Test adapter initialization."""

    def test_init_stores_dependencies(self, mock_db, mock_logger):
        """Test that adapter stores db and logger."""
        adapter = PostgresCheckpointAdapter(mock_db, mock_logger)

        assert adapter._db is mock_db
        assert adapter._logger is mock_logger
        assert adapter._stage_counter == {}

    def test_init_without_logger(self, mock_db):
        """Test that adapter works without explicit logger."""
        adapter = PostgresCheckpointAdapter(mock_db)

        assert adapter._db is mock_db
        assert adapter._logger is not None  # Should get default logger

    @pytest.mark.asyncio
    async def test_initialize_schema(self, adapter, mock_db, mock_logger):
        """Test schema initialization."""
        await adapter.initialize_schema()

        # Should execute schema SQL
        mock_db.execute_script.assert_called_once()
        sql = mock_db.execute_script.call_args[0][0]
        assert "CREATE TABLE IF NOT EXISTS execution_checkpoints" in sql
        assert "checkpoint_id TEXT PRIMARY KEY" in sql
        assert "state_json JSONB NOT NULL" in sql

        # Should log
        mock_logger.info.assert_called_with("checkpoint_schema_initialized")


# =============================================================================
# Save Checkpoint Tests
# =============================================================================

class TestSaveCheckpoint:
    """Test checkpoint saving."""

    @pytest.mark.asyncio
    async def test_save_first_checkpoint(self, adapter, mock_db):
        """Test saving first checkpoint (stage_order=0, no parent)."""
        # Setup
        envelope_id = "env_123"
        checkpoint_id = "ckpt_abc"
        agent_name = "planner"
        state = {"foo": "bar", "baz": [1, 2, 3]}
        metadata = {"duration_ms": 1500}

        # No previous checkpoints
        mock_db.fetch_one.return_value = None

        # Execute
        record = await adapter.save_checkpoint(
            envelope_id=envelope_id,
            checkpoint_id=checkpoint_id,
            agent_name=agent_name,
            state=state,
            metadata=metadata,
        )

        # Verify database insert
        mock_db.insert.assert_called_once()
        call_args = mock_db.insert.call_args
        assert call_args[0][0] == "execution_checkpoints"

        insert_data = call_args[0][1]
        assert insert_data["checkpoint_id"] == checkpoint_id
        assert insert_data["envelope_id"] == envelope_id
        assert insert_data["agent_name"] == agent_name
        assert insert_data["stage_order"] == 0  # First checkpoint
        assert insert_data["state_json"] == state
        assert insert_data["metadata_json"] == metadata
        assert insert_data["parent_checkpoint_id"] is None  # No parent
        assert isinstance(insert_data["created_at"], datetime)

        # Verify returned record
        assert record.checkpoint_id == checkpoint_id
        assert record.envelope_id == envelope_id
        assert record.agent_name == agent_name
        assert record.stage_order == 0
        assert record.parent_checkpoint_id is None
        assert record.metadata == metadata

        # Verify stage counter updated
        assert adapter._stage_counter[envelope_id] == 1

    @pytest.mark.asyncio
    async def test_save_second_checkpoint_links_parent(self, adapter, mock_db):
        """Test that second checkpoint links to first as parent."""
        envelope_id = "env_123"

        # Save first checkpoint
        mock_db.fetch_one.return_value = None
        await adapter.save_checkpoint(
            envelope_id=envelope_id,
            checkpoint_id="ckpt_1",
            agent_name="planner",
            state={"step": 1},
        )

        # Setup for second checkpoint - mock previous checkpoint query
        mock_db.fetch_one.return_value = {"checkpoint_id": "ckpt_1"}

        # Save second checkpoint
        record = await adapter.save_checkpoint(
            envelope_id=envelope_id,
            checkpoint_id="ckpt_2",
            agent_name="executor",
            state={"step": 2},
        )

        # Should query for parent
        mock_db.fetch_one.assert_called_with(
            """
                SELECT checkpoint_id FROM execution_checkpoints
                WHERE envelope_id = :envelope_id
                ORDER BY stage_order DESC LIMIT 1
                """,
            {"envelope_id": envelope_id},
        )

        # Second checkpoint should link to first
        assert record.stage_order == 1
        assert record.parent_checkpoint_id == "ckpt_1"
        assert adapter._stage_counter[envelope_id] == 2

    @pytest.mark.asyncio
    async def test_save_checkpoint_without_metadata(self, adapter, mock_db):
        """Test saving checkpoint without optional metadata."""
        mock_db.fetch_one.return_value = None

        record = await adapter.save_checkpoint(
            envelope_id="env_123",
            checkpoint_id="ckpt_abc",
            agent_name="planner",
            state={"foo": "bar"},
            metadata=None,  # No metadata
        )

        # Should accept None metadata
        assert record.metadata is None

        insert_data = mock_db.insert.call_args[0][1]
        assert insert_data["metadata_json"] is None

    @pytest.mark.asyncio
    async def test_save_multiple_checkpoints_in_sequence(self, adapter, mock_db):
        """Test saving multiple checkpoints maintains order."""
        envelope_id = "env_123"

        # First checkpoint - no parent (stage_order 0)
        mock_db.fetch_one.return_value = None
        r1 = await adapter.save_checkpoint(envelope_id, "ckpt_1", "agent1", {"step": 1})

        # Second checkpoint - parent is ckpt_1 (stage_order 1)
        mock_db.fetch_one.return_value = {"checkpoint_id": "ckpt_1"}
        r2 = await adapter.save_checkpoint(envelope_id, "ckpt_2", "agent2", {"step": 2})

        # Third checkpoint - parent is ckpt_2 (stage_order 2)
        mock_db.fetch_one.return_value = {"checkpoint_id": "ckpt_2"}
        r3 = await adapter.save_checkpoint(envelope_id, "ckpt_3", "agent3", {"step": 3})

        # Verify sequential ordering
        assert r1.stage_order == 0
        assert r2.stage_order == 1
        assert r3.stage_order == 2

        # Verify parent chain
        assert r1.parent_checkpoint_id is None
        assert r2.parent_checkpoint_id == "ckpt_1"
        assert r3.parent_checkpoint_id == "ckpt_2"

        # Verify counter
        assert adapter._stage_counter[envelope_id] == 3


# =============================================================================
# Load Checkpoint Tests
# =============================================================================

class TestLoadCheckpoint:
    """Test checkpoint loading."""

    @pytest.mark.asyncio
    async def test_load_existing_checkpoint(self, adapter, mock_db, mock_logger):
        """Test loading checkpoint that exists."""
        checkpoint_id = "ckpt_abc"
        state = {"foo": "bar", "nested": {"key": "value"}}

        mock_db.fetch_one.return_value = {"state_json": state}

        result = await adapter.load_checkpoint(checkpoint_id)

        # Verify query
        mock_db.fetch_one.assert_called_once_with(
            """
            SELECT state_json FROM execution_checkpoints
            WHERE checkpoint_id = :checkpoint_id
            """,
            {"checkpoint_id": checkpoint_id},
        )

        # Should return state
        assert result == state

        # Should log
        mock_logger.debug.assert_called_with(
            "checkpoint_loaded",
            checkpoint_id=checkpoint_id,
        )

    @pytest.mark.asyncio
    async def test_load_nonexistent_checkpoint(self, adapter, mock_db, mock_logger):
        """Test loading checkpoint that doesn't exist."""
        checkpoint_id = "ckpt_missing"

        mock_db.fetch_one.return_value = None

        result = await adapter.load_checkpoint(checkpoint_id)

        # Should return None
        assert result is None

        # Should log warning
        mock_logger.warning.assert_called_with(
            "checkpoint_not_found",
            checkpoint_id=checkpoint_id,
        )


# =============================================================================
# List Checkpoints Tests
# =============================================================================

class TestListCheckpoints:
    """Test checkpoint listing."""

    @pytest.mark.asyncio
    async def test_list_empty_checkpoints(self, adapter, mock_db):
        """Test listing when no checkpoints exist."""
        mock_db.fetch_all.return_value = []

        results = await adapter.list_checkpoints("env_123")

        assert results == []
        mock_db.fetch_all.assert_called_once()

    @pytest.mark.asyncio
    async def test_list_single_checkpoint(self, adapter, mock_db):
        """Test listing single checkpoint."""
        envelope_id = "env_123"
        created_at = datetime.now(timezone.utc)

        mock_db.fetch_all.return_value = [
            {
                "checkpoint_id": "ckpt_1",
                "envelope_id": envelope_id,
                "agent_name": "planner",
                "stage_order": 0,
                "created_at": created_at,
                "parent_checkpoint_id": None,
                "metadata_json": {"duration_ms": 1000},
            }
        ]

        results = await adapter.list_checkpoints(envelope_id)

        assert len(results) == 1
        record = results[0]
        assert record.checkpoint_id == "ckpt_1"
        assert record.envelope_id == envelope_id
        assert record.agent_name == "planner"
        assert record.stage_order == 0
        assert record.created_at == created_at
        assert record.parent_checkpoint_id is None
        assert record.metadata == {"duration_ms": 1000}

    @pytest.mark.asyncio
    async def test_list_multiple_checkpoints_ordered(self, adapter, mock_db):
        """Test that checkpoints are returned in stage order."""
        envelope_id = "env_123"
        created_at = datetime.now(timezone.utc)

        mock_db.fetch_all.return_value = [
            {
                "checkpoint_id": "ckpt_1",
                "envelope_id": envelope_id,
                "agent_name": "planner",
                "stage_order": 0,
                "created_at": created_at,
                "parent_checkpoint_id": None,
                "metadata_json": None,
            },
            {
                "checkpoint_id": "ckpt_2",
                "envelope_id": envelope_id,
                "agent_name": "executor",
                "stage_order": 1,
                "created_at": created_at,
                "parent_checkpoint_id": "ckpt_1",
                "metadata_json": None,
            },
            {
                "checkpoint_id": "ckpt_3",
                "envelope_id": envelope_id,
                "agent_name": "synthesizer",
                "stage_order": 2,
                "created_at": created_at,
                "parent_checkpoint_id": "ckpt_2",
                "metadata_json": None,
            },
        ]

        results = await adapter.list_checkpoints(envelope_id)

        # Verify query includes ORDER BY
        call_args = mock_db.fetch_all.call_args
        sql = call_args[0][0]
        assert "ORDER BY stage_order ASC" in sql

        # Verify results maintain order
        assert len(results) == 3
        assert results[0].stage_order == 0
        assert results[1].stage_order == 1
        assert results[2].stage_order == 2

    @pytest.mark.asyncio
    async def test_list_checkpoints_respects_limit(self, adapter, mock_db):
        """Test that list_checkpoints respects limit parameter."""
        envelope_id = "env_123"
        limit = 50

        mock_db.fetch_all.return_value = []

        await adapter.list_checkpoints(envelope_id, limit=limit)

        # Verify limit passed to query
        call_args = mock_db.fetch_all.call_args
        params = call_args[0][1]
        assert params["limit"] == limit

    @pytest.mark.asyncio
    async def test_list_checkpoints_default_limit(self, adapter, mock_db):
        """Test default limit of 100."""
        envelope_id = "env_123"

        mock_db.fetch_all.return_value = []

        await adapter.list_checkpoints(envelope_id)

        # Should use default limit
        params = mock_db.fetch_all.call_args[0][1]
        assert params["limit"] == 100


# =============================================================================
# Delete Checkpoints Tests
# =============================================================================

class TestDeleteCheckpoints:
    """Test checkpoint deletion."""

    @pytest.mark.asyncio
    async def test_delete_all_checkpoints_for_envelope(self, adapter, mock_db, mock_logger):
        """Test deleting all checkpoints for an envelope."""
        envelope_id = "env_123"

        # Setup stage counter
        adapter._stage_counter[envelope_id] = 5

        # Mock delete result
        mock_result = MagicMock()
        mock_result.rowcount = 5
        mock_db.execute.return_value = mock_result

        deleted = await adapter.delete_checkpoints(envelope_id)

        # Verify delete query
        call_args = mock_db.execute.call_args
        sql = call_args[0][0]
        params = call_args[0][1]

        assert "DELETE FROM execution_checkpoints" in sql
        assert "WHERE envelope_id = :envelope_id" in sql
        assert params["envelope_id"] == envelope_id

        # Should return rowcount
        assert deleted == 5

        # Should clear stage counter
        assert envelope_id not in adapter._stage_counter

        # Should log
        mock_logger.info.assert_called_with(
            "checkpoints_deleted",
            envelope_id=envelope_id,
            count=5,
        )

    @pytest.mark.asyncio
    async def test_delete_checkpoints_before_boundary(self, adapter, mock_db):
        """Test deleting checkpoints before a specific checkpoint."""
        envelope_id = "env_123"
        boundary_checkpoint = "ckpt_5"

        # Mock boundary checkpoint lookup
        mock_db.fetch_one.return_value = {"stage_order": 5}

        # Mock delete result
        mock_result = MagicMock()
        mock_result.rowcount = 4
        mock_db.execute.return_value = mock_result

        deleted = await adapter.delete_checkpoints(
            envelope_id=envelope_id,
            before_checkpoint_id=boundary_checkpoint,
        )

        # Should query boundary checkpoint
        fetch_call = mock_db.fetch_one.call_args
        assert "SELECT stage_order FROM execution_checkpoints" in fetch_call[0][0]
        assert fetch_call[0][1]["checkpoint_id"] == boundary_checkpoint

        # Should delete with stage_order condition
        delete_call = mock_db.execute.call_args
        sql = delete_call[0][0]
        params = delete_call[0][1]

        assert "AND stage_order < :boundary_stage" in sql
        assert params["boundary_stage"] == 5
        assert params["envelope_id"] == envelope_id

        # Should return count
        assert deleted == 4

    @pytest.mark.asyncio
    async def test_delete_before_nonexistent_boundary(self, adapter, mock_db):
        """Test delete when boundary checkpoint doesn't exist."""
        envelope_id = "env_123"
        boundary_checkpoint = "ckpt_missing"

        # Boundary checkpoint not found
        mock_db.fetch_one.return_value = None

        deleted = await adapter.delete_checkpoints(
            envelope_id=envelope_id,
            before_checkpoint_id=boundary_checkpoint,
        )

        # Should return 0
        assert deleted == 0

        # Should not execute delete
        mock_db.execute.assert_not_called()

    @pytest.mark.asyncio
    async def test_delete_handles_no_rowcount_attribute(self, adapter, mock_db):
        """Test delete when result has no rowcount attribute."""
        envelope_id = "env_123"

        # Mock result without rowcount
        mock_result = object()
        mock_db.execute.return_value = mock_result

        deleted = await adapter.delete_checkpoints(envelope_id)

        # Should handle gracefully and return 0
        assert deleted == 0


# =============================================================================
# Fork Checkpoint Tests
# =============================================================================

class TestForkCheckpoint:
    """Test checkpoint forking for time-travel debugging."""

    @pytest.mark.asyncio
    async def test_fork_creates_new_branch(self, adapter, mock_db, mock_logger):
        """Test forking creates new checkpoint with new envelope_id."""
        source_checkpoint = "ckpt_abc"
        new_envelope_id = "env_fork_123"
        created_at = datetime.now(timezone.utc)

        # Mock source checkpoint
        source_state = {
            "envelope_id": "env_original",
            "foo": "bar",
            "nested": {"key": "value"},
        }
        mock_db.fetch_one.return_value = {
            "checkpoint_id": source_checkpoint,
            "envelope_id": "env_original",
            "agent_name": "planner",
            "stage_order": 3,
            "state_json": source_state,
            "metadata_json": {"duration_ms": 1000},
            "parent_checkpoint_id": "ckpt_parent",
            "created_at": created_at,
        }

        # Execute fork
        new_checkpoint_id = await adapter.fork_from_checkpoint(
            checkpoint_id=source_checkpoint,
            new_envelope_id=new_envelope_id,
        )

        # Should query source checkpoint
        mock_db.fetch_one.assert_called_once()

        # Should insert forked checkpoint
        mock_db.insert.assert_called_once()
        call_args = mock_db.insert.call_args
        assert call_args[0][0] == "execution_checkpoints"

        insert_data = call_args[0][1]
        assert insert_data["checkpoint_id"] == new_checkpoint_id
        assert new_checkpoint_id.startswith("ckpt_")
        assert insert_data["envelope_id"] == new_envelope_id
        assert insert_data["agent_name"] == "planner"
        assert insert_data["stage_order"] == 0  # Start of new branch

        # Should update state with new envelope_id
        forked_state = insert_data["state_json"]
        assert forked_state["envelope_id"] == new_envelope_id
        assert forked_state["foo"] == "bar"
        assert forked_state["nested"] == {"key": "value"}

        # Should add fork metadata
        metadata = insert_data["metadata_json"]
        assert metadata["forked_from"] == source_checkpoint
        assert metadata["original_envelope_id"] == "env_original"

        # Should link to source as parent
        assert insert_data["parent_checkpoint_id"] == source_checkpoint

        # Should initialize stage counter for new envelope
        assert adapter._stage_counter[new_envelope_id] == 1

        # Should log
        mock_logger.info.assert_called_with(
            "checkpoint_forked",
            source_checkpoint=source_checkpoint,
            new_checkpoint=new_checkpoint_id,
            new_envelope_id=new_envelope_id,
        )

    @pytest.mark.asyncio
    async def test_fork_from_nonexistent_checkpoint_raises_error(self, adapter, mock_db):
        """Test forking from non-existent checkpoint raises ValueError."""
        source_checkpoint = "ckpt_missing"
        new_envelope_id = "env_fork"

        # Source checkpoint not found
        mock_db.fetch_one.return_value = None

        # Should raise ValueError
        with pytest.raises(ValueError, match=f"Checkpoint not found: {source_checkpoint}"):
            await adapter.fork_from_checkpoint(
                checkpoint_id=source_checkpoint,
                new_envelope_id=new_envelope_id,
            )

        # Should not insert anything
        mock_db.insert.assert_not_called()

    @pytest.mark.asyncio
    async def test_fork_preserves_agent_name(self, adapter, mock_db):
        """Test that fork preserves agent_name from source."""
        mock_db.fetch_one.return_value = {
            "checkpoint_id": "ckpt_src",
            "envelope_id": "env_orig",
            "agent_name": "synthesizer",  # Specific agent
            "stage_order": 5,
            "state_json": {"envelope_id": "env_orig"},
            "metadata_json": None,
            "parent_checkpoint_id": None,
            "created_at": datetime.now(timezone.utc),
        }

        await adapter.fork_from_checkpoint("ckpt_src", "env_new")

        insert_data = mock_db.insert.call_args[0][1]
        assert insert_data["agent_name"] == "synthesizer"


# =============================================================================
# Edge Cases and Error Handling
# =============================================================================

class TestEdgeCases:
    """Test edge cases and error handling."""

    @pytest.mark.asyncio
    async def test_save_with_empty_state(self, adapter, mock_db):
        """Test saving checkpoint with empty state dict."""
        mock_db.fetch_one.return_value = None

        record = await adapter.save_checkpoint(
            envelope_id="env_123",
            checkpoint_id="ckpt_abc",
            agent_name="test",
            state={},  # Empty state
        )

        assert record.checkpoint_id == "ckpt_abc"
        insert_data = mock_db.insert.call_args[0][1]
        assert insert_data["state_json"] == {}

    @pytest.mark.asyncio
    async def test_save_with_complex_nested_state(self, adapter, mock_db):
        """Test saving checkpoint with deeply nested state."""
        mock_db.fetch_one.return_value = None

        complex_state = {
            "level1": {
                "level2": {
                    "level3": {
                        "arrays": [1, 2, 3],
                        "mixed": [{"key": "value"}, None, True],
                    }
                }
            },
            "unicode": "Hello 世界 🎉",
        }

        record = await adapter.save_checkpoint(
            envelope_id="env_123",
            checkpoint_id="ckpt_abc",
            agent_name="test",
            state=complex_state,
        )

        insert_data = mock_db.insert.call_args[0][1]
        assert insert_data["state_json"] == complex_state

    @pytest.mark.asyncio
    async def test_multiple_envelopes_independent_counters(self, adapter, mock_db):
        """Test that different envelopes maintain independent stage counters."""
        mock_db.fetch_one.return_value = None

        # Save checkpoints for envelope 1
        await adapter.save_checkpoint("env_1", "ckpt_1a", "agent", {"step": 1})
        await adapter.save_checkpoint("env_1", "ckpt_1b", "agent", {"step": 2})

        # Save checkpoints for envelope 2
        await adapter.save_checkpoint("env_2", "ckpt_2a", "agent", {"step": 1})

        # Counters should be independent
        assert adapter._stage_counter["env_1"] == 2
        assert adapter._stage_counter["env_2"] == 1

    @pytest.mark.asyncio
    async def test_list_with_zero_limit(self, adapter, mock_db):
        """Test list with limit=0."""
        mock_db.fetch_all.return_value = []

        results = await adapter.list_checkpoints("env_123", limit=0)

        # Should query with limit=0 (database will handle)
        params = mock_db.fetch_all.call_args[0][1]
        assert params["limit"] == 0
