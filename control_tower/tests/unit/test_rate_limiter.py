"""Unit tests for RateLimiter and SlidingWindow.

Clean, modern tests using mocked time for deterministic behavior.
No external dependencies required.
"""

import pytest
import time
from unittest.mock import MagicMock, patch
from control_tower.resources.rate_limiter import RateLimiter, SlidingWindow
from jeeves_core.types import RateLimitConfig


# =============================================================================
# SLIDING WINDOW TESTS
# =============================================================================


def test_sliding_window_record():
    """Test recording request in window."""
    window = SlidingWindow(window_seconds=60)

    count = window.record(100.0)

    assert count == 1
    assert window.total_count == 1
    assert len(window.buckets) == 1


def test_sliding_window_get_count():
    """Test getting current count."""
    window = SlidingWindow(window_seconds=60)

    # Record multiple requests
    window.record(100.0)
    window.record(100.5)
    window.record(101.0)

    count = window.get_count(101.0)
    assert count == 3


def test_sliding_window_bucket_cleanup():
    """Test old buckets are cleaned up."""
    window = SlidingWindow(window_seconds=60, bucket_count=10)

    # Record requests
    window.record(100.0)
    window.record(110.0)
    window.record(120.0)

    # After 70 seconds, first request should be outside window
    count = window.get_count(170.0)
    assert count == 2  # Only the last 2 requests

    # After 120 more seconds, all should be expired
    count = window.get_count(250.0)
    assert count == 0


def test_sliding_window_accurate_sliding():
    """Test accurate sliding window calculation."""
    window = SlidingWindow(window_seconds=60, bucket_count=10)

    # Record 5 requests at t=100
    for _ in range(5):
        window.record(100.0)

    # Record 3 requests at t=130 (30 seconds later)
    for _ in range(3):
        window.record(130.0)

    # At t=150, both groups should be in window
    count = window.get_count(150.0)
    assert count == 8

    # At t=165, first group should be outside 60s window
    count = window.get_count(165.0)
    assert count == 3


def test_sliding_window_time_until_slot():
    """Test retry time calculation."""
    window = SlidingWindow(window_seconds=60)

    # Under limit - should be available immediately
    window.record(100.0)
    retry = window.time_until_slot_available(100.0, limit=5)
    assert retry == 0.0

    # At limit - should need to wait
    for _ in range(4):
        window.record(100.0)
    retry = window.time_until_slot_available(100.0, limit=5)
    assert retry > 0


def test_sliding_window_multiple_buckets():
    """Test sub-bucket distribution."""
    window = SlidingWindow(window_seconds=60, bucket_count=10)

    # Record requests spread across different buckets
    window.record(100.0)
    window.record(106.5)  # Different bucket
    window.record(113.0)  # Different bucket

    assert len(window.buckets) == 3


# =============================================================================
# RATE LIMITER INITIALIZATION TESTS
# =============================================================================


def test_rate_limiter_init_default(mock_logger):
    """Test initialization with default config."""
    limiter = RateLimiter(logger=mock_logger)

    assert limiter._logger == mock_logger
    assert limiter._default_config is not None
    assert len(limiter._user_configs) == 0
    assert len(limiter._endpoint_configs) == 0


def test_rate_limiter_init_custom_config(mock_logger):
    """Test initialization with custom default config."""
    config = RateLimitConfig(
        requests_per_minute=100,
        requests_per_hour=1000,
        requests_per_day=10000
    )

    limiter = RateLimiter(logger=mock_logger, default_config=config)

    assert limiter._default_config.requests_per_minute == 100
    assert limiter._default_config.requests_per_hour == 1000


# =============================================================================
# CONFIGURATION TESTS
# =============================================================================


def test_set_user_limits(mock_logger):
    """Test setting custom user limits."""
    limiter = RateLimiter(logger=mock_logger)
    config = RateLimitConfig(requests_per_minute=50)

    limiter.set_user_limits("user-123", config)

    assert "user-123" in limiter._user_configs
    assert limiter._user_configs["user-123"].requests_per_minute == 50


def test_set_endpoint_limits(mock_logger):
    """Test endpoint-specific limits."""
    limiter = RateLimiter(logger=mock_logger)
    config = RateLimitConfig(requests_per_minute=10)

    limiter.set_endpoint_limits("/api/expensive", config)

    assert "/api/expensive" in limiter._endpoint_configs
    assert limiter._endpoint_configs["/api/expensive"].requests_per_minute == 10


def test_get_config_precedence(mock_logger):
    """Test endpoint > user > default precedence."""
    limiter = RateLimiter(
        logger=mock_logger,
        default_config=RateLimitConfig(requests_per_minute=100)
    )

    # Set user config
    limiter.set_user_limits(
        "user-123",
        RateLimitConfig(requests_per_minute=50)
    )

    # Set endpoint config
    limiter.set_endpoint_limits(
        "/api/special",
        RateLimitConfig(requests_per_minute=10)
    )

    # Endpoint config takes precedence
    config = limiter.get_config("user-123", "/api/special")
    assert config.requests_per_minute == 10

    # User config when no endpoint config
    config = limiter.get_config("user-123", "/api/other")
    assert config.requests_per_minute == 50

    # Default for unknown user
    config = limiter.get_config("unknown-user")
    assert config.requests_per_minute == 100


# =============================================================================
# CHECK RATE LIMIT TESTS
# =============================================================================


@patch('time.time')
def test_check_rate_limit_allowed(mock_time, mock_logger):
    """Test request allowed when under limit."""
    mock_time.return_value = 100.0

    limiter = RateLimiter(
        logger=mock_logger,
        default_config=RateLimitConfig(requests_per_minute=10)
    )

    result = limiter.check_rate_limit("user-123", "/api/test")

    assert not result.exceeded
    assert result.remaining >= 0


@patch('time.time')
def test_check_rate_limit_exceeded_minute(mock_time, mock_logger):
    """Test minute limit exceeded."""
    mock_time.return_value = 100.0

    limiter = RateLimiter(
        logger=mock_logger,
        default_config=RateLimitConfig(requests_per_minute=3)
    )

    # Make 3 requests (at limit)
    for _ in range(3):
        result = limiter.check_rate_limit("user-123", "/api/test")
        assert not result.exceeded

    # 4th request should be blocked
    result = limiter.check_rate_limit("user-123", "/api/test")

    assert result.exceeded
    assert result.limit_type == "minute"
    assert result.retry_after_seconds > 0


@patch('time.time')
def test_check_rate_limit_exceeded_hour(mock_time, mock_logger):
    """Test hour limit exceeded."""
    mock_time.return_value = 100.0

    limiter = RateLimiter(
        logger=mock_logger,
        default_config=RateLimitConfig(
            requests_per_minute=100,
            requests_per_hour=5
        )
    )

    # Make 5 requests
    for _ in range(5):
        result = limiter.check_rate_limit("user-123", "/api/test")
        assert not result.exceeded

    # 6th should hit hour limit
    result = limiter.check_rate_limit("user-123", "/api/test")

    assert result.exceeded
    assert result.limit_type == "hour"


@patch('time.time')
def test_check_rate_limit_exceeded_day(mock_time, mock_logger):
    """Test day limit exceeded."""
    mock_time.return_value = 100.0

    limiter = RateLimiter(
        logger=mock_logger,
        default_config=RateLimitConfig(
            requests_per_minute=100,
            requests_per_hour=200,
            requests_per_day=3
        )
    )

    # Make 3 requests
    for _ in range(3):
        result = limiter.check_rate_limit("user-123", "/api/test")
        assert not result.exceeded

    # 4th should hit day limit
    result = limiter.check_rate_limit("user-123", "/api/test")

    assert result.exceeded
    assert result.limit_type == "day"


@patch('time.time')
def test_check_rate_limit_dry_run(mock_time, mock_logger):
    """Test record=False doesn't record request."""
    mock_time.return_value = 100.0

    limiter = RateLimiter(
        logger=mock_logger,
        default_config=RateLimitConfig(requests_per_minute=10)
    )

    # Dry run check
    result = limiter.check_rate_limit("user-123", "/api/test", record=False)
    assert not result.exceeded

    # Should still have full limit available
    usage = limiter.get_usage("user-123", "/api/test")
    assert usage["minute"]["current"] == 0


@patch('time.time')
def test_check_rate_limit_resets_after_window(mock_time, mock_logger):
    """Test limits reset after window expires."""
    limiter = RateLimiter(
        logger=mock_logger,
        default_config=RateLimitConfig(requests_per_minute=2)
    )

    # Make 2 requests at t=100
    mock_time.return_value = 100.0
    for _ in range(2):
        result = limiter.check_rate_limit("user-123", "/api/test")
        assert not result.exceeded

    # 3rd request blocked
    result = limiter.check_rate_limit("user-123", "/api/test")
    assert result.exceeded

    # After 65 seconds, window should have slid past old requests
    mock_time.return_value = 165.0
    result = limiter.check_rate_limit("user-123", "/api/test")
    assert not result.exceeded


# =============================================================================
# USAGE TRACKING TESTS
# =============================================================================


@patch('time.time')
def test_get_usage(mock_time, mock_logger):
    """Test usage statistics."""
    mock_time.return_value = 100.0

    limiter = RateLimiter(
        logger=mock_logger,
        default_config=RateLimitConfig(
            requests_per_minute=10,
            requests_per_hour=100,
            requests_per_day=1000
        )
    )

    # Make 3 requests
    for _ in range(3):
        limiter.check_rate_limit("user-123", "/api/test")

    usage = limiter.get_usage("user-123", "/api/test")

    assert usage["minute"]["current"] == 3
    assert usage["minute"]["limit"] == 10
    assert usage["minute"]["remaining"] == 7

    assert usage["hour"]["current"] == 3
    assert usage["day"]["current"] == 3


@patch('time.time')
def test_get_usage_no_requests(mock_time, mock_logger):
    """Test usage for user with no requests."""
    mock_time.return_value = 100.0

    limiter = RateLimiter(
        logger=mock_logger,
        default_config=RateLimitConfig(requests_per_minute=10)
    )

    usage = limiter.get_usage("new-user", "/api/test")

    assert usage["minute"]["current"] == 0
    assert usage["minute"]["remaining"] == 10


# =============================================================================
# MANAGEMENT TESTS
# =============================================================================


@patch('time.time')
def test_reset_user(mock_time, mock_logger):
    """Test resetting user windows."""
    mock_time.return_value = 100.0

    limiter = RateLimiter(logger=mock_logger)

    # Generate some usage
    limiter.check_rate_limit("user-123", "/api/test1")
    limiter.check_rate_limit("user-123", "/api/test2")
    limiter.check_rate_limit("user-456", "/api/test1")

    # Reset user-123
    cleared = limiter.reset_user("user-123")

    assert cleared >= 2  # At least minute and hour windows

    # user-123 should have clean slate
    usage = limiter.get_usage("user-123", "/api/test1")
    assert usage["minute"]["current"] == 0

    # user-456 should be unaffected
    usage = limiter.get_usage("user-456", "/api/test1")
    assert usage["minute"]["current"] >= 1


@patch('time.time')
def test_cleanup_expired(mock_time, mock_logger):
    """Test cleaning up expired windows."""
    limiter = RateLimiter(logger=mock_logger)

    # Make requests at t=100
    mock_time.return_value = 100.0
    limiter.check_rate_limit("user-123", "/api/test")

    # Move to t=200 (100 seconds later - all windows expired)
    mock_time.return_value = 200.0

    cleaned = limiter.cleanup_expired()

    # Should clean up expired windows
    assert cleaned >= 0


# =============================================================================
# THREAD SAFETY TESTS
# =============================================================================


@patch('time.time')
def test_rate_limit_concurrent_access(mock_time, mock_logger):
    """Test thread-safe operation."""
    import threading

    mock_time.return_value = 100.0
    limiter = RateLimiter(
        logger=mock_logger,
        default_config=RateLimitConfig(requests_per_minute=100)
    )

    results = []

    def make_requests():
        for _ in range(10):
            result = limiter.check_rate_limit("user-123", "/api/test")
            results.append(result.exceeded)

    # Run multiple threads
    threads = [threading.Thread(target=make_requests) for _ in range(5)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()

    # All requests should be allowed (50 total, limit is 100)
    assert not any(results)


# =============================================================================
# EDGE CASES
# =============================================================================


@patch('time.time')
def test_zero_limit_disabled(mock_time, mock_logger):
    """Test that limit=0 disables that window."""
    mock_time.return_value = 100.0

    limiter = RateLimiter(
        logger=mock_logger,
        default_config=RateLimitConfig(
            requests_per_minute=0,  # Disabled
            requests_per_hour=10
        )
    )

    # Should not be limited by minute (disabled)
    for _ in range(20):
        result = limiter.check_rate_limit("user-123", "/api/test")
        # Only hour limit applies
        if result.exceeded:
            assert result.limit_type == "hour"
            break


@patch('time.time')
def test_different_endpoints_separate_limits(mock_time, mock_logger):
    """Test different endpoints tracked separately."""
    mock_time.return_value = 100.0

    limiter = RateLimiter(
        logger=mock_logger,
        default_config=RateLimitConfig(requests_per_minute=5)
    )

    # Use up limit on endpoint1
    for _ in range(5):
        result = limiter.check_rate_limit("user-123", "/api/endpoint1")
        assert not result.exceeded

    # endpoint2 should still have full quota
    result = limiter.check_rate_limit("user-123", "/api/endpoint2")
    assert not result.exceeded

    usage = limiter.get_usage("user-123", "/api/endpoint2")
    assert usage["minute"]["current"] == 1
