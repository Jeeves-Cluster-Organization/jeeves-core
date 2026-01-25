# Resources Module

**Module:** `control_tower.resources`  
**Main Classes:** `ResourceTracker`, `RateLimiter`

---

## Overview

The resources module implements resource management for the kernel:

- **ResourceTracker** - Quota allocation and usage tracking (cgroups equivalent)
- **RateLimiter** - Sliding window rate limiting for requests over time

---

## ResourceTracker

### Purpose

The `ResourceTracker` is the kernel's cgroups equivalent, managing resource quotas and usage:

- Allocate quotas per process
- Track usage (LLM calls, tool calls, tokens, time)
- Enforce limits and return violations
- Report system-wide metrics

### Constructor

```python
class ResourceTracker(ResourceTrackerProtocol):
    def __init__(
        self,
        logger: LoggerProtocol,
        default_quota: Optional[ResourceQuota] = None,
    ) -> None:
```

---

## Quota Management

### allocate

Allocate resources to a process.

```python
def allocate(
    self,
    pid: str,
    quota: ResourceQuota,
) -> bool:
```

**Returns:** `True` if allocated, `False` if already allocated.

---

### release

Release resources from a terminated process.

```python
def release(self, pid: str) -> bool:
```

---

### get_quota

Get quota for a process.

```python
def get_quota(self, pid: str) -> Optional[ResourceQuota]:
```

---

### adjust_quota

Adjust quota for a running process.

```python
def adjust_quota(
    self,
    pid: str,
    **adjustments: int,
) -> bool:
```

**Example:**

```python
tracker.adjust_quota(pid, max_llm_calls=20, timeout_seconds=600)
```

---

## Usage Tracking

### record_usage

Record resource usage for a process.

```python
def record_usage(
    self,
    pid: str,
    llm_calls: int = 0,
    tool_calls: int = 0,
    agent_hops: int = 0,
    tokens_in: int = 0,
    tokens_out: int = 0,
) -> ResourceUsage:
```

**Behavior:**
- Increments counters for the process
- Auto-creates tracking with default quota if not exists
- Updates elapsed time
- Logs warnings when approaching limits (80% threshold)

---

### get_usage

Get current usage for a process.

```python
def get_usage(self, pid: str) -> Optional[ResourceUsage]:
```

---

### get_all_usage

Get usage for all tracked processes.

```python
def get_all_usage(self) -> Dict[str, ResourceUsage]:
```

---

### update_elapsed_time

Update and return elapsed time for a process.

```python
def update_elapsed_time(self, pid: str) -> Optional[float]:
```

---

## Quota Enforcement

### check_quota

Check if process is within quota.

```python
def check_quota(self, pid: str) -> Optional[str]:
```

**Returns:** `None` if within quota, or reason string if exceeded:

| Reason | Description |
|--------|-------------|
| `"max_llm_calls_exceeded"` | LLM call limit hit |
| `"max_tool_calls_exceeded"` | Tool call limit hit |
| `"max_agent_hops_exceeded"` | Agent hop limit hit |
| `"max_iterations_exceeded"` | Iteration limit hit |
| `"timeout_exceeded"` | Time limit hit |

---

### get_remaining_budget

Get remaining resource budget for a process.

```python
def get_remaining_budget(self, pid: str) -> Optional[Dict[str, int]]:
```

**Returns:**

```python
{
    "llm_calls": 7,
    "tool_calls": 45,
    "agent_hops": 18,
    "iterations": 2,
    "time_seconds": 254,
}
```

---

## System Metrics

### get_system_usage

Get system-wide resource usage.

```python
def get_system_usage(self) -> Dict[str, Any]:
```

**Returns:**

```python
{
    "total_processes": 150,
    "active_processes": 20,
    "system_llm_calls": 1500,
    "system_tool_calls": 5000,
    "system_tokens_in": 500000,
    "system_tokens_out": 200000,
}
```

---

### is_tracked

Check if a process is being tracked.

```python
def is_tracked(self, pid: str) -> bool:
```

---

## ResourceQuota

```python
@dataclass
class ResourceQuota:
    # Token limits (like memory limits)
    max_input_tokens: int = 4096
    max_output_tokens: int = 2048
    max_context_tokens: int = 16384
    
    # Call limits (like CPU time)
    max_llm_calls: int = 10
    max_tool_calls: int = 50
    max_agent_hops: int = 21
    max_iterations: int = 3
    
    # Time limits
    timeout_seconds: int = 300
    soft_timeout_seconds: int = 240  # Warn before hard timeout
    
    # Rate limits (per-user)
    rate_limit: Optional[RateLimitConfig] = None
    
    def is_within_bounds(
        self,
        llm_calls: int,
        tool_calls: int,
        agent_hops: int,
        iterations: int,
    ) -> bool:
```

---

## ResourceUsage

```python
@dataclass
class ResourceUsage:
    llm_calls: int = 0
    tool_calls: int = 0
    agent_hops: int = 0
    iterations: int = 0
    tokens_in: int = 0
    tokens_out: int = 0
    elapsed_seconds: float = 0.0
    
    def exceeds_quota(self, quota: ResourceQuota) -> Optional[str]:
        """Check if usage exceeds quota. Returns reason or None."""
```

---

## RateLimiter

### Purpose

The `RateLimiter` uses a sliding window algorithm for accurate rate limiting without fixed window boundary issues.

### Features

- Per-user rate limits
- Per-endpoint rate limits
- Configurable time windows (minute, hour, day)
- Burst allowance
- Thread-safe implementation

### Constructor

```python
class RateLimiter:
    def __init__(
        self,
        logger: Optional[LoggerProtocol] = None,
        default_config: Optional[RateLimitConfig] = None,
    ):
```

---

## Rate Limit Configuration

### set_default_config

Set the default rate limit config.

```python
def set_default_config(self, config: RateLimitConfig) -> None:
```

---

### set_user_limits

Set rate limits for a specific user.

```python
def set_user_limits(self, user_id: str, config: RateLimitConfig) -> None:
```

**Example:**

```python
from protocols import RateLimitConfig

limiter.set_user_limits("user-123", RateLimitConfig(
    requests_per_minute=60,
    requests_per_hour=1000,
    requests_per_day=10000,
))
```

---

### set_endpoint_limits

Set rate limits for a specific endpoint (overrides user limits).

```python
def set_endpoint_limits(self, endpoint: str, config: RateLimitConfig) -> None:
```

---

### get_config

Get the effective rate limit config for a user/endpoint.

```python
def get_config(
    self,
    user_id: str,
    endpoint: Optional[str] = None,
) -> RateLimitConfig:
```

---

## Rate Limit Checking

### check_rate_limit

Check if a request is within rate limits.

```python
def check_rate_limit(
    self,
    user_id: str,
    endpoint: str = "default",
    record: bool = True,
) -> RateLimitResult:
```

**Parameters:**

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `user_id` | `str` | - | User identifier |
| `endpoint` | `str` | `"default"` | Endpoint being accessed |
| `record` | `bool` | `True` | Whether to record this request |

**Returns:** `RateLimitResult`

```python
# If allowed:
RateLimitResult(exceeded=False, remaining=55)

# If exceeded:
RateLimitResult(
    exceeded=True,
    limit_type="minute",
    current=60,
    limit=60,
    retry_after=5.2,
)
```

---

### get_usage

Get current rate limit usage for a user/endpoint.

```python
def get_usage(
    self,
    user_id: str,
    endpoint: str = "default",
) -> Dict[str, Dict[str, Any]]:
```

**Returns:**

```python
{
    "minute": {
        "current": 45,
        "limit": 60,
        "remaining": 15,
        "reset_in_seconds": 60,
    },
    "hour": {
        "current": 500,
        "limit": 1000,
        "remaining": 500,
        "reset_in_seconds": 3600,
    },
    "day": {
        "current": 5000,
        "limit": 10000,
        "remaining": 5000,
        "reset_in_seconds": 86400,
    },
}
```

---

## Rate Limit Administration

### reset_user

Reset all rate limit windows for a user.

```python
def reset_user(self, user_id: str) -> int:
```

**Returns:** Number of windows cleared.

---

### cleanup_expired

Clean up expired window data (call periodically).

```python
def cleanup_expired(self) -> int:
```

**Returns:** Number of entries cleaned up.

---

## SlidingWindow

Internal class for sliding window rate limiting.

```python
@dataclass
class SlidingWindow:
    window_seconds: int
    bucket_count: int = 10
    buckets: Dict[int, int] = field(default_factory=dict)
    total_count: int = 0
    
    def record(self, timestamp: float) -> int:
        """Record a request and return current count."""
    
    def get_count(self, timestamp: float) -> int:
        """Get current count in the sliding window."""
    
    def time_until_slot_available(self, timestamp: float, limit: int) -> float:
        """Calculate seconds until a slot becomes available."""
```

---

## Protocol

```python
@runtime_checkable
class ResourceTrackerProtocol(Protocol):
    def allocate(self, pid: str, quota: ResourceQuota) -> bool: ...
    def release(self, pid: str) -> bool: ...
    
    def record_usage(
        self,
        pid: str,
        llm_calls: int = 0,
        tool_calls: int = 0,
        agent_hops: int = 0,
        tokens_in: int = 0,
        tokens_out: int = 0,
    ) -> ResourceUsage: ...
    
    def check_quota(self, pid: str) -> Optional[str]: ...
    def get_usage(self, pid: str) -> Optional[ResourceUsage]: ...
    def get_quota(self, pid: str) -> Optional[ResourceQuota]: ...
    def get_system_usage(self) -> Dict[str, Any]: ...
```

---

## Thread Safety

Both `ResourceTracker` and `RateLimiter` are thread-safe:

- All state modifications protected by `threading.RLock`
- Safe for concurrent access from multiple coroutines
- Logging occurs inside the lock to ensure consistency

---

## Usage Example

```python
from control_tower.resources import ResourceTracker, RateLimiter
from control_tower.types import ResourceQuota

# Resource tracking
tracker = ResourceTracker(logger)

# Allocate quota for a process
tracker.allocate("env-123", ResourceQuota(max_llm_calls=10))

# Record usage
tracker.record_usage("env-123", llm_calls=1, tokens_in=500, tokens_out=200)

# Check quota
exceeded = tracker.check_quota("env-123")
if exceeded:
    print(f"Quota exceeded: {exceeded}")

# Get remaining budget
remaining = tracker.get_remaining_budget("env-123")
print(f"Remaining LLM calls: {remaining['llm_calls']}")

# Rate limiting
limiter = RateLimiter(logger)
limiter.set_user_limits("user-1", RateLimitConfig(requests_per_minute=60))

result = limiter.check_rate_limit("user-1", "/api/analyze")
if result.exceeded:
    raise HTTPException(
        status_code=429,
        detail=f"Rate limit exceeded. Retry after {result.retry_after}s",
    )
```

---

## Navigation

- [← Lifecycle](./lifecycle.md)
- [Back to README](./README.md)
- [Services →](./services.md)
