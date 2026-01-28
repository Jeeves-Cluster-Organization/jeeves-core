"""Root conftest.py for jeeves-core tests.

This file MUST be at the repository root for fixtures to be discovered
when running tests from any subdirectory.

Provides shared fixtures used across all test modules:
- control_tower/tests
- memory_module/tests
- avionics/tests
- mission_system/tests
- protocols/tests
"""

import pytest
from unittest.mock import MagicMock, AsyncMock


@pytest.fixture
def mock_logger():
    """Create a mock logger for testing.

    This fixture is used across all modules to provide a consistent
    mock logger interface following ADR-001 dependency injection pattern.
    """
    logger = MagicMock()
    logger.info = MagicMock()
    logger.debug = MagicMock()
    logger.warning = MagicMock()
    logger.error = MagicMock()
    logger.exception = MagicMock()
    logger.bind = MagicMock(return_value=logger)
    return logger


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
