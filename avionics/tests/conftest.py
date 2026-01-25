"""Pytest configuration for jeeves_avionics tests.

This conftest.py provides fixtures and configuration for the avionics
test suite. Avionics tests may use database containers but should mock
core engine protocols when testing in isolation.

Key Principles:
- Use testcontainers for PostgreSQL
- Mock jeeves_core_engine protocols when needed
- Each test gets a fresh database
"""

import sys
from pathlib import Path

import pytest

# Add the project root to the path for imports
project_root = Path(__file__).parent.parent.parent
if str(project_root) not in sys.path:
    sys.path.insert(0, str(project_root))


# =============================================================================
# IMPORT FIXTURES FROM FIXTURES PACKAGE
# =============================================================================

from avionics.tests.fixtures.database import (
    postgres_container,
    pg_test_db,
    create_test_prerequisites,
    create_session_only,
)

from avionics.tests.fixtures.llm import (
    mock_llm_provider,
    mock_llm_provider_factory,
    MockLLMProvider,
)

from avionics.tests.fixtures.mocks.core_mocks import (
    MockEnvelope,
    mock_envelope_factory,
    mock_envelope,
)


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


# Note: anyio_backend fixture is centralized in root conftest.py
