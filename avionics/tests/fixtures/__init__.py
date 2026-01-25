"""Test fixtures for jeeves_avionics.

This package provides reusable fixtures for testing the infrastructure layer.
Includes database containers, LLM mocks, and core engine protocol mocks.

Centralized Architecture (v4.0):
- MockEnvelope replaces MockCoreEnvelope
"""

from .database import (
    postgres_container,
    pg_test_db,
    create_test_prerequisites,
    create_session_only,
)
from .llm import (
    mock_llm_provider,
)
from .mocks.core_mocks import (
    MockEnvelope,
    mock_envelope_factory,
)

__all__ = [
    # Database
    "postgres_container",
    "pg_test_db",
    "create_test_prerequisites",
    "create_session_only",
    # LLM
    "mock_llm_provider",
    # Core mocks (v4.0)
    "MockEnvelope",
    "mock_envelope_factory",
]
