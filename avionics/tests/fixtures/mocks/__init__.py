"""Mock implementations for testing avionics.

This package provides mocks for jeeves_core_engine protocols,
allowing avionics tests to run in isolation.

Centralized Architecture (v4.0):
- MockEnvelope replaces MockCoreEnvelope
"""

from .core_mocks import (
    MockEnvelope,
    mock_envelope_factory,
)

__all__ = [
    "MockEnvelope",
    "mock_envelope_factory",
]
