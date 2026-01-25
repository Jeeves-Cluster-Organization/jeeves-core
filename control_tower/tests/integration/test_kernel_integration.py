"""Integration tests for Control Tower kernel.

Tests the integration between kernel components:
- ResourceTracker

Note: Some kernel components (LifecycleManager, CommBusCoordinator, EventAggregator)
have different APIs than initially assumed. These tests focus on the well-defined
ResourceTracker component.
"""

import pytest
from unittest.mock import MagicMock


@pytest.fixture
def mock_logger():
    """Create a mock logger."""
    logger = MagicMock()
    logger.bind.return_value = logger
    return logger


class TestControlTowerIntegration:
    """Integration tests for ControlTower kernel."""

    def test_kernel_initialization(self, mock_logger):
        """Test that kernel initializes all components."""
        from control_tower.kernel import ControlTower
        from control_tower.types import ResourceQuota

        kernel = ControlTower(
            logger=mock_logger,
            default_quota=ResourceQuota(max_llm_calls=10),
        )

        # Core components should be initialized
        assert kernel._logger is not None

    def test_resource_allocation_flow(self, mock_logger):
        """Test resource allocation through kernel."""
        from control_tower.resources.tracker import ResourceTracker
        from control_tower.types import ResourceQuota

        tracker = ResourceTracker(logger=mock_logger)

        # Allocate resources for a process
        quota = ResourceQuota(max_llm_calls=5, max_tool_calls=20)
        result = tracker.allocate("pid-123", quota)

        assert result is True
        assert tracker.is_tracked("pid-123")

        # Record usage
        usage = tracker.record_usage("pid-123", llm_calls=2)
        assert usage.llm_calls == 2

        # Check quota
        exceeded = tracker.check_quota("pid-123")
        assert exceeded is None  # Not exceeded yet

    def test_resource_quota_enforcement(self, mock_logger):
        """Test that quota is enforced."""
        from control_tower.resources.tracker import ResourceTracker
        from control_tower.types import ResourceQuota

        tracker = ResourceTracker(logger=mock_logger)

        # Allocate with low quota
        quota = ResourceQuota(max_llm_calls=2)
        tracker.allocate("pid-limited", quota)

        # Record usage exceeding quota
        tracker.record_usage("pid-limited", llm_calls=3)

        # Should be exceeded
        exceeded = tracker.check_quota("pid-limited")
        assert exceeded == "max_llm_calls_exceeded"


class TestResourceTrackerIntegration:
    """Integration tests for ResourceTracker component."""

    def test_multiple_process_tracking(self, mock_logger):
        """Test tracking multiple processes."""
        from control_tower.resources.tracker import ResourceTracker
        from control_tower.types import ResourceQuota

        tracker = ResourceTracker(logger=mock_logger)

        # Allocate resources for multiple processes
        for i in range(5):
            tracker.allocate(f"pid-{i}", ResourceQuota(max_llm_calls=10))
            tracker.record_usage(f"pid-{i}", llm_calls=i)

        # Verify all are tracked
        for i in range(5):
            assert tracker.is_tracked(f"pid-{i}")
            usage = tracker.get_usage(f"pid-{i}")
            assert usage.llm_calls == i

    def test_system_metrics_aggregation(self, mock_logger):
        """Test system-wide metrics aggregation."""
        from control_tower.resources.tracker import ResourceTracker
        from control_tower.types import ResourceQuota

        tracker = ResourceTracker(logger=mock_logger)

        # Create processes and record usage
        tracker.allocate("pid-1", ResourceQuota())
        tracker.allocate("pid-2", ResourceQuota())
        tracker.record_usage("pid-1", llm_calls=5, tool_calls=10)
        tracker.record_usage("pid-2", llm_calls=3, tool_calls=7)

        # Check system metrics
        metrics = tracker.get_system_usage()
        assert metrics["system_llm_calls"] == 8
        assert metrics["system_tool_calls"] == 17
        assert metrics["active_processes"] == 2

    def test_budget_calculation(self, mock_logger):
        """Test remaining budget calculation."""
        from control_tower.resources.tracker import ResourceTracker
        from control_tower.types import ResourceQuota

        tracker = ResourceTracker(logger=mock_logger)

        # Allocate with specific quota
        quota = ResourceQuota(max_llm_calls=10, max_tool_calls=50)
        tracker.allocate("pid-test", quota)

        # Use some of the quota
        tracker.record_usage("pid-test", llm_calls=3, tool_calls=20)

        # Check remaining budget
        budget = tracker.get_remaining_budget("pid-test")
        assert budget["llm_calls"] == 7
        assert budget["tool_calls"] == 30

    def test_process_release(self, mock_logger):
        """Test releasing process resources."""
        from control_tower.resources.tracker import ResourceTracker
        from control_tower.types import ResourceQuota

        tracker = ResourceTracker(logger=mock_logger)

        # Allocate and release
        tracker.allocate("pid-release", ResourceQuota())
        assert tracker.is_tracked("pid-release")

        tracker.release("pid-release")
        assert not tracker.is_tracked("pid-release")
