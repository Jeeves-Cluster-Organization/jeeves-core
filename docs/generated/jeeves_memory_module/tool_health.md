# Tool Health (L7)

**Layer:** L7 - Meta (System Introspection)  
**Scope:** Tool metrics and health monitoring  
**Location:** `jeeves_memory_module/repositories/tool_metrics_repository.py`, `jeeves_memory_module/services/tool_health_service.py`

---

## Overview

The Tool Health layer provides performance monitoring and governance for tools in the Jeeves system. It tracks execution metrics, detects performance degradation, and provides circuit breaker recommendations.

### Key Features

- Execution metrics recording
- Health status determination
- Error pattern analysis
- Circuit breaker support
- Dashboard-friendly summaries
- Performance degradation detection

---

## Architecture

```
┌─────────────────────────┐     ┌──────────────────────────┐
│   ToolHealthService     │────▶│  ToolMetricsRepository   │
│  (Health Assessment)    │     │  (Metrics Storage)       │
└─────────────────────────┘     └──────────────────────────┘
          │                              │
          │ assess                       │ record
          ▼                              ▼
┌─────────────────────────┐     ┌──────────────────────────┐
│   ToolHealthReport      │     │     tool_metrics         │
│   SystemHealthReport    │     │       (Table)            │
└─────────────────────────┘     └──────────────────────────┘
```

---

## ToolMetric

Represents a single tool execution metric event.

### Fields

| Field | Type | Description |
|-------|------|-------------|
| `metric_id` | `str` | Unique metric identifier (UUID) |
| `tool_name` | `str` | Name of the executed tool |
| `user_id` | `str` | User who triggered execution |
| `session_id` | `Optional[str]` | Session context |
| `request_id` | `Optional[str]` | Request context |
| `status` | `str` | Execution status ('success', 'error', 'timeout') |
| `execution_time_ms` | `int` | Execution time in milliseconds |
| `error_type` | `Optional[str]` | Type of error if failed |
| `error_message` | `Optional[str]` | Error message if failed |
| `parameters_hash` | `Optional[str]` | Hash for duplicate detection |
| `input_size` | `int` | Size of input in bytes/chars |
| `output_size` | `int` | Size of output in bytes/chars |
| `metadata` | `Dict[str, Any]` | Additional context |
| `recorded_at` | `datetime` | When the metric was recorded |

### Methods

```python
def to_dict(self) -> Dict[str, Any]:
    """Convert to dictionary."""

@classmethod
def from_dict(cls, data: Dict[str, Any]) -> "ToolMetric":
    """Create from dictionary."""
```

---

## ToolMetricsRepository

Repository for tool execution metrics.

### Constructor

```python
def __init__(
    self,
    db: DatabaseClientProtocol,
    logger: Optional[LoggerProtocol] = None
)
```

### Methods

#### record

```python
async def record(self, metric: ToolMetric) -> ToolMetric:
    """Record a tool metric."""
```

#### get_tool_stats

```python
async def get_tool_stats(
    self,
    tool_name: str,
    since: Optional[datetime] = None,
    until: Optional[datetime] = None
) -> Dict[str, Any]:
    """
    Get aggregated statistics for a tool.
    
    Returns:
        {
            "tool_name": "code_search",
            "total_calls": 100,
            "success_count": 95,
            "error_count": 3,
            "timeout_count": 2,
            "success_rate": 0.95,
            "avg_time_ms": 150.5,
            "min_time_ms": 50,
            "max_time_ms": 500,
            "total_input_bytes": 10000,
            "total_output_bytes": 50000,
            "period": {"since": "...", "until": "..."}
        }
    """
```

#### get_recent_errors

```python
async def get_recent_errors(
    self,
    tool_name: Optional[str] = None,
    limit: int = 10
) -> List[ToolMetric]:
    """Get recent error metrics."""
```

#### get_all_tool_names

```python
async def get_all_tool_names(self) -> List[str]:
    """Get list of all tools with metrics."""
```

#### get_slow_executions

```python
async def get_slow_executions(
    self,
    threshold_ms: int = 5000,
    limit: int = 20
) -> List[ToolMetric]:
    """Get tool executions slower than threshold."""
```

#### get_recent_executions

```python
async def get_recent_executions(
    self,
    tool_name: str,
    limit: int = 100
) -> List[ToolMetric]:
    """Get recent executions for a specific tool."""
```

#### cleanup_old_metrics

```python
async def cleanup_old_metrics(
    self,
    older_than_days: int = 30
) -> int:
    """Clean up old metrics for storage management."""
```

---

## ToolHealthReport

Health report for a single tool.

### Fields

| Field | Type | Description |
|-------|------|-------------|
| `tool_name` | `str` | Tool name |
| `status` | `HealthStatus` | Health status (HEALTHY, DEGRADED, UNHEALTHY, UNKNOWN) |
| `success_rate` | `float` | Success rate (0.0-1.0) |
| `avg_latency_ms` | `float` | Average latency |
| `total_calls` | `int` | Total call count |
| `recent_errors` | `int` | Recent error count |
| `issues` | `List[str]` | Identified issues |
| `recommendations` | `List[str]` | Recommended actions |
| `checked_at` | `datetime` | When checked |

### Methods

```python
def to_dict(self) -> Dict[str, Any]:
    """Convert to dictionary."""
```

---

## SystemHealthReport

Overall system health report.

### Fields

| Field | Type | Description |
|-------|------|-------------|
| `status` | `HealthStatus` | Overall system status |
| `tool_reports` | `List[ToolHealthReport]` | Per-tool reports |
| `summary` | `Dict[str, int]` | Count by status (healthy, degraded, unhealthy, unknown) |
| `checked_at` | `datetime` | When checked |

---

## ToolHealthService

Service for monitoring tool health.

### Health Thresholds

```python
SUCCESS_RATE_HEALTHY = 0.95    # >= 95% success = healthy
SUCCESS_RATE_DEGRADED = 0.80   # >= 80% success = degraded
# Below 80% = unhealthy

LATENCY_HEALTHY_MS = 2000      # <= 2s avg = healthy
LATENCY_DEGRADED_MS = 5000     # <= 5s avg = degraded
# Above 5s = unhealthy

MIN_CALLS_FOR_ASSESSMENT = 5   # Minimum calls for valid assessment
```

### Constructor

```python
def __init__(
    self,
    db: DatabaseClientProtocol,
    repository: Optional[ToolMetricsRepository] = None,
    registered_tool_names: Optional[List[str]] = None,
    logger: Optional[LoggerProtocol] = None
)
```

### Methods

#### ensure_initialized

```python
async def ensure_initialized(self) -> None:
    """Ensure the repository table exists."""
```

#### record_execution

```python
async def record_execution(
    self,
    tool_name: str,
    user_id: str,
    status: str = "success",
    execution_time_ms: int = 0,
    session_id: Optional[str] = None,
    request_id: Optional[str] = None,
    error_type: Optional[str] = None,
    error_message: Optional[str] = None,
    metadata: Optional[Dict[str, Any]] = None
) -> ToolMetric:
    """Record a tool execution metric."""
```

#### check_tool_health

```python
async def check_tool_health(
    self,
    tool_name: str,
    period_hours: int = 1
) -> ToolHealthReport:
    """
    Check health of a specific tool.
    
    Returns:
        ToolHealthReport with status and recommendations
    """
```

#### check_all_tools_health

```python
async def check_all_tools_health(
    self,
    period_hours: int = 1
) -> SystemHealthReport:
    """Check health of all tools."""
```

#### should_circuit_break

```python
async def should_circuit_break(
    self,
    tool_name: str,
    error_threshold: int = 5,
    window_minutes: int = 5
) -> bool:
    """
    Check if circuit breaker should be triggered for a tool.
    
    Args:
        tool_name: Tool to check
        error_threshold: Number of errors to trigger
        window_minutes: Time window for error count
        
    Returns:
        True if circuit breaker should be triggered
    """
```

#### get_error_patterns

```python
async def get_error_patterns(
    self,
    tool_name: str,
    limit: int = 20
) -> Dict[str, Any]:
    """
    Get error patterns for a tool.
    
    Returns:
        {
            "tool_name": "...",
            "total_errors": 10,
            "patterns": [
                {"error_type": "timeout", "count": 5},
                {"error_type": "validation", "count": 3},
                ...
            ],
            "recent_messages": [...]
        }
    """
```

#### get_dashboard_summary

```python
async def get_dashboard_summary(self) -> Dict[str, Any]:
    """
    Get a dashboard-friendly summary of tool health.
    
    Returns:
        {
            "overall_status": "degraded",
            "summary": {"healthy": 10, "degraded": 2, "unhealthy": 1, "unknown": 0},
            "tools_needing_attention": [...],
            "slow_executions": [...],
            "checked_at": "..."
        }
    """
```

#### get_health_summary

```python
async def get_health_summary(self) -> Dict[str, Any]:
    """Get health summary formatted for gRPC service."""
```

#### get_tool_health

```python
async def get_tool_health(self, tool_name: str) -> Optional[Dict[str, Any]]:
    """
    Get health info for a specific tool, formatted for gRPC.
    
    Returns:
        {
            "status": "healthy",
            "total_calls": 100,
            "successful_calls": 95,
            "failed_calls": 5,
            "success_rate": 0.95,
            "avg_latency_ms": 150.5,
            "last_success": datetime,
            "last_failure": datetime,
            "last_error": "..."
        }
    """
```

#### set_registered_tools

```python
def set_registered_tools(self, tool_names: List[str]) -> None:
    """
    Set the list of registered tool names.
    
    Called after tool registry is populated to ensure health
    dashboard shows all available tools, not just executed ones.
    """
```

---

## HealthStatus Enum

From `jeeves_protocols`:

```python
class HealthStatus(Enum):
    HEALTHY = "healthy"
    DEGRADED = "degraded"
    UNHEALTHY = "unhealthy"
    UNKNOWN = "unknown"
```

---

## Usage Examples

### Recording Metrics

```python
from jeeves_memory_module.services.tool_health_service import ToolHealthService

health_service = ToolHealthService(db)

# Record successful execution
await health_service.record_execution(
    tool_name="code_search",
    user_id="user-123",
    status="success",
    execution_time_ms=150,
    session_id="sess-456"
)

# Record failed execution
await health_service.record_execution(
    tool_name="code_search",
    user_id="user-123",
    status="error",
    execution_time_ms=5000,
    error_type="timeout",
    error_message="Search timed out after 5000ms"
)
```

### Health Checking

```python
# Check single tool
report = await health_service.check_tool_health("code_search")
print(f"Status: {report.status.value}")
print(f"Success Rate: {report.success_rate:.1%}")
print(f"Issues: {report.issues}")
print(f"Recommendations: {report.recommendations}")

# Check all tools
system_report = await health_service.check_all_tools_health()
print(f"System Status: {system_report.status.value}")
print(f"Summary: {system_report.summary}")
```

### Circuit Breaker Integration

```python
# Before executing a tool
if await health_service.should_circuit_break("code_search"):
    # Skip tool execution or use fallback
    return fallback_response()

# Execute tool normally
result = await execute_tool("code_search", params)
```

### Dashboard Integration

```python
# Get dashboard data
dashboard = await health_service.get_dashboard_summary()

# Example response:
{
    "overall_status": "degraded",
    "summary": {"healthy": 10, "degraded": 2, "unhealthy": 0, "unknown": 1},
    "tools_needing_attention": [
        {"tool_name": "external_api", "status": "degraded", "issues": [...]}
    ],
    "slow_executions": [
        {"tool_name": "heavy_analysis", "execution_time_ms": 8000, ...}
    ],
    "checked_at": "2026-01-23T10:30:00Z"
}
```

---

## Database Schema

```sql
CREATE TABLE tool_metrics (
    metric_id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    tool_name TEXT NOT NULL,
    user_id TEXT NOT NULL,
    session_id UUID,
    request_id UUID,
    status TEXT NOT NULL DEFAULT 'success',
    execution_time_ms INTEGER DEFAULT 0,
    error_type TEXT,
    error_message TEXT,
    parameters_hash TEXT,
    input_size INTEGER DEFAULT 0,
    output_size INTEGER DEFAULT 0,
    metadata TEXT,
    recorded_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_tool_metrics_tool ON tool_metrics(tool_name);
CREATE INDEX idx_tool_metrics_status ON tool_metrics(status);
CREATE INDEX idx_tool_metrics_recorded ON tool_metrics(recorded_at);
CREATE INDEX idx_tool_metrics_tool_status ON tool_metrics(tool_name, status);
```

---

## Navigation

- [Back to README](./README.md)
- [Previous: Skills/Patterns (L6)](./skills.md)
- [Next: CommBus Messages](./messages.md)
