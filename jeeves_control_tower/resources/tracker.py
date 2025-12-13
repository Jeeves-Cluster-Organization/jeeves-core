"""Resource Tracker - cgroups equivalent.

This implements resource management:
- Quota allocation per process
- Usage tracking
- Limit enforcement
- System-wide metrics

Layering: ONLY imports from jeeves_protocols (syscall interface).
"""

import threading
from dataclasses import dataclass, field
from datetime import datetime
from typing import Any, Dict, Optional

from jeeves_protocols import LoggerProtocol
from jeeves_shared.serialization import utc_now

from jeeves_control_tower.protocols import ResourceTrackerProtocol
from jeeves_control_tower.types import ResourceQuota, ResourceUsage


@dataclass
class _ProcessResources:
    """Internal resource tracking for a process."""
    pid: str
    quota: ResourceQuota
    usage: ResourceUsage = field(default_factory=ResourceUsage)
    allocated_at: datetime = field(default_factory=datetime.utcnow)
    last_updated_at: datetime = field(default_factory=datetime.utcnow)


class ResourceTracker(ResourceTrackerProtocol):
    """Resource tracker - kernel cgroups equivalent.

    Thread-safe implementation for tracking resource usage.

    Usage:
        tracker = ResourceTracker(logger)

        # Allocate quota for a process
        tracker.allocate(pid, quota)

        # Record usage
        tracker.record_usage(pid, llm_calls=1, tokens_in=100)

        # Check if within quota
        exceeded_reason = tracker.check_quota(pid)
        if exceeded_reason:
            # Handle quota exceeded
            ...
    """

    def __init__(
        self,
        logger: LoggerProtocol,
        default_quota: Optional[ResourceQuota] = None,
    ) -> None:
        """Initialize resource tracker.

        Args:
            logger: Logger instance
            default_quota: Default quota for new allocations
        """
        self._logger = logger.bind(component="resource_tracker")
        self._default_quota = default_quota or ResourceQuota()

        # Per-process resource tracking
        self._resources: Dict[str, _ProcessResources] = {}

        # System-wide counters
        self._system_llm_calls: int = 0
        self._system_tool_calls: int = 0
        self._system_tokens_in: int = 0
        self._system_tokens_out: int = 0
        self._total_processes: int = 0
        self._active_processes: int = 0

        # Lock for thread safety
        self._lock = threading.RLock()

    def allocate(
        self,
        pid: str,
        quota: ResourceQuota,
    ) -> bool:
        """Allocate resources to a process."""
        with self._lock:
            if pid in self._resources:
                self._logger.warning(
                    "duplicate_allocation",
                    pid=pid,
                )
                return False

            self._resources[pid] = _ProcessResources(
                pid=pid,
                quota=quota,
            )

            self._total_processes += 1
            self._active_processes += 1

            self._logger.debug(
                "resources_allocated",
                pid=pid,
                max_llm_calls=quota.max_llm_calls,
                max_tool_calls=quota.max_tool_calls,
                timeout_seconds=quota.timeout_seconds,
            )

            return True

    def release(self, pid: str) -> bool:
        """Release resources from a process."""
        with self._lock:
            if pid not in self._resources:
                return False

            del self._resources[pid]
            self._active_processes -= 1

            self._logger.debug("resources_released", pid=pid)

            return True

    def record_usage(
        self,
        pid: str,
        llm_calls: int = 0,
        tool_calls: int = 0,
        agent_hops: int = 0,
        tokens_in: int = 0,
        tokens_out: int = 0,
    ) -> ResourceUsage:
        """Record resource usage."""
        with self._lock:
            pr = self._resources.get(pid)
            if not pr:
                # Create with default quota if not exists
                pr = _ProcessResources(
                    pid=pid,
                    quota=self._default_quota,
                )
                self._resources[pid] = pr
                self._total_processes += 1
                self._active_processes += 1

            # Update process usage
            pr.usage.llm_calls += llm_calls
            pr.usage.tool_calls += tool_calls
            pr.usage.agent_hops += agent_hops
            pr.usage.tokens_in += tokens_in
            pr.usage.tokens_out += tokens_out
            pr.last_updated_at = utc_now()

            # Update elapsed time
            elapsed = (pr.last_updated_at - pr.allocated_at).total_seconds()
            pr.usage.elapsed_seconds = elapsed

            # Update system counters
            self._system_llm_calls += llm_calls
            self._system_tool_calls += tool_calls
            self._system_tokens_in += tokens_in
            self._system_tokens_out += tokens_out

            # Log if approaching limits
            quota = pr.quota
            usage = pr.usage

            if usage.llm_calls >= quota.max_llm_calls * 0.8:
                self._logger.warning(
                    "approaching_llm_limit",
                    pid=pid,
                    usage=usage.llm_calls,
                    quota=quota.max_llm_calls,
                )

            if elapsed >= quota.soft_timeout_seconds:
                self._logger.warning(
                    "approaching_timeout",
                    pid=pid,
                    elapsed=elapsed,
                    soft_timeout=quota.soft_timeout_seconds,
                    hard_timeout=quota.timeout_seconds,
                )

            return pr.usage

    def check_quota(self, pid: str) -> Optional[str]:
        """Check if process is within quota.

        Returns None if within quota, or reason string if exceeded.
        """
        with self._lock:
            pr = self._resources.get(pid)
            if not pr:
                return None  # No tracking = no limits

            return pr.usage.exceeds_quota(pr.quota)

    def get_usage(self, pid: str) -> Optional[ResourceUsage]:
        """Get current usage for a process."""
        with self._lock:
            pr = self._resources.get(pid)
            if not pr:
                return None
            return pr.usage

    def get_quota(self, pid: str) -> Optional[ResourceQuota]:
        """Get quota for a process."""
        with self._lock:
            pr = self._resources.get(pid)
            if not pr:
                return None
            return pr.quota

    def get_system_usage(self) -> Dict[str, Any]:
        """Get system-wide resource usage."""
        with self._lock:
            return {
                "total_processes": self._total_processes,
                "active_processes": self._active_processes,
                "system_llm_calls": self._system_llm_calls,
                "system_tool_calls": self._system_tool_calls,
                "system_tokens_in": self._system_tokens_in,
                "system_tokens_out": self._system_tokens_out,
            }

    # =========================================================================
    # Additional utility methods
    # =========================================================================

    def update_elapsed_time(self, pid: str) -> Optional[float]:
        """Update and return elapsed time for a process."""
        with self._lock:
            pr = self._resources.get(pid)
            if not pr:
                return None

            pr.last_updated_at = utc_now()
            elapsed = (pr.last_updated_at - pr.allocated_at).total_seconds()
            pr.usage.elapsed_seconds = elapsed

            return elapsed

    def get_remaining_budget(self, pid: str) -> Optional[Dict[str, int]]:
        """Get remaining resource budget for a process."""
        with self._lock:
            pr = self._resources.get(pid)
            if not pr:
                return None

            quota = pr.quota
            usage = pr.usage

            return {
                "llm_calls": max(0, quota.max_llm_calls - usage.llm_calls),
                "tool_calls": max(0, quota.max_tool_calls - usage.tool_calls),
                "agent_hops": max(0, quota.max_agent_hops - usage.agent_hops),
                "iterations": max(0, quota.max_iterations - usage.iterations),
                "time_seconds": max(
                    0, quota.timeout_seconds - usage.elapsed_seconds
                ),
            }

    def adjust_quota(
        self,
        pid: str,
        **adjustments: int,
    ) -> bool:
        """Adjust quota for a process.

        Args:
            pid: Process ID
            **adjustments: Quota field adjustments (e.g., max_llm_calls=20)

        Returns:
            True if adjusted
        """
        with self._lock:
            pr = self._resources.get(pid)
            if not pr:
                return False

            for key, value in adjustments.items():
                if hasattr(pr.quota, key):
                    setattr(pr.quota, key, value)

            self._logger.info(
                "quota_adjusted",
                pid=pid,
                adjustments=adjustments,
            )

            return True

    def get_all_usage(self) -> Dict[str, ResourceUsage]:
        """Get usage for all tracked processes."""
        with self._lock:
            return {pid: pr.usage for pid, pr in self._resources.items()}

    def is_tracked(self, pid: str) -> bool:
        """Check if a process is being tracked."""
        with self._lock:
            return pid in self._resources
