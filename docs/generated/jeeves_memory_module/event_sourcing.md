# Event Sourcing (L2)

**Layer:** L2 - Events  
**Scope:** Append-only event log  
**Location:** `jeeves_memory_module/repositories/event_repository.py`, `jeeves_memory_module/services/event_emitter.py`

---

## Overview

The Event Sourcing layer provides an immutable, append-only event log for tracking all domain events in the Jeeves system. Events are never updated or deleted, providing a complete audit trail and enabling timeline reconstruction.

### Key Features

- Append-only event storage (per Memory Contract M2)
- Idempotent event insertion with idempotency keys
- Correlation and causation ID tracking
- Session-scoped deduplication (v0.14 Phase 3)
- CommBus event publishing for observability

---

## Architecture

```
┌─────────────────────┐     ┌──────────────────────┐
│    EventEmitter     │────▶│   EventRepository    │
│  (Service Layer)    │     │  (Persistence Layer) │
└─────────────────────┘     └──────────────────────┘
         │                            │
         │ publish                    │ append
         ▼                            ▼
┌─────────────────────┐     ┌──────────────────────┐
│     CommBus         │     │   domain_events      │
│  (Observability)    │     │      (Table)         │
└─────────────────────┘     └──────────────────────┘
```

---

## DomainEvent

Represents an immutable domain event.

### Fields

| Field | Type | Description |
|-------|------|-------------|
| `event_id` | `str` | Unique event ID (auto-generated UUID if not provided) |
| `timestamp` | `datetime` | Event timestamp (auto-generated if not provided) |
| `aggregate_type` | `str` | Type of aggregate ('task', 'journal', 'session', etc.) |
| `aggregate_id` | `str` | ID of the aggregate |
| `event_type` | `str` | Type of event ('task_created', 'task_completed', etc.) |
| `schema_version` | `int` | Payload schema version (default: 1) |
| `payload` | `Dict[str, Any]` | Event payload (JSON-serializable) |
| `correlation_id` | `Optional[str]` | Request-level grouping |
| `causation_id` | `Optional[str]` | Direct cause (e.g., parent event_id) |
| `user_id` | `str` | User ID |
| `actor` | `str` | Who caused this event ('user', 'planner', 'executor', 'system') |

### Properties

```python
@property
def idempotency_key(self) -> str:
    """Generate idempotency key for deduplication."""
    # Format: aggregate_type:aggregate_id:event_type:schema_version
```

### Methods

```python
def to_dict(self) -> Dict[str, Any]:
    """Convert to dictionary."""

@classmethod
def from_dict(cls, data: Dict[str, Any]) -> "DomainEvent":
    """Create from dictionary."""
```

---

## EventRepository

Repository for domain event persistence.

### Constructor

```python
def __init__(
    self,
    db_client: DatabaseClientProtocol,
    logger: Optional[LoggerProtocol] = None
)
```

### Methods

#### append

```python
async def append(self, event: DomainEvent) -> str:
    """
    Append an event to the event log.
    
    Per Memory Contract M2: Events are append-only, never updated or deleted.
    
    Args:
        event: Domain event to append
        
    Returns:
        Event ID
        
    Raises:
        ValueError: If idempotency key already exists (returns existing ID - idempotent)
    """
```

#### append_batch

```python
async def append_batch(self, events: List[DomainEvent]) -> List[str]:
    """
    Append multiple events in a single transaction.
    
    Args:
        events: List of domain events
        
    Returns:
        List of event IDs
    """
```

#### get_by_id

```python
async def get_by_id(self, event_id: str) -> Optional[DomainEvent]:
    """Get event by ID."""
```

#### get_by_correlation

```python
async def get_by_correlation(
    self,
    correlation_id: str,
    limit: int = 100
) -> List[DomainEvent]:
    """Get all events for a correlation ID (single request)."""
```

#### get_by_aggregate

```python
async def get_by_aggregate(
    self,
    aggregate_type: str,
    aggregate_id: str,
    since: Optional[datetime] = None,
    limit: int = 100
) -> List[DomainEvent]:
    """Get all events for an aggregate (entity history)."""
```

#### get_by_type

```python
async def get_by_type(
    self,
    event_type: str,
    user_id: str,
    since: Optional[datetime] = None,
    limit: int = 100
) -> List[DomainEvent]:
    """Get events by type for a user."""
```

#### get_recent

```python
async def get_recent(
    self,
    user_id: str,
    limit: int = 50,
    aggregate_types: Optional[List[str]] = None
) -> List[DomainEvent]:
    """Get recent events for a user."""
```

#### count_by_type

```python
async def count_by_type(
    self,
    user_id: str,
    since: Optional[datetime] = None
) -> Dict[str, int]:
    """Count events by type for a user."""
```

---

## EventEmitter

Service for emitting domain events with session-scoped deduplication.

### Constructor

```python
def __init__(
    self,
    event_repository: EventRepository,
    feature_flags: Optional[FeatureFlagsProtocol] = None,
    dedup_cache: Optional[SessionDedupCache] = None,
    logger: Optional[LoggerProtocol] = None,
    commbus: Optional["InMemoryCommBus"] = None,
)
```

### Properties

```python
@property
def enabled(self) -> bool:
    """Check if event sourcing is enabled via feature flags."""

@property
def dedup_cache(self) -> SessionDedupCache:
    """Access to deduplication cache for testing/management."""
```

### Core Methods

#### emit

```python
async def emit(
    self,
    aggregate_type: str,
    aggregate_id: str,
    event_type: str,
    payload: Dict[str, Any],
    user_id: str,
    actor: str = "system",
    correlation_id: Optional[str] = None,
    causation_id: Optional[str] = None,
    session_id: Optional[str] = None
) -> Optional[str]:
    """
    Emit a domain event with session-scoped deduplication.
    
    v0.14 Phase 3: When session_id is provided, duplicate events within
    the same session are automatically skipped.
    
    Returns:
        Event ID if emitted, None if disabled or deduplicated
    """
```

#### emit_async

```python
async def emit_async(...) -> None:
    """Emit a domain event asynchronously (fire-and-forget)."""
```

### Event Factory Methods

The EventEmitter provides convenience methods for common event types:

#### Task Events

```python
async def emit_task_created(self, task_id, user_id, title, ...) -> Optional[str]
async def emit_task_updated(self, task_id, user_id, changes, ...) -> Optional[str]
async def emit_task_completed(self, task_id, user_id, ...) -> Optional[str]
async def emit_task_deleted(self, task_id, user_id, soft=True, ...) -> Optional[str]
```

#### Journal Events

```python
async def emit_journal_ingested(self, entry_id, user_id, content, ...) -> Optional[str]
async def emit_journal_summarized(self, entry_id, user_id, summary, ...) -> Optional[str]
```

#### Session Events

```python
async def emit_session_started(self, session_id, user_id, ...) -> Optional[str]
async def emit_session_summarized(self, session_id, user_id, summary, ...) -> Optional[str]
```

#### Memory Events

```python
async def emit_memory_stored(self, item_id, layer, user_id, item_type, ...) -> Optional[str]
async def emit_memory_retrieved(self, user_id, query, layer, results_count, ...) -> Optional[str]
async def emit_memory_deleted(self, item_id, layer, user_id, soft_delete=True, ...) -> Optional[str]
```

#### Workflow Events

```python
async def emit_plan_created(self, plan_id, request_id, user_id, intent, ...) -> Optional[str]
async def emit_critic_decision(self, request_id, user_id, action, confidence, ...) -> Optional[str]
async def emit_retry_attempt(self, request_id, user_id, retry_type, attempt_number, ...) -> Optional[str]
async def emit_tool_executed(self, request_id, user_id, tool_name, status, ...) -> Optional[str]
```

---

## SessionDedupCache

Session-scoped event deduplication cache for v0.14 Phase 3.

### Purpose

Prevents duplicate L2 event emission within a session. Critical for retry safety - if the same action is retried within a session, duplicate events are not emitted.

### Key Format

```
sha256(session_id:aggregate_type:aggregate_id:event_type:payload_hash)[:32]
```

### Methods

```python
def is_duplicate(
    self,
    session_id: str,
    aggregate_type: str,
    aggregate_id: str,
    event_type: str,
    payload: Dict[str, Any]
) -> bool:
    """Check if this event was already emitted in this session."""

def mark_emitted(
    self,
    session_id: str,
    aggregate_type: str,
    aggregate_id: str,
    event_type: str,
    payload: Dict[str, Any]
) -> None:
    """Mark an event as emitted in this session."""

def clear_session(self, session_id: str) -> None:
    """Clear cache for a specific session."""

def clear_all(self) -> None:
    """Clear all session caches."""
```

---

## Usage Examples

### Basic Event Emission

```python
from jeeves_memory_module.services.event_emitter import EventEmitter
from jeeves_memory_module.repositories.event_repository import EventRepository

# Initialize
event_repo = EventRepository(db_client)
emitter = EventEmitter(event_repo)

# Emit a task creation event
event_id = await emitter.emit_task_created(
    task_id="task-123",
    user_id="user-456",
    title="Complete code review",
    priority=2,
    actor="user",
    session_id="session-789"  # For deduplication
)
```

### Timeline Reconstruction

```python
# Get all events for a specific request
events = await event_repo.get_by_correlation(correlation_id="req-001")

# Get entity history
task_events = await event_repo.get_by_aggregate(
    aggregate_type="task",
    aggregate_id="task-123"
)
```

### Event Statistics

```python
# Count events by type
counts = await event_repo.count_by_type(
    user_id="user-456",
    since=datetime.now(timezone.utc) - timedelta(days=7)
)
# Returns: {"task_created": 10, "task_completed": 5, ...}
```

---

## Database Schema

The event repository uses the `domain_events` table:

```sql
CREATE TABLE domain_events (
    event_id UUID PRIMARY KEY,
    occurred_at TIMESTAMPTZ NOT NULL,
    aggregate_type TEXT NOT NULL,
    aggregate_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    schema_version INTEGER DEFAULT 1,
    payload JSONB,
    causation_id UUID,
    correlation_id UUID,
    idempotency_key TEXT UNIQUE,
    user_id TEXT NOT NULL,
    actor TEXT DEFAULT 'system'
);

CREATE INDEX idx_domain_events_correlation ON domain_events(correlation_id);
CREATE INDEX idx_domain_events_aggregate ON domain_events(aggregate_type, aggregate_id);
CREATE INDEX idx_domain_events_type ON domain_events(event_type);
```

---

## Navigation

- [Back to README](./README.md)
- [Next: Semantic Memory (L3)](./semantic_memory.md)
