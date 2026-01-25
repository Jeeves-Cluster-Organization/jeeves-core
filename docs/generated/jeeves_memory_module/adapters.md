# Adapters

**Location:** `memory_module/adapters/`

---

## Overview

Adapters provide the data access layer for memory operations. They implement the actual database interactions using protocol-based abstractions.

### Available Adapters

| Adapter | Purpose |
|---------|---------|
| `SQLAdapter` | SQL database operations for memory tables |

> Note: ChromaDB VectorAdapter was removed in Constitution v3.0. pgvector is the only supported vector backend.

---

## SQLAdapter

Handles SQL database operations for memory.

### Purpose

Provides a unified interface to all SQL tables for memory management:
- `knowledge_facts` - Fact storage
- `messages` - Conversation messages

### Constructor

```python
def __init__(
    self,
    db_client: DatabaseClientProtocol,
    logger: Optional[LoggerProtocol] = None
)
```

### Write Operations

#### write_fact

```python
async def write_fact(
    self,
    user_id: str,
    data: Dict[str, Any]
) -> str:
    """
    Write to knowledge_facts table.
    
    Args:
        user_id: User identifier
        data: Fact data with keys:
            - domain: Fact category (default: "preferences")
            - key: Fact key
            - value: Fact value
            - confidence: Confidence score (0.0-1.0)
            
    Returns:
        Fact ID (UUID string)
        
    Note: Uses UPSERT on (user_id, domain, key) conflict
    """
```

#### write_message

```python
async def write_message(
    self,
    session_id: str,
    data: Dict[str, Any]
) -> str:
    """
    Write to messages table.
    
    Args:
        session_id: Session identifier
        data: Message data with keys:
            - role: Message role (default: "user")
            - content: Message content
            
    Returns:
        Message ID (as string, from SERIAL column)
    """
```

### Read Operations

#### read_by_id

```python
async def read_by_id(
    self,
    item_id: str,
    item_type: str
) -> Optional[Dict[str, Any]]:
    """
    Read single item by ID.
    
    Args:
        item_id: Unique identifier
        item_type: Type ('fact' or 'message')
        
    Returns:
        Item data or None if not found
        
    Raises:
        ValueError: If item_type is invalid
    """
```

#### read_by_filter

```python
async def read_by_filter(
    self,
    user_id: str,
    item_type: str,
    filters: Dict[str, Any],
    limit: int = 10
) -> List[Dict[str, Any]]:
    """
    Query items with filters.
    
    Args:
        user_id: User identifier
        item_type: Type ('fact' or 'message')
        filters: Filter conditions (column: value)
        limit: Maximum results
        
    Returns:
        List of matching items
        
    Note: messages table doesn't have user_id column,
          so user_id filter is only applied to facts
    """
```

### Update Operations

#### update_item

```python
async def update_item(
    self,
    item_id: str,
    item_type: str,
    updates: Dict[str, Any]
) -> bool:
    """
    Update item fields.
    
    Args:
        item_id: Unique identifier
        item_type: Type ('fact' or 'message')
        updates: Fields to update (column: new_value)
        
    Returns:
        True if successful
        
    Note: Automatically updates last_updated/edited_at timestamp
    """
```

### Delete Operations

#### delete_item

```python
async def delete_item(
    self,
    item_id: str,
    item_type: str,
    soft: bool = True
) -> bool:
    """
    Delete item (soft or hard).
    
    Args:
        item_id: Unique identifier
        item_type: Type ('fact' or 'message')
        soft: If True, mark as deleted; if False, remove from DB
        
    Returns:
        True if successful
        
    Note: Only messages table supports soft delete (deleted_at column)
    """
```

### Table Mappings

| Item Type | Table | ID Column | Timestamp Column |
|-----------|-------|-----------|------------------|
| `fact` | `knowledge_facts` | `fact_id` (UUID) | `last_updated` |
| `message` | `messages` | `message_id` (SERIAL) | `created_at` |

### Usage Example

```python
from memory_module.adapters import SQLAdapter

adapter = SQLAdapter(db_client)

# Write a fact
fact_id = await adapter.write_fact(
    user_id="user-123",
    data={
        "domain": "preferences",
        "key": "theme",
        "value": "dark",
        "confidence": 1.0
    }
)

# Write a message
message_id = await adapter.write_message(
    session_id="sess-456",
    data={
        "role": "user",
        "content": "Hello, world!"
    }
)

# Read by ID
fact = await adapter.read_by_id(fact_id, "fact")
message = await adapter.read_by_id(message_id, "message")

# Query with filters
facts = await adapter.read_by_filter(
    user_id="user-123",
    item_type="fact",
    filters={"domain": "preferences"},
    limit=10
)

# Update
await adapter.update_item(
    item_id=fact_id,
    item_type="fact",
    updates={"value": "light", "confidence": 0.9}
)

# Delete (soft)
await adapter.delete_item(message_id, "message", soft=True)

# Delete (hard)
await adapter.delete_item(fact_id, "fact", soft=False)
```

---

## Database Schema Reference

### knowledge_facts

```sql
CREATE TABLE knowledge_facts (
    fact_id UUID PRIMARY KEY,
    user_id TEXT NOT NULL,
    domain TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    confidence REAL DEFAULT 1.0,
    last_updated TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(user_id, domain, key)
);
```

### messages

```sql
CREATE TABLE messages (
    message_id SERIAL PRIMARY KEY,
    session_id UUID NOT NULL,
    role TEXT NOT NULL DEFAULT 'user',
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    edited_at TIMESTAMPTZ,
    deleted_at TIMESTAMPTZ
);
```

---

## Error Handling

The SQLAdapter logs all operations and errors using structured logging:

```python
# Success logging
self._logger.info("fact_written", fact_id=fact_id, domain=domain, key=key)

# Error logging
self._logger.error("fact_write_failed", error=str(e), domain=domain, key=key)
```

All database errors are re-raised after logging for caller handling.

---

## UUID Handling

The adapter uses `convert_uuids_to_strings` from `shared` to ensure consistent string representation of UUIDs in returned data:

```python
from shared import convert_uuids_to_strings

# In read operations
return convert_uuids_to_strings(dict(result))
```

---

## Navigation

- [Back to README](./README.md)
- [Previous: Manager and IntentClassifier](./manager.md)
