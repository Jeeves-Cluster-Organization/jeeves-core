"""Memory repositories for data persistence.

Memory Module Audit (2025-12-09):
- Moved from jeeves_avionics/memory/repositories/

Repositories:
- EventRepository: L2 append-only event log
- TraceRepository: Agent execution traces
- PgVectorRepository: L3 semantic vector storage
- ChunkRepository: L3 semantic chunks
- GraphRepository: L5 entity relationships
- SessionStateRepository: L4 working memory
- ToolMetricsRepository: L7 tool health metrics
"""

from jeeves_memory_module.repositories.event_repository import EventRepository, DomainEvent
from jeeves_memory_module.repositories.trace_repository import TraceRepository, AgentTrace
from jeeves_memory_module.repositories.pgvector_repository import PgVectorRepository
from jeeves_memory_module.repositories.chunk_repository import ChunkRepository, Chunk
from jeeves_memory_module.repositories.graph_repository import GraphRepository
from jeeves_memory_module.repositories.session_state_repository import SessionStateRepository, SessionState
from jeeves_memory_module.repositories.tool_metrics_repository import ToolMetricsRepository, ToolMetric

__all__ = [
    # L2 Events
    "EventRepository",
    "DomainEvent",
    # Traces
    "TraceRepository",
    "AgentTrace",
    # L3 Semantic
    "PgVectorRepository",
    "ChunkRepository",
    "Chunk",
    # L5 Graph
    "GraphRepository",
    # L4 Session
    "SessionStateRepository",
    "SessionState",
    # L7 Metrics
    "ToolMetricsRepository",
    "ToolMetric",
]
