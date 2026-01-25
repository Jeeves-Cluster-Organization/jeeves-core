"""Unit tests for SQLAdapter.

Clean, modern async/await tests using mocked database client.
All external dependencies are mocked - no database required.
"""

import pytest
from datetime import datetime, timezone
from uuid import UUID
from unittest.mock import AsyncMock, MagicMock, call
from jeeves_memory_module.adapters.sql_adapter import SQLAdapter


@pytest.fixture
def adapter(mock_db, mock_logger):
    """Create SQLAdapter instance with mocked dependencies."""
    return SQLAdapter(db_client=mock_db, logger=mock_logger)


# =============================================================================
# WRITE_FACT TESTS
# =============================================================================


@pytest.mark.asyncio
async def test_write_fact_new(adapter, mock_db):
    """Test writing a new fact to knowledge_facts table."""
    await adapter.write_fact(
        user_id="user123",
        data={
            "domain": "preferences",
            "key": "favorite_color",
            "value": "blue",
            "confidence": 0.95
        }
    )

    # Verify database execute was called
    mock_db.execute.assert_called_once()
    call_args = mock_db.execute.call_args
    query, params = call_args[0]

    # Verify query structure
    assert "INSERT INTO knowledge_facts" in query
    assert "ON CONFLICT" in query

    # Verify parameters
    assert params[1] == "user123"  # user_id
    assert params[2] == "preferences"  # domain
    assert params[3] == "favorite_color"  # key
    assert params[4] == "blue"  # value
    assert params[5] == 0.95  # confidence


@pytest.mark.asyncio
async def test_write_fact_upsert(adapter, mock_db):
    """Test upsert behavior updates existing fact on conflict."""
    await adapter.write_fact(
        user_id="user123",
        data={
            "domain": "preferences",
            "key": "theme",
            "value": "dark",
            "confidence": 1.0
        }
    )

    # Verify upsert query contains ON CONFLICT clause
    call_args = mock_db.execute.call_args
    query = call_args[0][0]
    assert "ON CONFLICT(user_id, domain, key)" in query
    assert "DO UPDATE SET" in query


@pytest.mark.asyncio
async def test_write_fact_default_domain(adapter, mock_db):
    """Test fact defaults to 'preferences' domain when not specified."""
    await adapter.write_fact(
        user_id="user123",
        data={
            "key": "setting",
            "value": "enabled"
        }
    )

    call_args = mock_db.execute.call_args
    params = call_args[0][1]
    assert params[2] == "preferences"  # domain defaults to 'preferences'


@pytest.mark.asyncio
async def test_write_fact_error_handling(adapter, mock_db):
    """Test write_fact handles database errors."""
    mock_db.execute.side_effect = Exception("Database connection failed")

    with pytest.raises(Exception, match="Database connection failed"):
        await adapter.write_fact(
            user_id="user123",
            data={"domain": "test", "key": "key", "value": "value"}
        )


# =============================================================================
# WRITE_MESSAGE TESTS
# =============================================================================


@pytest.mark.asyncio
async def test_write_message_success(adapter, mock_db):
    """Test writing message with RETURNING clause."""
    # Mock the RETURNING result
    mock_db.fetch_one.return_value = {"message_id": 42}

    message_id = await adapter.write_message(
        session_id="sess-123",
        data={
            "role": "user",
            "content": "Hello, assistant!"
        }
    )

    # Verify result
    assert message_id == "42"

    # Verify database call
    mock_db.fetch_one.assert_called_once()
    call_args = mock_db.fetch_one.call_args
    query, params = call_args[0]

    assert "INSERT INTO messages" in query
    assert "RETURNING message_id" in query
    assert params[0] == "sess-123"  # session_id
    assert params[1] == "user"  # role
    assert params[2] == "Hello, assistant!"  # content
    assert call_args[1]["commit"] is True  # commit=True for RETURNING


@pytest.mark.asyncio
async def test_write_message_default_role(adapter, mock_db):
    """Test message defaults to 'user' role when not specified."""
    mock_db.fetch_one.return_value = {"message_id": 123}

    await adapter.write_message(
        session_id="sess-456",
        data={"content": "Test message"}
    )

    call_args = mock_db.fetch_one.call_args
    params = call_args[0][1]
    assert params[1] == "user"  # role defaults to 'user'


@pytest.mark.asyncio
async def test_write_message_error_handling(adapter, mock_db):
    """Test write_message handles database errors."""
    mock_db.fetch_one.side_effect = Exception("Insert failed")

    with pytest.raises(Exception, match="Insert failed"):
        await adapter.write_message(
            session_id="sess-789",
            data={"content": "Test"}
        )


# =============================================================================
# READ_BY_ID TESTS
# =============================================================================


@pytest.mark.asyncio
async def test_read_fact_by_id(adapter, mock_db):
    """Test reading fact by UUID."""
    fact_id = "550e8400-e29b-41d4-a716-446655440000"
    mock_db.fetch_one.return_value = {
        "fact_id": fact_id,
        "user_id": "user123",
        "domain": "preferences",
        "key": "color",
        "value": "blue"
    }

    result = await adapter.read_by_id(fact_id, "fact")

    # Verify result
    assert result is not None
    assert result["fact_id"] == fact_id

    # Verify query
    mock_db.fetch_one.assert_called_once()
    call_args = mock_db.fetch_one.call_args
    query, params = call_args[0]

    assert "SELECT * FROM knowledge_facts" in query
    assert "WHERE fact_id = ?" in query
    assert params[0] == fact_id


@pytest.mark.asyncio
async def test_read_message_by_id(adapter, mock_db):
    """Test reading message by INTEGER id."""
    mock_db.fetch_one.return_value = {
        "message_id": 42,
        "session_id": "sess-123",
        "role": "user",
        "content": "Hello"
    }

    result = await adapter.read_by_id("42", "message")

    # Verify result
    assert result is not None
    assert result["message_id"] == 42

    # Verify query uses INTEGER
    call_args = mock_db.fetch_one.call_args
    query, params = call_args[0]

    assert "SELECT * FROM messages" in query
    assert "WHERE message_id = ?" in query
    assert params[0] == 42  # Converted to int


@pytest.mark.asyncio
async def test_read_by_id_not_found(adapter, mock_db):
    """Test read_by_id returns None when item not found."""
    mock_db.fetch_one.return_value = None

    result = await adapter.read_by_id("nonexistent-id", "fact")

    assert result is None


@pytest.mark.asyncio
async def test_read_by_id_invalid_type(adapter, mock_db):
    """Test read_by_id raises ValueError for invalid item_type."""
    with pytest.raises(ValueError, match="Invalid item type"):
        await adapter.read_by_id("some-id", "invalid_type")


@pytest.mark.asyncio
async def test_read_by_id_error_handling(adapter, mock_db):
    """Test read_by_id handles database errors."""
    mock_db.fetch_one.side_effect = Exception("Query failed")

    with pytest.raises(Exception, match="Query failed"):
        await adapter.read_by_id("some-id", "fact")


# =============================================================================
# READ_BY_FILTER TESTS
# =============================================================================


@pytest.mark.asyncio
async def test_read_by_filter_facts(adapter, mock_db):
    """Test filtering facts by domain."""
    mock_db.fetch_all.return_value = [
        {"fact_id": "id1", "domain": "preferences", "key": "color1", "value": "red"},
        {"fact_id": "id2", "domain": "preferences", "key": "color2", "value": "blue"}
    ]

    results = await adapter.read_by_filter(
        user_id="user123",
        item_type="fact",
        filters={"domain": "preferences"},
        limit=10
    )

    # Verify results
    assert len(results) == 2

    # Verify query
    mock_db.fetch_all.assert_called_once()
    call_args = mock_db.fetch_all.call_args
    query, params = call_args[0]

    assert "SELECT * FROM knowledge_facts" in query
    assert "user_id = ?" in query
    assert "domain = ?" in query
    assert "ORDER BY last_updated DESC" in query
    assert "LIMIT ?" in query

    # Verify params
    assert params[0] == "user123"
    assert params[1] == "preferences"
    assert params[2] == 10


@pytest.mark.asyncio
async def test_read_by_filter_messages(adapter, mock_db):
    """Test filtering messages (no user_id filter)."""
    mock_db.fetch_all.return_value = [
        {"message_id": 1, "session_id": "sess-123", "content": "msg1"},
        {"message_id": 2, "session_id": "sess-123", "content": "msg2"}
    ]

    results = await adapter.read_by_filter(
        user_id="user123",  # This should be ignored for messages
        item_type="message",
        filters={"session_id": "sess-123"},
        limit=5
    )

    # Verify results
    assert len(results) == 2

    # Verify query doesn't include user_id
    call_args = mock_db.fetch_all.call_args
    query, params = call_args[0]

    assert "SELECT * FROM messages" in query
    assert "user_id" not in query  # messages table has no user_id
    assert "session_id = ?" in query
    assert "ORDER BY created_at DESC" in query

    # Verify params (no user_id in params)
    assert params[0] == "sess-123"
    assert params[1] == 5


@pytest.mark.asyncio
async def test_read_by_filter_limit(adapter, mock_db):
    """Test read_by_filter respects limit parameter."""
    mock_db.fetch_all.return_value = [{"fact_id": f"id{i}"} for i in range(3)]

    await adapter.read_by_filter(
        user_id="user123",
        item_type="fact",
        filters={},
        limit=3
    )

    # Verify limit is passed
    call_args = mock_db.fetch_all.call_args
    params = call_args[0][1]
    assert params[-1] == 3  # Last param is limit


@pytest.mark.asyncio
async def test_read_by_filter_invalid_type(adapter, mock_db):
    """Test read_by_filter raises ValueError for invalid item_type."""
    with pytest.raises(ValueError, match="Invalid item type"):
        await adapter.read_by_filter(
            user_id="user123",
            item_type="invalid",
            filters={},
            limit=10
        )


# =============================================================================
# UPDATE_ITEM TESTS
# =============================================================================


@pytest.mark.asyncio
async def test_update_fact(adapter, mock_db):
    """Test updating fact fields."""
    fact_id = "550e8400-e29b-41d4-a716-446655440000"

    result = await adapter.update_item(
        item_id=fact_id,
        item_type="fact",
        updates={"value": "updated_value", "confidence": 0.8}
    )

    # Verify success
    assert result is True

    # Verify query
    mock_db.execute.assert_called_once()
    call_args = mock_db.execute.call_args
    query, params = call_args[0]

    assert "UPDATE knowledge_facts" in query
    assert "value = ?" in query
    assert "confidence = ?" in query
    assert "last_updated = ?" in query
    assert "WHERE fact_id = ?" in query

    # Verify params
    assert params[0] == "updated_value"
    assert params[1] == 0.8
    assert isinstance(params[2], datetime)  # last_updated timestamp
    assert params[3] == fact_id


@pytest.mark.asyncio
async def test_update_message_sets_edited_at(adapter, mock_db):
    """Test updating message sets edited_at timestamp."""
    message_id = "42"

    await adapter.update_item(
        item_id=message_id,
        item_type="message",
        updates={"content": "edited content"}
    )

    # Verify query uses edited_at for messages
    call_args = mock_db.execute.call_args
    query = call_args[0][0]

    assert "UPDATE messages" in query
    assert "edited_at = ?" in query
    assert "last_updated" not in query


@pytest.mark.asyncio
async def test_update_item_invalid_type(adapter, mock_db):
    """Test update_item raises ValueError for invalid item_type."""
    with pytest.raises(ValueError, match="Invalid item type"):
        await adapter.update_item(
            item_id="some-id",
            item_type="invalid",
            updates={"field": "value"}
        )


# =============================================================================
# DELETE_ITEM TESTS
# =============================================================================


@pytest.mark.asyncio
async def test_delete_message_soft(adapter, mock_db):
    """Test soft delete sets deleted_at timestamp for messages."""
    message_id = "42"

    result = await adapter.delete_item(
        item_id=message_id,
        item_type="message",
        soft=True
    )

    # Verify success
    assert result is True

    # Verify soft delete query
    mock_db.execute.assert_called_once()
    call_args = mock_db.execute.call_args
    query, params = call_args[0]

    assert "UPDATE messages" in query
    assert "SET deleted_at = ?" in query
    assert "WHERE message_id = ?" in query

    # Verify params
    assert isinstance(params[0], datetime)  # deleted_at timestamp
    assert params[1] == 42  # Converted to int


@pytest.mark.asyncio
async def test_delete_message_hard(adapter, mock_db):
    """Test hard delete removes message from database."""
    message_id = "42"

    result = await adapter.delete_item(
        item_id=message_id,
        item_type="message",
        soft=False
    )

    # Verify success
    assert result is True

    # Verify hard delete query
    call_args = mock_db.execute.call_args
    query, params = call_args[0]

    assert "DELETE FROM messages" in query
    assert "WHERE message_id = ?" in query
    assert params[0] == 42


@pytest.mark.asyncio
async def test_delete_fact_hard(adapter, mock_db):
    """Test fact deletion (no soft delete support for facts)."""
    fact_id = "550e8400-e29b-41d4-a716-446655440000"

    result = await adapter.delete_item(
        item_id=fact_id,
        item_type="fact",
        soft=True  # Should be ignored, facts don't support soft delete
    )

    # Verify success
    assert result is True

    # Verify hard delete even when soft=True
    call_args = mock_db.execute.call_args
    query = call_args[0][0]

    assert "DELETE FROM knowledge_facts" in query
    assert "UPDATE" not in query  # Not a soft delete


@pytest.mark.asyncio
async def test_delete_item_invalid_type(adapter, mock_db):
    """Test delete_item raises ValueError for invalid item_type."""
    with pytest.raises(ValueError, match="Invalid item type"):
        await adapter.delete_item(
            item_id="some-id",
            item_type="invalid",
            soft=False
        )
