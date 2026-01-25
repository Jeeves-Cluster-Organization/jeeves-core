"""Rate Limiter - Sliding window rate limiting.

Implements rate limiting using the sliding window algorithm for accurate
limiting without the boundary issues of fixed windows.

Features:
- Per-user rate limits
- Per-endpoint rate limits
- Configurable time windows (minute, hour, day)
- Burst allowance
- Thread-safe implementation

Usage:
    limiter = RateLimiter(logger)

    # Configure limits
    limiter.set_user_limits("user-123", RateLimitConfig(
        requests_per_minute=60,
        requests_per_hour=1000,
    ))

    # Check rate limit
    result = limiter.check_rate_limit("user-123", "/api/analyze")
    if result.exceeded:
        raise RateLimitExceeded(result.reason)

Constitutional Reference: Control Tower (kernel layer)
"""

import threading
import time
from collections import defaultdict
from dataclasses import dataclass, field
from datetime import datetime
from typing import Any, Dict, List, Optional, Tuple

from protocols import (
    LoggerProtocol,
    RateLimitConfig,
    RateLimitResult,
)


@dataclass
class SlidingWindow:
    """Sliding window counter for rate limiting.

    Uses sub-buckets for accurate sliding window calculation.
    Each bucket represents a fraction of the window period.
    """
    window_seconds: int
    bucket_count: int = 10
    buckets: Dict[int, int] = field(default_factory=dict)
    total_count: int = 0

    def record(self, timestamp: float) -> int:
        """Record a request and return the current count.

        Args:
            timestamp: Unix timestamp of the request

        Returns:
            Current count in the sliding window
        """
        bucket_size = self.window_seconds / self.bucket_count
        current_bucket = int(timestamp / bucket_size)

        # Clean up old buckets
        min_bucket = current_bucket - self.bucket_count
        old_buckets = [b for b in self.buckets if b < min_bucket]
        for b in old_buckets:
            self.total_count -= self.buckets[b]
            del self.buckets[b]

        # Record in current bucket
        if current_bucket not in self.buckets:
            self.buckets[current_bucket] = 0
        self.buckets[current_bucket] += 1
        self.total_count += 1

        return self.get_count(timestamp)

    def get_count(self, timestamp: float) -> int:
        """Get the current count in the sliding window.

        Args:
            timestamp: Current timestamp

        Returns:
            Count of requests in the window
        """
        bucket_size = self.window_seconds / self.bucket_count
        current_bucket = int(timestamp / bucket_size)
        min_bucket = current_bucket - self.bucket_count

        # Calculate weighted count for accurate sliding window
        count = 0
        for bucket, bucket_count in self.buckets.items():
            if bucket >= min_bucket:
                count += bucket_count

        return count

    def time_until_slot_available(self, timestamp: float, limit: int) -> float:
        """Calculate seconds until a slot becomes available.

        Args:
            timestamp: Current timestamp
            limit: Rate limit

        Returns:
            Seconds until request would be allowed (0 if allowed now)
        """
        if self.get_count(timestamp) < limit:
            return 0.0

        bucket_size = self.window_seconds / self.bucket_count

        # Find the oldest bucket that would need to expire
        current_bucket = int(timestamp / bucket_size)
        min_bucket = current_bucket - self.bucket_count

        sorted_buckets = sorted(
            [(b, c) for b, c in self.buckets.items() if b >= min_bucket],
            key=lambda x: x[0]
        )

        # Calculate when enough requests will expire
        excess = self.get_count(timestamp) - limit + 1
        expired = 0
        for bucket, count in sorted_buckets:
            expired += count
            if expired >= excess:
                # This bucket needs to fully expire
                bucket_end = (bucket + 1) * bucket_size
                return max(0, bucket_end - timestamp + self.window_seconds)

        return float(self.window_seconds)


class RateLimiter:
    """Rate limiter using sliding window algorithm.

    Thread-safe implementation supporting multiple windows
    (minute, hour, day) per user/endpoint combination.

    Integration with ResourceTracker:
    - RateLimiter defines limits and tracks usage
    - ResourceTracker calls check_rate_limit before processing
    - API middleware uses this for request-level limiting
    """

    def __init__(
        self,
        logger: Optional[LoggerProtocol] = None,
        default_config: Optional[RateLimitConfig] = None,
    ):
        """Initialize rate limiter.

        Args:
            logger: Optional logger
            default_config: Default rate limit config for users without custom limits
        """
        self._logger = logger
        self._default_config = default_config or RateLimitConfig()

        # Per-user configs
        self._user_configs: Dict[str, RateLimitConfig] = {}

        # Per-endpoint configs (overrides user config)
        self._endpoint_configs: Dict[str, RateLimitConfig] = {}

        # Sliding windows: (user_id, endpoint, window_type) -> SlidingWindow
        self._windows: Dict[Tuple[str, str, str], SlidingWindow] = defaultdict(
            lambda: SlidingWindow(window_seconds=60)
        )

        self._lock = threading.RLock()

    def set_default_config(self, config: RateLimitConfig) -> None:
        """Set the default rate limit config.

        Args:
            config: Default config for users without custom limits
        """
        with self._lock:
            self._default_config = config

    def set_user_limits(self, user_id: str, config: RateLimitConfig) -> None:
        """Set rate limits for a specific user.

        Args:
            user_id: User identifier
            config: Rate limit configuration
        """
        with self._lock:
            self._user_configs[user_id] = config

        if self._logger:
            self._logger.info(
                "rate_limit_configured",
                user_id=user_id,
                requests_per_minute=config.requests_per_minute,
                requests_per_hour=config.requests_per_hour,
            )

    def set_endpoint_limits(self, endpoint: str, config: RateLimitConfig) -> None:
        """Set rate limits for a specific endpoint.

        Endpoint limits override user limits for that endpoint.

        Args:
            endpoint: Endpoint path (e.g., "/api/analyze")
            config: Rate limit configuration
        """
        with self._lock:
            self._endpoint_configs[endpoint] = config

    def get_config(
        self,
        user_id: str,
        endpoint: Optional[str] = None,
    ) -> RateLimitConfig:
        """Get the effective rate limit config.

        Args:
            user_id: User identifier
            endpoint: Optional endpoint for endpoint-specific limits

        Returns:
            Effective rate limit configuration
        """
        with self._lock:
            # Endpoint config takes precedence
            if endpoint and endpoint in self._endpoint_configs:
                return self._endpoint_configs[endpoint]

            # Then user config
            if user_id in self._user_configs:
                return self._user_configs[user_id]

            # Fall back to default
            return self._default_config

    def check_rate_limit(
        self,
        user_id: str,
        endpoint: str = "default",
        record: bool = True,
    ) -> RateLimitResult:
        """Check if a request is within rate limits.

        Args:
            user_id: User identifier
            endpoint: Endpoint being accessed
            record: Whether to record this request (False for dry-run check)

        Returns:
            RateLimitResult indicating if request is allowed
        """
        now = time.time()
        config = self.get_config(user_id, endpoint)

        with self._lock:
            # Check each window type
            checks = [
                ("minute", 60, config.requests_per_minute),
                ("hour", 3600, config.requests_per_hour),
                ("day", 86400, config.requests_per_day),
            ]

            for window_type, window_seconds, limit in checks:
                if limit <= 0:
                    continue  # No limit for this window

                window_key = (user_id, endpoint, window_type)

                # Get or create window
                if window_key not in self._windows:
                    self._windows[window_key] = SlidingWindow(
                        window_seconds=window_seconds
                    )

                window = self._windows[window_key]
                current = window.get_count(now)

                if current >= limit:
                    retry_after = window.time_until_slot_available(now, limit)

                    if self._logger:
                        self._logger.warning(
                            "rate_limit_exceeded",
                            user_id=user_id,
                            endpoint=endpoint,
                            limit_type=window_type,
                            current=current,
                            limit=limit,
                            retry_after=retry_after,
                        )

                    return RateLimitResult.exceeded_limit(
                        limit_type=window_type,
                        current=current,
                        limit=limit,
                        retry_after=retry_after,
                    )

            # All checks passed, record the request
            if record:
                for window_type, window_seconds, limit in checks:
                    if limit <= 0:
                        continue

                    window_key = (user_id, endpoint, window_type)
                    if window_key not in self._windows:
                        self._windows[window_key] = SlidingWindow(
                            window_seconds=window_seconds
                        )
                    self._windows[window_key].record(now)

            # Calculate remaining for minute window (most relevant for clients)
            minute_key = (user_id, endpoint, "minute")
            if minute_key in self._windows:
                remaining = config.requests_per_minute - self._windows[minute_key].get_count(now)
            else:
                remaining = config.requests_per_minute

            return RateLimitResult.ok(remaining=max(0, remaining))

    def get_usage(
        self,
        user_id: str,
        endpoint: str = "default",
    ) -> Dict[str, Dict[str, Any]]:
        """Get current rate limit usage for a user/endpoint.

        Args:
            user_id: User identifier
            endpoint: Endpoint

        Returns:
            Dict with usage stats per window type
        """
        now = time.time()
        config = self.get_config(user_id, endpoint)
        usage = {}

        with self._lock:
            windows = [
                ("minute", 60, config.requests_per_minute),
                ("hour", 3600, config.requests_per_hour),
                ("day", 86400, config.requests_per_day),
            ]

            for window_type, window_seconds, limit in windows:
                window_key = (user_id, endpoint, window_type)
                current = 0

                if window_key in self._windows:
                    current = self._windows[window_key].get_count(now)

                usage[window_type] = {
                    "current": current,
                    "limit": limit,
                    "remaining": max(0, limit - current),
                    "reset_in_seconds": window_seconds,  # Approximate
                }

        return usage

    def reset_user(self, user_id: str) -> int:
        """Reset all rate limit windows for a user.

        Args:
            user_id: User identifier

        Returns:
            Number of windows cleared
        """
        with self._lock:
            keys_to_remove = [
                k for k in self._windows if k[0] == user_id
            ]
            for key in keys_to_remove:
                del self._windows[key]

            if self._logger:
                self._logger.info(
                    "rate_limit_reset",
                    user_id=user_id,
                    windows_cleared=len(keys_to_remove),
                )

            return len(keys_to_remove)

    def cleanup_expired(self) -> int:
        """Clean up expired window data.

        Should be called periodically to prevent memory growth.

        Returns:
            Number of entries cleaned up
        """
        now = time.time()
        cleaned = 0

        with self._lock:
            keys_to_check = list(self._windows.keys())

            for key in keys_to_check:
                window = self._windows[key]
                # Clean up windows with no recent activity
                if window.get_count(now) == 0 and not window.buckets:
                    del self._windows[key]
                    cleaned += 1

        if self._logger and cleaned > 0:
            self._logger.debug(
                "rate_limit_cleanup",
                entries_cleaned=cleaned,
            )

        return cleaned
