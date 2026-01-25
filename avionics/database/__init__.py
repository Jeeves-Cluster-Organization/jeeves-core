"""Database infrastructure - PostgreSQL and Redis clients.

Note: create_vector_adapter moved to jeeves_mission_system.adapters
      to respect layer boundaries (avionics should not import from memory_module).

Graph Storage:
- PostgresGraphAdapter: Production graph storage implementing GraphStorageProtocol
- For in-memory testing, use InMemoryGraphStorage from memory_module
"""

from avionics.database.client import DatabaseClientProtocol
from avionics.database.connection_manager import ConnectionManager
from avionics.database.factory import create_database_client
from avionics.database.postgres_client import PostgreSQLClient
from avionics.database.redis_client import RedisClient
from avionics.database.postgres_graph import PostgresGraphAdapter

__all__ = [
    "DatabaseClientProtocol",
    "ConnectionManager",
    "create_database_client",
    "PostgreSQLClient",
    "RedisClient",
    # Graph storage adapter
    "PostgresGraphAdapter",
]
