# Infrastructure Module

Supporting infrastructure: checkpoints, distributed systems, Go interop, and webhooks.

## Navigation

- [README](./README.md) - Overview
- [Settings](./settings.md)
- [Gateway](./gateway.md)
- [Database](./database.md)
- [LLM](./llm.md)
- [Observability](./observability.md)
- [Tools](./tools.md)
- **Infrastructure** (this file)

---

## checkpoint/postgres_adapter.py

PostgreSQL checkpoint adapter for time-travel debugging.

**Constitutional Amendment XXIII**: Time-Travel Debugging Support.

### Class: PostgresCheckpointAdapter

```python
class PostgresCheckpointAdapter:
    """PostgreSQL implementation of CheckpointProtocol.
    
    Stores execution checkpoints in PostgreSQL with JSONB state.
    Supports efficient querying by envelope_id and checkpoint ordering.
    """
    
    def __init__(
        self,
        postgres_client: Any,
        logger: Optional[LoggerProtocol] = None,
    ):
        ...
```

#### Schema

```sql
CREATE TABLE IF NOT EXISTS execution_checkpoints (
    checkpoint_id TEXT PRIMARY KEY,
    envelope_id TEXT NOT NULL,
    agent_name TEXT NOT NULL,
    stage_order INTEGER NOT NULL,
    state_json JSONB NOT NULL,
    metadata_json JSONB,
    parent_checkpoint_id TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_checkpoints_envelope ON execution_checkpoints(envelope_id, stage_order);
```

#### Methods

```python
async def initialize_schema(self) -> None:
    """Create checkpoint table if not exists."""

async def save_checkpoint(
    self,
    envelope_id: str,
    checkpoint_id: str,
    agent_name: str,
    state: Dict[str, Any],
    metadata: Optional[Dict[str, Any]] = None,
) -> CheckpointRecord:
    """Save execution checkpoint after agent completion."""

async def load_checkpoint(self, checkpoint_id: str) -> Optional[Dict[str, Any]]:
    """Load checkpoint state for restoration."""

async def list_checkpoints(self, envelope_id: str, limit: int = 100) -> List[CheckpointRecord]:
    """List all checkpoints for an envelope (execution timeline)."""

async def delete_checkpoints(
    self,
    envelope_id: str,
    before_checkpoint_id: Optional[str] = None,
) -> int:
    """Delete checkpoints for cleanup."""

async def fork_from_checkpoint(
    self,
    checkpoint_id: str,
    new_envelope_id: str,
) -> str:
    """Create new execution branch from checkpoint (time-travel replay)."""
```

---

## distributed/redis_bus.py

Redis distributed bus for horizontal scaling.

**Constitutional Amendment XXIV**: Horizontal Scaling Support.

### Class: RedisDistributedBus

```python
class RedisDistributedBus:
    """Redis implementation of DistributedBusProtocol.
    
    Uses Redis data structures for distributed task execution:
    - Sorted sets for priority queues (ZADD/BZPOPMIN)
    - Hashes for task state and worker registration
    - Streams for task completion events
    """
    
    def __init__(
        self,
        redis_client: Any,
        logger: Optional[LoggerProtocol] = None,
    ):
        ...
```

#### Key Prefixes

| Prefix | Purpose |
|--------|---------|
| `dbus:queue:` | Priority queues |
| `dbus:task:` | Task state hashes |
| `dbus:worker:` | Worker registration |
| `dbus:stats:` | Queue statistics |
| `dbus:processing:` | In-progress tasks |

#### Task Methods

```python
async def enqueue_task(self, queue_name: str, task: DistributedTask) -> str:
    """Enqueue task for worker processing. Returns task ID."""

async def dequeue_task(
    self,
    queue_name: str,
    worker_id: str,
    timeout_seconds: int = 30,
) -> Optional[DistributedTask]:
    """Dequeue task for processing (blocking with timeout)."""

async def complete_task(self, task_id: str, result: Dict[str, Any]) -> None:
    """Mark task as completed with result."""

async def fail_task(self, task_id: str, error: str, retry: bool = True) -> None:
    """Mark task as failed, optionally retry with exponential backoff."""
```

#### Worker Methods

```python
async def register_worker(self, worker_id: str, capabilities: List[str]) -> None:
    """Register worker with capabilities (queue patterns)."""

async def deregister_worker(self, worker_id: str) -> None:
    """Deregister worker (requeues in-progress tasks)."""

async def heartbeat(self, worker_id: str) -> None:
    """Send worker heartbeat (extends TTL)."""
```

#### Stats Methods

```python
async def get_queue_stats(self, queue_name: str) -> QueueStats:
    """Get queue statistics for monitoring."""

async def list_queues(self) -> List[str]:
    """List all active queues."""
```

---

## interop/go_bridge.py

Go bridge module for Python-Go interoperability.

**Note**: Go binary is REQUIRED. No Python fallback.

### Exceptions

```python
class GoBridgeError(Exception):
    """Base exception for Go bridge errors."""

class GoNotAvailableError(GoBridgeError):
    """Raised when Go binary is required but not available."""

class GoExecutionError(GoBridgeError):
    """Raised when Go binary execution fails."""
    code: str  # Error code
```

### Dataclass: GoResult

```python
@dataclass
class GoResult:
    """Result from a Go binary call."""
    success: bool
    data: Dict[str, Any] = field(default_factory=dict)
    error_code: Optional[str] = None
    error_message: Optional[str] = None
```

### Class: GoBridge

```python
class GoBridge:
    """Bridge to Go binary. Fails if binary not available."""
    
    def __init__(self, binary_name: str, timeout_seconds: float = 30.0):
        """Initialize Go bridge.
        
        Raises:
            GoNotAvailableError: If binary not found in PATH
        """
    
    def call(self, command: str, args: Dict[str, Any]) -> GoResult:
        """Call the Go binary with a command.
        
        Raises:
            GoExecutionError: If execution fails
        """
```

### Class: GoEnvelopeBridge

```python
class GoEnvelopeBridge(GoBridge):
    """Bridge to Go envelope operations. Go binary is REQUIRED."""
    
    def __init__(self):
        """Initialize envelope bridge."""
        super().__init__("go-envelope")
```

#### Methods

```python
def create_envelope(
    self,
    raw_input: str,
    user_id: str,
    session_id: str,
    **kwargs: Any,
) -> Dict[str, Any]:
    """Create a new envelope via Go."""

def can_continue(self, envelope: Dict[str, Any]) -> Dict[str, Any]:
    """Check if envelope processing can continue."""

def get_result(self, envelope: Dict[str, Any]) -> Dict[str, Any]:
    """Get result from envelope."""

def validate(self, envelope: Dict[str, Any]) -> Dict[str, Any]:
    """Validate an envelope."""
```

---

## webhooks/service.py

Webhook service for async event delivery.

### Enum: DeliveryStatus

```python
class DeliveryStatus(str, Enum):
    PENDING = "pending"
    DELIVERED = "delivered"
    FAILED = "failed"
    RETRYING = "retrying"
```

### Dataclass: WebhookConfig

```python
@dataclass
class WebhookConfig:
    """Configuration for a webhook endpoint."""
    url: str
    events: List[str]  # e.g., ["request.completed", "agent.*"]
    secret: Optional[str] = None
    headers: Dict[str, str] = field(default_factory=dict)
    timeout_seconds: int = 30
    max_retries: int = 3
    retry_delay_seconds: float = 1.0
    
    def matches_event(self, event_type: str) -> bool:
        """Check if this config subscribes to an event type.
        Supports wildcards: "request.*", "*"
        """
```

### Dataclass: WebhookDeliveryResult

```python
@dataclass
class WebhookDeliveryResult:
    webhook_id: str
    event_type: str
    status: DeliveryStatus
    attempt: int
    response_status: Optional[int] = None
    response_body: Optional[str] = None
    error: Optional[str] = None
    delivered_at: Optional[datetime] = None
    duration_ms: float = 0.0
```

### Class: WebhookService

```python
class WebhookService:
    """Service for managing webhook subscriptions and delivery.
    
    Features:
    - HMAC signature for payload verification
    - Exponential backoff retry
    - Delivery queue for reliability
    - Scoped subscriptions (per-request or global)
    """
    
    def __init__(
        self,
        logger: Optional[LoggerProtocol] = None,
        delivery_queue_size: int = 1000,
    ):
        ...
```

#### Registration Methods

```python
def register(
    self,
    config: WebhookConfig,
    scope: Optional[str] = None,
) -> str:
    """Register a webhook. Returns webhook ID."""

def unregister(self, scope: Optional[str] = None, url: Optional[str] = None) -> int:
    """Unregister webhooks. Returns count removed."""

def get_webhooks(self, scope: Optional[str] = None) -> List[WebhookConfig]:
    """Get registered webhooks."""
```

#### Event Emission

```python
async def emit_event(
    self,
    event_type: str,
    data: Dict[str, Any],
    request_id: Optional[str] = None,
    event_id: Optional[str] = None,
) -> int:
    """Emit an event to matching webhooks. Returns count queued."""
```

#### Background Worker

```python
async def start_delivery_worker(self) -> None:
    """Start the background delivery worker."""

async def stop_delivery_worker(self) -> None:
    """Stop the background delivery worker."""
```

#### Results

```python
def get_delivery_results(
    self,
    scope: Optional[str] = None,
    limit: int = 100,
) -> List[WebhookDeliveryResult]:
    """Get recent delivery results."""
```

#### Subscriber

```python
def create_subscriber(self, scope: Optional[str] = None) -> WebhookSubscriber:
    """Create an event subscriber for this service."""
```

### HMAC Signature

Webhooks include HMAC-SHA256 signature in `X-Webhook-Signature` header:

```
X-Webhook-Signature: sha256=<hex_signature>
```

Verify in your handler:

```python
import hmac
import hashlib

expected = hmac.new(
    secret.encode(),
    request.body,
    hashlib.sha256
).hexdigest()

if not hmac.compare_digest(f"sha256={expected}", request.headers["X-Webhook-Signature"]):
    raise ValueError("Invalid signature")
```

---

## middleware/rate_limit.py

Rate limiting middleware for API layer.

### Class: RateLimitMiddleware

```python
class RateLimitMiddleware:
    """Middleware for API rate limiting.
    
    Integrates with Control Tower's RateLimiter.
    Returns HTTP 429 with Retry-After header.
    """
    
    def __init__(
        self,
        rate_limiter: RateLimiterProtocol,
        config: Optional[RateLimitMiddlewareConfig] = None,
        logger: Optional[LoggerProtocol] = None,
    ):
        ...
```

#### Response Headers

| Header | Description |
|--------|-------------|
| `X-RateLimit-Limit` | Maximum requests allowed |
| `X-RateLimit-Remaining` | Requests remaining |
| `X-RateLimit-Reset` | Unix timestamp when limit resets |
| `Retry-After` | Seconds until request allowed (on 429) |

---

## Usage Examples

### Checkpointing

```python
from jeeves_avionics.checkpoint.postgres_adapter import PostgresCheckpointAdapter

adapter = PostgresCheckpointAdapter(postgres_client, logger)
await adapter.initialize_schema()

# Save checkpoint
record = await adapter.save_checkpoint(
    envelope_id="env_123",
    checkpoint_id="ckpt_abc",
    agent_name="planner",
    state=envelope.to_state_dict(),
)

# Load and fork for replay
state = await adapter.load_checkpoint("ckpt_abc")
new_ckpt = await adapter.fork_from_checkpoint("ckpt_abc", "env_replay_456")
```

### Distributed Task Queue

```python
from jeeves_avionics.distributed.redis_bus import RedisDistributedBus
from jeeves_protocols import DistributedTask

bus = RedisDistributedBus(redis_client, logger)

# Producer
task_id = await bus.enqueue_task("agent:planner", DistributedTask(
    task_id="task_123",
    envelope_state=envelope.to_state_dict(),
    agent_name="planner",
    stage_order=1,
    priority=10,  # Higher = higher priority
))

# Worker
await bus.register_worker("worker_1", ["agent:*"])
while True:
    task = await bus.dequeue_task("agent:planner", "worker_1")
    if task:
        try:
            result = await process(task)
            await bus.complete_task(task.task_id, result)
        except Exception as e:
            await bus.fail_task(task.task_id, str(e))
```

### Webhooks

```python
from jeeves_avionics.webhooks.service import WebhookService, WebhookConfig

service = WebhookService(logger)
await service.start_delivery_worker()

# Register webhook
config = WebhookConfig(
    url="https://example.com/webhook",
    events=["request.completed", "agent.*"],
    secret="my-hmac-secret",
)
webhook_id = service.register(config, scope="request-123")

# Emit event
count = await service.emit_event(
    event_type="request.completed",
    data={"status": "success", "result": "..."},
    request_id="request-123",
)

# Cleanup
await service.stop_delivery_worker()
```
