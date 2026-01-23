# IPC Module

**Module:** `jeeves_control_tower.ipc`  
**Main Classes:** `CommBusCoordinator`, `InMemoryCommBus`

---

## Overview

The IPC module implements the kernel's inter-process communication system:

- **CommBusCoordinator** - Service registration and request dispatch
- **InMemoryCommBus** - Python message bus matching Go design
- **Adapters** - Bridges to external message buses (extensible)

---

## CommBusCoordinator

### Purpose

The `CommBusCoordinator` is the kernel's IPC manager:

- Service registration (like init daemon registration)
- Message routing (like kernel message passing)
- Request dispatch (like syscall dispatch)
- Event publish/subscribe via InMemoryCommBus

### Constructor

```python
class CommBusCoordinator(CommBusCoordinatorProtocol):
    def __init__(
        self,
        logger: LoggerProtocol,
        commbus: Optional[InMemoryCommBus] = None,
    ) -> None:
```

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `logger` | `LoggerProtocol` | Logger instance |
| `commbus` | `InMemoryCommBus` | Optional CommBus instance (uses singleton if not provided) |

---

## Service Registration

### register_service

Register a service with the kernel.

```python
def register_service(
    self,
    descriptor: ServiceDescriptor,
) -> bool:
```

**Returns:** `True` if registered, `False` if service already exists.

**Example:**

```python
from jeeves_control_tower.types import ServiceDescriptor

coordinator.register_service(ServiceDescriptor(
    name="flow_service",
    service_type="flow",
    capabilities=["execute_pipeline"],
    max_concurrent=10,
))
```

---

### unregister_service

Remove a service from the registry.

```python
def unregister_service(self, service_name: str) -> bool:
```

---

### get_service

Get a service descriptor by name.

```python
def get_service(self, service_name: str) -> Optional[ServiceDescriptor]:
```

---

### list_services

List registered services with optional filtering.

```python
def list_services(
    self,
    service_type: Optional[str] = None,
    healthy_only: bool = True,
) -> List[ServiceDescriptor]:
```

---

## Request Dispatch

### dispatch

Dispatch a request to a service.

```python
async def dispatch(
    self,
    target: DispatchTarget,
    envelope: Envelope,
) -> Envelope:
```

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `target` | `DispatchTarget` | Service name, method, priority, timeout |
| `envelope` | `Envelope` | Request envelope |

**Behavior:**
1. Validate service exists and is healthy
2. Update service load counter
3. Try local handler first
4. Fall back to CommBus adapter if available
5. Apply timeout
6. Handle retries on failure

**Example:**

```python
from jeeves_control_tower.types import DispatchTarget

result = await coordinator.dispatch(
    target=DispatchTarget(
        service_name="flow_service",
        method="run",
        priority=SchedulingPriority.HIGH,
        timeout_seconds=300,
    ),
    envelope=envelope,
)
```

---

### register_handler

Register a local dispatch handler for a service.

```python
def register_handler(
    self,
    service_name: str,
    handler: DispatchHandler,
) -> None:
```

The handler type is:

```python
DispatchHandler = Callable[[Envelope], asyncio.Future[Envelope]]
```

---

## Event Communication

### broadcast

Broadcast an event to all subscribers.

```python
async def broadcast(
    self,
    event_type: str,
    payload: Dict[str, Any],
) -> None:
```

---

### publish_event

Publish a typed event via CommBus.

```python
async def publish_event(self, event: Any) -> None:
```

**Example:**

```python
from dataclasses import dataclass, field

@dataclass
class MemoryStored:
    category: str = field(default="event", init=False)
    item_id: str
    layer: str

await coordinator.publish_event(MemoryStored(item_id="123", layer="L2"))
```

---

## Query Communication

### request

Send a request to a service and wait for response.

```python
async def request(
    self,
    service_name: str,
    query_type: str,
    payload: Dict[str, Any],
    timeout_seconds: float = 30.0,
) -> Dict[str, Any]:
```

**Returns:**

```python
{"result": <response>}  # On success
{"error": "..."}        # On failure
```

---

### send_query

Send a typed query via CommBus.

```python
async def send_query(self, query: Any, timeout: Optional[float] = None) -> Any:
```

---

## Subscription

### subscribe

Subscribe to CommBus events.

```python
def subscribe(self, event_type: str, handler: Any) -> Callable[[], None]:
```

**Returns:** Unsubscribe function.

---

### register_query_handler

Register a handler for CommBus queries.

```python
def register_query_handler(self, query_type: str, handler: Any) -> None:
```

---

## InMemoryCommBus

### Purpose

The `InMemoryCommBus` provides the same semantics as the Go `commbus/bus.go`:

| Pattern | Method | Semantics |
|---------|--------|-----------|
| **Publish** | `await bus.publish(event)` | Fan-out to all subscribers |
| **Send** | `await bus.send(command)` | Fire-and-forget to single handler |
| **Query** | `await bus.query(query)` | Request-response with timeout |

### Constructor

```python
class InMemoryCommBus:
    def __init__(
        self,
        query_timeout: float = 30.0,
        logger: Optional[LoggerProtocol] = None,
    ):
```

### Message Types

```python
@runtime_checkable
class Message(Protocol):
    """Base protocol for all CommBus messages."""
    category: str  # "event", "command", or "query"

@runtime_checkable
class Event(Protocol):
    """Event message protocol - fan-out to all subscribers."""
    category: str

@runtime_checkable
class Command(Protocol):
    """Command message protocol - fire-and-forget to single handler."""
    category: str

@runtime_checkable
class Query(Protocol):
    """Query message protocol - request-response with single handler."""
    category: str
```

---

### publish

Publish an event to all subscribers.

```python
async def publish(self, event: Any) -> None:
```

**Behavior:**
- Events are processed concurrently by all subscribers
- Subscriber errors are logged but don't stop other subscribers
- Middleware chain is executed before and after

---

### send

Send a command to handler (fire-and-forget).

```python
async def send(self, command: Any) -> None:
```

**Behavior:**
- Commands have exactly one handler
- Handler errors are logged
- No return value

---

### query

Send a query and wait for response.

```python
async def query(self, query: Any, timeout: Optional[float] = None) -> Any:
```

**Raises:**
- `asyncio.TimeoutError` - If query times out
- `ValueError` - If no handler registered

---

### subscribe

Subscribe to an event type.

```python
def subscribe(self, event_type: str, handler: HandlerFunc) -> Callable[[], None]:
```

**Returns:** Unsubscribe function for cleanup.

---

### register_handler

Register a handler for a command/query type.

```python
def register_handler(self, message_type: str, handler: HandlerFunc) -> None:
```

**Raises:** `ValueError` if handler already registered.

---

## Middleware

### MiddlewareContext

```python
@dataclass
class MiddlewareContext:
    message: Any
    message_type: str
    timestamp: datetime
    metadata: Dict[str, Any]
```

### Middleware Protocol

```python
class Middleware(Protocol):
    async def before(self, ctx: MiddlewareContext, message: Any) -> Optional[Any]:
        """Called before handler. Return None to abort, or modified message."""
        ...

    async def after(
        self, ctx: MiddlewareContext, message: Any, result: Any, error: Optional[Exception]
    ) -> Optional[Any]:
        """Called after handler. Can modify result."""
        ...
```

### add_middleware

```python
def add_middleware(self, middleware: Middleware) -> None:
```

---

## Introspection

```python
def has_handler(self, message_type: str) -> bool:
    """Check if handler is registered for message type."""

def get_subscribers(self, event_type: str) -> List[HandlerFunc]:
    """Get all subscribers for event type."""

def get_registered_types(self) -> List[str]:
    """Get all registered message types."""
```

---

## Singleton Access

```python
def get_commbus(logger: Optional[LoggerProtocol] = None) -> InMemoryCommBus:
    """Get the global CommBus instance (creates lazily)."""

def reset_commbus() -> None:
    """Reset the global CommBus instance (for testing)."""
```

---

## ServiceDescriptor

```python
@dataclass
class ServiceDescriptor:
    name: str                           # Unique service name
    service_type: str                   # "flow", "worker", "vertical"
    version: str = "1.0.0"
    capabilities: List[str] = field(default_factory=list)
    max_concurrent: int = 10
    current_load: int = 0               # Current request count
    healthy: bool = True
    metadata: Dict[str, Any] = field(default_factory=dict)
```

---

## DispatchTarget

```python
@dataclass
class DispatchTarget:
    service_name: str
    method: str
    priority: SchedulingPriority = SchedulingPriority.NORMAL
    timeout_seconds: int = 300
    retry_count: int = 0
    max_retries: int = 3
```

---

## Adapters

The `adapters/` subdirectory provides bridges to external message buses:

```python
# jeeves_control_tower/ipc/adapters/__init__.py
__all__: list[str] = []  # Currently empty, adapters added as needed
```

Adapters are implemented as needed for:
- Go CommBus (via gRPC)
- HTTP endpoints
- Redis Pub/Sub
- RabbitMQ

---

## Protocol

```python
@runtime_checkable
class CommBusCoordinatorProtocol(Protocol):
    def register_service(self, descriptor: ServiceDescriptor) -> bool: ...
    def unregister_service(self, service_name: str) -> bool: ...
    def get_service(self, service_name: str) -> Optional[ServiceDescriptor]: ...
    def list_services(
        self,
        service_type: Optional[str] = None,
        healthy_only: bool = True,
    ) -> List[ServiceDescriptor]: ...
    
    async def dispatch(
        self,
        target: DispatchTarget,
        envelope: Envelope,
    ) -> Envelope: ...
    
    async def broadcast(
        self,
        event_type: str,
        payload: Dict[str, Any],
    ) -> None: ...
    
    async def request(
        self,
        service_name: str,
        query_type: str,
        payload: Dict[str, Any],
        timeout_seconds: float = 30.0,
    ) -> Dict[str, Any]: ...
```

---

## Usage Example

```python
from jeeves_control_tower.ipc import get_commbus, CommBusCoordinator
from jeeves_control_tower.types import ServiceDescriptor, DispatchTarget

# Get singleton CommBus
bus = get_commbus()

# Create coordinator
coordinator = CommBusCoordinator(logger)

# Register a service
coordinator.register_service(ServiceDescriptor(
    name="flow_service",
    service_type="flow",
    capabilities=["execute"],
))

# Register handler
async def handle_flow(envelope):
    return envelope  # Process and return

coordinator.register_handler("flow_service", handle_flow)

# Dispatch request
result = await coordinator.dispatch(
    target=DispatchTarget(service_name="flow_service", method="run"),
    envelope=envelope,
)

# Subscribe to events
def on_memory_stored(event):
    print(f"Memory stored: {event}")

unsubscribe = bus.subscribe("MemoryStored", on_memory_stored)

# Query with response
state = await bus.query(GetSessionState(session_id="abc"))
```

---

## Navigation

- [← Events](./events.md)
- [Back to README](./README.md)
- [Lifecycle →](./lifecycle.md)
