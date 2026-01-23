# CommBus Messages

**Location:** `jeeves_memory_module/messages/`

---

## Overview

The Memory Module defines typed messages for communication via the CommBus. These enable event-driven architecture for memory operations, allowing other components to subscribe to memory changes.

### Message Categories

| Category | Purpose | Example |
|----------|---------|---------|
| **Events** | Notifications when something happened | `MemoryStored`, `FocusChanged` |
| **Queries** | Request/response for data retrieval | `GetSessionState`, `SearchMemory` |
| **Commands** | Imperative operations with side effects | `ClearSession`, `UpdateFocus` |

---

## Events

Events are published when memory operations occur. They enable observability and allow other components to react to memory changes.

### MemoryStored

Published when a memory item is stored.

```python
@dataclass(frozen=True)
class MemoryStored:
    category: str = "event"  # Auto-set
    item_id: str = ""
    layer: str = ""          # L1-L7
    user_id: str = ""
    item_type: str = ""      # fact, message, etc.
    timestamp: datetime = field(default_factory=lambda: datetime.now(timezone.utc))
    metadata: Dict[str, Any] = field(default_factory=dict)
```

### MemoryRetrieved

Published when memory is queried.

```python
@dataclass(frozen=True)
class MemoryRetrieved:
    category: str = "event"
    query: str = ""
    layer: str = ""
    results_count: int = 0
    latency_ms: float = 0.0
    user_id: Optional[str] = None
```

### MemoryDeleted

Published when a memory item is deleted.

```python
@dataclass(frozen=True)
class MemoryDeleted:
    category: str = "event"
    item_id: str = ""
    layer: str = ""
    user_id: str = ""
    soft_delete: bool = True
```

### SessionStateChanged

Published when session state changes.

```python
@dataclass(frozen=True)
class SessionStateChanged:
    category: str = "event"
    session_id: str = ""
    user_id: str = ""
    change_type: str = ""    # focus, entity_added, clarification
    timestamp: datetime = field(default_factory=lambda: datetime.now(timezone.utc))
```

### FocusChanged

Published when session focus changes.

```python
@dataclass(frozen=True)
class FocusChanged:
    category: str = "event"
    session_id: str = ""
    focus_type: str = ""
    focus_id: Optional[str] = None
    focus_label: Optional[str] = None
```

### EntityReferenced

Published when an entity is referenced in a session.

```python
@dataclass(frozen=True)
class EntityReferenced:
    category: str = "event"
    session_id: str = ""
    entity_type: str = ""
    entity_id: str = ""
    label: str = ""
```

### ClarificationRequested

Published when clarification is needed.

```python
@dataclass(frozen=True)
class ClarificationRequested:
    category: str = "event"
    session_id: str = ""
    question: str = ""
    original_request: str = ""
    options: tuple = field(default_factory=tuple)
```

### ClarificationResolved

Published when clarification is resolved.

```python
@dataclass(frozen=True)
class ClarificationResolved:
    category: str = "event"
    session_id: str = ""
    answer: str = ""
    was_expired: bool = False
```

---

## Queries

Queries request data from memory services. They are sent via CommBus and handlers return responses.

### GetSessionState

Query for session working memory.

```python
@dataclass(frozen=True)
class GetSessionState:
    category: str = "query"
    session_id: str = ""
    user_id: str = ""
    
    # Returns: WorkingMemory instance
```

**Response Format:**
```python
{
    "session_id": "...",
    "user_id": "...",
    "focus": {
        "type": "task",
        "id": "...",
        "label": "...",
        "context": {...}
    },
    "entities": [...],
    "short_term_context": "...",
    "created_at": "...",
    "updated_at": "..."
}
```

### SearchMemory

Query for memory search.

```python
@dataclass(frozen=True)
class SearchMemory:
    category: str = "query"
    query: str = ""
    layer: Optional[str] = None   # None = all layers
    user_id: Optional[str] = None
    limit: int = 10
    filters: Dict[str, Any] = field(default_factory=dict)
    
    # Returns: List[SearchResult]
```

**Response Format:**
```python
{
    "results": [
        {"item_id": "...", "content": "...", "score": 0.85, ...},
        ...
    ],
    "total": 10,
    "query": "..."
}
```

### GetClarificationContext

Query for pending clarification.

```python
@dataclass(frozen=True)
class GetClarificationContext:
    category: str = "query"
    session_id: str = ""
    
    # Returns: Optional[ClarificationContext]
```

### GetRecentEntities

Query for recently referenced entities.

```python
@dataclass(frozen=True)
class GetRecentEntities:
    category: str = "query"
    session_id: str = ""
    limit: int = 10
    
    # Returns: List[EntityRef]
```

---

## Commands

Commands are imperative operations that cause side effects. They modify memory state.

### ClearSession

Command to clear session state.

```python
@dataclass(frozen=True)
class ClearSession:
    category: str = "command"
    session_id: str = ""
```

### InvalidateMemoryCache

Command to invalidate memory cache.

```python
@dataclass(frozen=True)
class InvalidateMemoryCache:
    category: str = "command"
    user_id: Optional[str] = None   # None = all users
    layer: Optional[str] = None      # None = all layers
```

### UpdateFocus

Command to update session focus.

```python
@dataclass(frozen=True)
class UpdateFocus:
    category: str = "command"
    session_id: str = ""
    focus_type: str = ""
    focus_id: Optional[str] = None
    focus_label: Optional[str] = None
    focus_context: Dict[str, Any] = field(default_factory=dict)
```

### AddEntityReference

Command to add an entity reference to a session.

```python
@dataclass(frozen=True)
class AddEntityReference:
    category: str = "command"
    session_id: str = ""
    entity_type: str = ""
    entity_id: str = ""
    label: str = ""
    context: str = ""
```

---

## Handler Registration

Messages are connected to handlers in `handlers.py`:

```python
from jeeves_memory_module.handlers import register_memory_handlers

# In application startup
register_memory_handlers(
    commbus=commbus,
    session_state_service=session_state_service,
    db=db_client,
    logger=logger,
)
```

### Registered Handlers

| Message | Handler |
|---------|---------|
| `GetSessionState` | `handle_get_session_state` |
| `GetRecentEntities` | `handle_get_recent_entities` |
| `SearchMemory` | `handle_search_memory` |
| `GetClarificationContext` | `handle_get_clarification_context` |
| `ClearSession` | `handle_clear_session` |
| `UpdateFocus` | `handle_update_focus` |
| `AddEntityReference` | `handle_add_entity_reference` |

---

## Usage Examples

### Publishing Events

```python
from jeeves_memory_module.services.event_emitter import EventEmitter

# Events are published automatically by EventEmitter
await emitter.emit_memory_stored(
    item_id="item-123",
    layer="L3",
    user_id="user-456",
    item_type="chunk"
)
```

### Sending Queries

```python
from jeeves_memory_module.messages import GetSessionState

# Via CommBus
query = GetSessionState(session_id="sess-123", user_id="user-456")
response = await commbus.query(query)

# Response contains session state data
print(response["focus"]["type"])
```

### Sending Commands

```python
from jeeves_memory_module.messages import UpdateFocus

# Via CommBus
command = UpdateFocus(
    session_id="sess-123",
    focus_type="task",
    focus_id="task-789",
    focus_label="Code review"
)
await commbus.send(command)
```

### Subscribing to Events

```python
from jeeves_memory_module.messages import MemoryStored

async def on_memory_stored(event: MemoryStored):
    print(f"Item {event.item_id} stored in layer {event.layer}")

commbus.subscribe(MemoryStored, on_memory_stored)
```

---

## Message Design Principles

1. **Immutable**: All messages use `@dataclass(frozen=True)`
2. **Typed**: Strong typing for all fields
3. **Self-describing**: `category` field identifies message type
4. **Defaults**: Sensible defaults for optional fields
5. **Timestamped**: Events include timestamps automatically

---

## Navigation

- [Back to README](./README.md)
- [Previous: Tool Health (L7)](./tool_health.md)
- [Next: Services](./services.md)
