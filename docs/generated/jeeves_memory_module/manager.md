# MemoryManager and IntentClassifier

**Location:** `jeeves_memory_module/manager.py`, `jeeves_memory_module/intent_classifier.py`

---

## Overview

The Memory Module provides two top-level orchestration components:
- **MemoryManager**: Unified facade for memory operations
- **IntentClassifier**: LLM-based classification of user input intent

---

## MemoryManager

Unified memory management interface that coordinates SQL, vector, and cross-reference storage.

### Purpose

- Route writes to appropriate storage
- Coordinate SQL + Vector updates
- Manage cross-references
- Orchestrate hybrid search

### Valid Item Types

Per Code Analysis Agent v3.0, MemoryManager supports only:
- `message` - Conversation messages
- `fact` - Knowledge facts/preferences

> Note: Task and Journal types were removed in Constitution v3.0

### Constructor

```python
def __init__(
    self,
    sql_adapter: SQLAdapter,
    vector_adapter: VectorStorageProtocol,
    xref_manager: CrossRefManager,
    logger: Optional[LoggerProtocol] = None
)
```

### Methods

#### write_message

```python
async def write_message(
    self,
    session_id: str,
    content: str,
    role: str = "user",
    metadata: Optional[Dict[str, Any]] = None
) -> Dict[str, Any]:
    """
    Write a message to memory.
    
    Args:
        session_id: Session identifier
        content: Message content
        role: Message role (user, assistant, system)
        metadata: Additional metadata
        
    Returns:
        {
            "status": "success",
            "item_id": "...",
            "item_type": "message"
        }
    """
```

#### write_fact

```python
async def write_fact(
    self,
    user_id: str,
    key: str,
    value: str,
    domain: str = "preferences",
    confidence: float = 1.0,
    metadata: Optional[Dict[str, Any]] = None
) -> Dict[str, Any]:
    """
    Write a fact to memory.
    
    Args:
        user_id: User identifier
        key: Fact key
        value: Fact value
        domain: Fact domain/category
        confidence: Confidence score (0.0-1.0)
        metadata: Additional metadata
        
    Returns:
        {
            "status": "success",
            "item_id": "...",
            "item_type": "fact"
        }
    """
```

#### read

```python
async def read(
    self,
    user_id: str,
    query: Optional[str] = None,
    filters: Optional[Dict[str, Any]] = None,
    mode: str = "hybrid",
    types: Optional[List[str]] = None,
    limit: int = 10
) -> List[Dict[str, Any]]:
    """
    Read from memory with hybrid search.
    
    Modes:
    - semantic: Vector search only
    - sql: SQL queries only
    - hybrid: Both, merged by relevance
    
    Args:
        user_id: User identifier
        query: Search query (semantic if provided)
        filters: SQL filters
        mode: Search mode
        types: Item types to search (message, fact)
        limit: Maximum results
        
    Returns:
        List of matching items with scores:
        [{"item_id": "...", "source": "sql|vector", "score": 0.85, ...}, ...]
    """
```

#### update

```python
async def update(
    self,
    user_id: str,
    item_id: str,
    item_type: str,
    updates: Dict[str, Any]
) -> Dict[str, Any]:
    """
    Update existing memory item.
    
    If content changed, automatically re-embeds in vector store.
    """
```

#### delete

```python
async def delete(
    self,
    user_id: str,
    item_id: str,
    item_type: str,
    soft: bool = True
) -> bool:
    """
    Delete memory item.
    
    Args:
        soft: If True, soft delete; if False, hard delete with xref cleanup
    """
```

#### wait_for_background_tasks

```python
async def wait_for_background_tasks(
    self,
    timeout: Optional[float] = None
) -> None:
    """Wait for all background tasks (vector writes) to complete."""
```

#### close

```python
async def close(self) -> None:
    """Clean up resources."""
```

### Usage Example

```python
from jeeves_memory_module.manager import MemoryManager
from jeeves_memory_module.adapters import SQLAdapter
from jeeves_memory_module.services import CrossRefManager

# Initialize components
sql_adapter = SQLAdapter(db_client)
vector_adapter = PgVectorRepository(postgres_client, embedding_service)
xref_manager = CrossRefManager(db_client)

manager = MemoryManager(
    sql_adapter=sql_adapter,
    vector_adapter=vector_adapter,
    xref_manager=xref_manager
)

# Write operations
result = await manager.write_message(
    session_id="sess-123",
    content="Hello, world!",
    role="user"
)

await manager.write_fact(
    user_id="user-456",
    key="preferred_language",
    value="python",
    domain="preferences"
)

# Hybrid search
results = await manager.read(
    user_id="user-456",
    query="python programming",
    mode="hybrid",
    types=["message", "fact"],
    limit=5
)

# Cleanup
await manager.close()
```

### Hybrid Search Scoring

When `mode="hybrid"`, results are merged with weighted scoring:
- SQL results: 40% weight
- Vector results: 60% weight

Results are deduplicated by item_id and sorted by combined score.

---

## IntentClassifier

LLM-based classification of user input intent.

### Purpose

Determines whether user input is:
- **Task**: Actionable item with clear completion criteria
- **Journal**: Thought, note, observation, or reflection
- **Fact**: Persistent preference, setting, or knowledge
- **Message**: Conversational continuation (default)

### Constructor

```python
def __init__(
    self,
    llm_provider: LLMProviderProtocol,
    model: Optional[str] = None,
    task_threshold: float = 0.7,
    journal_threshold: float = 0.5,
    fact_threshold: float = 0.8,
    logger: Optional[LoggerProtocol] = None
)
```

### Thresholds

| Type | Default Threshold | Description |
|------|-------------------|-------------|
| Task | 0.7 | Higher threshold for actionable items |
| Journal | 0.5 | Lower threshold for thoughts/notes |
| Fact | 0.8 | High threshold for persistent facts |
| Message | - | Default when others don't meet threshold |

### Methods

#### classify

```python
async def classify(
    self,
    content: str,
    context: Optional[Dict[str, Any]] = None
) -> Dict[str, Any]:
    """
    Classify content intent.
    
    Args:
        content: User input to classify
        context: Optional context (session info, recent messages)
        
    Returns:
        {
            "is_task": 0.0-1.0,
            "is_journal": 0.0-1.0,
            "is_fact": 0.0-1.0,
            "is_message": 0.0-1.0,
            "primary_type": "message|task|journal|fact",
            "task_attributes": {...},
            "journal_attributes": {...},
            "fact_attributes": {...}
        }
    """
```

#### classify_batch

```python
async def classify_batch(
    self,
    contents: List[str],
    contexts: Optional[List[Dict[str, Any]]] = None
) -> List[Dict[str, Any]]:
    """Batch classification for efficiency."""
```

### Extracted Attributes

When confidence meets threshold, additional attributes are extracted:

**Task Attributes** (if `is_task >= 0.7`):
```python
{
    "title": "...",
    "description": "...",
    "due_date": "...",
    "priority": "low|medium|high|urgent",
    "tags": [...]
}
```

**Journal Attributes** (if `is_journal >= 0.5`):
```python
{
    "category": "thought|observation|reflection",
    "sentiment": "positive|negative|neutral"
}
```

**Fact Attributes** (if `is_fact >= 0.8`):
```python
{
    "key": "...",
    "value": "...",
    "domain": "..."
}
```

### Usage Example

```python
from jeeves_memory_module.intent_classifier import IntentClassifier

classifier = IntentClassifier(
    llm_provider=llm_provider,
    model="llama3.1:8b-instruct-q4_0"
)

# Classify user input
result = await classifier.classify(
    content="Buy milk tomorrow",
    context={"session_type": "task_management"}
)

# Result:
{
    "is_task": 0.95,
    "is_journal": 0.05,
    "is_fact": 0.02,
    "is_message": 0.1,
    "primary_type": "task",
    "task_attributes": {
        "title": "Buy milk",
        "due_date": "tomorrow",
        "priority": "low"
    },
    ...
}

# Use primary_type to route the input
if result["primary_type"] == "task":
    await task_service.create(result["task_attributes"])
elif result["primary_type"] == "fact":
    await manager.write_fact(...)
```

### Classification Prompt

The classifier uses a structured prompt requesting JSON output with confidence scores for each type. The LLM analyzes:
- Actionability (task indicators)
- Reflective nature (journal indicators)
- Persistence requirements (fact indicators)
- Conversational context (message indicators)

---

## Navigation

- [Back to README](./README.md)
- [Previous: Services](./services.md)
