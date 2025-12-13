"""Mock implementations for testing jeeves_avionics.

This package provides mocks for jeeves_core_engine protocols,
allowing avionics tests to run in isolation.

Centralized Architecture (v4.0):
- MockGenericEnvelope replaces MockCoreEnvelope
"""

from .core_mocks import (
    MockGenericEnvelope,
    mock_envelope_factory,
)

__all__ = [
    "MockGenericEnvelope",
    "mock_envelope_factory",
]
