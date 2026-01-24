# jeeves_protocols.events

**Layer**: L0 (Foundation)  
**Purpose**: Unified Event Schema - Constitutional Event Standard

## Overview

This module defines the canonical event schema that all Jeeves event systems must implement. This ensures standardization across:
- **AgentEvent**: Real-time streaming
- **DomainEvent**: Audit/persistence
- **GatewayEvent**: Internal pub/sub

## Constitutional Authority

- **Avionics CONSTITUTION.md R2**: Configuration Over Code
- **Avionics CONSTITUTION.md R3**: No Domain Logic
- **Layer L0 principle**: Pure types, zero dependencies

---

## Enums

### EventCategory

Canonical event categories for classification and filtering.

```python
class EventCategory(Enum):
    AGENT_LIFECYCLE = "agent_lifecycle"      # Agent start/complete
    TOOL_EXECUTION = "tool_execution"        # Tool start/complete
    PIPELINE_FLOW = "pipeline_flow"          # Flow start/complete/error
    CRITIC_DECISION = "critic_decision"      # Critic verdicts
    STAGE_TRANSITION = "stage_transition"    # Multi-stage progress
    DOMAIN_EVENT = "domain_event"            # Business events
    SESSION_EVENT = "session_event"          # Session lifecycle
    WORKFLOW_EVENT = "workflow_event"        # Workflow operations
```

---

### EventSeverity

Event severity for filtering and alerting.

```python
class EventSeverity(Enum):
    DEBUG = "debug"
    INFO = "info"
    WARNING = "warning"
    ERROR = "error"
    CRITICAL = "critical"
```

---

## Dataclasses

### Event

Constitutional event schema. All event systems MUST emit this format after conversion.

```python
@dataclass
class Event:
    # ─── Identity ───
    event_id: str                          # UUID v4
    event_type: str                        # Namespaced: "agent.started", "critic.decision"
    category: EventCategory                # Broad classification
    
    # ─── Timing (STANDARDIZED) ───
    timestamp_iso: str                     # ISO 8601 format (UTC)
    timestamp_ms: int                      # Epoch milliseconds for ordering
    
    # ─── Context (REQUIRED for correlation) ───
    request_id: str                        # Request correlation ID
    session_id: str                        # Session correlation ID
    user_id: Optional[str] = None          # User (for multi-tenancy)
    
    # ─── Payload ───
    payload: Dict[str, Any] = field(default_factory=dict)
    
    # ─── Metadata ───
    severity: EventSeverity = EventSeverity.INFO
    source: str = "unknown"                # Component that emitted event
    version: str = "1.0"                   # Event schema version
    
    # ─── Audit ───
    correlation_id: Optional[str] = None   # Causation chain (event sourcing)
```

**Methods**:

| Method | Signature | Description |
|--------|-----------|-------------|
| `to_dict` | `() -> Dict[str, Any]` | Convert to JSON-serializable dict |
| `from_dict` | `(data: Dict) -> Event` | Parse from dict (class method) |
| `create_now` | `(**params) -> Event` | Factory with current timestamp (class method) |

**Example**:
```python
from jeeves_protocols import Event, EventCategory, EventSeverity

# Create event manually
event = Event(
    event_id=str(uuid4()),
    event_type="agent.started",
    category=EventCategory.AGENT_LIFECYCLE,
    timestamp_iso=datetime.now(timezone.utc).isoformat(),
    timestamp_ms=int(datetime.now(timezone.utc).timestamp() * 1000),
    request_id="req_123",
    session_id="sess_456",
    payload={"agent": "planner"},
)

# Create event with factory
event = Event.create_now(
    event_type="agent.completed",
    category=EventCategory.AGENT_LIFECYCLE,
    request_id="req_123",
    session_id="sess_456",
    payload={"agent": "planner", "duration_ms": 1500},
    severity=EventSeverity.INFO,
    source="pipeline",
)

# Serialize
data = event.to_dict()

# Deserialize
restored = Event.from_dict(data)
```

---

## EventEmitterProtocol

Protocol all event emitters must implement. This ensures all event systems can be swapped without changing consumer code (Avionics R4: Swappable Implementations).

```python
class EventEmitterProtocol(Protocol):
    async def emit(self, event: Event) -> None:
        """Emit a unified event."""
        ...

    async def subscribe(
        self,
        pattern: str,
        handler: Callable[[Event], Awaitable[None]],
    ) -> str:
        """Subscribe to events matching pattern.
        
        Args:
            pattern: Event type pattern (supports wildcards like "agent.*")
            handler: Async callback that receives Event
            
        Returns:
            Subscription ID for later unsubscription
        """
        ...

    async def unsubscribe(self, subscription_id: str) -> None:
        """Unsubscribe from events."""
        ...
```

**Example Implementation**:
```python
class MyEventEmitter:
    async def emit(self, event: Event) -> None:
        # Publish to message bus
        await self.bus.publish(event.event_type, event.to_dict())
    
    async def subscribe(self, pattern: str, handler) -> str:
        subscription_id = str(uuid4())
        self._handlers[subscription_id] = (pattern, handler)
        return subscription_id
    
    async def unsubscribe(self, subscription_id: str) -> None:
        del self._handlers[subscription_id]
```

---

## StandardEventTypes

Standard event type strings for consistency.

```python
class StandardEventTypes:
    # ─── Agent Lifecycle ───
    AGENT_STARTED = "agent.started"
    AGENT_COMPLETED = "agent.completed"
    AGENT_FAILED = "agent.failed"

    # ─── Pipeline Stages ───
    PERCEPTION_STARTED = "perception.started"
    PERCEPTION_COMPLETED = "perception.completed"
    INTENT_STARTED = "intent.started"
    INTENT_COMPLETED = "intent.completed"
    PLANNER_STARTED = "planner.started"
    PLANNER_COMPLETED = "planner.plan_created"
    EXECUTOR_STARTED = "executor.started"
    EXECUTOR_COMPLETED = "executor.completed"
    SYNTHESIZER_STARTED = "synthesizer.started"
    SYNTHESIZER_COMPLETED = "synthesizer.completed"
    CRITIC_STARTED = "critic.started"
    CRITIC_DECISION = "critic.decision"
    INTEGRATION_STARTED = "integration.started"
    INTEGRATION_COMPLETED = "integration.completed"

    # ─── Tool Execution ───
    TOOL_STARTED = "executor.tool_started"
    TOOL_COMPLETED = "executor.tool_completed"
    TOOL_FAILED = "executor.tool_failed"

    # ─── Pipeline Flow ───
    FLOW_STARTED = "orchestrator.started"
    FLOW_COMPLETED = "orchestrator.completed"
    FLOW_ERROR = "orchestrator.error"
    STAGE_TRANSITION = "orchestrator.stage_transition"

    # ─── Session Events ───
    SESSION_CREATED = "session.created"
    SESSION_UPDATED = "session.updated"
    SESSION_DELETED = "session.deleted"

    # ─── Message Events ───
    MESSAGE_CREATED = "message.created"
    MESSAGE_UPDATED = "message.updated"
```

**Usage**:
```python
from jeeves_protocols import Event, EventCategory, StandardEventTypes

event = Event.create_now(
    event_type=StandardEventTypes.PLANNER_COMPLETED,
    category=EventCategory.AGENT_LIFECYCLE,
    request_id="req_123",
    session_id="sess_456",
    payload={"steps": 5, "duration_ms": 2000},
    source="planner",
)
```

---

## Event Patterns

### Subscribing with Wildcards

```python
# Subscribe to all agent events
await emitter.subscribe("agent.*", handle_agent_event)

# Subscribe to all executor events
await emitter.subscribe("executor.*", handle_executor_event)

# Subscribe to specific event
await emitter.subscribe(StandardEventTypes.CRITIC_DECISION, handle_critic_decision)
```

### Event Correlation

Events include correlation fields for tracing:

```python
# All events in a request share request_id
event1 = Event.create_now(
    event_type="agent.started",
    request_id="req_123",  # Same across all events in request
    session_id="sess_456",
    correlation_id="evt_001",  # Links to causing event
    ...
)

# Child event references parent
event2 = Event.create_now(
    event_type="tool.started",
    request_id="req_123",
    session_id="sess_456",
    correlation_id="evt_001",  # Points to parent event
    ...
)
```

---

## Navigation

- [Back to README](README.md)
- [Previous: Memory](memory.md)
- [Next: Interrupts](interrupts.md)
