"""Pytest configuration for jeeves_memory_module tests.

This conftest.py provides fixtures and configuration for the Memory Module
test suite. Memory tests may need database access but should mock services
when testing in isolation.

Key Principles:
- Use AsyncMock for async database operations
- Mock repositories when testing services
- Each test should be isolated and deterministic

Constitutional Compliance:
- Memory Module imports from jeeves_protocols
- Tests validate memory behavior without external services
"""

import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List, Optional
from unittest.mock import AsyncMock

import pytest

# Add the project root to the path for imports
project_root = Path(__file__).parent.parent.parent
if str(project_root) not in sys.path:
    sys.path.insert(0, str(project_root))


# =============================================================================
# PYTEST MARKERS
# =============================================================================

def pytest_configure(config):
    """Register custom markers."""
    config.addinivalue_line(
        "markers", "unit: Unit tests (may use mocks)"
    )
    config.addinivalue_line(
        "markers", "integration: Integration tests requiring database"
    )
    config.addinivalue_line(
        "markers", "slow: Slow-running tests"
    )
    config.addinivalue_line(
        "markers", "memory: Memory module specific tests"
    )
    config.addinivalue_line(
        "markers", "requires_postgres: Tests requiring PostgreSQL database"
    )


# =============================================================================
# MOCK DATABASE FIXTURE
# =============================================================================

@pytest.fixture
def mock_db():
    """Create a mock async database client.

    Provides async methods for:
    - execute(query, *params) -> None
    - fetch_one(query, *params) -> Optional[Dict]
    - fetch_all(query, *params) -> List[Dict]
    """
    db = AsyncMock()
    db.execute = AsyncMock(return_value=None)
    db.fetch_one = AsyncMock(return_value=None)
    db.fetch_all = AsyncMock(return_value=[])
    return db


# Note: mock_logger fixture is centralized in root conftest.py

# =============================================================================
# REPOSITORY FIXTURES
# =============================================================================

@pytest.fixture
def mock_session_state_repository():
    """Create a mock SessionStateRepository."""
    repo = AsyncMock()
    repo.ensure_table = AsyncMock()
    repo.get = AsyncMock(return_value=None)
    repo.upsert = AsyncMock()
    repo.update_focus = AsyncMock()
    repo.add_referenced_entity = AsyncMock()
    repo.increment_turn = AsyncMock()
    repo.update_short_term_memory = AsyncMock()
    repo.delete = AsyncMock()
    return repo


@pytest.fixture
def mock_event_repository():
    """Create a mock EventRepository."""
    repo = AsyncMock()
    repo.ensure_table = AsyncMock()
    repo.store = AsyncMock()
    repo.get_by_session = AsyncMock(return_value=[])
    repo.get_by_request = AsyncMock(return_value=[])
    repo.search = AsyncMock(return_value=[])
    return repo


@pytest.fixture
def mock_trace_repository():
    """Create a mock TraceRepository."""
    repo = AsyncMock()
    repo.ensure_table = AsyncMock()
    repo.store = AsyncMock()
    repo.get_by_request = AsyncMock(return_value=[])
    repo.get_latest_by_session = AsyncMock(return_value=None)
    return repo


@pytest.fixture
def mock_tool_metrics_repository():
    """Create a mock ToolMetricsRepository."""
    repo = AsyncMock()
    repo.ensure_table = AsyncMock()
    repo.record_tool_usage = AsyncMock()
    repo.get_tool_stats = AsyncMock(return_value={})
    repo.get_failing_tools = AsyncMock(return_value=[])
    return repo


# =============================================================================
# SERVICE FIXTURES
# =============================================================================

@pytest.fixture
def mock_embedding_service():
    """Create a mock EmbeddingService."""
    service = AsyncMock()
    service.embed_text = AsyncMock(return_value=[0.1] * 384)  # Default embedding dim
    service.embed_batch = AsyncMock(return_value=[[0.1] * 384])
    return service


@pytest.fixture
def mock_llm_provider():
    """Create a mock LLM provider for memory services."""
    provider = AsyncMock()
    provider.call = AsyncMock(return_value={
        "content": "Mock LLM response",
        "model": "test-model",
        "usage": {"prompt_tokens": 10, "completion_tokens": 20},
    })
    return provider


# =============================================================================
# DATA FIXTURES
# =============================================================================

@pytest.fixture
def sample_session_state():
    """Create a sample session state for testing."""
    return {
        "session_id": "sess-test-123",
        "user_id": "user-456",
        "focus_type": "task",
        "focus_id": "task-789",
        "focus_context": {"last_action": "created"},
        "referenced_entities": [
            {"type": "task", "id": "task-789", "title": "Test Task"}
        ],
        "short_term_memory": "User created a task about testing",
        "turn_count": 5,
        "created_at": datetime.now(timezone.utc),
        "updated_at": datetime.now(timezone.utc),
    }


@pytest.fixture
def sample_event():
    """Create a sample event for testing."""
    return {
        "event_id": "evt-123",
        "session_id": "sess-test-123",
        "request_id": "req-456",
        "event_type": "user_message",
        "payload": {"content": "Hello, world!"},
        "timestamp": datetime.now(timezone.utc),
    }


@pytest.fixture
def sample_trace():
    """Create a sample trace for testing."""
    return {
        "trace_id": "trace-123",
        "request_id": "req-456",
        "session_id": "sess-test-123",
        "agent_name": "perception",
        "start_time": datetime.now(timezone.utc),
        "end_time": datetime.now(timezone.utc),
        "status": "success",
        "metadata": {},
    }


@pytest.fixture
def sample_tool_metrics():
    """Create sample tool metrics for testing."""
    return {
        "tool_name": "file_reader",
        "success_count": 95,
        "failure_count": 5,
        "avg_latency_ms": 150.5,
        "last_used": datetime.now(timezone.utc),
    }


# Note: anyio_backend fixture is centralized in root conftest.py

# Re-export for pytest discovery
__all__ = [
    # Database
    "mock_db",
    "mock_logger",
    # Repositories
    "mock_session_state_repository",
    "mock_event_repository",
    "mock_trace_repository",
    "mock_tool_metrics_repository",
    # Services
    "mock_embedding_service",
    "mock_llm_provider",
    # Data
    "sample_session_state",
    "sample_event",
    "sample_trace",
    "sample_tool_metrics",
]
