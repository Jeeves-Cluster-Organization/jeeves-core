# jeeves_protocols.interrupts

**Layer**: L0 (Foundation)  
**Purpose**: Interrupt types and protocols for flow control

## Overview

This module defines the contract for interrupt handling across layers. Interrupts allow the pipeline to pause execution and wait for external input (user clarification, confirmation, etc.).

---

## Enums

### InterruptKind

Type of flow interrupt.

```python
class InterruptKind(str, Enum):
    CLARIFICATION = "clarification"       # Need user clarification
    CONFIRMATION = "confirmation"         # Need user confirmation
    AGENT_REVIEW = "agent_review"         # Need agent/human review
    CHECKPOINT = "checkpoint"             # Checkpoint for resume
    RESOURCE_EXHAUSTED = "resource_exhausted"  # Hit resource limits
    TIMEOUT = "timeout"                   # Operation timed out
    SYSTEM_ERROR = "system_error"         # System error occurred
```

---

### InterruptStatus

Status of an interrupt.

```python
class InterruptStatus(str, Enum):
    PENDING = "pending"       # Waiting for response
    RESOLVED = "resolved"     # Response received
    EXPIRED = "expired"       # Timed out
    CANCELLED = "cancelled"   # Cancelled by user/system
```

---

## Dataclasses

### InterruptResponse

Response to an interrupt.

```python
@dataclass
class InterruptResponse:
    text: Optional[str] = None           # For clarification responses
    approved: Optional[bool] = None      # For confirmation responses (True/False)
    decision: Optional[str] = None       # For critic decisions (approve/reject/modify)
    data: Optional[Dict[str, Any]] = None  # Extensible payload
```

**Methods**:

| Method | Signature | Description |
|--------|-----------|-------------|
| `to_dict` | `() -> Dict[str, Any]` | Convert to dictionary for storage |
| `from_dict` | `(data: Dict) -> InterruptResponse` | Create from dictionary (class method) |

---

### FlowInterrupt

Unified flow interrupt.

```python
@dataclass
class FlowInterrupt:
    # ─── Identification ───
    id: str                              # Unique interrupt identifier
    kind: InterruptKind                  # Type of interrupt
    request_id: str                      # Associated request
    user_id: str                         # User who triggered
    session_id: str                      # Session context
    envelope_id: Optional[str] = None    # Optional envelope reference
    
    # ─── Content ───
    question: Optional[str] = None       # Clarification question
    message: Optional[str] = None        # Confirmation message
    data: Dict[str, Any] = field(default_factory=dict)  # Extensible payload
    
    # ─── Response ───
    response: Optional[InterruptResponse] = None
    status: InterruptStatus = InterruptStatus.PENDING
    
    # ─── Timing ───
    created_at: datetime = field(default_factory=lambda: datetime.now(timezone.utc))
    expires_at: Optional[datetime] = None
    resolved_at: Optional[datetime] = None
    
    # ─── Tracing ───
    trace_id: Optional[str] = None       # OpenTelemetry trace ID
    span_id: Optional[str] = None        # OpenTelemetry span ID
```

**Methods**:

| Method | Signature | Description |
|--------|-----------|-------------|
| `to_dict` | `() -> Dict[str, Any]` | Convert to dictionary for API responses |
| `to_db_row` | `() -> Dict[str, Any]` | Convert to database row format |
| `from_db_row` | `(row: Dict) -> FlowInterrupt` | Create from database row (class method) |

**Example**:
```python
from jeeves_protocols import FlowInterrupt, InterruptKind

# Create clarification interrupt
interrupt = FlowInterrupt(
    id=str(uuid4()),
    kind=InterruptKind.CLARIFICATION,
    request_id="req_123",
    user_id="user_456",
    session_id="sess_789",
    question="Which authentication method should I analyze?",
    data={"options": ["OAuth2", "JWT", "Session-based"]},
)

# Create confirmation interrupt
interrupt = FlowInterrupt(
    id=str(uuid4()),
    kind=InterruptKind.CONFIRMATION,
    request_id="req_123",
    user_id="user_456",
    session_id="sess_789",
    message="This will delete 15 files. Continue?",
)
```

---

## InterruptServiceProtocol

Protocol for interrupt handling service.

```python
@runtime_checkable
class InterruptServiceProtocol(Protocol):
    async def create_interrupt(
        self,
        kind: InterruptKind,
        request_id: str,
        user_id: str,
        session_id: str,
        envelope_id: Optional[str] = None,
        question: Optional[str] = None,
        message: Optional[str] = None,
        data: Optional[Dict[str, Any]] = None,
        ttl_seconds: Optional[int] = None,
    ) -> FlowInterrupt:
        """Create a new interrupt."""

    async def get_interrupt(self, interrupt_id: str) -> Optional[FlowInterrupt]:
        """Get an interrupt by ID."""

    async def respond(
        self,
        interrupt_id: str,
        response: InterruptResponse,
    ) -> Optional[FlowInterrupt]:
        """Respond to an interrupt."""

    async def list_pending(
        self,
        user_id: Optional[str] = None,
        session_id: Optional[str] = None,
        request_id: Optional[str] = None,
    ) -> List[FlowInterrupt]:
        """List pending interrupts."""

    async def cancel(self, interrupt_id: str) -> bool:
        """Cancel an interrupt."""
```

---

## Rate Limiting Types

### RateLimitConfig

Configuration for rate limiting.

```python
@dataclass
class RateLimitConfig:
    requests_per_minute: int = 60
    requests_per_hour: int = 1000
    requests_per_day: int = 10000
    burst_size: int = 10  # Max requests in a burst
```

---

### RateLimitResult

Result of a rate limit check.

```python
@dataclass
class RateLimitResult:
    allowed: bool                        # Whether the request is allowed
    exceeded: bool = False               # Whether a limit was exceeded
    reason: Optional[str] = None         # Human-readable reason
    limit_type: Optional[str] = None     # Which limit was exceeded
    current_count: int = 0               # Current request count
    limit: int = 0                       # The limit that was checked
    retry_after_seconds: float = 0.0     # Seconds until allowed
    remaining: int = 0                   # Remaining requests
```

**Static Methods**:

| Method | Signature | Description |
|--------|-----------|-------------|
| `ok` | `(remaining: int = 0) -> RateLimitResult` | Create an 'allowed' result |
| `exceeded_limit` | `(limit_type, current, limit, retry_after) -> RateLimitResult` | Create an 'exceeded' result |

**Example**:
```python
from jeeves_protocols import RateLimitResult

# Request allowed
result = RateLimitResult.ok(remaining=55)

# Request denied
result = RateLimitResult.exceeded_limit(
    limit_type="minute",
    current=61,
    limit=60,
    retry_after=30.0,
)

if not result.allowed:
    raise HTTPException(429, result.reason)
```

---

## RateLimiterProtocol

Protocol for rate limiting service.

```python
@runtime_checkable
class RateLimiterProtocol(Protocol):
    def check_rate_limit(
        self,
        user_id: str,
        endpoint: str,
    ) -> RateLimitResult:
        """Check if a request is allowed under rate limits."""

    def set_user_limits(
        self,
        user_id: str,
        config: RateLimitConfig,
    ) -> None:
        """Set rate limits for a specific user."""

    def get_user_limits(self, user_id: str) -> Optional[RateLimitConfig]:
        """Get rate limits for a specific user."""
```

---

## Usage Examples

### Creating and Responding to Interrupts

```python
from jeeves_protocols import (
    FlowInterrupt,
    InterruptKind,
    InterruptResponse,
    InterruptServiceProtocol,
)

# Service implementation creates interrupt
interrupt = await service.create_interrupt(
    kind=InterruptKind.CLARIFICATION,
    request_id="req_123",
    user_id="user_456",
    session_id="sess_789",
    question="What file should I focus on?",
    data={"suggestions": ["main.py", "auth.py", "utils.py"]},
    ttl_seconds=300,
)

# Store interrupt ID in envelope
envelope.interrupt_pending = True
envelope.interrupt = interrupt

# Later, user responds
response = InterruptResponse(text="auth.py")
resolved = await service.respond(interrupt.id, response)

# Resume pipeline
if resolved.response.text:
    envelope.interrupt_pending = False
    envelope.interrupt = None
    # Continue processing with response
```

### Rate Limiting

```python
from jeeves_protocols import RateLimitConfig, RateLimiterProtocol

# Configure limits for a user
config = RateLimitConfig(
    requests_per_minute=30,
    requests_per_hour=500,
    requests_per_day=5000,
    burst_size=5,
)
limiter.set_user_limits("user_123", config)

# Check before processing
result = limiter.check_rate_limit("user_123", "/api/analyze")
if not result.allowed:
    raise HTTPException(
        status_code=429,
        detail=result.reason,
        headers={"Retry-After": str(int(result.retry_after_seconds))}
    )
```

---

## Navigation

- [Back to README](README.md)
- [Previous: Events](events.md)
- [Next: gRPC](grpc.md)
