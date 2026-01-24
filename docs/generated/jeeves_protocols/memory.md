# jeeves_protocols.memory

**Layer**: L0 (Foundation)  
**Purpose**: Working memory types, findings, and focus state

## Overview

This module provides types for agent working memory - the transient state that agents use during processing. It includes focus tracking, entity references, findings, and clarification contexts.

---

## Enums

### FocusType

Type of focus in working memory.

```python
class FocusType(str, Enum):
    FILE = "file"           # Focused on a file
    FUNCTION = "function"   # Focused on a function
    CLASS = "class"         # Focused on a class
    MODULE = "module"       # Focused on a module
    CONCEPT = "concept"     # Focused on a concept/topic
```

---

## Dataclasses

### EntityRef

Reference to an entity in working memory.

```python
@dataclass
class EntityRef:
    entity_type: str                     # Type of entity (file, function, etc.)
    entity_id: str                       # Unique identifier
    name: str                            # Display name
    context: Optional[str] = None        # Additional context
    timestamp: Optional[datetime] = None # When referenced
```

**Example**:
```python
entity = EntityRef(
    entity_type="function",
    entity_id="auth.py::login",
    name="login",
    context="Authentication entry point",
)
```

---

### FocusState

Current focus state in working memory.

```python
@dataclass
class FocusState:
    focus_type: FocusType                    # Type of focus
    focus_id: str                            # Unique focus identifier
    focus_name: str                          # Display name
    context: Dict[str, Any] = field(default_factory=dict)
    set_at: Optional[datetime] = None        # When focus was set
```

**Example**:
```python
focus = FocusState(
    focus_type=FocusType.FILE,
    focus_id="src/auth/login.py",
    focus_name="login.py",
    context={"reason": "User requested analysis of authentication"},
)
```

---

### Finding

Discovery result from analysis.

```python
@dataclass
class Finding:
    finding_id: str                          # Unique identifier
    finding_type: str                        # Type (vulnerability, pattern, etc.)
    title: str                               # Short title
    description: str                         # Detailed description
    location: Optional[str] = None           # File/line location
    severity: str = "info"                   # info, warning, error, critical
    evidence: List[Dict[str, Any]] = field(default_factory=list)
    metadata: Dict[str, Any] = field(default_factory=dict)
    created_at: Optional[datetime] = None
```

**Example**:
```python
finding = Finding(
    finding_id="find-001",
    finding_type="security",
    title="SQL Injection Vulnerability",
    description="User input is not sanitized before database query",
    location="src/db/queries.py:42",
    severity="critical",
    evidence=[
        {"type": "code", "content": "cursor.execute(f'SELECT * FROM users WHERE id={user_id}')"}
    ],
)
```

---

### WorkingMemory

Agent working memory state.

```python
@dataclass
class WorkingMemory:
    session_id: str                          # Session identifier
    user_id: str                             # User identifier
    
    # Focus
    current_focus: Optional[FocusState] = None
    focus_history: List[FocusState] = field(default_factory=list)
    
    # Entities
    entities: List[EntityRef] = field(default_factory=list)
    
    # Findings
    findings: List[Finding] = field(default_factory=list)
    
    # Context
    context: Dict[str, Any] = field(default_factory=dict)
    
    # Metadata
    created_at: Optional[datetime] = None
    updated_at: Optional[datetime] = None
```

**Methods**:

| Method | Signature | Description |
|--------|-----------|-------------|
| `add_entity` | `(entity: EntityRef) -> None` | Add entity to working memory |
| `add_finding` | `(finding: Finding) -> None` | Add finding to working memory |
| `set_focus` | `(focus: FocusState) -> None` | Set current focus (previous saved to history) |
| `clear_focus` | `() -> None` | Clear current focus (saved to history) |

---

### MemoryItem

Generic memory item for storage.

```python
@dataclass
class MemoryItem:
    item_id: str                             # Unique identifier
    item_type: str                           # Type of item
    content: Any                             # Item content
    metadata: Dict[str, Any] = field(default_factory=dict)
    created_at: Optional[datetime] = None
    expires_at: Optional[datetime] = None    # Optional TTL
```

---

### ClarificationContext

Context for clarification requests.

```python
@dataclass
class ClarificationContext:
    question: str                            # Clarification question
    options: List[str] = field(default_factory=list)  # Available options
    context: Dict[str, Any] = field(default_factory=dict)
    timeout_seconds: int = 300               # Timeout for response
    created_at: Optional[datetime] = None
```

**Example**:
```python
clarification = ClarificationContext(
    question="Which authentication method should I analyze?",
    options=["OAuth2", "JWT", "Session-based", "All"],
    context={"detected_methods": ["OAuth2", "JWT"]},
)
```

---

## Factory Functions

### create_working_memory

Create a new working memory instance.

```python
def create_working_memory(session_id: str, user_id: str) -> WorkingMemory:
    """Factory to create new working memory."""
```

### merge_working_memory

Merge two working memory instances.

```python
def merge_working_memory(base: WorkingMemory, update: WorkingMemory) -> WorkingMemory:
    """Merge two working memory instances.
    
    - Focus: update's focus takes precedence
    - History: both histories combined
    - Entities: both entity lists combined
    - Findings: both finding lists combined
    - Context: merged with update taking precedence
    """
```

---

## Focus Operations

### set_focus

Set focus in working memory.

```python
def set_focus(memory: WorkingMemory, focus: FocusState) -> WorkingMemory:
    """Set focus in working memory."""
```

### clear_focus

Clear focus in working memory.

```python
def clear_focus(memory: WorkingMemory) -> WorkingMemory:
    """Clear focus in working memory."""
```

---

## Entity Operations

### add_entity_ref

Add entity reference to working memory.

```python
def add_entity_ref(memory: WorkingMemory, entity: EntityRef) -> WorkingMemory:
    """Add entity reference to working memory."""
```

### get_recent_entities

Get most recent entities from working memory.

```python
def get_recent_entities(memory: WorkingMemory, limit: int = 10) -> List[EntityRef]:
    """Get most recent entities from working memory."""
```

---

## Clarification Operations

### set_clarification

Set clarification context in working memory.

```python
def set_clarification(memory: WorkingMemory, clarification: ClarificationContext) -> WorkingMemory:
    """Set clarification context in working memory."""
```

### clear_clarification

Clear clarification context from working memory.

```python
def clear_clarification(memory: WorkingMemory) -> WorkingMemory:
    """Clear clarification context from working memory."""
```

---

## Serialization

### serialize_working_memory

Serialize working memory to dictionary.

```python
def serialize_working_memory(memory: WorkingMemory) -> Dict[str, Any]:
    """Serialize working memory to dictionary."""
```

**Returns**:
```python
{
    "session_id": "...",
    "user_id": "...",
    "current_focus": {
        "focus_type": "file",
        "focus_id": "...",
        "focus_name": "...",
        "context": {...}
    },
    "entities": [
        {"entity_type": "...", "entity_id": "...", "name": "...", "context": "..."}
    ],
    "findings": [
        {"finding_id": "...", "finding_type": "...", "title": "...", ...}
    ],
    "context": {...}
}
```

### deserialize_working_memory

Deserialize working memory from dictionary.

```python
def deserialize_working_memory(data: Dict[str, Any]) -> WorkingMemory:
    """Deserialize working memory from dictionary."""
```

---

## Usage Examples

### Creating and Managing Memory

```python
from jeeves_protocols import (
    create_working_memory,
    set_focus,
    add_entity_ref,
    FocusState,
    FocusType,
    EntityRef,
    Finding,
)

# Create working memory
memory = create_working_memory(
    session_id="session-123",
    user_id="user-456"
)

# Set focus
memory = set_focus(memory, FocusState(
    focus_type=FocusType.FILE,
    focus_id="src/auth/login.py",
    focus_name="login.py",
))

# Add entity reference
memory = add_entity_ref(memory, EntityRef(
    entity_type="function",
    entity_id="login.py::authenticate",
    name="authenticate",
))

# Add finding
memory.add_finding(Finding(
    finding_id="f-001",
    finding_type="pattern",
    title="Password hashing used",
    description="bcrypt hashing detected in authentication flow",
    location="login.py:45",
    severity="info",
))
```

### Serialization Round-Trip

```python
from jeeves_protocols import (
    serialize_working_memory,
    deserialize_working_memory,
)

# Serialize for storage
data = serialize_working_memory(memory)
json_str = json.dumps(data)

# Deserialize from storage
loaded_data = json.loads(json_str)
restored_memory = deserialize_working_memory(loaded_data)
```

---

## Navigation

- [Back to README](README.md)
- [Previous: Capability](capability.md)
- [Next: Events](events.md)
