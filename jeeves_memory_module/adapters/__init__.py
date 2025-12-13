"""Memory storage adapters.

Constitution v3.0 Compliance:
  - VectorAdapter (ChromaDB) has been removed
  - pgvector is the only supported vector backend
"""

from jeeves_memory_module.adapters.sql_adapter import SQLAdapter

__all__ = ["SQLAdapter"]
