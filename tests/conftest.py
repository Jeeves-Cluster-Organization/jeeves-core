"""Root conftest.py for jeeves-core tests.

Provides shared fixtures used across all test modules.
"""

import pytest
from unittest.mock import MagicMock, AsyncMock


@pytest.fixture
def mock_logger():
    """Create a mock logger for testing.

    This fixture is used across all modules (control_tower, memory_module,
    avionics, mission_system) to provide a consistent mock logger interface.
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
