"""Unit tests for SQLAdapter.

PostgreSQL-only as of 2025-11-27 (Amendment V).
Tests use pg_test_db fixture from conftest.py which includes all required tables.

Note: v3.0 Pivot - tasks, journal_entries, and kv_store tables were removed.
Only knowledge_facts and messages tables remain for structured data.
"""

import pytest
from datetime import datetime, timezone
from uuid import uuid4
from jeeves_memory_module.adapters.sql_adapter import SQLAdapter

# Requires PostgreSQL database
pytestmark = pytest.mark.requires_postgres


class TestAdapter:
    """Tests for SQLAdapter CRUD operations."""

    @pytest.fixture
    def adapter(self, pg_test_db):
        """Create SQLAdapter instance."""
        return SQLAdapter(pg_test_db)

    @pytest.mark.asyncio
    async def test_write_fact(self, adapter):
        """Test writing a fact to knowledge_facts table."""
        fact_id = await adapter.write_fact(
            user_id="user123",
            data={
                "domain": "preferences",
                "key": "favorite_color",
                "value": "blue",
                "confidence": 0.95
            }
        )

        assert fact_id is not None
        assert isinstance(fact_id, str)

    @pytest.mark.asyncio
    async def test_write_fact_upsert(self, adapter):
        """Test that writing same fact updates it (UPSERT behavior)."""
        # Write fact
        await adapter.write_fact(
            user_id="user123",
            data={"domain": "preferences", "key": "theme", "value": "dark"}
        )

        # Update fact with same user_id/domain/key
        await adapter.write_fact(
            user_id="user123",
            data={"domain": "preferences", "key": "theme", "value": "light"}
        )

        # Read should return new value
        result = await adapter.read_by_filter(
            user_id="user123",
            item_type="fact",
            filters={"key": "theme"},
            limit=1
        )
        assert len(result) == 1
        assert result[0]["value"] == "light"

    @pytest.mark.asyncio
    async def test_write_message(self, adapter, pg_test_db):
        """Test writing a message."""
        # Create a valid session first (messages.session_id is a foreign key)
        session_id = uuid4()
        await pg_test_db.insert("sessions", {
            "session_id": session_id,
            "user_id": "test-user",
        })

        message_id = await adapter.write_message(
            session_id=session_id,
            data={
                "role": "user",
                "content": "Hello, assistant!",
            }
        )

        assert message_id is not None

        # Verify message was written
        message = await adapter.read_by_id(message_id, "message")
        assert message is not None
        assert message["content"] == "Hello, assistant!"

    @pytest.mark.asyncio
    async def test_read_by_id_nonexistent(self, adapter):
        """Test reading nonexistent item returns None."""
        # Use a UUID that doesn't exist in DB
        nonexistent_id = str(uuid4())
        result = await adapter.read_by_id(nonexistent_id, "fact")
        assert result is None

    @pytest.mark.asyncio
    async def test_read_by_id_invalid_type(self, adapter):
        """Test reading with invalid type raises error."""
        with pytest.raises(ValueError, match="Invalid item type"):
            await adapter.read_by_id("test_id", "invalid_type")

    @pytest.mark.asyncio
    async def test_read_by_filter(self, adapter):
        """Test reading items with filters."""
        # Create multiple facts
        await adapter.write_fact("user123", {"domain": "preferences", "key": "color1", "value": "red"})
        await adapter.write_fact("user123", {"domain": "preferences", "key": "color2", "value": "blue"})
        await adapter.write_fact("user123", {"domain": "habits", "key": "color3", "value": "green"})

        # Read facts in preferences domain
        results = await adapter.read_by_filter(
            user_id="user123",
            item_type="fact",
            filters={"domain": "preferences"},
            limit=10
        )

        assert len(results) == 2
        assert all(r["domain"] == "preferences" for r in results)

    @pytest.mark.asyncio
    async def test_read_by_filter_limit(self, adapter):
        """Test read respects limit parameter."""
        # Create many facts
        for i in range(10):
            await adapter.write_fact("user123", {"domain": "test", "key": f"key{i}", "value": f"value{i}"})

        # Read with limit
        results = await adapter.read_by_filter(
            user_id="user123",
            item_type="fact",
            filters={"domain": "test"},
            limit=5
        )

        assert len(results) == 5

    @pytest.mark.asyncio
    async def test_update_item(self, adapter):
        """Test updating an item."""
        # Create fact
        fact_id = await adapter.write_fact(
            "user123",
            {"domain": "preferences", "key": "update_test", "value": "original"}
        )

        # Update fact
        await adapter.update_item(
            fact_id,
            "fact",
            {"value": "updated", "confidence": 0.8}
        )

        # Verify update
        fact = await adapter.read_by_id(fact_id, "fact")
        assert fact["value"] == "updated"

    @pytest.mark.asyncio
    async def test_delete_item_soft(self, adapter, pg_test_db):
        """Test soft delete marks item as deleted.

        Note: Soft delete only supported for messages table (which has deleted_at column).
        """
        # Create a valid session first
        session_id = uuid4()
        await pg_test_db.insert("sessions", {
            "session_id": session_id,
            "user_id": "test-user",
        })

        # Create message
        message_id = await adapter.write_message(
            session_id=session_id,
            data={"role": "user", "content": "To delete"}
        )

        # Soft delete
        result = await adapter.delete_item(message_id, "message", soft=True)
        assert result is True

        # Verify marked as deleted via deleted_at column
        # Note: message_id is SERIAL (INTEGER), convert from string
        message_row = await pg_test_db.fetch_one(
            "SELECT deleted_at FROM messages WHERE message_id = ?",
            (int(message_id),)
        )
        assert message_row["deleted_at"] is not None

    @pytest.mark.asyncio
    async def test_delete_item_hard(self, adapter):
        """Test hard delete removes item."""
        # Create fact
        fact_id = await adapter.write_fact("user123", {"domain": "delete_test", "key": "to_delete", "value": "temp"})

        # Hard delete
        result = await adapter.delete_item(fact_id, "fact", soft=False)
        assert result is True

        # Verify item is gone
        fact = await adapter.read_by_id(fact_id, "fact")
        assert fact is None

    @pytest.mark.asyncio
    async def test_delete_item_invalid_type(self, adapter):
        """Test delete with invalid type raises error."""
        with pytest.raises(ValueError, match="Invalid item type"):
            await adapter.delete_item("test_id", "invalid_type", soft=False)
