"""Unit tests for ResourceTracker.

Tests the kernel-level resource tracking component:
- Quota allocation and release
- Usage recording
- Quota enforcement
- System-wide metrics
"""

import pytest
from unittest.mock import MagicMock

from control_tower.resources.tracker import ResourceTracker
from control_tower.types import ResourceQuota, ResourceUsage


@pytest.fixture
def mock_logger():
    """Create a mock logger."""
    logger = MagicMock()
    logger.bind.return_value = logger
    return logger


@pytest.fixture
def tracker(mock_logger):
    """Create a ResourceTracker instance."""
    return ResourceTracker(
        logger=mock_logger,
        default_quota=ResourceQuota(
            max_llm_calls=10,
            max_tool_calls=50,
            max_agent_hops=21,
            max_iterations=3,
            timeout_seconds=300,
        ),
    )


class TestResourceAllocation:
    """Test resource allocation and release."""

    def test_allocate_creates_tracking(self, tracker):
        """Test that allocate creates resource tracking for a process."""
        quota = ResourceQuota(max_llm_calls=5)
        result = tracker.allocate("pid-1", quota)

        assert result is True
        assert tracker.is_tracked("pid-1")
        assert tracker.get_quota("pid-1").max_llm_calls == 5

    def test_allocate_duplicate_returns_false(self, tracker):
        """Test that duplicate allocation returns False."""
        quota = ResourceQuota()
        tracker.allocate("pid-1", quota)

        result = tracker.allocate("pid-1", quota)
        assert result is False

    def test_release_removes_tracking(self, tracker):
        """Test that release removes resource tracking."""
        tracker.allocate("pid-1", ResourceQuota())

        result = tracker.release("pid-1")

        assert result is True
        assert not tracker.is_tracked("pid-1")

    def test_release_nonexistent_returns_false(self, tracker):
        """Test that releasing non-existent process returns False."""
        result = tracker.release("nonexistent")
        assert result is False


class TestUsageRecording:
    """Test resource usage recording."""

    def test_record_llm_calls(self, tracker):
        """Test recording LLM calls."""
        tracker.allocate("pid-1", ResourceQuota())

        usage = tracker.record_usage("pid-1", llm_calls=3)

        assert usage.llm_calls == 3

    def test_record_tool_calls(self, tracker):
        """Test recording tool calls."""
        tracker.allocate("pid-1", ResourceQuota())

        usage = tracker.record_usage("pid-1", tool_calls=5)

        assert usage.tool_calls == 5

    def test_record_agent_hops(self, tracker):
        """Test recording agent hops."""
        tracker.allocate("pid-1", ResourceQuota())

        usage = tracker.record_usage("pid-1", agent_hops=2)

        assert usage.agent_hops == 2

    def test_record_tokens(self, tracker):
        """Test recording token usage."""
        tracker.allocate("pid-1", ResourceQuota())

        usage = tracker.record_usage("pid-1", tokens_in=100, tokens_out=50)

        assert usage.tokens_in == 100
        assert usage.tokens_out == 50

    def test_record_accumulates(self, tracker):
        """Test that recording accumulates usage."""
        tracker.allocate("pid-1", ResourceQuota())

        tracker.record_usage("pid-1", llm_calls=2)
        tracker.record_usage("pid-1", llm_calls=3)
        usage = tracker.get_usage("pid-1")

        assert usage.llm_calls == 5

    def test_record_creates_tracking_if_missing(self, tracker):
        """Test that recording creates tracking with default quota if missing."""
        usage = tracker.record_usage("pid-new", llm_calls=1)

        assert tracker.is_tracked("pid-new")
        assert usage.llm_calls == 1


class TestQuotaEnforcement:
    """Test quota checking and enforcement."""

    def test_check_quota_within_limits(self, tracker):
        """Test that check_quota returns None when within limits."""
        tracker.allocate("pid-1", ResourceQuota(max_llm_calls=10))
        tracker.record_usage("pid-1", llm_calls=5)

        result = tracker.check_quota("pid-1")

        assert result is None

    def test_check_quota_llm_exceeded(self, tracker):
        """Test that check_quota returns reason when LLM calls exceeded."""
        tracker.allocate("pid-1", ResourceQuota(max_llm_calls=5))
        tracker.record_usage("pid-1", llm_calls=6)

        result = tracker.check_quota("pid-1")

        assert result == "max_llm_calls_exceeded"

    def test_check_quota_tool_exceeded(self, tracker):
        """Test that check_quota returns reason when tool calls exceeded."""
        tracker.allocate("pid-1", ResourceQuota(max_tool_calls=10))
        tracker.record_usage("pid-1", tool_calls=11)

        result = tracker.check_quota("pid-1")

        assert result == "max_tool_calls_exceeded"

    def test_check_quota_agent_hops_exceeded(self, tracker):
        """Test that check_quota returns reason when agent hops exceeded."""
        tracker.allocate("pid-1", ResourceQuota(max_agent_hops=5))
        tracker.record_usage("pid-1", agent_hops=6)

        result = tracker.check_quota("pid-1")

        assert result == "max_agent_hops_exceeded"

    def test_check_quota_untracked_returns_none(self, tracker):
        """Test that check_quota returns None for untracked processes."""
        result = tracker.check_quota("untracked")
        assert result is None


class TestSystemMetrics:
    """Test system-wide metrics."""

    def test_get_system_usage_initial(self, tracker):
        """Test initial system usage."""
        usage = tracker.get_system_usage()

        assert usage["total_processes"] == 0
        assert usage["active_processes"] == 0
        assert usage["system_llm_calls"] == 0

    def test_get_system_usage_tracks_processes(self, tracker):
        """Test that system usage tracks process counts."""
        tracker.allocate("pid-1", ResourceQuota())
        tracker.allocate("pid-2", ResourceQuota())

        usage = tracker.get_system_usage()

        assert usage["total_processes"] == 2
        assert usage["active_processes"] == 2

    def test_get_system_usage_tracks_releases(self, tracker):
        """Test that system usage tracks releases."""
        tracker.allocate("pid-1", ResourceQuota())
        tracker.allocate("pid-2", ResourceQuota())
        tracker.release("pid-1")

        usage = tracker.get_system_usage()

        assert usage["total_processes"] == 2
        assert usage["active_processes"] == 1

    def test_get_system_usage_aggregates_calls(self, tracker):
        """Test that system usage aggregates LLM/tool calls."""
        tracker.allocate("pid-1", ResourceQuota())
        tracker.allocate("pid-2", ResourceQuota())
        tracker.record_usage("pid-1", llm_calls=3, tool_calls=5)
        tracker.record_usage("pid-2", llm_calls=2, tool_calls=3)

        usage = tracker.get_system_usage()

        assert usage["system_llm_calls"] == 5
        assert usage["system_tool_calls"] == 8

    def test_get_system_usage_aggregates_tokens(self, tracker):
        """Test that system usage aggregates tokens."""
        tracker.allocate("pid-1", ResourceQuota())
        tracker.record_usage("pid-1", tokens_in=100, tokens_out=50)
        tracker.record_usage("pid-1", tokens_in=200, tokens_out=100)

        usage = tracker.get_system_usage()

        assert usage["system_tokens_in"] == 300
        assert usage["system_tokens_out"] == 150


class TestUtilityMethods:
    """Test utility methods."""

    def test_get_remaining_budget(self, tracker):
        """Test get_remaining_budget returns correct values."""
        tracker.allocate(
            "pid-1",
            ResourceQuota(max_llm_calls=10, max_tool_calls=50),
        )
        tracker.record_usage("pid-1", llm_calls=3, tool_calls=20)

        budget = tracker.get_remaining_budget("pid-1")

        assert budget["llm_calls"] == 7
        assert budget["tool_calls"] == 30

    def test_adjust_quota(self, tracker):
        """Test quota adjustment."""
        tracker.allocate("pid-1", ResourceQuota(max_llm_calls=10))

        result = tracker.adjust_quota("pid-1", max_llm_calls=20)

        assert result is True
        assert tracker.get_quota("pid-1").max_llm_calls == 20

    def test_get_all_usage(self, tracker):
        """Test get_all_usage returns all tracked processes."""
        tracker.allocate("pid-1", ResourceQuota())
        tracker.allocate("pid-2", ResourceQuota())
        tracker.record_usage("pid-1", llm_calls=1)
        tracker.record_usage("pid-2", llm_calls=2)

        all_usage = tracker.get_all_usage()

        assert len(all_usage) == 2
        assert all_usage["pid-1"].llm_calls == 1
        assert all_usage["pid-2"].llm_calls == 2


class TestThreadSafety:
    """Test thread safety."""

    def test_concurrent_allocations(self, tracker):
        """Test concurrent allocations are thread-safe."""
        import threading

        results = []

        def allocate(pid):
            result = tracker.allocate(pid, ResourceQuota())
            results.append((pid, result))

        threads = [
            threading.Thread(target=allocate, args=(f"pid-{i}",))
            for i in range(10)
        ]

        for t in threads:
            t.start()
        for t in threads:
            t.join()

        # All allocations should succeed
        assert len(results) == 10
        assert all(r[1] for r in results)

    def test_concurrent_recordings(self, tracker):
        """Test concurrent recordings are thread-safe."""
        import threading

        tracker.allocate("pid-1", ResourceQuota())

        def record():
            tracker.record_usage("pid-1", llm_calls=1)

        threads = [threading.Thread(target=record) for _ in range(100)]

        for t in threads:
            t.start()
        for t in threads:
            t.join()

        usage = tracker.get_usage("pid-1")
        assert usage.llm_calls == 100
