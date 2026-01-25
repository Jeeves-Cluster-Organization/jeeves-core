"""Unit tests for PgVectorRepository.

Clean, modern async/await tests using mocked dependencies.
All external dependencies (PostgreSQL, embedding service) are mocked.
"""

import pytest
import numpy as np
from unittest.mock import AsyncMock, MagicMock, call
from jeeves_memory_module.repositories.pgvector_repository import PgVectorRepository


# =============================================================================
# FIXTURES
# =============================================================================


@pytest.fixture
def mock_logger():
    """Create a mock logger."""
    logger = MagicMock()
    logger.bind.return_value = logger
    return logger


@pytest.fixture
def mock_postgres_client():
    """Mock PostgreSQLClient with async session support."""
    client = MagicMock()

    # Mock session context manager
    mock_session = AsyncMock()
    mock_session.execute = AsyncMock()
    mock_session.commit = AsyncMock()
    mock_session.fetchone = AsyncMock()
    mock_session.fetchall = AsyncMock()

    # session() returns a context manager
    mock_session_context = MagicMock()
    mock_session_context.__aenter__ = AsyncMock(return_value=mock_session)
    mock_session_context.__aexit__ = AsyncMock(return_value=None)

    client.session = MagicMock(return_value=mock_session_context)

    # Mock transaction context manager
    mock_transaction = AsyncMock()
    mock_transaction.execute = AsyncMock()

    mock_transaction_context = MagicMock()
    mock_transaction_context.__aenter__ = AsyncMock(return_value=mock_transaction)
    mock_transaction_context.__aexit__ = AsyncMock(return_value=None)

    client.transaction = MagicMock(return_value=mock_transaction_context)

    # Mock engine for index operations
    mock_engine = MagicMock()
    mock_conn = AsyncMock()
    mock_conn.execute = AsyncMock()

    mock_engine_context = MagicMock()
    mock_engine_context.__aenter__ = AsyncMock(return_value=mock_conn)
    mock_engine_context.__aexit__ = AsyncMock(return_value=None)

    mock_engine.begin = MagicMock(return_value=mock_engine_context)
    client.engine = mock_engine

    return client


@pytest.fixture
def mock_embedding_service():
    """Mock EmbeddingService."""
    service = MagicMock()
    # Return a deterministic embedding vector
    service.embed = MagicMock(return_value=np.array([0.1, 0.2, 0.3, 0.4]))
    return service


@pytest.fixture
def pgvector_repo(mock_postgres_client, mock_embedding_service, mock_logger):
    """PgVectorRepository instance with mocked dependencies."""
    return PgVectorRepository(
        postgres_client=mock_postgres_client,
        embedding_service=mock_embedding_service,
        dimension=4,
        logger=mock_logger
    )


# =============================================================================
# INITIALIZATION TESTS
# =============================================================================


def test_init_success(pgvector_repo):
    """Test successful initialization."""
    assert pgvector_repo.dimension == 4
    assert pgvector_repo.db is not None
    assert pgvector_repo.embeddings is not None
    assert "tasks" in pgvector_repo.collection_config
    assert "journal" in pgvector_repo.collection_config
    assert "facts" in pgvector_repo.collection_config


def test_collection_config(pgvector_repo):
    """Test collection configuration structure."""
    assert pgvector_repo.collection_config["tasks"]["table"] == "tasks"
    assert pgvector_repo.collection_config["tasks"]["id_col"] == "task_id"
    assert pgvector_repo.collection_config["journal"]["table"] == "journal_entries"
    assert pgvector_repo.collection_config["facts"]["table"] == "knowledge_facts"


# =============================================================================
# _VALIDATE_COLLECTION TESTS
# =============================================================================


def test_validate_collection_valid(pgvector_repo):
    """Test valid collection returns config."""
    config = pgvector_repo._validate_collection("tasks")

    assert config["table"] == "tasks"
    assert config["id_col"] == "task_id"
    assert config["content_col"] == "title"


def test_validate_collection_invalid(pgvector_repo):
    """Test invalid collection raises ValueError."""
    with pytest.raises(ValueError, match="Invalid collection: invalid_name"):
        pgvector_repo._validate_collection("invalid_name")


# =============================================================================
# UPSERT TESTS
# =============================================================================


@pytest.mark.asyncio
async def test_upsert_success(pgvector_repo, mock_postgres_client, mock_embedding_service):
    """Test successful embedding upsert."""
    # Mock database result
    mock_result = MagicMock()
    mock_result.rowcount = 1

    mock_session = await mock_postgres_client.session().__aenter__()
    mock_session.execute.return_value = mock_result

    # Execute
    result = await pgvector_repo.upsert(
        item_id="task-123",
        content="Test task content",
        collection="tasks"
    )

    # Verify success
    assert result is True

    # Verify embedding was generated
    mock_embedding_service.embed.assert_called_once_with("Test task content")

    # Verify database update was called
    mock_session.execute.assert_called_once()
    mock_session.commit.assert_called_once()


@pytest.mark.asyncio
async def test_upsert_invalid_collection(pgvector_repo):
    """Test upsert with invalid collection raises ValueError."""
    with pytest.raises(ValueError, match="Invalid collection"):
        await pgvector_repo.upsert(
            item_id="item-1",
            content="Test content",
            collection="invalid"
        )


@pytest.mark.asyncio
async def test_upsert_no_match(pgvector_repo, mock_postgres_client, mock_embedding_service):
    """Test upsert returns False when item_id not found."""
    # Mock database result with 0 rows updated
    mock_result = MagicMock()
    mock_result.rowcount = 0

    mock_session = await mock_postgres_client.session().__aenter__()
    mock_session.execute.return_value = mock_result

    # Execute
    result = await pgvector_repo.upsert(
        item_id="nonexistent",
        content="Test content",
        collection="tasks"
    )

    # Verify failure
    assert result is False


@pytest.mark.asyncio
async def test_upsert_numpy_array_conversion(pgvector_repo, mock_postgres_client, mock_embedding_service):
    """Test numpy array embedding converted to list."""
    # Mock embedding as numpy array
    mock_embedding_service.embed.return_value = np.array([0.5, 0.6, 0.7, 0.8])

    mock_result = MagicMock()
    mock_result.rowcount = 1

    mock_session = await mock_postgres_client.session().__aenter__()
    mock_session.execute.return_value = mock_result

    # Execute
    await pgvector_repo.upsert(
        item_id="task-456",
        content="Test task",
        collection="tasks"
    )

    # Verify execute was called with list format
    call_args = mock_session.execute.call_args
    query_params = call_args[0][1]

    # Embedding should be in format '[0.5,0.6,0.7,0.8]'
    assert "[" in query_params["embedding"]
    assert "]" in query_params["embedding"]
    assert "0.5" in query_params["embedding"]


@pytest.mark.asyncio
async def test_upsert_error_handling(pgvector_repo, mock_postgres_client, mock_embedding_service):
    """Test upsert handles database errors gracefully."""
    # Mock database raises exception
    mock_session = await mock_postgres_client.session().__aenter__()
    mock_session.execute.side_effect = Exception("Database error")

    # Execute
    result = await pgvector_repo.upsert(
        item_id="task-999",
        content="Test",
        collection="tasks"
    )

    # Verify failure
    assert result is False


# =============================================================================
# SEARCH TESTS
# =============================================================================


@pytest.mark.asyncio
async def test_search_success(pgvector_repo, mock_postgres_client, mock_embedding_service):
    """Test semantic search returns ranked results."""
    # Mock embedding service
    mock_embedding_service.embed.return_value = np.array([0.1, 0.2, 0.3, 0.4])

    # Mock database results
    mock_row1 = MagicMock()
    mock_row1.item_id = "task-1"
    mock_row1.content = "First result"
    mock_row1.score = 0.95

    mock_row2 = MagicMock()
    mock_row2.item_id = "task-2"
    mock_row2.content = "Second result"
    mock_row2.score = 0.85

    mock_result = MagicMock()
    mock_result.fetchall.return_value = [mock_row1, mock_row2]

    mock_session = await mock_postgres_client.session().__aenter__()
    mock_session.execute.return_value = mock_result

    # Execute
    results = await pgvector_repo.search(
        query="test query",
        collections=["tasks"],
        limit=10
    )

    # Verify results
    assert len(results) == 2
    assert results[0]["item_id"] == "task-1"
    assert results[0]["score"] == 0.95
    assert results[0]["content"] == "First result"
    assert results[0]["collection"] == "tasks"
    assert results[1]["item_id"] == "task-2"
    assert results[1]["score"] == 0.85

    # Verify embedding was generated
    mock_embedding_service.embed.assert_called_once_with("test query")


@pytest.mark.asyncio
async def test_search_empty_query(pgvector_repo):
    """Test empty query returns empty list."""
    results = await pgvector_repo.search(
        query="",
        collections=["tasks"],
        limit=10
    )

    # Verify empty results
    assert results == []


@pytest.mark.asyncio
async def test_search_with_filters(pgvector_repo, mock_postgres_client, mock_embedding_service):
    """Test search with metadata filters."""
    mock_result = MagicMock()
    mock_result.fetchall.return_value = []

    mock_session = await mock_postgres_client.session().__aenter__()
    mock_session.execute.return_value = mock_result

    # Execute with filters
    await pgvector_repo.search(
        query="test query",
        collections=["tasks"],
        filters={"user_id": "user-123", "status": "pending"},
        limit=10
    )

    # Verify filters were included in query
    call_args = mock_session.execute.call_args
    query_params = call_args[0][1]

    assert "filter_user_id" in query_params
    assert query_params["filter_user_id"] == "user-123"
    assert "filter_status" in query_params
    assert query_params["filter_status"] == "pending"


@pytest.mark.asyncio
async def test_search_min_similarity_threshold(pgvector_repo, mock_postgres_client, mock_embedding_service):
    """Test min_similarity filters low-scoring results."""
    mock_result = MagicMock()
    mock_result.fetchall.return_value = []

    mock_session = await mock_postgres_client.session().__aenter__()
    mock_session.execute.return_value = mock_result

    # Execute with min_similarity
    await pgvector_repo.search(
        query="test query",
        collections=["tasks"],
        limit=10,
        min_similarity=0.7
    )

    # Verify min_similarity parameter was passed
    call_args = mock_session.execute.call_args
    query_params = call_args[0][1]

    assert query_params["min_similarity"] == 0.7


@pytest.mark.asyncio
async def test_search_multiple_collections(pgvector_repo, mock_postgres_client, mock_embedding_service):
    """Test searching across multiple collections."""
    # Mock results from different collections
    mock_row_task = MagicMock()
    mock_row_task.item_id = "task-1"
    mock_row_task.content = "Task result"
    mock_row_task.score = 0.9

    mock_row_journal = MagicMock()
    mock_row_journal.item_id = "journal-1"
    mock_row_journal.content = "Journal result"
    mock_row_journal.score = 0.85

    mock_result = MagicMock()
    # Return different results for each collection query
    mock_result.fetchall.side_effect = [
        [mock_row_task],  # tasks collection
        [mock_row_journal]  # journal collection
    ]

    mock_session = await mock_postgres_client.session().__aenter__()
    mock_session.execute.return_value = mock_result

    # Execute across multiple collections
    results = await pgvector_repo.search(
        query="test query",
        collections=["tasks", "journal"],
        limit=10
    )

    # Verify results from both collections
    assert len(results) == 2
    collections = [r["collection"] for r in results]
    assert "tasks" in collections
    assert "journal" in collections


@pytest.mark.asyncio
async def test_search_invalid_collection(pgvector_repo):
    """Test search with invalid collection raises ValueError."""
    with pytest.raises(ValueError, match="Invalid collection"):
        await pgvector_repo.search(
            query="test query",
            collections=["invalid"],
            limit=10
        )


# =============================================================================
# DELETE TESTS
# =============================================================================


@pytest.mark.asyncio
async def test_delete_success(pgvector_repo, mock_postgres_client):
    """Test embedding deletion (sets to NULL)."""
    mock_result = MagicMock()
    mock_result.rowcount = 1

    mock_session = await mock_postgres_client.session().__aenter__()
    mock_session.execute.return_value = mock_result

    # Execute
    result = await pgvector_repo.delete(
        item_id="task-123",
        collection="tasks"
    )

    # Verify success
    assert result is True

    # Verify database update was called
    mock_session.execute.assert_called_once()
    mock_session.commit.assert_called_once()

    # Verify SET embedding = NULL was in query
    call_args = mock_session.execute.call_args
    query = str(call_args[0][0])
    assert "SET embedding = NULL" in query or "embedding" in str(call_args)


@pytest.mark.asyncio
async def test_delete_no_match(pgvector_repo, mock_postgres_client):
    """Test delete returns False when item not found."""
    mock_result = MagicMock()
    mock_result.rowcount = 0

    mock_session = await mock_postgres_client.session().__aenter__()
    mock_session.execute.return_value = mock_result

    # Execute
    result = await pgvector_repo.delete(
        item_id="nonexistent",
        collection="tasks"
    )

    # Verify failure
    assert result is False


@pytest.mark.asyncio
async def test_delete_error_handling(pgvector_repo, mock_postgres_client):
    """Test delete handles errors gracefully."""
    mock_session = await mock_postgres_client.session().__aenter__()
    mock_session.execute.side_effect = Exception("Database error")

    # Execute
    result = await pgvector_repo.delete(
        item_id="task-999",
        collection="tasks"
    )

    # Verify failure
    assert result is False


# =============================================================================
# GET TESTS
# =============================================================================


@pytest.mark.asyncio
async def test_get_success(pgvector_repo, mock_postgres_client):
    """Test retrieving item with embedding."""
    # Mock database result
    mock_row = MagicMock()
    mock_row.item_id = "task-123"
    mock_row.content = "Test task"
    mock_row.embedding = "[0.1,0.2,0.3,0.4]"

    mock_result = MagicMock()
    mock_result.fetchone.return_value = mock_row

    mock_session = await mock_postgres_client.session().__aenter__()
    mock_session.execute.return_value = mock_result

    # Execute
    result = await pgvector_repo.get(
        item_id="task-123",
        collection="tasks"
    )

    # Verify result
    assert result is not None
    assert result["item_id"] == "task-123"
    assert result["content"] == "Test task"
    assert result["embedding"] is not None


@pytest.mark.asyncio
async def test_get_not_found(pgvector_repo, mock_postgres_client):
    """Test get returns None when item not found."""
    mock_result = MagicMock()
    mock_result.fetchone.return_value = None

    mock_session = await mock_postgres_client.session().__aenter__()
    mock_session.execute.return_value = mock_result

    # Execute
    result = await pgvector_repo.get(
        item_id="nonexistent",
        collection="tasks"
    )

    # Verify None returned
    assert result is None


@pytest.mark.asyncio
async def test_get_without_embedding(pgvector_repo, mock_postgres_client):
    """Test get handles item without embedding."""
    # Mock row with NULL embedding
    mock_row = MagicMock()
    mock_row.item_id = "task-456"
    mock_row.content = "Task without embedding"
    mock_row.embedding = None

    mock_result = MagicMock()
    mock_result.fetchone.return_value = mock_row

    mock_session = await mock_postgres_client.session().__aenter__()
    mock_session.execute.return_value = mock_result

    # Execute
    result = await pgvector_repo.get(
        item_id="task-456",
        collection="tasks"
    )

    # Verify result with None embedding
    assert result is not None
    assert result["item_id"] == "task-456"
    assert result["embedding"] is None


# =============================================================================
# GET_COLLECTION_STATS TESTS
# =============================================================================


@pytest.mark.asyncio
async def test_get_collection_stats(pgvector_repo, mock_postgres_client):
    """Test collection statistics."""
    # Mock database result
    mock_row = MagicMock()
    mock_row.total_count = 100
    mock_row.embedded_count = 85
    mock_row.missing_embeddings = 15

    mock_result = MagicMock()
    mock_result.fetchone.return_value = mock_row

    mock_session = await mock_postgres_client.session().__aenter__()
    mock_session.execute.return_value = mock_result

    # Execute
    stats = await pgvector_repo.get_collection_stats("tasks")

    # Verify stats
    assert stats["collection"] == "tasks"
    assert stats["table"] == "tasks"
    assert stats["total_count"] == 100
    assert stats["embedded_count"] == 85
    assert stats["missing_embeddings"] == 15


@pytest.mark.asyncio
async def test_get_collection_stats_error(pgvector_repo, mock_postgres_client):
    """Test collection stats handles errors."""
    mock_session = await mock_postgres_client.session().__aenter__()
    mock_session.execute.side_effect = Exception("Database error")

    # Execute
    stats = await pgvector_repo.get_collection_stats("tasks")

    # Verify error in stats
    assert "error" in stats
    assert stats["collection"] == "tasks"


# =============================================================================
# REBUILD_INDEX TESTS
# =============================================================================


@pytest.mark.asyncio
async def test_rebuild_index(pgvector_repo, mock_postgres_client):
    """Test index rebuild."""
    mock_conn = await mock_postgres_client.engine.begin().__aenter__()

    # Execute
    await pgvector_repo.rebuild_index("tasks", lists=100)

    # Verify two execute calls (drop + create)
    assert mock_conn.execute.call_count == 2

    # Verify both execute calls were made (drop and create index)
    # Note: We verify by call count since SQLAlchemy text() objects are opaque
    assert mock_conn.execute.called


@pytest.mark.asyncio
async def test_rebuild_index_error(pgvector_repo, mock_postgres_client):
    """Test rebuild index handles errors."""
    mock_conn = await mock_postgres_client.engine.begin().__aenter__()
    mock_conn.execute.side_effect = Exception("Index error")

    # Execute and expect exception
    with pytest.raises(Exception, match="Index error"):
        await pgvector_repo.rebuild_index("tasks")


# =============================================================================
# BATCH_UPSERT TESTS
# =============================================================================


@pytest.mark.asyncio
async def test_batch_upsert_success(pgvector_repo, mock_postgres_client, mock_embedding_service):
    """Test batch upsert."""
    # Mock embedding service to return different embeddings
    mock_embedding_service.embed.side_effect = [
        np.array([0.1, 0.2, 0.3, 0.4]),
        np.array([0.5, 0.6, 0.7, 0.8]),
        np.array([0.9, 0.8, 0.7, 0.6])
    ]

    # Mock database results
    mock_result = MagicMock()
    mock_result.rowcount = 1

    mock_transaction = await mock_postgres_client.transaction().__aenter__()
    mock_transaction.execute.return_value = mock_result

    # Execute batch upsert
    items = [
        {"item_id": "task-1", "content": "Task 1"},
        {"item_id": "task-2", "content": "Task 2"},
        {"item_id": "task-3", "content": "Task 3"}
    ]

    success_count = await pgvector_repo.batch_upsert(items, "tasks")

    # Verify all succeeded
    assert success_count == 3

    # Verify embedding service was called for each item
    assert mock_embedding_service.embed.call_count == 3

    # Verify transaction execute was called for each item
    assert mock_transaction.execute.call_count == 3


@pytest.mark.asyncio
async def test_batch_upsert_partial_success(pgvector_repo, mock_postgres_client, mock_embedding_service):
    """Test batch upsert with some failures."""
    mock_embedding_service.embed.side_effect = [
        np.array([0.1, 0.2, 0.3, 0.4]),
        np.array([0.5, 0.6, 0.7, 0.8])
    ]

    # Mock results - first succeeds, second fails (0 rows)
    mock_result_success = MagicMock()
    mock_result_success.rowcount = 1

    mock_result_fail = MagicMock()
    mock_result_fail.rowcount = 0

    mock_transaction = await mock_postgres_client.transaction().__aenter__()
    mock_transaction.execute.side_effect = [mock_result_success, mock_result_fail]

    # Execute batch upsert
    items = [
        {"item_id": "task-1", "content": "Task 1"},
        {"item_id": "nonexistent", "content": "Task 2"}
    ]

    success_count = await pgvector_repo.batch_upsert(items, "tasks")

    # Verify partial success
    assert success_count == 1


@pytest.mark.asyncio
async def test_batch_upsert_error(pgvector_repo, mock_postgres_client, mock_embedding_service):
    """Test batch upsert handles errors."""
    mock_embedding_service.embed.return_value = np.array([0.1, 0.2, 0.3, 0.4])

    mock_transaction = await mock_postgres_client.transaction().__aenter__()
    mock_transaction.execute.side_effect = Exception("Transaction error")

    # Execute batch upsert
    items = [{"item_id": "task-1", "content": "Task 1"}]

    success_count = await pgvector_repo.batch_upsert(items, "tasks")

    # Verify zero success
    assert success_count == 0
