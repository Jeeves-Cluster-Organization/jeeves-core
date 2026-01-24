# Database Module

Database infrastructure for PostgreSQL (with pgvector) and Redis.

## Navigation

- [README](./README.md) - Overview
- [Settings](./settings.md)
- [Gateway](./gateway.md)
- **Database** (this file)
- [LLM](./llm.md)
- [Observability](./observability.md)
- [Tools](./tools.md)
- [Infrastructure](./infrastructure.md)

---

## database/client.py

Protocol re-exports and registry functions.

### Re-exported Protocols

```python
from jeeves_protocols import DatabaseClientProtocol, VectorStorageProtocol
```

### Factory Function

```python
async def create_database_client(
    settings: Settings,
    logger: Optional[LoggerProtocol] = None,
    auto_init_schema: bool = True
) -> DatabaseClientProtocol:
    """Create and initialize a database client."""
```

### Registry Functions

```python
def register_backend(name: str, factory: Callable) -> None:
    """Register a database backend factory."""

def unregister_backend(name: str) -> None:
    """Unregister a database backend."""

def list_backends() -> List[str]:
    """List registered backends."""

def is_backend_registered(name: str) -> bool:
    """Check if a backend is registered."""
```

---

## database/postgres_client.py

Async PostgreSQL client with pgvector support using SQLAlchemy.

### Class: PostgreSQLClient

```python
class PostgreSQLClient:
    """Async PostgreSQL client with helper methods and pgvector support."""
    
    def __init__(
        self,
        database_url: str,
        pool_size: int = 20,
        max_overflow: int = 10,
        pool_timeout: int = 30,
        pool_recycle: int = 3600,
        echo: bool = False,
        logger: Optional[LoggerProtocol] = None,
    ):
        ...
    
    backend = "postgres"  # Backend identifier
```

#### Connection Methods

```python
async def connect(self) -> None:
    """Establish database connection and create engine."""

async def disconnect(self) -> None:
    """Close database connection and dispose engine."""

async def close(self) -> None:
    """Alias for disconnect()."""
```

#### Context Managers

```python
@asynccontextmanager
async def session(self) -> AsyncIterator[AsyncSession]:
    """Get a database session context manager."""

@asynccontextmanager
async def transaction(self) -> AsyncIterator[AsyncSession]:
    """Get a transaction context manager (auto-commit/rollback)."""
```

#### Schema Methods

```python
async def initialize_schema(self, schema_path: str) -> None:
    """Initialize database schema from SQL file."""
```

#### Query Methods

```python
async def execute(self, query: str, parameters: Optional[Dict] = None) -> Any:
    """Execute a query with parameters."""

async def execute_many(self, query: str, parameters_list: List[Dict]) -> None:
    """Execute a query multiple times with different parameters."""

async def execute_script(self, script: str) -> None:
    """Execute a multi-statement SQL script."""

async def insert(self, table: str, data: Dict[str, Any]) -> Any:
    """Insert a row and return the first column's value."""

async def update(self, table: str, data: Dict, where_clause: str, where_params: Any = None) -> int:
    """Update rows and return affected count."""

async def fetch_one(self, query: str, parameters: Optional[Dict] = None, commit: bool = False) -> Optional[Dict]:
    """Fetch one row from query results."""

async def fetch_all(self, query: str, parameters: Optional[Dict] = None) -> List[Dict]:
    """Fetch all rows from query results."""
```

#### Health and Stats

```python
async def health_check(self) -> Dict[str, Any]:
    """Perform a health check on the database connection."""

async def get_table_stats(self, table_name: str) -> Dict[str, Any]:
    """Get statistics for a specific table."""

async def get_all_tables_stats(self) -> List[Dict]:
    """Get statistics for all tables."""

async def vacuum_analyze(self, table_name: Optional[str] = None) -> None:
    """Run VACUUM ANALYZE on a table or all tables."""
```

---

## database/redis_client.py

Redis client for distributed state management.

### Class: RedisClient

```python
class RedisClient:
    """Async Redis client for distributed state management.
    
    Provides common distributed primitives:
    - Rate limiting (sliding window)
    - Distributed locks with TTL
    - Embedding cache
    - Session state
    """
    
    def __init__(
        self,
        redis_url: str = "redis://localhost:6379",
        logger: Optional[LoggerProtocol] = None,
    ):
        ...
```

#### Connection Methods

```python
async def connect(self) -> None:
    """Establish Redis connection."""

async def disconnect(self) -> None:
    """Close Redis connection."""
```

#### Rate Limiting

```python
async def rate_limit_check(
    self,
    user_id: str,
    limit: int,
    window_seconds: int = 60
) -> Tuple[bool, int]:
    """Sliding window rate limiter. Returns (allowed, remaining)."""
```

#### Distributed Locks

```python
async def acquire_lock(
    self,
    key: str,
    ttl_seconds: int = 30,
    owner: Optional[str] = None
) -> bool:
    """Acquire distributed lock with automatic expiry."""

async def release_lock(self, key: str) -> bool:
    """Release distributed lock."""
```

#### Embedding Cache

```python
async def cache_embedding(
    self,
    text_hash: str,
    vector: List[float],
    ttl: int = 3600
) -> None:
    """Cache computed embedding vector."""

async def get_cached_embedding(self, text_hash: str) -> Optional[List[float]]:
    """Retrieve cached embedding."""
```

#### Generic Operations

```python
async def set_value(self, key: str, value: Any, ttl: Optional[int] = None) -> None:
    """Set arbitrary value with optional TTL."""

async def get_value(self, key: str) -> Optional[Any]:
    """Get value by key."""

async def delete(self, key: str) -> bool:
    """Delete key."""

async def increment(self, key: str, amount: int = 1) -> int:
    """Increment counter."""

async def get_stats(self) -> dict:
    """Get Redis server stats."""
```

---

## database/connection_manager.py

Deployment-mode-aware backend selection.

### Class: StateBackend

```python
class StateBackend:
    """Abstract interface for state storage backends."""
    
    async def rate_limit_check(self, user_id: str, limit: int, window_seconds: int) -> Tuple[bool, int]: ...
    async def acquire_lock(self, key: str, ttl_seconds: int, owner: Optional[str] = None) -> bool: ...
    async def release_lock(self, key: str) -> bool: ...
    async def cache_embedding(self, text_hash: str, vector: List[float], ttl: int) -> None: ...
    async def get_cached_embedding(self, text_hash: str) -> Optional[List[float]]: ...
```

### Class: InMemoryStateBackend

```python
class InMemoryStateBackend(StateBackend):
    """In-memory state backend for single-node deployments."""
```

### Class: RedisStateBackend

```python
class RedisStateBackend(StateBackend):
    """Redis-based state backend for distributed deployments."""
```

### Class: ConnectionManager

```python
class ConnectionManager:
    """Manages state backend selection based on deployment mode."""
    
    def __init__(self, settings: Settings, logger: Optional[LoggerProtocol] = None):
        ...
    
    async def get_state_backend(self) -> StateBackend:
        """Get appropriate state backend for current deployment mode."""
    
    async def close(self) -> None:
        """Close backend connections."""
```

---

## database/factory.py

Database factory using registry pattern.

### Function: create_database_client

```python
async def create_database_client(
    settings: Settings,
    logger: Optional[LoggerProtocol] = None,
    auto_init_schema: bool = True
) -> DatabaseClientProtocol:
    """Create and initialize a database client.
    
    Args:
        settings: Application settings
        logger: Logger for DI
        auto_init_schema: If True, initialize schema on first run
    
    Returns:
        DatabaseClientProtocol instance (connected, schema verified)
    """
```

---

---

## Exports

The database module exports:

```python
from jeeves_avionics.database import (
    DatabaseClientProtocol,
    ConnectionManager,
    create_database_client,
    PostgreSQLClient,
    RedisClient,
    PostgresGraphAdapter,  # Graph storage adapter
)
```

---

## Usage Examples

### PostgreSQL Client

```python
from jeeves_avionics.database import create_database_client
from jeeves_avionics.settings import get_settings

settings = get_settings()
client = await create_database_client(settings)

# Query
users = await client.fetch_all("SELECT * FROM users WHERE active = :active", {"active": True})

# Insert
await client.insert("users", {"name": "Alice", "email": "alice@example.com"})

# Transaction
async with client.transaction() as session:
    await session.execute("UPDATE users SET status = 'verified' WHERE id = :id", {"id": 1})
    await session.execute("INSERT INTO audit_log (action) VALUES ('verify')")

await client.disconnect()
```

### Redis Client

```python
from jeeves_avionics.database.redis_client import get_redis_client

redis = get_redis_client("redis://localhost:6379")
await redis.connect()

# Rate limiting
allowed, remaining = await redis.rate_limit_check("user_123", limit=10, window_seconds=60)
if not allowed:
    raise Exception("Rate limit exceeded")

# Distributed lock
if await redis.acquire_lock("job:123", ttl_seconds=60):
    try:
        # Do work
        pass
    finally:
        await redis.release_lock("job:123")

await redis.disconnect()
```

### Connection Manager

```python
from jeeves_avionics.database.connection_manager import ConnectionManager
from jeeves_avionics.settings import get_settings

settings = get_settings()
manager = ConnectionManager(settings)

backend = await manager.get_state_backend()
allowed, remaining = await backend.rate_limit_check("user_123", limit=10, window_seconds=60)

await manager.close()
```
