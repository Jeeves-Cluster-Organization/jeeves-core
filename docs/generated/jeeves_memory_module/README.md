# Jeeves Memory Module

**Layer:** L2 (Memory Services)  
**Updated:** 2026-01-23  
**Constitution:** `memory_module/CONSTITUTION.md`

---

## Overview

The Jeeves Memory Module provides comprehensive memory services (L1-L7) for the Jeeves system. It implements persistence, semantic search, event sourcing, session state management, and tool health governance.

### Key Responsibilities

- **Event Sourcing** (L2): Append-only domain event log with correlation tracking
- **Semantic Memory** (L3): Embeddings-based search with pgvector
- **Working Memory** (L4): Per-session state management and focus tracking
- **Entity Graphs** (L5): Relationship tracking between entities (stub)
- **Skills/Patterns** (L6): Learned behavior patterns (stub)
- **Tool Health** (L7): Performance monitoring and circuit breaker support

---

## Constitutional Hierarchy

```
protocols/         ← Foundation (protocols, memory types)
          ↑
shared/            ← Shared utilities (logging, serialization)
          ↑
memory_module/     ← THIS (memory services, implementations)
          ↑
avionics/          ← Infrastructure (database factory only)
          ↑
mission_system/    ← Application layer
```

---

## Memory Layers Architecture

| Layer | Name | Scope | Implementation | Documentation |
|-------|------|-------|----------------|---------------|
| **L1** | Episodic | Per-request | `Envelope` (core_engine) | N/A |
| **L2** | Events | Append-only log | `EventRepository` + `EventEmitter` | [event_sourcing.md](./event_sourcing.md) |
| **L3** | Semantic | Embeddings/search | `ChunkService` + `PgVectorRepository` | [semantic_memory.md](./semantic_memory.md) |
| **L4** | Working | Per-session state | `SessionStateService` | [session_state.md](./session_state.md) |
| **L5** | Graph | Entity relationships | `InMemoryGraphStorage` (stub) | [entity_graphs.md](./entity_graphs.md) |
| **L6** | Skills | Learned patterns | `InMemorySkillStorage` (stub) | [skills.md](./skills.md) |
| **L7** | Meta | Tool metrics | `ToolHealthService` | [tool_health.md](./tool_health.md) |

---

## Module Structure

```
memory_module/
├── CONSTITUTION.md          # Constitutional rules and principles
├── __init__.py              # Public exports (register_memory_handlers, reset_cached_services)
├── manager.py               # MemoryManager facade
├── handlers.py              # CommBus handler registration
├── intent_classifier.py     # LLM-based intent classification
│
├── adapters/                # Protocol implementations
│   ├── __init__.py
│   └── sql_adapter.py       # SQLAdapter for SQL operations
│
├── services/                # Memory services
│   ├── __init__.py
│   ├── session_state_adapter.py  # SessionStateProtocol impl
│   ├── session_state_service.py  # L4 session management
│   ├── chunk_service.py     # L3 semantic chunking
│   ├── embedding_service.py # Embedding generation
│   ├── event_emitter.py     # L2 event emission
│   ├── trace_recorder.py    # Agent trace recording
│   ├── tool_health_service.py # L7 tool metrics
│   ├── nli_service.py       # NLI-based claim verification
│   ├── code_indexer.py      # Code file indexing for RAG
│   └── xref_manager.py      # Cross-reference management
│
├── repositories/            # Persistence layer
│   ├── __init__.py
│   ├── chunk_repository.py  # L3 semantic chunks
│   ├── event_repository.py  # L2 event log
│   ├── graph_stub.py        # L5 in-memory stub
│   ├── skill_stub.py        # L6 in-memory stub
│   ├── session_state_repository.py  # L4 working memory
│   ├── pgvector_repository.py # Vector storage
│   ├── trace_repository.py  # Agent traces
│   └── tool_metrics_repository.py # L7 metrics
│
├── messages/                # CommBus messages
│   ├── __init__.py
│   ├── events.py            # MemoryStored, SessionStateChanged, etc.
│   ├── queries.py           # GetSessionState, SearchMemory, etc.
│   └── commands.py          # ClearSession, UpdateFocus, etc.
│
└── tests/                   # Unit tests
```

---

## Quick Start

### Handler Registration

```python
from memory_module import register_memory_handlers

# In application startup
register_memory_handlers(
    commbus=commbus,
    session_state_service=session_state_service,
    db=db_client,
)
```

### Using MemoryManager

```python
from memory_module.manager import MemoryManager

manager = MemoryManager(
    sql_adapter=sql_adapter,
    vector_adapter=vector_adapter,
    xref_manager=xref_manager,
)

# Write a message
result = await manager.write_message(
    session_id="sess-123",
    content="Hello, world!",
    role="user"
)

# Search memory
results = await manager.read(
    user_id="user-456",
    query="greeting messages",
    mode="hybrid"
)
```

---

## Core Principles

### P1: Memory Types in protocols

All memory types are defined in `protocols/memory.py`:
- `WorkingMemory` - Session-level cognitive state
- `FocusState` / `FocusType` - Focus tracking
- `EntityRef` - Entity reference tracking
- `MemoryItem` - Generic memory item
- `Finding` - Exploration finding

### P2: Memory Protocols in protocols

Memory protocols are defined in `protocols/protocols.py`:
- `MemoryServiceProtocol` - CRUD + search operations
- `SemanticSearchProtocol` - Embedding-based search
- `SessionStateProtocol` - Session state management

### P3: Single Ownership

This module **owns** all memory-specific implementations:
- Repositories (persistence layer)
- Services (business logic)
- Adapters (protocol implementations)
- Manager (facade)

### P4: CommBus Communication

Memory operations publish events via CommBus for observability:
- `MemoryStored` - When items are persisted
- `MemoryRetrieved` - When searches complete
- `SessionStateChanged` - When session state mutates

---

## Dependency Rules

### ALLOWED Dependencies

```python
from protocols import WorkingMemory, Finding, DatabaseClientProtocol
from shared import create_logger, to_json, uuid_str
from avionics.database.factory import create_database_client
```

### FORBIDDEN Dependencies

```
memory_module ✗→ avionics.database.postgres_client
memory_module ✗→ avionics.llm/*
memory_module ✗→ mission_system
memory_module ✗→ jeeves-capability-*
memory_module ✗→ control_tower
```

---

## Component Documentation

| Component | Description | Link |
|-----------|-------------|------|
| Event Sourcing | L2 append-only event log with deduplication | [event_sourcing.md](./event_sourcing.md) |
| Semantic Memory | L3 embeddings, chunking, and vector search | [semantic_memory.md](./semantic_memory.md) |
| Session State | L4 working memory and focus management | [session_state.md](./session_state.md) |
| Entity Graphs | L5 relationship tracking (extensible stub) | [entity_graphs.md](./entity_graphs.md) |
| Skills/Patterns | L6 learned patterns (extensible stub) | [skills.md](./skills.md) |
| Tool Health | L7 performance monitoring and circuit breakers | [tool_health.md](./tool_health.md) |
| CommBus Messages | Events, queries, and commands | [messages.md](./messages.md) |
| Services | Service layer components | [services.md](./services.md) |
| Manager & Classifier | MemoryManager facade and IntentClassifier | [manager.md](./manager.md) |
| Adapters | SQL database adapter | [adapters.md](./adapters.md) |

---

## Testing

```bash
# Run memory module tests
pytest jeeves-core/memory_module/tests/ -v

# Run specific layer tests
pytest jeeves-core/memory_module/tests/unit/repositories/ -v
pytest jeeves-core/memory_module/tests/unit/services/ -v
```
