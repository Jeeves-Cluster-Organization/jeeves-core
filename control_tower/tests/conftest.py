"""Pytest configuration for jeeves_control_tower tests.

This conftest.py provides fixtures and configuration for the Control Tower
test suite. Control Tower tests should test kernel-level components in isolation.

Key Principles:
- Control Tower is a kernel layer - test without external dependencies
- Mock jeeves_protocols types when needed
- Test resource tracking, lifecycle management, and IPC coordination

Constitutional Compliance:
- Control Tower only imports from protocols
- Tests should validate kernel behavior, not service behavior
"""

import sys
from pathlib import Path
from typing import Optional

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
        "markers", "integration: Integration tests requiring kernel components"
    )
    config.addinivalue_line(
        "markers", "slow: Slow-running tests"
    )
    config.addinivalue_line(
        "markers", "kernel: Kernel-level tests"
    )


# =============================================================================
# CONTROL TOWER TYPE FIXTURES
# =============================================================================
# Note: mock_logger fixture is centralized in root conftest.py

@pytest.fixture
def sample_resource_quota():
    """Create a sample ResourceQuota for testing."""
    from control_tower.types import ResourceQuota

    return ResourceQuota(
        max_llm_calls=10,
        max_tool_calls=50,
        max_agent_hops=21,
        max_iterations=3,
        timeout_seconds=300,
    )


@pytest.fixture
def sample_resource_usage():
    """Create a sample ResourceUsage for testing."""
    from control_tower.types import ResourceUsage

    return ResourceUsage(
        llm_calls=5,
        tool_calls=20,
        agent_hops=3,
        tokens_in=1000,
        tokens_out=500,
    )


@pytest.fixture
def sample_service_descriptor():
    """Create a sample ServiceDescriptor for testing."""
    from control_tower.types import ServiceDescriptor, SchedulingPriority

    return ServiceDescriptor(
        name="test_service",
        priority=SchedulingPriority.NORMAL,
        capabilities=["process_request", "handle_event"],
    )


# =============================================================================
# COMPONENT FIXTURES
# =============================================================================

@pytest.fixture
def resource_tracker(mock_logger, sample_resource_quota):
    """Create a ResourceTracker instance for testing."""
    from control_tower.resources.tracker import ResourceTracker

    return ResourceTracker(
        logger=mock_logger,
        default_quota=sample_resource_quota,
    )


@pytest.fixture
def lifecycle_manager(mock_logger):
    """Create a LifecycleManager instance for testing."""
    from control_tower.lifecycle.manager import LifecycleManager

    return LifecycleManager(logger=mock_logger)


@pytest.fixture
def event_aggregator(mock_logger):
    """Create an EventAggregator instance for testing."""
    from control_tower.events.aggregator import EventAggregator

    return EventAggregator(logger=mock_logger)


@pytest.fixture
def commbus_coordinator(mock_logger):
    """Create a CommBusCoordinator instance for testing."""
    from control_tower.ipc.coordinator import CommBusCoordinator

    return CommBusCoordinator(logger=mock_logger)


@pytest.fixture
def control_tower(mock_logger, sample_resource_quota):
    """Create a ControlTower (kernel) instance for testing."""
    from control_tower.kernel import ControlTower

    return ControlTower(
        logger=mock_logger,
        default_quota=sample_resource_quota,
    )


# =============================================================================
# MOCK ENVELOPE FIXTURE
# =============================================================================

@pytest.fixture
def mock_envelope_factory():
    """Factory for creating mock envelopes for testing."""
    def _create_envelope(
        session_id: str = "test-session",
        request_id: Optional[str] = None,
        user_input: str = "test input",
    ):
        envelope = MagicMock()
        envelope.session_id = session_id
        envelope.request_id = request_id or f"req-{session_id}"
        envelope.user_input = user_input
        envelope.metadata = {}
        return envelope

    return _create_envelope


@pytest.fixture
def mock_envelope(mock_envelope_factory):
    """Create a default mock envelope for testing."""
    return mock_envelope_factory()


# Note: anyio_backend fixture is centralized in root conftest.py

# Re-export for pytest discovery
__all__ = [
    "mock_logger",
    "sample_resource_quota",
    "sample_resource_usage",
    "sample_service_descriptor",
    "resource_tracker",
    "lifecycle_manager",
    "event_aggregator",
    "commbus_coordinator",
    "control_tower",
    "mock_envelope_factory",
    "mock_envelope",
]
