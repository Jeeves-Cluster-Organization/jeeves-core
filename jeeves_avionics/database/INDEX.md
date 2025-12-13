# Jeeves Avionics - Database Module

**Parent:** [Avionics Index](../INDEX.md)

---

## Overview

Database clients and repositories for PostgreSQL (primary). Implements connection pooling, health checks, and schema management.

**Note:** Redis support is available for distributed bus functionality (see `jeeves_avionics/distributed/`).
Database clients implement `DatabaseClientProtocol` from `jeeves_protocols`.

---

## Files

| File | Description |
|------|-------------|
| `__init__.py` | Package exports |
| `client.py` | Base database client interface |
| `postgres_client.py` | `PostgresClient` - Full PostgreSQL client with pooling |
| `redis_client.py` | `RedisClient` - Redis client for caching |
| `connection_manager.py` | Connection pool management |
| `factory.py` | Database client factory |
| `init_schema.py` | Schema initialization utilities |

## Subdirectories

| Directory | Description |
|-----------|-------------|
| `migrations/` | SQL migration scripts |
| `repositories/` | Data access repositories |
| `schemas/` | Database schema definitions |

---

## Key Components

### PostgresClient (`postgres_client.py`)
Primary database client:
```python
class PostgresClient:
    async def execute(self, query: str, params: dict = None) -> None
    async def fetch_one(self, query: str, params: dict = None) -> Optional[dict]
    async def fetch_all(self, query: str, params: dict = None) -> List[dict]
    async def health_check(self) -> bool
```

### Connection Management (`connection_manager.py`)
- Connection pooling with configurable min/max connections
- Automatic reconnection on failure
- Health monitoring

---

## Constitutional Compliance

**S3 (Database Credentials):**
- Credentials via environment variables only
- No hardcoded passwords in production code
- Test fixtures use placeholder credentials

---

*Last updated: 2025-12-08*
