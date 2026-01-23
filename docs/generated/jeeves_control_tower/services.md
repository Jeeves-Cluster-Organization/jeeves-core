# Services Module

**Module:** `jeeves_control_tower.services`  
**Main Class:** `InterruptService`

---

## Overview

The services module provides kernel-level services for the Control Tower:

- **InterruptService** - Unified handling of all interrupt types

---

## InterruptService

### Purpose

The `InterruptService` is the unified service for all interrupt types, replacing:

- ConfirmationManager
- ConfirmationCoordinator
- Clarification methods in SessionStateService
- CriticInterruptHandler

All interrupt types are handled through this single service with config-driven behavior.

### Constructor

```python
class InterruptService:
    def __init__(
        self,
        db: Optional[DatabaseProtocol] = None,
        logger: Optional[LoggerProtocol] = None,
        webhook_service: Optional[WebhookServiceProtocol] = None,
        otel_adapter: Optional[OTelAdapterProtocol] = None,
        configs: Optional[Dict[InterruptKind, InterruptConfig]] = None,
    ):
```

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `db` | `DatabaseProtocol` | Database for persistence |
| `logger` | `LoggerProtocol` | Logger for structured logging |
| `webhook_service` | `WebhookServiceProtocol` | Webhook service for event emission |
| `otel_adapter` | `OTelAdapterProtocol` | OpenTelemetry adapter for tracing |
| `configs` | `Dict[InterruptKind, InterruptConfig]` | Custom configs per interrupt kind |

---

## Interrupt Kinds

```python
class InterruptKind(str, Enum):
    CLARIFICATION = "clarification"
    CONFIRMATION = "confirmation"
    AGENT_REVIEW = "agent_review"
    CHECKPOINT = "checkpoint"
    RESOURCE_EXHAUSTED = "resource_exhausted"
    TIMEOUT = "timeout"
    SYSTEM_ERROR = "system_error"
```

---

## Default Configurations

```python
DEFAULT_INTERRUPT_CONFIGS = {
    InterruptKind.CLARIFICATION: InterruptConfig(
        default_ttl=timedelta(hours=24),
        webhook_event="interrupt.clarification_needed",
        allowed_responses=["text"],
    ),
    InterruptKind.CONFIRMATION: InterruptConfig(
        default_ttl=timedelta(hours=1),
        webhook_event="interrupt.confirmation_needed",
        allowed_responses=["approved"],
    ),
    InterruptKind.AGENT_REVIEW: InterruptConfig(
        default_ttl=timedelta(minutes=30),
        webhook_event="interrupt.agent_review",
        allowed_responses=["decision"],
    ),
    InterruptKind.CHECKPOINT: InterruptConfig(
        default_ttl=None,  # No expiry
        auto_expire=False,
        require_response=False,
        webhook_event="interrupt.checkpoint",
    ),
    InterruptKind.RESOURCE_EXHAUSTED: InterruptConfig(
        default_ttl=timedelta(minutes=5),
        webhook_event="interrupt.resource_exhausted",
        require_response=False,
    ),
    InterruptKind.TIMEOUT: InterruptConfig(
        default_ttl=timedelta(minutes=5),
        webhook_event="interrupt.timeout",
        require_response=False,
    ),
    InterruptKind.SYSTEM_ERROR: InterruptConfig(
        default_ttl=timedelta(hours=1),
        webhook_event="interrupt.system_error",
        require_response=False,
    ),
}
```

---

## InterruptConfig

```python
@dataclass
class InterruptConfig:
    default_ttl: Optional[timedelta] = None
    auto_expire: bool = True
    require_response: bool = True
    webhook_event: str = "interrupt.created"
    allowed_responses: List[str] = field(default_factory=list)
```

---

## Creating Interrupts

### create_interrupt

Create a new interrupt with any kind.

```python
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
    ttl: Optional[timedelta] = None,
) -> FlowInterrupt:
```

**Behavior:**
1. Get config for interrupt kind
2. Calculate expiration from TTL
3. Get trace context from OTel if available
4. Create FlowInterrupt
5. Persist to database or memory store
6. Log creation
7. Record OTel event
8. Emit webhook

---

### Convenience Methods

#### create_clarification

```python
async def create_clarification(
    self,
    request_id: str,
    user_id: str,
    session_id: str,
    question: str,
    envelope_id: Optional[str] = None,
    context: Optional[Dict[str, Any]] = None,
) -> FlowInterrupt:
```

**Example:**

```python
interrupt = await service.create_clarification(
    request_id="req-123",
    user_id="user-1",
    session_id="sess-1",
    question="Which authentication method do you want to analyze?",
    context={"options": ["OAuth", "JWT", "Session"]},
)
```

---

#### create_confirmation

```python
async def create_confirmation(
    self,
    request_id: str,
    user_id: str,
    session_id: str,
    message: str,
    envelope_id: Optional[str] = None,
    action_data: Optional[Dict[str, Any]] = None,
) -> FlowInterrupt:
```

**Example:**

```python
interrupt = await service.create_confirmation(
    request_id="req-123",
    user_id="user-1",
    session_id="sess-1",
    message="This will modify 15 files. Proceed?",
    action_data={"files_count": 15, "operation": "refactor"},
)
```

---

#### create_resource_exhausted

```python
async def create_resource_exhausted(
    self,
    request_id: str,
    user_id: str,
    session_id: str,
    resource_type: str,
    retry_after_seconds: float,
    envelope_id: Optional[str] = None,
) -> FlowInterrupt:
```

**Example:**

```python
interrupt = await service.create_resource_exhausted(
    request_id="req-123",
    user_id="user-1",
    session_id="sess-1",
    resource_type="max_llm_calls",
    retry_after_seconds=60.0,
)
```

---

## Querying Interrupts

### get_interrupt

Get an interrupt by ID.

```python
async def get_interrupt(self, interrupt_id: str) -> Optional[FlowInterrupt]:
```

---

### get_pending_for_request

Get pending interrupt for a request.

```python
async def get_pending_for_request(
    self, request_id: str
) -> Optional[FlowInterrupt]:
```

---

### get_pending_for_session

Get all pending interrupts for a session.

```python
async def get_pending_for_session(
    self, session_id: str, kinds: Optional[List[InterruptKind]] = None
) -> List[FlowInterrupt]:
```

---

## Responding to Interrupts

### respond

Respond to an interrupt and resolve it.

```python
async def respond(
    self,
    interrupt_id: str,
    response: InterruptResponse,
    user_id: Optional[str] = None,
) -> Optional[FlowInterrupt]:
```

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `interrupt_id` | `str` | Interrupt ID |
| `response` | `InterruptResponse` | Response data |
| `user_id` | `str` | Optional user ID for validation |

**Behavior:**
1. Get interrupt by ID
2. Validate it's pending
3. Validate user if provided
4. Update status to RESOLVED
5. Persist changes
6. Log resolution
7. Record OTel event
8. Emit webhook

**Example:**

```python
resolved = await service.respond(
    interrupt_id="int-456",
    response=InterruptResponse(text="Yes, proceed with OAuth"),
    user_id="user-1",
)
```

---

## Cancellation and Expiration

### cancel

Cancel an interrupt.

```python
async def cancel(
    self, interrupt_id: str, reason: Optional[str] = None
) -> Optional[FlowInterrupt]:
```

---

### expire_pending

Expire all pending interrupts that have passed their expiry time.

```python
async def expire_pending(self) -> int:
```

**Returns:** Number of interrupts expired.

Should be called periodically (e.g., every minute) to clean up expired interrupts.

---

## FlowInterrupt

```python
@dataclass
class FlowInterrupt:
    id: str
    kind: InterruptKind
    request_id: str
    user_id: str
    session_id: str
    envelope_id: Optional[str] = None
    
    # Content
    question: Optional[str] = None
    message: Optional[str] = None
    data: Dict[str, Any] = field(default_factory=dict)
    
    # Status
    status: InterruptStatus = InterruptStatus.PENDING
    response: Optional[InterruptResponse] = None
    
    # Timing
    created_at: datetime = field(default_factory=lambda: datetime.now(timezone.utc))
    expires_at: Optional[datetime] = None
    resolved_at: Optional[datetime] = None
    
    # Tracing
    trace_id: Optional[str] = None
    span_id: Optional[str] = None
    
    def to_dict(self) -> Dict[str, Any]: ...
    def to_db_row(self) -> Dict[str, Any]: ...
    @classmethod
    def from_db_row(cls, row: Dict[str, Any]) -> "FlowInterrupt": ...
```

---

## InterruptStatus

```python
class InterruptStatus(str, Enum):
    PENDING = "pending"
    RESOLVED = "resolved"
    CANCELLED = "cancelled"
    EXPIRED = "expired"
```

---

## InterruptResponse

```python
@dataclass
class InterruptResponse:
    text: Optional[str] = None
    approved: Optional[bool] = None
    decision: Optional[str] = None
    data: Dict[str, Any] = field(default_factory=dict)
    
    def to_dict(self) -> Dict[str, Any]: ...
```

---

## Protocols

### DatabaseProtocol

```python
@runtime_checkable
class DatabaseProtocol(Protocol):
    async def insert(self, table: str, data: Dict[str, Any]) -> str: ...
    async def update(
        self, table: str, data: Dict[str, Any], where: Dict[str, Any]
    ) -> int: ...
    async def fetch_one(self, query: str, params: tuple = ()) -> Optional[Dict[str, Any]]: ...
    async def fetch_all(self, query: str, params: tuple = ()) -> List[Dict[str, Any]]: ...
```

### WebhookServiceProtocol

```python
@runtime_checkable
class WebhookServiceProtocol(Protocol):
    async def emit_event(
        self, event_type: str, data: Dict[str, Any], request_id: Optional[str] = None
    ) -> int: ...
```

### OTelAdapterProtocol

```python
@runtime_checkable
class OTelAdapterProtocol(Protocol):
    def record_event(
        self, name: str, request_id: str, attributes: Optional[Dict[str, Any]] = None
    ) -> None: ...
    def get_trace_context(self, request_id: str) -> Optional[Any]: ...
```

---

## Thread Safety

The `InterruptService` uses `asyncio.Lock` for thread safety:

- Memory store modifications are protected
- Safe for concurrent access from multiple coroutines
- Database operations are naturally serialized

---

## Usage Example

```python
from jeeves_control_tower.services import InterruptService, InterruptConfig
from jeeves_protocols import InterruptKind, InterruptResponse
from datetime import timedelta

# Create service with custom config
service = InterruptService(
    db=database,
    logger=logger,
    webhook_service=webhook,
    otel_adapter=otel,
    configs={
        InterruptKind.CLARIFICATION: InterruptConfig(
            default_ttl=timedelta(hours=48),  # Longer TTL
        ),
    },
)

# Create a clarification interrupt
interrupt = await service.create_clarification(
    request_id="req-123",
    user_id="user-1",
    session_id="sess-1",
    question="Which module should I analyze first?",
)

print(f"Created interrupt: {interrupt.id}")

# User responds
resolved = await service.respond(
    interrupt_id=interrupt.id,
    response=InterruptResponse(text="Start with the auth module"),
)

# Check for pending interrupts
pending = await service.get_pending_for_session("sess-1")
for p in pending:
    print(f"Pending: {p.kind} - {p.question or p.message}")

# Expire old interrupts (run periodically)
expired_count = await service.expire_pending()
print(f"Expired {expired_count} interrupts")
```

---

## Webhook Events

The service emits the following webhook events:

| Event | Trigger |
|-------|---------|
| `interrupt.clarification_needed` | Clarification interrupt created |
| `interrupt.confirmation_needed` | Confirmation interrupt created |
| `interrupt.agent_review` | Agent review interrupt created |
| `interrupt.checkpoint` | Checkpoint interrupt created |
| `interrupt.resource_exhausted` | Resource exhausted interrupt created |
| `interrupt.timeout` | Timeout interrupt created |
| `interrupt.system_error` | System error interrupt created |
| `interrupt.resolved` | Any interrupt resolved |
| `interrupt.cancelled` | Any interrupt cancelled |
| `interrupt.expired` | Any interrupt expired |

---

## Navigation

- [← Resources](./resources.md)
- [Back to README](./README.md)
