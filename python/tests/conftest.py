"""Pytest configuration for jeeves-core tests.

This conftest.py provides fixtures and configuration for the
infrastructure test suite.

Key Principles:
- Use in-memory SQLite for fast, isolated tests
- Mock protocols when testing in isolation
- Each test gets a fresh database
"""

import sys
import warnings
from pathlib import Path

import pytest

# Add paths for imports
# 1. jeeves-core root (for jeeves_core package)
jeeves_core_root = Path(__file__).parent.parent
if str(jeeves_core_root) not in sys.path:
    sys.path.insert(0, str(jeeves_core_root))

# 2. tests directory (for config.* and fixtures.*)
tests_root = Path(__file__).parent
if str(tests_root) not in sys.path:
    sys.path.insert(0, str(tests_root))


# =============================================================================
# IMPORT FIXTURES FROM FIXTURES PACKAGE
# =============================================================================
# Note: Some fixtures have complex dependencies (jeeves_core, shared).
# Import failures are handled gracefully to allow unit tests to run.

try:
    from fixtures.database import (
        test_db,
        create_test_prerequisites,
        create_session_only,
    )
except ImportError as e:
    warnings.warn(f"Database fixtures unavailable: {e}", stacklevel=1)

try:
    from fixtures.llm import (
        mock_llm_provider,
        mock_llm_provider_factory,
        MockLLMProvider,
    )
except ImportError as e:
    warnings.warn(f"LLM fixtures unavailable: {e}", stacklevel=1)

try:
    from fixtures.mocks.core_mocks import (
        MockEnvelope,
        mock_envelope_factory,
        mock_envelope,
    )
except ImportError as e:
    warnings.warn(f"Core mock fixtures unavailable: {e}", stacklevel=1)

try:
    from fixtures.mocks.kernel_mocks import (
        mock_transport,
        mock_kernel_client,
    )
except ImportError as e:
    warnings.warn(f"Kernel mock fixtures unavailable: {e}", stacklevel=1)


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
