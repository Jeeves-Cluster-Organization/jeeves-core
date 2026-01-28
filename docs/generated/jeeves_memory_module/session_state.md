# Session State (L4)

**Layer:** L4 - Working Memory  
**Scope:** Per-session state  
**Location:** `jeeves_infra/memory/repositories/session_state_repository.py`, `jeeves_infra/memory/services/session_state_service.py`, `jeeves_infra/memory/services/session_state_adapter.py`

---

## Overview

The Session State layer provides hot-path storage for active session context, including focus tracking, entity references, and short-term memory. This is the "working memory" that maintains conversation context across turns.

### Key Features

- Session lifecycle management
- Focus tracking (what the user is working on)
- Entity reference tracking
- Short-term memory coordination
- Turn counting for summarization triggers
- Protocol adapter for Core integration

---

## Architecture

```
┌─────────────────────────┐     ┌────────────────────────────┐
│  SessionStateAdapter    │────▶│   SessionStateService      │
│  (Protocol Bridge)      │     │   (Business Logic)         │
└─────────────────────────┘     └────────────────────────────┘
                                          │
                                          │ persist
                                          ▼
                                ┌────────────────────────────┐
                                │  SessionStateRepository    │
                                │  (Persistence Layer)       │
                                └────────────────────────────┘
```

---

## SessionState

Represents the current state of a user session.

### Fields

| Field | Type | Description |
|-------|------|-------------|
| `session_id` | `str` | Session identifier |
| `user_id` | `str` | User identifier |
| `focus_type` | `Optional[str]` | Type of current focus ('task', 'journal', 'planning', 'general') |
| `focus_id` | `Optional[str]` | ID of focused entity if applicable |
| `focus_context` | `Dict[str, Any]` | Additional context about the focus |
| `referenced_entities` | `List[Dict[str, str]]` | List of entities referenced in session |
| `short_term_memory` | `Optional[str]` | Compressed summary of recent conversation |
| `turn_count` | `int` | Number of conversation turns in this session |
| `created_at` | `datetime` | When the session state was created |
| `updated_at` | `datetime` | When the session state was last updated |

### Referenced Entities Format

```python
[
    {
        "type": "task",
        "id": "task-123",
        "title": "Complete code review",
        "referenced_at": "2026-01-23T10:30:00Z"
    },
    ...
]
```

### Methods

```python
def to_dict(self) -> Dict[str, Any]:
    """Convert to dictionary."""

@classmethod
def from_dict(cls, data: Dict[str, Any]) -> "SessionState":
    """Create from dictionary."""
```

---

## SessionStateRepository

Repository for session state persistence.

### Constructor

```python
def __init__(
    self,
    db: DatabaseClientProtocol,
    logger: Optional[LoggerProtocol] = None
)
```

### Methods

#### get

```python
async def get(self, session_id: str) -> Optional[SessionState]:
    """Get session state by session ID."""
```

#### upsert

```python
async def upsert(self, state: SessionState) -> SessionState:
    """Insert or update session state."""
```

#### update_focus

```python
async def update_focus(
    self,
    session_id: str,
    focus_type: str,
    focus_id: Optional[str] = None,
    focus_context: Optional[Dict[str, Any]] = None
) -> Optional[SessionState]:
    """Update the focus of a session."""
```

#### add_referenced_entity

```python
async def add_referenced_entity(
    self,
    session_id: str,
    entity_type: str,
    entity_id: str,
    entity_title: Optional[str] = None
) -> Optional[SessionState]:
    """
    Add an entity to the session's referenced entities.
    
    Note: Keeps only last 20 references to prevent unbounded growth.
    """
```

#### increment_turn

```python
async def increment_turn(self, session_id: str) -> Optional[SessionState]:
    """Increment the turn count for a session."""
```

#### update_short_term_memory

```python
async def update_short_term_memory(
    self,
    session_id: str,
    memory: str
) -> Optional[SessionState]:
    """Update the short-term memory (conversation summary)."""
```

#### delete

```python
async def delete(self, session_id: str) -> bool:
    """Delete session state."""
```

#### get_by_user

```python
async def get_by_user(self, user_id: str) -> List[SessionState]:
    """Get all session states for a user."""
```

---

## SessionStateService

High-level service for managing session state.

### Constructor

```python
def __init__(
    self,
    db: DatabaseClientProtocol,
    repository: Optional[SessionStateRepository] = None,
    logger: Optional[LoggerProtocol] = None
)
```

### Methods

#### ensure_initialized

```python
async def ensure_initialized(self) -> None:
    """Ensure the repository table exists."""
```

#### get

```python
async def get(self, session_id: str) -> Optional[SessionState]:
    """
    Get session state by ID.
    
    Args:
        session_id: Session identifier
        
    Returns:
        SessionState or None if not found
    """
```

#### get_or_create

```python
async def get_or_create(
    self,
    session_id: str,
    user_id: str
) -> SessionState:
    """Get existing session state or create a new one."""
```

#### update_focus

```python
async def update_focus(
    self,
    session_id: str,
    focus_type: str,
    focus_id: Optional[str] = None,
    focus_context: Optional[Dict[str, Any]] = None
) -> Optional[SessionState]:
    """Update the current focus of a session."""
```

#### record_entity_reference

```python
async def record_entity_reference(
    self,
    session_id: str,
    entity_type: str,
    entity_id: str,
    entity_title: Optional[str] = None
) -> Optional[SessionState]:
    """Record that an entity was referenced in the conversation."""
```

#### on_user_turn

```python
async def on_user_turn(
    self,
    session_id: str,
    user_message: str
) -> Optional[SessionState]:
    """Handle a new user message (increment turn counter)."""
```

#### update_memory

```python
async def update_memory(
    self,
    session_id: str,
    memory_summary: str
) -> Optional[SessionState]:
    """Update the short-term memory summary."""
```

#### get_context_for_prompt

```python
async def get_context_for_prompt(
    self,
    session_id: str
) -> Dict[str, Any]:
    """
    Get session context formatted for LLM prompts.
    
    Returns:
        Dictionary with session context:
        {
            "turn_count": 5,
            "focus": {"type": "task", "id": "...", "context": {...}},
            "recent_entities": [...],
            "conversation_summary": "..."
        }
    """
```

#### should_summarize

```python
async def should_summarize(
    self,
    session_id: str,
    summarize_every_n_turns: int = 5
) -> bool:
    """Check if the session should trigger summarization."""
```

#### clear_focus

```python
async def clear_focus(self, session_id: str) -> Optional[SessionState]:
    """Clear the current focus (return to general mode)."""
```

#### delete

```python
async def delete(self, session_id: str) -> bool:
    """
    Delete session state.
    
    Called when a session is terminated or cleaned up.
    
    Args:
        session_id: Session identifier
        
    Returns:
        True if deleted, False if not found
    """
```

---

## SessionStateAdapter

Adapter implementing Core's `SessionStateProtocol`, bridging Avionics SessionState to Core WorkingMemory.

### Constructor

```python
def __init__(
    self,
    db: DatabaseClientProtocol,
    service: Optional[SessionStateService] = None,
    logger: Optional[LoggerProtocol] = None
)
```

### Protocol Implementation

```python
async def get_or_create(
    self,
    session_id: str,
    user_id: str
) -> WorkingMemory:
    """Get existing session state or create new, returning WorkingMemory."""

async def save(
    self,
    session_id: str,
    state: WorkingMemory
) -> bool:
    """Save WorkingMemory as SessionState."""

async def delete(self, session_id: str) -> bool:
    """Delete session state."""

async def update_focus(
    self,
    session_id: str,
    focus: FocusState
) -> bool:
    """Update just the focus state."""

async def add_entity_reference(
    self,
    session_id: str,
    entity: EntityRef
) -> bool:
    """Add an entity reference to session."""

async def get_active_sessions(
    self,
    user_id: str,
    limit: int = 10
) -> List[str]:
    """Get active session IDs for a user."""
```

### Focus Type Conversion

The adapter converts between Core's `FocusType` enum and Avionics focus type strings:

| Core FocusType | Avionics String |
|----------------|-----------------|
| `FocusType.GENERAL` | `"general"` |
| `FocusType.TASK` | `"task"` |
| `FocusType.EXPLORATION` | `"exploration"` |
| `FocusType.ENTITY` | `"entity"` |
| `FocusType.CLARIFICATION` | `"clarification"` |

---

## Usage Examples

### Basic Session Management

```python
from jeeves_infra.memory.services.session_state_service import SessionStateService

service = SessionStateService(db)

# Get or create session
state = await service.get_or_create(
    session_id="sess-123",
    user_id="user-456"
)

# Update focus
await service.update_focus(
    session_id="sess-123",
    focus_type="task",
    focus_id="task-789",
    focus_context={"title": "Code review"}
)

# Record entity reference
await service.record_entity_reference(
    session_id="sess-123",
    entity_type="task",
    entity_id="task-789",
    entity_title="Code review"
)
```

### Using the Protocol Adapter

```python
from jeeves_infra.memory.services.session_state_adapter import SessionStateAdapter

adapter = SessionStateAdapter(db)

# Get WorkingMemory (Core protocol type)
memory = await adapter.get_or_create("sess-123", "user-456")

# Access through protocol interface
print(memory.focus.focus_type)  # FocusType.GENERAL
print(memory.turn_count)        # 0
print(memory.referenced_entities)  # []

# Update via protocol
from protocols import FocusState, FocusType
focus = FocusState(focus_type=FocusType.TASK, focus_id="task-789")
await adapter.update_focus("sess-123", focus)
```

### Context for LLM Prompts

```python
# Get formatted context for prompts
context = await service.get_context_for_prompt("sess-123")

# Returns:
{
    "turn_count": 5,
    "focus": {
        "type": "task",
        "id": "task-789",
        "context": {"title": "Code review"}
    },
    "recent_entities": [
        {"type": "task", "id": "task-789", "title": "Code review"},
        ...
    ],
    "conversation_summary": "User discussed code review workflow..."
}
```

---

## Database Schema

```sql
CREATE TABLE session_state (
    session_id UUID PRIMARY KEY,
    user_id TEXT NOT NULL,
    focus_type TEXT,
    focus_id TEXT,
    focus_context TEXT,  -- JSONB
    referenced_entities TEXT,  -- JSONB array
    short_term_memory TEXT,
    turn_count INTEGER DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_session_state_user ON session_state(user_id);
```

---

## Clarification Handling Note

**Unified Interrupt System (v4.0):** Clarification and confirmation methods have been removed from SessionStateService. All interrupt-related functionality is now handled by `control_tower.services.InterruptService`.

---

## Navigation

- [Back to README](./README.md)
- [Previous: Semantic Memory (L3)](./semantic_memory.md)
- [Next: Entity Graphs (L5)](./entity_graphs.md)
