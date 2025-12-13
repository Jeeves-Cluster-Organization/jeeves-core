"""Test database client functionality.

PostgreSQL-only as of 2025-11-27 (Amendment V).
Tests use pg_test_db fixture from conftest.py.
"""

import pytest
import uuid

pytestmark = [pytest.mark.integration, pytest.mark.requires_docker]


@pytest.mark.asyncio
async def test_database_connection(pg_test_db):
    """Test database connection."""
    assert pg_test_db.pool is not None


@pytest.mark.asyncio
async def test_insert_and_fetch(pg_test_db):
    """Test inserting and fetching data."""
    # Insert a session (PostgreSQL uses UUID)
    session_id = uuid.uuid4()
    session_data = {
        "session_id": session_id,
        "user_id": "test-user",
    }

    result_id = await pg_test_db.insert("sessions", session_data)
    assert result_id == session_id

    # Fetch the session
    result = await pg_test_db.fetch_one(
        "SELECT * FROM sessions WHERE session_id = ?",
        (session_id,)
    )

    assert result is not None
    assert result["user_id"] == "test-user"


@pytest.mark.asyncio
async def test_insert_request(pg_test_db):
    """Test inserting a request."""
    # First create a session
    session_id = uuid.uuid4()
    await pg_test_db.insert("sessions", {
        "session_id": session_id,
        "user_id": "test-user",
    })

    # Insert a request
    request_id = uuid.uuid4()
    request_data = {
        "request_id": request_id,
        "user_id": "test-user",
        "session_id": session_id,
        "user_message": "Add task: test task",
    }

    result_id = await pg_test_db.insert("requests", request_data)
    assert result_id == request_id

    # Verify it was inserted
    result = await pg_test_db.fetch_one(
        "SELECT * FROM requests WHERE request_id = ?",
        (request_id,)
    )

    assert result is not None
    assert result["user_message"] == "Add task: test task"
    assert result["status"] == "pending"


@pytest.mark.asyncio
async def test_update(pg_test_db):
    """Test updating records."""
    # Insert a session
    session_id = uuid.uuid4()
    await pg_test_db.insert("sessions", {
        "session_id": session_id,
        "user_id": "test-user",
    })

    # Insert a request
    request_id = uuid.uuid4()
    await pg_test_db.insert("requests", {
        "request_id": request_id,
        "user_id": "test-user",
        "session_id": session_id,
        "user_message": "Test",
    })

    # Update the status
    rows_updated = await pg_test_db.update(
        "requests",
        {"status": "completed"},
        "request_id = ?",
        (request_id,)
    )

    assert rows_updated == 1

    # Verify the update
    result = await pg_test_db.fetch_one(
        "SELECT status FROM requests WHERE request_id = ?",
        (request_id,)
    )

    assert result["status"] == "completed"


@pytest.mark.asyncio
async def test_fetch_all(pg_test_db):
    """Test fetching multiple records."""
    # Insert a session
    session_id = uuid.uuid4()
    await pg_test_db.insert("sessions", {
        "session_id": session_id,
        "user_id": "test-user",
    })

    # Insert multiple requests
    request_ids = []
    for i in range(3):
        request_id = uuid.uuid4()
        request_ids.append(request_id)
        await pg_test_db.insert("requests", {
            "request_id": request_id,
            "user_id": "test-user",
            "session_id": session_id,
            "user_message": f"Test message {i}",
        })

    # Fetch all requests for this session
    results = await pg_test_db.fetch_all(
        "SELECT * FROM requests WHERE session_id = ? ORDER BY request_id",
        (session_id,)
    )

    assert len(results) == 3
    assert results[0]["user_message"] in ["Test message 0", "Test message 1", "Test message 2"]


@pytest.mark.asyncio
async def test_json_helpers():
    """Test JSON helper methods."""
    from jeeves_avionics.database.postgres_client import PostgreSQLClient

    data = {"key": "value", "number": 42}

    # Convert to JSON
    json_str = PostgreSQLClient.to_json(data)
    assert isinstance(json_str, str)

    # Convert back from JSON
    result = PostgreSQLClient.from_json(json_str)
    assert result == data

    # Test with None
    assert PostgreSQLClient.from_json(None) is None


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
