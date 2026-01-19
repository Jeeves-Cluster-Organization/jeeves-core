"""Database infrastructure - PostgreSQL and Redis clients.

Note: create_vector_adapter moved to jeeves_mission_system.adapters
      to respect layer boundaries (avionics should not import from memory_module).

Graph Storage:
- PostgresGraphAdapter: Production graph storage implementing GraphStorageProtocol
- For in-memory testing, use InMemoryGraphStorage from jeeves_memory_module
"""

from jeeves_avionics.database.client import DatabaseClientProtocol
from jeeves_avionics.database.connection_manager import ConnectionManager
from jeeves_avionics.database.factory import create_database_client
from jeeves_avionics.database.postgres_client import PostgreSQLClient
from jeeves_avionics.database.redis_client import RedisClient
from jeeves_avionics.database.postgres_graph import PostgresGraphAdapter

__all__ = [
    "DatabaseClientProtocol",
    "ConnectionManager",
    "create_database_client",
    "PostgreSQLClient",
    "RedisClient",
    # Graph storage adapter
    "PostgresGraphAdapter",
]
